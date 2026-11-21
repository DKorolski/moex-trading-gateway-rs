#!/usr/bin/env python3
"""Disposable Linux/root filesystem tests; only systemd/proc observations mocked."""
import copy
import fcntl
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import stage8b_p1f_o2_replay_install as m

PACKAGE = Path(os.environ.get('MOEX_REPLAY_PACKAGE', '/package'))
OLD_ELF = Path('/old-elf')


class InputTests(unittest.TestCase):
    def test_exact_prepared(self):
        spec, _, old, payload = m.validate_prepared(PACKAGE)
        self.assertEqual(len(payload), 5)
        self.assertEqual(spec['expected_terminal'], 'FAILED/1/23')
        self.assertEqual(json.loads(payload[m.c.MANIFEST])['installation_id'], old['installation_id'])
        self.assertFalse(spec['execution_authorized'])

    def test_negative_inputs(self):
        count = 0
        cases = [('spec.json', 'compiled_source_ref', '0'*40),
                 ('spec.json', 'accepted_artifact_sha256', '0'*64),
                 ('spec.json', 'expected_terminal', 'EXPIRED/1/12'),
                 ('spec.json', 'installation_authorized', True),
                 ('spec.json', 'execution_authorized', True),
                 ('spec.json', 'old_manifest_sha256', '0'*64),
                 ('spec.json', 'new_manifest_sha256', '0'*64),
                 ('spec.json', 'installer_sha256', '0'*64),
                 ('spec.json', 'before_sha256', '0'*64)]
        for name, key, value in cases:
            with self.subTest(key=key), tempfile.TemporaryDirectory() as t:
                p = Path(t)/'package';shutil.copytree(PACKAGE,p)
                data=json.loads((p/name).read_bytes());data[key]=value;(p/name).write_bytes(m.canonical(data))
                with self.assertRaises(m.c.Error):m.validate_prepared(p)
                count += 1
        for name in ('before.json', 'old/installation-o2-v1.json', 'old/installation-v1.json',
                     *['payload/'+Path(p).name for p in m.CHANGES],
                     'old-tree/etc/moex-finam-p1-paper/o2/source-template.json'):
            with self.subTest(file=name), tempfile.TemporaryDirectory() as t:
                p=Path(t)/'package';shutil.copytree(PACKAGE,p)
                (p/name).write_bytes((p/name).read_bytes()+b' ')
                with self.assertRaises(m.c.Error):m.validate_prepared(p)
                count+=1
        for key, value in [('latest_sequence',12),('latest_sequence',23.0),('authority_generation',True),('state','ACTIVE')]:
            head=json.loads((PACKAGE/'before.json').read_bytes())['history_head'];head[key]=value
            with self.assertRaises(m.c.Error):m.terminal(head)
            count+=1
        with tempfile.TemporaryDirectory() as t:
            p=Path(t)/'package';shutil.copytree(PACKAGE,p)
            data=json.loads((p/'spec.json').read_bytes());data['calendar']['session_date']='2026-10-09'
            (p/'spec.json').write_bytes(m.canonical(data))
            with self.assertRaises(m.c.Error):m.validate_prepared(p)
            count+=1
        print(f'PASS prepared negatives {count}/{count}')


class Crash(Exception):pass


