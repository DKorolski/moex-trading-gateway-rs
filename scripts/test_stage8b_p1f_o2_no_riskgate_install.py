#!/usr/bin/env python3
"""Linux/root fixture tests; systemd host observations deliberately mocked."""
import copy
import fcntl
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import stage8b_p1f_o2_no_riskgate_install as m
import stage8b_p1f_o2_no_riskgate_install_package as package

OBS = {'p1_stopped': True, 'p0': 'fixture-unchanged', 'host_key': m.custody.HOST_KEY}
INPUT = Path('/package')
SPEC = INPUT / 'spec.json'
PAYLOAD = INPUT / 'payload'

class SimulatedCrash(Exception):
    pass

class UpdateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.root.chmod(0o700)
        self.spec = json.loads(SPEC.read_text())
        prior, retained = package.load_predecessor(INPUT/'retained-terminal.zip', INPUT/'prior-installation.zip')
        for name, raw in prior.items():
            e = self.spec['old_inventory']['payload'].get(name, {'mode': '0644', 'group': 'root'})
            mode, group = int(e['mode'], 8), e['group']
            p = self.root / name.lstrip('/')
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(raw)
            p.chmod(mode)
            os.chown(p, 0, 987 if group == 'service' else 0)
        for name in ('etc/passwd', 'etc/group'):
            p = self.root / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text('root:x:0:0:root:/root:/bin/bash\nmoex-p1-paper:x:987:987::/:/usr/sbin/nologin\n' if name.endswith('passwd') else 'root:x:0:\nmoex-p1-paper:x:987:\n')
        for name in (m.custody.CONTROL, '/etc/moex-finam-p1-paper',
                     '/var/lib/moex-finam-p1-paper/state', m.custody.STAGING):
            p = self.root / name.lstrip('/')
            p.mkdir(parents=True, exist_ok=True)
            p.chmod(0o750)
        control = self.root / m.custody.CONTROL.lstrip('/')
        (control / 'authority').mkdir()
        for name in ('history-head.json', 'events/0.json', 'events/1.json', 'events/2.json', 'events/3.json', 'events/4.json',
                     'manifests/old/terminal-receipt-2.json', 'manifests/new/terminal-receipt-4.json', 'genesis-manifest.json'):
            p = control / 'authority' / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(m.custody.canonical(retained['history_head']) if name == 'history-head.json' else b'fixture-retained-history')
            p.chmod(0o440)
            os.chown(p, 0, 987)
        for name in ('.execution.lock', '.guardian.lock'):
            (control / name).write_bytes(b'')
            (control / name).chmod(0o600)
        self.expected = {'observation': OBS, 'history_head': retained['history_head']}
        for name, key in ((m.custody.CONTROL, 'authority_inventory'),
                          ('/etc/moex-finam-p1-paper', 'config_inventory'),
                          ('/var/lib/moex-finam-p1-paper/state', 'durable_inventory'),
                          (m.custody.STAGING, 'staging_inventory')):
            self.expected[key] = m.inventory(self.root / name.lstrip('/'))
        self.payload = {name: (PAYLOAD / Path(name).name).read_bytes() for name in m.CHANGES}
        observer = patch.object(m.custody, 'observe', return_value=copy.deepcopy(OBS))
        self.observe = observer.start()
        self.addCleanup(observer.stop)
        self.u = m.Update(self.root, self.payload, self.spec, self.expected)

    def digest_all(self):
        return {name: m.custody.sha(self.u.read(name)) for name in self.spec['old_inventory']['payload']} | {m.custody.MANIFEST: m.custody.sha(self.u.read(m.custody.MANIFEST))}

    def test_apply_resume_rollback_preserve_all_history(self):
        before = self.digest_all()
        self.assertEqual(self.u.transition('preflight')['state'], 'EXACT_OLD_TERMINAL')
        self.assertEqual(self.u.transition('apply')['state'], 'APPLIED')
        self.assertEqual(self.u.transition('resume')['state'], 'APPLIED')
        self.u.invariants()
        self.assertEqual(self.u.transition('rollback')['state'], 'ROLLED_BACK')
        self.assertEqual(self.u.transition('rollback')['state'], 'ROLLED_BACK')
        self.assertEqual(before, self.digest_all())
        self.u.invariants()
        with self.assertRaises(m.custody.Error):
            self.u.transition('resume')

    def test_each_durable_payload_frontier_resumes_or_rolls_back(self):
        # Fresh fixture for each injected post-fsync slot; reopen the updater.
        original = m.custody.atomic_write
        for action in ('resume', 'rollback'):
            for frontier in m.CHANGES:
                with self.subTest(action=action, frontier=frontier):
                    self.setUp()
                    before = self.digest_all()
                    def fault(root, name, *args):
                        original(root, name, *args)
                        if name == frontier:
                            raise SimulatedCrash(name)
                    with patch.object(m.custody, 'atomic_write', side_effect=fault):
                        with self.assertRaises(SimulatedCrash):
                            self.u.transition('apply')
                    reopened = m.Update(self.root, self.payload, self.spec, self.expected)
                    self.assertEqual(reopened.transition(action)['state'], 'APPLIED' if action == 'resume' else 'ROLLED_BACK')
                    reopened.invariants()
                    if action == 'rollback':
                        self.assertEqual(before, self.digest_all())
        print('PASS no-riskgate-install durable-payload-frontiers 16/16')

    def test_history_advance_blocks_update_and_rollback(self):
        for applied in (False, True):
            with self.subTest(applied=applied):
                self.setUp()
                if applied:
                    self.u.transition('apply')
                before = self.digest_all()
                self.u.path(m.custody.CONTROL + '/authority/history-head.json').write_text('new claim')
                with self.assertRaises(m.custody.Error):
                    self.u.transition('rollback' if applied else 'apply')
                self.assertEqual(before, self.digest_all())

    def test_unknown_pending_and_credentials_block(self):
        for name in (m.custody.CONTROL + '/authority/pending-terminal.json',
                     '/etc/moex-finam-p1-paper/supervisor.json', '/run/moex-finam-p1f-o2-input'):
            with self.subTest(name=name):
                self.setUp()
                p = self.u.path(name)
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_bytes(b'foreign')
                with self.assertRaises(m.custody.Error):
                    self.u.transition('apply')
                self.assertFalse(self.u.path(m.TRANSACTION).exists())

    def test_stopped_or_p0_drift_blocks(self):
        self.observe.return_value = {'p1_stopped': False}
        with self.assertRaises(m.custody.Error):
            self.u.transition('apply')
        self.assertFalse(self.u.path(m.TRANSACTION).exists())

    def test_existing_execution_lock_excludes_update(self):
        with self.u.path(m.custody.CONTROL + '/.execution.lock').open('rb') as stream:
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaises(BlockingIOError):
                self.u.transition('apply')
        self.assertFalse(self.u.path(m.TRANSACTION).exists())

    def test_foreign_installed_bytes_or_modes_block(self):
        for mode_only in (False, True):
            with self.subTest(mode_only=mode_only):
                self.setUp()
                p = self.u.path(m.CHANGES[0])
                p.chmod(0o777) if mode_only else p.write_bytes(b'foreign')
                with self.assertRaises(m.custody.Error):
                    self.u.transition('apply')
                self.assertFalse(self.u.path(m.TRANSACTION).exists())

    def test_corrupt_backup_or_journal_refused(self):
        for filename in ('before-0', 'journal.json'):
            with self.subTest(filename=filename):
                self.setUp()
                self.u.transition('apply')
                before = self.digest_all()
                self.u.path(m.TRANSACTION + '/' + filename).write_bytes(b'{}')
                with self.assertRaises(m.custody.Error):
                    self.u.transition('rollback')
                self.assertEqual(before, self.digest_all())

    def test_symlink_hardlink_and_transaction_custody_refused(self):
        self.u.transition('apply')
        directory = self.u.path(m.TRANSACTION)
        directory.chmod(0o755)
        with self.assertRaises(m.custody.Error):
            self.u.transition('rollback')
        directory.chmod(0o700)
        p = self.u.path(m.CHANGES[0])
        os.link(p, self.root / 'extra-link')
        with self.assertRaises(m.custody.Error):
            self.u.transition('rollback')

    def test_payload_tamper_and_write_surface_refused(self):
        bad = dict(self.payload)
        bad[m.CHANGES[0]] = b'bad'
        with self.assertRaises(m.custody.Error):
            m.Update(self.root, bad, self.spec, self.expected)
        bad = dict(self.payload)
        bad['/etc/shadow'] = b'forbidden'
        with self.assertRaises(m.custody.Error):
            m.Update(self.root, bad, self.spec, self.expected)

    def test_incomplete_preparation_and_symlink_refuse_without_installed_write(self):
        before = self.digest_all()
        directory = self.u.path(m.TRANSACTION)
        directory.mkdir(mode=0o700)
        with self.assertRaises(m.custody.Error):
            self.u.transition('resume')
        self.assertEqual(before, self.digest_all())
        self.setUp()
        p = self.u.path(m.CHANGES[0])
        saved = self.root / 'saved-binary'
        p.rename(saved)
        p.symlink_to(saved)
        with self.assertRaises(m.custody.Error):
            self.u.transition('apply')
        self.assertFalse(self.u.path(m.TRANSACTION).exists())

    def test_sequence_two_and_numeric_smuggling_refused(self):
        for field, value in [('latest_sequence', 2), ('authority_generation', True), ('state', 'ACTIVE')]:
            bad = copy.deepcopy(self.expected)
            bad['history_head'][field] = value
            with self.assertRaises(m.custody.Error):
                m.Update(self.root, self.payload, self.spec, bad)

    def test_all_eight_slots_are_changed_and_service_config_custody_retained(self):
        self.assertEqual(len(m.CHANGES), 8)
        for path in m.CHANGES:
            self.assertNotEqual(self.spec['updates'][path]['old_sha256'], self.spec['updates'][path]['new_sha256'])
        self.u.transition('apply')
        for path in m.CHANGES[3:6]:
            st = self.u.path(path).stat()
            self.assertEqual(st.st_uid, 0)
            self.assertEqual(st.st_gid, 987)
            self.assertEqual(st.st_mode & 0o777, 0o440)

    def test_selector_pending_and_old_receipt_mutation_refuse(self):
        for path in ('/etc/moex-finam-p1-paper/o2/active-manifest.sha256',
                     m.custody.CONTROL + '/authority/manifests/old/terminal-receipt-2.json'):
            self.setUp()
            self.u.transition('apply')
            before = self.digest_all()
            self.u.path(path).parent.mkdir(parents=True, exist_ok=True)
            self.u.path(path).write_bytes(b'changed')
            with self.assertRaises(m.custody.Error):
                self.u.transition('rollback')
            self.assertEqual(before, self.digest_all())

if __name__ == '__main__':
    unittest.main(verbosity=2)
