"""Run only in a disposable Linux container; mocked systemd, real files/fsync."""
import contextlib
import copy
import io
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch
import stage8b_p1f_o2_boot_id_install as m

@unittest.skipUnless(sys.platform=='linux' and os.geteuid()==0 and os.environ.get('MOEX_BOOT_ID_DISPOSABLE_TEST')=='1','disposable root container only')
class Transaction(unittest.TestCase):
    def setUp(self):
        self.stack=contextlib.ExitStack();self.addCleanup(self.stack.close)
        self.root=Path(tempfile.mkdtemp(prefix='boot-id-test-',dir='/'))
        self.addCleanup(lambda:shutil.rmtree(self.root))
        self.config=self.root/'config';self.config.mkdir();(self.config/'bootstrap').mkdir();(self.config/'credentials').mkdir()
        self.control=self.root/'control';self.control.mkdir();self.state=self.root/'state';self.state.mkdir()
        self.staging=self.root/'staging';self.staging.mkdir();self.prior=self.root/'prior';self.prior.mkdir()
        self.input=self.root/'input';self.input.mkdir()
        self.unit=self.root/'runner.service';self.manifest=self.root/'installation.json';self.tx=self.root/'transaction'
        retired=tuple(str(self.config/p) for p in ('bootstrap/source.json','supervisor.json','credentials/key'))
        for name,value in {'UNIT':str(self.unit),'MANIFEST':str(self.manifest),'TX':str(self.tx),
                           'CONFIG':str(self.config),'STATE':str(self.state),'RETIRED':retired}.items():
            self.stack.enter_context(patch.object(m,name,value))
        self.stack.enter_context(patch.object(m.c,'CONTROL',str(self.control)))
        self.stack.enter_context(patch.object(m.c,'STAGING',str(self.staging)))
        self.stack.enter_context(patch.object(m.retained,'RETAINED_DIRS',()))
        self.stack.enter_context(patch.object(m.retained,'TRANSACTION',str(self.prior)))
        for name in ('.execution.lock','.guardian.lock'):
            p=self.control/name;p.write_bytes(b'');p.chmod(0o600)
        self.head=dict(state='FAILED',authority_generation=1,latest_sequence=15,latest_event_sha256=m.HEAD,active_manifest_sha256=None)
        (self.control/'history-head.json').write_bytes(m.c.canonical(self.head))
        newunit=(m.contract.ROOT/m.contract.UNIT).read_bytes()
        oldunit=newunit.decode().replace('# Production guardian reads /proc/sys/kernel/random/boot_id before admission.\n','').replace('# Keep kernel tunables read-only and all other sandbox restrictions below.\n','').replace('ProcSubset=all','ProcSubset=pid').encode()
        self.unit.write_bytes(oldunit);self.unit.chmod(0o644)
        old={'payload':{str(self.unit):dict(sha256=m.c.sha(oldunit),size=len(oldunit),mode='0644',group='root',owner='root')},'installation_id':'fixture'}
        oldraw=m.c.canonical(old);self.manifest.write_bytes(oldraw);self.manifest.chmod(0o644)
        self.stack.enter_context(patch.object(m,'OLD',m.c.sha(oldraw)))
        new=copy.deepcopy(old);new['payload'][str(self.unit)].update(sha256=m.c.sha(newunit),size=len(newunit))
        own=m.c.sha(Path(m.__file__).read_bytes())
        new.update(predecessor_manifest_sha256=m.OLD,installer_sha256=own,update_revision='boot-id-sandbox-after-failed-15-v1')
        self.newraw=m.c.canonical(new)
        for p,raw in zip(retired,(b'fixture-source',b'fixture-config',bytes(range(32)))):
            Path(p).write_bytes(raw);Path(p).chmod(0o400 if p==retired[2] else 0o440)
        inv={}
        for p in [self.config,*self.config.rglob('*')]:
            if p.is_file():v=m.meta(p,'credentials' in p.parts)
            else:
                s=p.stat();v=dict(uid=s.st_uid,gid=s.st_gid,mode=oct(s.st_mode&0o777))
            inv[str(p.relative_to(self.config))]=v
        self.e=dict(history_head=self.head,authority_inventory=m.retained.inventory(self.control),
            config_inventory=inv,durable_inventory=m.retained.inventory(self.state),staging_inventory=m.retained.inventory(self.staging),
            retained_inventories={'transaction_inventory':m.retained.inventory(self.prior)},units={},exhausted_units={},unit_file_sha256={},
            scheduler_service={'ActiveState':'failed','MainPID':'0'},scheduler_timer={'ActiveState':'active','NextElapseUSecRealtime':''},
            public_phase_files={'materialized-set-receipt.json':{'text':json.dumps(dict(source_sha256=m.c.sha(b'fixture-source'),final_config_sha256=m.c.sha(b'fixture-config')))}})
        before=m.c.canonical(self.e)
        spec=dict(domain='o2-boot-id-correction-v1',source_ref='a'*40,before_sha256=m.c.sha(before),new_manifest_sha256=m.c.sha(self.newraw),installer_sha256=own)
        for n,raw in {'before.json':before,'spec.json':m.c.canonical(spec),'runner.service':newunit,
                      'runner.before.service':oldunit,'installation.before.json':oldraw,'installation-o2-v1.json':self.newraw}.items():
            (self.input/n).write_bytes(raw)
        def command(args):
            if args[:2]==['systemctl','list-jobs']:return ''
            expected=self.e['scheduler_timer' if args[2].endswith('.timer') else 'scheduler_service']
            return '\n'.join(k+'='+v for k,v in expected.items())
        self.stack.enter_context(patch.object(m.c,'command',side_effect=command))
        self.install=m.Install(self.input)

    def apply(self):
        with contextlib.redirect_stdout(io.StringIO()):self.install.apply()

    def test_apply_exact_two_slots_and_retire_three(self):
        self.apply()
        self.assertEqual(self.manifest.read_bytes(),self.newraw)
        self.assertEqual(json.loads((self.tx/'journal.json').read_bytes())['state'],'APPLIED')
        self.assertEqual(m.retained.inventory(self.control),self.e['authority_inventory'])
        for i,p in enumerate(m.RETIRED):
            self.assertFalse(Path(p).exists());self.assertTrue((self.tx/('retired-'+str(i))).is_file())

    def test_no_repeat(self):
        self.apply()
        with self.assertRaises(RuntimeError):self.apply()

    def test_foreign_generated_source_rejected_before_write(self):
        Path(m.RETIRED[0]).chmod(0o600)
        Path(m.RETIRED[0]).write_bytes(b'foreign')
        Path(m.RETIRED[0]).chmod(0o440)
        with self.assertRaises(RuntimeError):self.apply()
        self.assertFalse(self.tx.exists())

    def test_authority_drift_rejected_before_write(self):
        (self.control/'foreign').write_bytes(b'foreign')
        with self.assertRaises(RuntimeError):self.apply()
        self.assertFalse(self.tx.exists())

    def test_wrong_payload_rejected(self):
        (self.input/'runner.service').write_bytes(b'foreign')
        with self.assertRaises(Exception):m.Install(self.input)

    def test_fault_after_first_slot_retains_backups_and_history(self):
        real=m.c.atomic_write
        def fail(root,path,data,mode,gid):
            real(root,path,data,mode,gid)
            if path==m.UNIT:raise RuntimeError('INJECTED_AFTER_UNIT')
        with patch.object(m.c,'atomic_write',side_effect=fail),self.assertRaises(RuntimeError):self.apply()
        self.assertEqual(json.loads((self.tx/'journal.json').read_bytes())['state'],'PREPARED')
        self.assertEqual(self.manifest.read_bytes(),(self.tx/'before-1').read_bytes())
        self.assertEqual(m.retained.inventory(self.control),self.e['authority_inventory'])
        for i in range(3):self.assertTrue((self.tx/('retired-'+str(i))).exists())

    def test_fault_after_first_retirement_is_retained_not_retried(self):
        real=m.os.rename
        def fail(source,dest):
            real(source,dest)
            raise RuntimeError('INJECTED_AFTER_RETIRE')
        with patch.object(m.os,'rename',side_effect=fail),self.assertRaises(RuntimeError):self.apply()
        self.assertEqual(json.loads((self.tx/'journal.json').read_bytes())['state'],'PREPARED')
        self.assertTrue((self.tx/'retired-0').exists());self.assertFalse(Path(m.RETIRED[0]).exists())
        self.assertTrue(Path(m.RETIRED[1]).exists());self.assertTrue(Path(m.RETIRED[2]).exists())
        self.assertEqual(m.retained.inventory(self.control),self.e['authority_inventory'])
        with self.assertRaises(RuntimeError):self.apply()

if __name__=='__main__':unittest.main()