@unittest.skipUnless(os.environ.get('MOEX_REPLAY_DISPOSABLE_TEST')=='1' and os.geteuid()==0, 'disposable Linux root only')
class FilesystemTests(unittest.TestCase):
    def make(self):
        temp=tempfile.TemporaryDirectory(dir='/root');self.addCleanup(temp.cleanup)
        root=Path(temp.name);root.chmod(0o700)
        old=json.loads((PACKAGE/'old/installation-o2-v1.json').read_bytes())
        for p, e in old['payload'].items():
            raw=(OLD_ELF/Path(p).name).read_bytes() if p in m.ELFS else (PACKAGE/'old-tree'/p.lstrip('/')).read_bytes()
            self.assertEqual(m.sha(raw),e['sha256'])
            q=root/p.lstrip('/');q.parent.mkdir(parents=True,exist_ok=True);q.write_bytes(raw)
            q.chmod(int(e['mode'],8));os.chown(q,0,987 if e['group']=='service' else 0)
        q=root/m.c.MANIFEST.lstrip('/');q.write_bytes((PACKAGE/'old/installation-o2-v1.json').read_bytes());q.chmod(0o644)
        for name,data in [('passwd','root:x:0:0::/root:/bin/bash\nmoex-p1-paper:x:987:987::/:/usr/sbin/nologin\n'),('group','root:x:0:\nmoex-p1-paper:x:987:\n')]:
            (root/'etc'/name).write_text(data)
        for p in m.TREES:(root/p.lstrip('/')).mkdir(parents=True,exist_ok=True)
        control=root/m.c.CONTROL.lstrip('/')
        for name in ('.execution.lock','.guardian.lock'):
            q=control/name;q.write_bytes(b'');q.chmod(0o600)
        for name in ('authority/history-head.json','authority/events/000023.json','authority/manifests/latest/terminal-receipt-19.json', 'authority/manifests/old/archive.json'):
            q=control/name;q.parent.mkdir(parents=True,exist_ok=True)
            q.write_bytes(m.canonical(json.loads((PACKAGE/'before.json').read_bytes())['history_head']) if name.endswith('history-head.json') else b'fixture-retained')
            q.chmod(0o440);os.chown(q,0,987)
        for name,raw in [(m.c.old.OPERATOR_FILES[0],b'fixture-config'),(m.c.old.OPERATOR_FILES[1],b'fixture-source'),(m.c.old.OPERATOR_FILES[2],b'x'*32)]:
            q=root/name.lstrip('/');q.parent.mkdir(parents=True,exist_ok=True);q.write_bytes(raw);q.chmod(0o400)
        (root/m.STATE.lstrip('/')/'.stage8b-p1-first-boot-quarantine').mkdir()
        (root/m.c.STAGING.lstrip('/')/'failed23.json').write_bytes(b'retained')
        for p in m.RETAINED:(root/p.lstrip('/')/'witness').write_bytes(b'retained')
        u=m.Update(root,PACKAGE)
        u.expected=m.observe(root)  # Test-only fixture inventories, not native claims.
        return u

    def setUp(self):
        mock=patch.object(m,'host_observation',return_value={'fixture_systemd':'stopped','p0':'unchanged'})
        self.host=mock.start();self.addCleanup(mock.stop)
        self.u=self.make()

    def test_apply_resume_rollback_and_retained_inputs(self):
        before=copy.deepcopy(self.u.expected)
        self.assertEqual(self.u.transition('preflight')['state'],'EXACT_OLD_FAILED_23')
        self.assertEqual(self.u.transition('apply')['state'],'APPLIED')
        self.assertEqual(self.u.transition('resume')['state'],'APPLIED')
        self.assertEqual(self.u.transition('rollback')['state'],'ROLLED_BACK')
        self.u.verify_files('old');self.assertEqual(m.observe(self.u.root),before)
        with self.assertRaises(m.c.Error):self.u.transition('resume')

    def test_each_slot_crash_resume_or_rollback(self):
        original=m.c.atomic_write
        count=0
        for action in ('resume','rollback'):
            for target in m.CHANGES:
                with self.subTest(action=action,target=target):
                    u=self.make()
                    def fault(root,name,*args):
                        original(root,name,*args)
                        if name==target:raise Crash(name)
                    with patch.object(m.c,'atomic_write',side_effect=fault):
                        with self.assertRaises(Crash):u.transition('apply')
                    restored=m.Update(u.root,PACKAGE);restored.expected=u.expected
                    self.assertEqual(restored.transition(action)['state'],'APPLIED' if action=='resume' else 'ROLLED_BACK')
                    restored.verify_files('new' if action=='resume' else 'old');restored.invariants();count+=1
        print(f'PASS durable slot frontiers {count}/{count}')

    def test_history_or_p0_drift_refuses_before_write(self):
        self.host.return_value={'drift':True}
        with self.assertRaises(m.c.Error):self.u.transition('apply')
        self.assertFalse(self.u.path(m.TX).exists())
        self.host.return_value={'fixture_systemd':'stopped','p0':'unchanged'}
        p=self.u.path(m.c.CONTROL+'/authority/events/000023.json');p.write_bytes(b'corrupt')
        with self.assertRaises(m.c.Error):self.u.transition('apply')
        self.assertFalse(self.u.path(m.TX).exists())

    def test_history_advance_refuses_rollback(self):
        self.u.transition('apply')
        p=self.u.path(m.c.CONTROL+'/authority/history-head.json');v=json.loads(p.read_bytes());v['latest_sequence']=24;p.write_bytes(m.canonical(v))
        with self.assertRaises(m.c.Error):self.u.transition('rollback')
        self.u.verify_files('new')

    def test_foreign_payload_modes_or_hardlinks_refuse(self):
        for kind in ('bytes','mode','hardlink','symlink'):
            u=self.make();p=u.path(m.CHANGES[0])
            if kind=='bytes':p.write_bytes(b'foreign')
            elif kind=='mode':p.chmod(0o777)
            elif kind=='hardlink':os.link(p,u.root/'alias')
            else:p.rename(u.root/'saved');p.symlink_to(u.root/'saved')
            with self.subTest(kind=kind), self.assertRaises(m.c.Error):u.transition('apply')
            self.assertFalse(u.path(m.TX).exists())

    def test_generated_config_and_secret_identity_preserved(self):
        for p in m.c.old.OPERATOR_FILES:
            u=self.make();u.path(p).write_bytes(b'changed')
            with self.assertRaises(m.c.Error):u.transition('apply')
            self.assertFalse(u.path(m.TX).exists())

    def test_transient_input_refuses(self):
        p=self.u.path(m.ABSENT[0]);p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(b'foreign')
        with self.assertRaises(m.c.Error):self.u.transition('apply')

    def test_locks_refuse(self):
        with self.u.path(m.c.CONTROL+'/.execution.lock').open('rb') as f:
            fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)
            with self.assertRaises(BlockingIOError):self.u.transition('apply')
        self.assertFalse(self.u.path(m.TX).exists())

    def test_corrupt_backup_or_journal_refuses(self):
        for name in ('before-0','journal.json'):
            u=self.make();u.transition('apply');u.path(m.TX+'/'+name).write_bytes(b'corrupt')
            with self.assertRaises((m.c.Error,ValueError)):u.transition('rollback')
            u.verify_files('new')

    def test_incomplete_backup_frontier_does_not_guess(self):
        self.u.path(m.TX).mkdir(mode=0o700)
        with self.assertRaises(m.c.Error):self.u.transition('resume')
        self.u.verify_files('old')


if __name__=='__main__':unittest.main()
