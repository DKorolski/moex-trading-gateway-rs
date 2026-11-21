#!/usr/bin/env python3
"""Disposable Linux/root filesystem tests; only systemd/Redis observations mocked.
No remote target, real secrets or Redis writes. Run in network-none container.
"""
import copy
import fcntl
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import stage8b_p1f_o3_config_binding as m


class Crash(Exception): pass


class ConfigBinding(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name); self.root.chmod(0o755)
        self.old = m.FIXTURE.read_bytes().rstrip(b'\n')
        self.write('/etc/passwd', b'root:x:0:0:root:/root:/bin/sh\nmoex-p1-paper:x:987:987::/nonexistent:/usr/sbin/nologin\n')
        self.write('/etc/group', b'root:x:0:\nmoex-p1-paper:x:987:\n')
        self.write(m.CONFIG, self.old, 0o440, 987)
        head = dict(state='COMPLETED', authority_generation=1, latest_sequence=26,
                    latest_event_sha256=m.inv.HEAD, active_manifest_sha256=None, deadline_utc=None)
        self.write(m.c.CONTROL+'/authority/history-head.json', m.canonical(head), 0o440, 987)
        self.write(m.c.CONTROL+'/authority/events/00000000000000000026.json', b'OLD_TERMINAL', 0o440, 987)
        self.write(m.inv.prior.STATE+'/stage7b-fixture/stage7b-recovery.seal', b'accepted seal', 0o600, 987, 987)
        self.write(m.inv.prior.STATE+'/.stage8b-p1-first-boot-receipt-v2.json', b'accepted receipt', 0o600, 987, 987)
        self.write(m.inv.prior.CONFIG+'/credentials/lifecycle.key', b'NOT_REAL_SECRET', 0o400)
        for n in ('.execution.lock', '.guardian.lock'): self.write(m.c.CONTROL+'/'+n, b'', 0o600)
        self.write(m.INSTALL_TX+'/journal.json', b'ACCEPTED_INSTALL_APPLIED', 0o600)
        self.write('/usr/local/libexec/moex/test-elf', b'accepted executable placeholder', 0o755)
        self.write(m.c.MANIFEST, b'accepted installed inventory')
        self.start_patch(m.inv.prior, 'host_observation', return_value={'p0_pid': 123, 'p1_stopped': True})
        self.start_patch(m.inv, 'TREES', (m.inv.prior.CONFIG, m.inv.prior.STATE, m.c.CONTROL))
        self.redis = self.start_patch(m.c, 'command', side_effect=self.command)
        installed = {}
        for n in ('/usr/local/libexec/moex/test-elf', m.c.MANIFEST):
            p=self.path(n);s=p.lstat();installed[n]=dict(sha256=m.sha(p.read_bytes()),size=s.st_size,
                uid=0,gid=0,mode=oct(s.st_mode&0o777),nlink=1)
        snapshot=dict(observed=m.inv.observe(self.root), installed=installed,
                      transaction_inventory=m.inv.inventory(self.path(m.INSTALL_TX)))
        self.baseline=m.canonical(snapshot)
        self.start_patch(m,'BASELINE_SHA',m.sha(self.baseline))
        self.data=m.expected_files(self.baseline)
        self.before=copy.deepcopy(snapshot)

    def start_patch(self, obj, name, *a, **kw):
        p=patch.object(obj,name,*a,**kw);v=p.start();self.addCleanup(p.stop);return v

    def command(self,args):
        self.assertEqual(args,['redis-cli','-h','127.0.0.1','-p','6379','-n','15','DBSIZE'])
        return '0\n'

    def path(self,n):return self.root/n.lstrip('/')

    def write(self,n,b,mode=0o644,gid=0,uid=0):
        p=self.path(n);p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(b);p.chmod(mode);os.chown(p,uid,gid)

    def update(self):return m.Update(self.root,self.data)

    def unchanged_except_config(self):
        got=m.inv.observe(self.root);want=copy.deepcopy(self.before['observed'])
        raw=self.path(m.CONFIG).read_bytes()
        want['trees'][m.inv.prior.CONFIG]['supervisor.json'].update(sha256=m.sha(raw),size=len(raw))
        self.assertEqual(got,want)
        self.assertEqual(m.inv.inventory(self.path(m.INSTALL_TX)),self.before['transaction_inventory'])

    def test_exact_single_field(self):
        a=m.parse(self.old);b=m.parse(self.data['supervisor.after.json'])
        self.assertEqual({k for k in a if a[k]!=b[k]},{'redis_deployment_manifest_sha256'})
        self.assertEqual(m.sha(m.independent_manifest(b)),m.NEW_PIN)

    def test_foreign_old_config_rejected(self):
        for change in ({'redis_url':'redis://127.0.0.1:6379/0'}, {'shutdown_grace_ms':1},
                       {'redis_deployment_manifest_sha256':m.NEW_PIN}):
            with self.subTest(change=change):
                with self.assertRaises(m.c.Error):m.corrected(m.canonical(m.parse(self.old)|change))

    def test_preflight_no_transaction(self):
        self.assertEqual(self.update().transition('preflight')['state'],'EXACT_OLD_CONFIG')
        self.assertFalse(self.path(m.TX).exists());self.unchanged_except_config()

    def test_apply_and_explicit_resume_idempotent(self):
        for action in ('apply','resume','apply','preflight'):
            self.assertEqual(self.update().transition(action)['state'],'APPLIED')
            self.unchanged_except_config()
        self.assertEqual(self.path(m.TX+'/supervisor.before.json').read_bytes(),self.old)

    def test_every_atomic_boundary_before_and_after_recovers(self):
        atomic=m.c.atomic_write
        for where in ('before','after'):
            for boundary in range(1,5):
                with self.subTest(where=where,boundary=boundary):
                    # Fresh fixture; run its registered cleanups explicitly.
                    self.doCleanups();self.setUp();count=0
                    def interrupted(*a):
                        nonlocal count
                        count+=1
                        if count==boundary and where=='before':raise Crash()
                        atomic(*a)
                        if count==boundary and where=='after':raise Crash()
                    with patch.object(m.c,'atomic_write',side_effect=interrupted):
                        with self.assertRaises(Crash):self.update().transition('apply')
                    self.assertEqual(self.update().transition('resume')['state'],'APPLIED')
                    self.unchanged_except_config()
                    print('PASS crash-'+where+'-atomic-'+str(boundary))

    def test_nonempty_db15_rejected_before_write(self):
        with patch.object(m.c,'command',return_value='1\n'):
            with self.assertRaises(m.c.Error):self.update().transition('apply')
        self.assertFalse(self.path(m.TX).exists())

    def test_either_existing_lock_blocks(self):
        for n in ('.execution.lock','.guardian.lock'):
            with self.path(m.c.CONTROL+'/'+n).open('rb') as lock:
                fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
                with self.assertRaises(BlockingIOError):self.update().transition('apply')
            self.assertFalse(self.path(m.TX).exists())

    def test_p0_drift_rejected(self):
        with patch.object(m.inv.prior,'host_observation',return_value={'p0_pid':124,'p1_stopped':True}):
            with self.assertRaises(m.c.Error):self.update().transition('apply')

    def test_history_or_root_or_receipt_or_key_drift_rejected(self):
        paths=[m.c.CONTROL+'/authority/events/00000000000000000026.json',
               m.inv.prior.STATE+'/stage7b-fixture/stage7b-recovery.seal',
               m.inv.prior.STATE+'/.stage8b-p1-first-boot-receipt-v2.json',
               m.inv.prior.CONFIG+'/credentials/lifecycle.key']
        for n in paths:
            with self.subTest(n=n):
                self.doCleanups();self.setUp();self.path(n).write_bytes(b'drift')
                with self.assertRaises(m.c.Error):self.update().transition('apply')
                self.assertFalse(self.path(m.TX).exists())

    def test_elf_or_install_transaction_drift_rejected(self):
        for n in ('/usr/local/libexec/moex/test-elf',m.INSTALL_TX+'/journal.json',m.c.MANIFEST):
            with self.subTest(n=n):
                self.doCleanups();self.setUp();self.path(n).write_bytes(b'drift')
                with self.assertRaises(m.c.Error):self.update().transition('apply')

    def test_operational_input_presence_rejected(self):
        self.path('/etc/moex-finam-p1-paper/o3').mkdir()
        with self.assertRaises(m.c.Error):self.update().transition('apply')

    def test_config_custody_rejected(self):
        self.path(m.CONFIG).chmod(0o444)
        with self.assertRaises(m.c.Error):self.update().transition('apply')

    def test_unrecorded_new_config_rejected(self):
        self.path(m.CONFIG).write_bytes(self.data['supervisor.after.json'])
        with self.assertRaises(m.c.Error):self.update().transition('apply')

    def test_corrupt_backup_journal_or_foreign_transaction_rejected(self):
        for n in ('supervisor.before.json','journal.json','foreign'):
            with self.subTest(n=n):
                self.doCleanups();self.setUp();self.update().transition('apply')
                self.write(m.TX+'/'+n,b'drift',0o600)
                with self.assertRaises((m.c.Error,ValueError)):self.update().transition('resume')

    def test_applied_config_reversion_rejected(self):
        self.update().transition('apply');self.path(m.CONFIG).write_bytes(self.old)
        with self.assertRaises(m.c.Error):self.update().transition('resume')

    def test_symlink_and_hardlink_rejected(self):
        self.update().transition('apply')
        p=self.path(m.TX+'/supervisor.before.json');p.unlink();p.symlink_to(self.path(m.CONFIG))
        with self.assertRaises(m.c.Error):self.update().transition('resume')
        p.unlink();self.write(m.TX+'/supervisor.before.json',self.old,0o600);os.link(p,self.root/'linked')
        with self.assertRaises(m.c.Error):self.update().transition('resume')

    def test_resume_without_prior_transaction_rejected(self):
        with self.assertRaises(m.c.Error):self.update().transition('resume')

    def test_rehashed_package_mutation_rejected(self):
        changed=copy.deepcopy(self.data)
        changed['supervisor.after.json']=m.canonical(m.parse(changed['supervisor.after.json'])|{'health_interval_ms':6000})
        with self.assertRaises(m.c.Error):m.Update(self.root,changed)


if __name__=='__main__':unittest.main(verbosity=2)
