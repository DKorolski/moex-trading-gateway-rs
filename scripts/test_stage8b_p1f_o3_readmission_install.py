#!/usr/bin/env python3
"""Offline Linux fixture tests: real files/flock/fsync, synthetic host/DBSIZE.

No installed host, network, Redis writes or private key input. Fixture payload
bytes are independently checked against the exact accepted predecessor manifest.
"""
import copy
import errno
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import stage8b_p1f_o3_readmission_install as m


class Crash(Exception):
    pass


class PackageReader(unittest.TestCase):
    def test_fresh_public_read_may_update_atime(self):
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp)/'public.json';p.write_bytes(b'{"public":true}\n')
            os.utime(p,ns=(0,p.stat().st_mtime_ns))
            before=p.stat()
            self.assertEqual(m.package_file(p),b'{"public":true}\n')
            after=p.stat()
            self.assertEqual(before.st_mtime_ns,after.st_mtime_ns)
            self.assertEqual(before.st_ctime_ns,after.st_ctime_ns)
            print('fresh-public-read atime_changed='+str(before.st_atime_ns!=after.st_atime_ns))


class Installation(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.package=Path(os.environ['O3_READMISSION_PACKAGE'])
        cls.spec,cls.baseline,cls.old,cls.payload=m.load(cls.package)
        snapshot=m.parse(Path(os.environ['O3_BEFORE_SNAPSHOT']).read_bytes())
        cls.before={m.c.MANIFEST:(cls.package/'before-outer.json').read_bytes()}
        for n,e in cls.old['payload'].items():
            if n in (*m.ELFS,*m.prior.TEMPLATES,m.O3):
                raw=(Path(os.environ['O3_BEFORE_PAYLOAD'])/Path(n).name).read_bytes()
            elif n.endswith(('/stage8b-p1-paper-supervisor','/stage8b-p1f-o2-materializer')):
                raw=(Path(os.environ['O2_BEFORE_ELFS'])/Path(n).name).read_bytes()
            else:raw=snapshot['old_public_payload'][n].encode()
            assert m.sha(raw)==e['sha256'] and len(raw)==e['size'],n
            cls.before[n]=raw

    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name);self.root.chmod(0o755)
        self.write('/etc/passwd',b'root:x:0:0:root:/root:/bin/sh\nmoex-p1-paper:x:987:987::/nonexistent:/usr/sbin/nologin\n')
        self.write('/etc/group',b'root:x:0:\nmoex-p1-paper:x:987:\n')
        for n,raw in self.before.items():
            e=self.old['payload'].get(n,dict(mode='0644',group='root'))
            self.write(n,raw,int(e['mode'],8),987 if e['group']=='service' else 0)
        self.write(m.config.CONFIG,m.config.corrected(m.config.FIXTURE.read_bytes().rstrip(b'\n')),0o440,987)
        self.write(m.c.CONTROL+'/authority/history-head.json',m.canonical(self.baseline['observed']['history_head']),0o440,987)
        for name in ('.execution.lock','.guardian.lock'):self.write(m.c.CONTROL+'/'+name,b'',0o600)
        for n in (m.inv.prior.STATE,m.c.STAGING):self.path(n).mkdir(parents=True,exist_ok=True)
        self.write(m.inv.prior.STATE+'/journal',b'synthetic retained durable state',0o600,987,987)
        self.write(m.inv.prior.CONFIG+'/credentials/lifecycle.key',b'not a usable key; synthetic metadata fixture',0o400)
        for tx in (m.prior.TX,m.config.TX):self.write(tx+'/journal.json',b'accepted prior transaction fixture',0o600)
        self.host=patch.object(m.inv.prior,'host_observation',return_value={'local_test_stopped_host':True})
        self.trees=patch.object(m.inv,'TREES',(m.inv.prior.STATE,m.inv.prior.CONFIG,m.c.CONTROL,m.c.STAGING))
        def command(args):
            self.assertEqual(args,['redis-cli','-h','127.0.0.1','-p','6379','-n','15','DBSIZE'])
            return '0\n'
        self.commands=patch.object(m.c,'command',side_effect=command)
        self.host.start();self.trees.start();self.commands.start()
        self.expected=m.inv.observe(self.root)
        self.prior_tx={key:m.inv.inventory(self.path(tx)) for tx,key in
            ((m.prior.TX,'old_installation_transaction'),(m.config.TX,'correction_transaction'))}

    def tearDown(self):
        self.commands.stop();self.trees.stop();self.host.stop();self.tmp.cleanup()

    def fresh(self):self.tearDown();self.setUp()
    def path(self,n):return self.root/n.lstrip('/')
    def write(self,n,raw,mode=0o644,gid=0,uid=0):
        p=self.path(n);p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(raw)
        os.chown(p,uid,gid);p.chmod(mode)

    def update(self):
        value=m.Update(self.root,self.package)
        # Only the synthetic host fixture differs. Production load/payload
        # derivation and all custody, locks, reads, writes remain unmocked.
        value.baseline['observed']=copy.deepcopy(self.expected)
        value.baseline.update(copy.deepcopy(self.prior_tx))
        return value

    def unchanged(self):
        self.assertEqual(m.inv.observe(self.root),self.expected)
        for tx,key in ((m.prior.TX,'old_installation_transaction'),(m.config.TX,'correction_transaction')):
            self.assertEqual(m.inv.inventory(self.path(tx)),self.prior_tx[key])
        for n,raw in self.before.items():
            if n not in m.CHANGES:self.assertEqual(self.path(n).read_bytes(),raw)

    def test_exact_seven_slots_and_no_activation(self):
        self.assertEqual(set(m.CHANGES),set(m.ELFS)|{m.O3,m.c.MANIFEST})
        self.assertEqual(len(m.CHANGES),7)
        self.assertFalse(self.spec['installation_authorized']);self.assertFalse(self.spec['activation_authorized'])
        for n in m.prior.TEMPLATES:self.assertNotIn(n,self.spec['updates'])
        old_o3=m.parse(self.before[m.O3]);new_o3=m.parse(self.payload[m.O3])
        for n in m.prior.TEMPLATES:self.assertEqual(new_o3['payload'][n],old_o3['payload'][n])

    def test_preflight_no_writes(self):
        self.assertEqual(self.update().transition('preflight')['state'],'EXACT_OLD_COMPLETED_26')
        self.assertFalse(self.path(m.TX).exists());self.unchanged()
        for n in m.CHANGES:self.assertEqual(self.path(n).read_bytes(),self.before[n])

    def test_apply_resume_rollback_idempotence(self):
        for action in ('apply','resume','apply'):
            result=self.update().transition(action)
            self.assertEqual(result['state'],'APPLIED')
            for flag in ('activation_performed','daemon_reload_performed','redis_writes','finam_contact'):
                self.assertFalse(result[flag])
            for n in m.CHANGES:self.assertEqual(self.path(n).read_bytes(),self.payload[n])
            self.unchanged()
        for action in ('rollback','rollback'):
            self.assertEqual(self.update().transition(action)['state'],'ROLLED_BACK');self.unchanged()
            for n in m.CHANGES:self.assertEqual(self.path(n).read_bytes(),self.before[n])
        with self.assertRaises(m.c.Error):self.update().transition('resume')

    def test_every_replacement_durable_boundary_resumes(self):
        atomic=m.c.atomic_write
        for n in m.CHANGES:
            with self.subTest(slot=n):
                self.fresh()
                def fail(root,name,*args):
                    atomic(root,name,*args)
                    if name==n:raise Crash(name)
                with patch.object(m.c,'atomic_write',side_effect=fail):
                    with self.assertRaises(Crash):self.update().transition('apply')
                self.unchanged();self.assertEqual(self.update().transition('resume')['state'],'APPLIED')
                self.unchanged();self.update().transition('rollback');self.unchanged()

    def test_every_rollback_durable_boundary_resumes(self):
        atomic=m.c.atomic_write
        for n in m.CHANGES:
            with self.subTest(slot=n):
                self.fresh();self.update().transition('apply')
                def fail(root,name,*args):
                    atomic(root,name,*args)
                    if name==n:raise Crash(name)
                with patch.object(m.c,'atomic_write',side_effect=fail):
                    with self.assertRaises(Crash):self.update().transition('rollback')
                with self.assertRaises(m.c.Error):self.update().transition('resume')
                self.assertEqual(self.update().transition('rollback')['state'],'ROLLED_BACK');self.unchanged()

    def directory_retry_witness(self,rollback):
        action='rollback' if rollback else 'apply'
        retry='rollback' if rollback else 'resume'
        interim='ROLLING_BACK' if rollback else 'PREPARED'
        terminal='ROLLED_BACK' if rollback else 'APPLIED'
        desired=self.before if rollback else self.payload
        if rollback:self.update().transition('apply')
        parent=self.path(m.ELFS[-1]).parent
        sync=m.c.sync_dir;journal=m.Update.write_journal;trace=[]
        def after_last_rename(path):
            # Real atomic_write has already replaced the last ELF. Fail before
            # its parent fsync, not after atomic_write's durable return.
            if path==parent and self.path(m.ELFS[-1]).read_bytes()==desired[m.ELFS[-1]]:
                trace.append('rename-visible-before-fsync')
                raise OSError(errno.EIO,'injected parent fsync failure')
            return sync(path)
        with patch.object(m.c,'sync_dir',side_effect=after_last_rename):
            with self.assertRaises(OSError):self.update().transition(action)
        self.assertEqual(trace,['rename-visible-before-fsync'])
        self.assertEqual(self.update().journal(),interim)
        for n in m.ELFS:self.assertEqual(self.path(n).read_bytes(),desired[n])
        self.unchanged()
        def record_journal(update,state):
            trace.append(('journal',state));return journal(update,state)
        def retry_sync(path):
            trace.append(('sync',path))
            if path==parent:raise OSError(errno.EIO,'injected final retry sync failure')
            return sync(path)
        trace.clear()
        with patch.object(m.c,'sync_dir',side_effect=retry_sync),patch.object(m.Update,'write_journal',new=record_journal):
            with self.assertRaises(OSError):self.update().transition(retry)
        self.assertNotIn(('journal',terminal),trace)
        self.assertEqual(self.update().journal(),interim);self.unchanged()
        # The failed final barrier ran after all desired bytes were written.
        # This retry must fsync despite skipping EVERY payload atomic_write.
        for n in m.CHANGES:self.assertEqual(self.path(n).read_bytes(),desired[n])
        trace.clear();atomic=m.c.atomic_write
        def no_payload_rewrite(root,name,*args):
            self.assertNotIn(name,m.CHANGES)
            return atomic(root,name,*args)
        def successful_sync(path):
            sync(path);trace.append(('sync',path))
        with patch.object(m.c,'sync_dir',side_effect=successful_sync),\
             patch.object(m.c,'atomic_write',side_effect=no_payload_rewrite),\
             patch.object(m.Update,'write_journal',new=record_journal):
            self.assertEqual(self.update().transition(retry)['state'],terminal)
        before_terminal=trace[:trace.index(('journal',terminal))]
        parents={self.path(n).parent for n in m.CHANGES}|{self.path(m.TX).parent}
        for directory in parents:self.assertIn(('sync',directory),before_terminal)
        self.assertEqual(self.update().journal(),terminal);self.unchanged()
        print('directory-retry-witness '+action+': visible-rename fault -> '+interim+
              '; final-sync fault -> no terminal; exact-bytes retry -> parents synced before '+terminal)

    def test_apply_last_elf_rename_fsync_retry(self):self.directory_retry_witness(False)

    def test_rollback_last_elf_rename_fsync_retry(self):self.directory_retry_witness(True)

    def test_incomplete_mkdir_parent_retry_syncs_before_prepared(self):
        parent=self.path(m.TX).parent;sync=m.c.sync_dir;journal=m.Update.write_journal;trace=[]
        def fail_parent(path):
            if path==parent:raise OSError(errno.EIO,'injected transaction-parent fsync failure')
            return sync(path)
        for action in ('apply','resume'):
            with patch.object(m.c,'sync_dir',side_effect=fail_parent):
                with self.assertRaises(OSError):self.update().transition(action)
            self.assertTrue(self.path(m.TX).is_dir())
            self.assertIsNone(self.update().journal())
            for n in m.CHANGES:self.assertEqual(self.path(n).read_bytes(),self.before[n])
        def successful_sync(path):sync(path);trace.append(('sync',path))
        def record_journal(update,state):trace.append(('journal',state));return journal(update,state)
        with patch.object(m.c,'sync_dir',side_effect=successful_sync),patch.object(m.Update,'write_journal',new=record_journal):
            self.assertEqual(self.update().transition('resume')['state'],'APPLIED')
        self.assertLess(trace.index(('sync',parent)),trace.index(('journal','PREPARED')))
        self.unchanged()

    def test_each_partial_backup_resumes_without_payload_change(self):
        atomic=m.c.atomic_write
        for i in range(len(m.CHANGES)):
            with self.subTest(backup=i):
                self.fresh()
                def fail(root,name,*args):
                    atomic(root,name,*args)
                    if name==m.TX+f'/before-{i}':raise Crash(name)
                with patch.object(m.c,'atomic_write',side_effect=fail):
                    with self.assertRaises(Crash):self.update().transition('apply')
                self.assertEqual(self.update().transition('preflight')['state'],'INCOMPLETE_PREPARATION')
                for n in m.CHANGES:self.assertEqual(self.path(n).read_bytes(),self.before[n])
                self.assertEqual(self.update().transition('resume')['state'],'APPLIED');self.unchanged()

    def test_empty_transaction_preparation_resumes(self):
        self.path(m.TX).mkdir(mode=0o700)
        self.assertEqual(self.update().transition('resume')['state'],'APPLIED');self.unchanged()

    def test_new_bytes_without_prepared_journal_rejected(self):
        self.path(m.TX).mkdir(mode=0o700)
        self.path(m.ELFS[0]).write_bytes(self.payload[m.ELFS[0]])
        with self.assertRaises(m.c.Error):self.update().transition('resume')
        self.assertFalse(self.path(m.TX+'/journal.json').exists())

    def test_wrong_mode_owner_symlink_hardlink_missing_foreign_rejected(self):
        for kind in ('mode','owner','symlink','hardlink','missing','foreign'):
            with self.subTest(kind=kind):
                self.fresh();p=self.path(m.ELFS[0])
                if kind=='mode':p.chmod(0o777)
                elif kind=='owner':os.chown(p,987,987)
                elif kind=='symlink':p.rename(p.with_name('original'));p.symlink_to('original')
                elif kind=='hardlink':os.link(p,p.with_name('extra-link'))
                elif kind=='missing':p.unlink()
                else:p.write_bytes(b'foreign')
                with self.assertRaises((m.c.Error,FileNotFoundError)):self.update().transition('apply')
                self.assertFalse(self.path(m.TX).exists())

    def test_retained_config_state_template_and_prior_transaction_drift_rejected(self):
        for n in (m.config.CONFIG,m.inv.prior.STATE+'/journal',m.prior.TEMPLATES[0],
                  m.prior.TX+'/journal.json',m.config.TX+'/journal.json',
                  m.inv.prior.CONFIG+'/credentials/lifecycle.key'):
            with self.subTest(path=n):
                self.fresh();self.path(n).write_bytes(b'changed')
                with self.assertRaises(m.c.Error):self.update().transition('apply')
                self.assertFalse(self.path(m.TX).exists())

    def test_history_drift_rejected(self):
        h=copy.deepcopy(self.baseline['observed']['history_head']);h['latest_sequence']=27
        self.path(m.c.CONTROL+'/authority/history-head.json').write_bytes(m.canonical(h))
        with self.assertRaises(m.c.Error):self.update().transition('apply')

    def test_active_host_rejected(self):
        with patch.object(m.inv.prior,'host_observation',return_value={'active':True}):
            with self.assertRaises(m.c.Error):self.update().transition('apply')

    def test_nonempty_redis_rejected_without_repair(self):
        with patch.object(m.c,'command',return_value='1\n') as command:
            with self.assertRaises(m.c.Error):self.update().transition('apply')
            command.assert_called_once_with(['redis-cli','-h','127.0.0.1','-p','6379','-n','15','DBSIZE'])
        self.assertFalse(self.path(m.TX).exists())

    def test_new_operational_inputs_rejected(self):
        self.path('/etc/moex-finam-p1-paper/o3').mkdir()
        with self.assertRaises(m.c.Error):self.update().transition('apply')

    def test_backup_or_journal_drift_fails_closed(self):
        for n in ('before-0','journal.json'):
            with self.subTest(name=n):
                self.fresh();self.update().transition('apply');self.path(m.TX+'/'+n).write_bytes(b'{}')
                with self.assertRaises(m.c.Error):self.update().transition('rollback')
                self.assertEqual(self.path(m.ELFS[0]).read_bytes(),self.payload[m.ELFS[0]])

    def test_rollback_foreign_file_never_overwrites(self):
        self.update().transition('apply');n=m.ELFS[0];self.path(n).write_bytes(b'foreign')
        with self.assertRaises(m.c.Error):self.update().transition('rollback')
        self.assertEqual(self.path(n).read_bytes(),b'foreign')

    def test_both_locks_exclusive(self):
        import fcntl
        for n in ('.execution.lock','.guardian.lock'):
            with self.path(m.c.CONTROL+'/'+n).open('r+b') as f:
                fcntl.flock(f.fileno(),fcntl.LOCK_EX|fcntl.LOCK_NB)
                with self.assertRaises(BlockingIOError):self.update().transition('apply')

    def test_support_record_and_foreign_binary_cannot_be_release(self):
        build=m.parse((self.package/'build.json').read_bytes());build['status']='SUPPORT_BUILT_NOT_RELEASE'
        with self.assertRaises(m.c.Error):m.derive(self.baseline,m.canonical(build),
            (self.package/'before-outer.json').read_bytes(),(self.package/'before-o3.json').read_bytes(),
            {n:self.payload[n] for n in m.ELFS})
        binaries={n:self.payload[n] for n in m.ELFS};binaries[m.ELFS[0]]+=b'x'
        with self.assertRaises(m.c.Error):m.derive(self.baseline,(self.package/'build.json').read_bytes(),
            (self.package/'before-outer.json').read_bytes(),(self.package/'before-o3.json').read_bytes(),binaries)

    def test_package_inventory_and_custody(self):
        with tempfile.TemporaryDirectory() as tmp:
            target=Path(tmp)/'package'
            for kind in ('extra','missing','symlink','hardlink','payload','spec'):
                with self.subTest(kind=kind):
                    if target.exists():shutil.rmtree(target)
                    shutil.copytree(self.package,target)
                    p=target/'spec.json'
                    if kind=='extra':(target/'foreign').write_bytes(b'x')
                    elif kind=='missing':p.unlink()
                    elif kind=='symlink':p.unlink();p.symlink_to(self.package/'spec.json')
                    elif kind=='hardlink':os.link(p,Path(tmp)/'spec-link')
                    elif kind=='payload':(target/'payload'/Path(m.ELFS[0]).name).write_bytes(b'x')
                    else:p.write_bytes(b'{}')
                    with self.assertRaises(m.c.Error):m.load(target)


if __name__=='__main__':unittest.main(verbosity=2)
