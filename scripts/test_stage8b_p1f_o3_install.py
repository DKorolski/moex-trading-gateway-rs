#!/usr/bin/env python3
"""Real Linux filesystem/locks/custody tests; systemd observation is mocked.

Inputs: O3_INSTALL_PACKAGE, O3_OLD_ELFS (accepted prior build). No SSH/Redis.
"""
import copy
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import stage8b_p1f_o3_install as m


class Crash(Exception):
    pass


class Installation(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(); self.root=Path(self.tmp.name); self.root.chmod(0o755)
        self.spec, _, self.old, self.payload=m.load(Path(os.environ['O3_INSTALL_PACKAGE']))
        snapshot=m.parse((Path(os.environ['O3_INSTALL_PACKAGE'])/'snapshot.json').read_bytes())
        self.write('/etc/passwd',b'root:x:0:0:root:/root:/bin/sh\nmoex-p1-paper:x:987:987::/nonexistent:/usr/sbin/nologin\n')
        self.write('/etc/group',b'root:x:0:\nmoex-p1-paper:x:987:\n')
        for n,e in self.old['payload'].items():
            raw=(Path(os.environ['O3_OLD_ELFS'])/Path(n).name).read_bytes() if n in m.inv.prior.ELFS else snapshot['old_public_payload'][n].encode()
            self.write(n,raw,int(e['mode'],8),987 if e['group']=='service' else 0)
        self.write(m.c.MANIFEST,snapshot['old_public_payload'][m.c.MANIFEST].encode())
        self.write(m.c.CONTROL+'/authority/history-head.json',m.canonical(snapshot['expected']['history_head']),0o440,987)
        for name in ('.execution.lock','.guardian.lock'):
            self.write(m.c.CONTROL+'/'+name,b'',0o600)
        for n in (m.inv.prior.STATE,m.c.STAGING):
            self.path(n).mkdir(parents=True,exist_ok=True)
        self.write(m.inv.prior.STATE+'/journal',b'synthetic durable root retained',0o600,987,987)
        self.write(m.inv.prior.STATE+'/.stage8b-p1-first-boot-receipt-v2.json',b'fixture immutable receipt',0o600,987,987)
        self.write(m.inv.prior.CONFIG+'/credentials/lifecycle.key',b'local dummy NOT a usable key',0o400)
        self.host_patch=patch.object(m.inv.prior,'host_observation',return_value={'local_test_host':True})
        self.tree_patch=patch.object(m.inv,'TREES',(m.inv.prior.STATE,m.inv.prior.CONFIG,m.c.CONTROL,m.c.STAGING))
        self.host_patch.start(); self.tree_patch.start()
        self.expected=m.inv.observe(self.root)
        self.initial={n:self.path(n).read_bytes() if self.path(n).exists() else None for n in m.CHANGES}

    def tearDown(self):
        self.tree_patch.stop(); self.host_patch.stop(); self.tmp.cleanup()

    def path(self,n):return self.root/n.lstrip('/')

    def write(self,n,raw,mode=0o644,gid=0,uid=0):
        p=self.path(n);p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(raw)
        os.chown(p,uid,gid);p.chmod(mode)

    def update(self):return m.Update(self.root,self.payload,self.spec,self.expected,self.old)

    def unchanged(self):
        self.assertEqual(m.inv.observe(self.root),self.expected)

    def test_preflight_no_writes(self):
        self.assertEqual(self.update().transition('preflight')['state'],'EXACT_OLD_COMPLETED_26')
        self.assertFalse(self.path(m.TX).exists());self.unchanged()

    def test_apply_resume_rollback_idempotence(self):
        for action in ('apply','resume','apply'):
            self.assertEqual(self.update().transition(action)['state'],'APPLIED');self.unchanged()
        for action in ('rollback','rollback'):
            self.assertEqual(self.update().transition(action)['state'],'ROLLED_BACK');self.unchanged()
            for n,raw in self.initial.items():
                self.assertEqual(self.path(n).read_bytes() if self.path(n).exists() else None,raw)
        with self.assertRaises(m.c.Error):self.update().transition('resume')

    def test_every_replacement_crash_resumes(self):
        # Real atomic writes, failure after each fixed slot has reached disk.
        atomic=m.c.atomic_write
        for n in m.CHANGES:
            with self.subTest(slot=n):
                self.tearDown();self.setUp()
                def fail(root,name,*args):
                    atomic(root,name,*args)
                    if name==n:raise Crash(name)
                with patch.object(m.c,'atomic_write',side_effect=fail):
                    with self.assertRaises(Crash):self.update().transition('apply')
                self.assertEqual(self.update().transition('resume')['state'],'APPLIED')
                self.unchanged();self.update().transition('rollback');self.unchanged()

    def test_every_absent_slot_rollback_crash_resumes(self):
        unlink=Path.unlink
        for n in m.inv.ADDED:
            with self.subTest(slot=n):
                self.tearDown();self.setUp();self.update().transition('apply')
                def fail(p,*args,**kwargs):
                    unlink(p,*args,**kwargs)
                    if p==self.path(n):raise Crash(n)
                with patch.object(Path,'unlink',fail):
                    with self.assertRaises(Crash):self.update().transition('rollback')
                self.assertEqual(self.update().transition('rollback')['state'],'ROLLED_BACK');self.unchanged()

    def test_foreign_new_slot_rejected(self):
        self.write(m.inv.ADDED[0],b'foreign',0o755)
        with self.assertRaises(m.c.Error):self.update().transition('apply')
        self.assertFalse(self.path(m.TX).exists())

    def test_old_slot_missing_rejected(self):
        self.path(m.OPERATOR).unlink()
        with self.assertRaises(m.c.Error):self.update().transition('preflight')

    def test_wrong_mode_owner_symlink_hardlink_rejected(self):
        for kind in ('mode','owner','symlink','hardlink'):
            with self.subTest(kind=kind):
                self.tearDown();self.setUp();p=self.path(m.OPERATOR)
                if kind=='mode':p.chmod(0o777)
                elif kind=='owner':os.chown(p,987,987)
                elif kind=='symlink':p.rename(p.with_name('old'));p.symlink_to('old')
                else:os.link(p,p.with_name('extra-link'))
                with self.assertRaises(m.c.Error):self.update().transition('apply')

    def test_state_or_history_or_key_metadata_drift_rejected(self):
        for n in (m.inv.prior.STATE+'/journal',m.c.CONTROL+'/authority/history-head.json',
                  m.inv.prior.CONFIG+'/credentials/lifecycle.key'):
            with self.subTest(path=n):
                self.tearDown();self.setUp();self.path(n).write_bytes(b'changed')
                with self.assertRaises((m.c.Error,ValueError)):self.update().transition('apply')

    def test_active_host_rejected(self):
        with patch.object(m.inv.prior,'host_observation',return_value={'changed':True}):
            with self.assertRaises(m.c.Error):self.update().transition('apply')

    def test_new_operational_inputs_rejected(self):
        self.path('/etc/moex-finam-p1-paper/o3').mkdir()
        with self.assertRaises(m.c.Error):self.update().transition('apply')

    def test_backup_corruption_fails_closed(self):
        self.update().transition('apply');self.path(m.TX+'/before-0').write_bytes(b'corrupt')
        with self.assertRaises(m.c.Error):self.update().transition('rollback')
        self.assertEqual(self.path(m.OPERATOR).read_bytes(),self.payload[m.OPERATOR])

    def test_rollback_foreign_file_never_deleted(self):
        self.update().transition('apply');n=m.inv.ADDED[0];self.path(n).write_bytes(b'foreign')
        with self.assertRaises(m.c.Error):self.update().transition('rollback')
        self.assertEqual(self.path(n).read_bytes(),b'foreign')

    def test_both_locks_exclusive(self):
        import fcntl
        for n in ('.execution.lock','.guardian.lock'):
            with self.path(m.c.CONTROL+'/'+n).open('r+b') as f:
                fcntl.flock(f.fileno(),fcntl.LOCK_EX|fcntl.LOCK_NB)
                with self.assertRaises(BlockingIOError):self.update().transition('apply')

    def test_package_tamper_rejected(self):
        snapshot=m.parse((Path(os.environ['O3_INSTALL_PACKAGE'])/'snapshot.json').read_bytes())
        build=m.parse((Path(os.environ['O3_INSTALL_PACKAGE'])/'build.json').read_bytes())
        binaries={n:self.payload[n] for n in m.ELFS};binaries[m.OPERATOR]+=b'x'
        with self.assertRaises(m.c.Error):m.replacements(snapshot,build,binaries,{n:self.payload[n] for n in m.TEMPLATES})


if __name__=='__main__':unittest.main(verbosity=2)
