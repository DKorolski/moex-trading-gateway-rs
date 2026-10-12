"""Offline mutations of retained NEW native evidence. No SSH/keys/execution."""
import copy
import json
import os
from pathlib import Path
import unittest

import stage8b_p1f_o3_readmission_handoff as m


class NativeEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.native = Path(os.environ['O3_READMISSION_NATIVE'])
        work = Path(os.environ['O3_READMISSION_EVIDENCE'])
        cls.original = {m.P+'native/'+p.relative_to(cls.native).as_posix():p.read_bytes()
                        for p in cls.native.rglob('*') if p.is_file() and 'payload' not in p.relative_to(cls.native).parts}
        helper = Path(os.environ['O3_READMISSION_HELPER'])
        cls.original[m.P+'helper/helper-build.json'] = (helper/'helper-build.json').read_bytes()
        for sub in ('logs','evidence','payload'):
            for p in (helper/sub).rglob('*'):
                if p.is_file(): cls.original[m.P+'helper/'+p.relative_to(helper).as_posix()] = p.read_bytes()
        cls.build = json.loads((work/'native-build-r1/build.json').read_bytes())
        cls.spec = json.loads((work/'installation-prepared-r1/spec.json').read_bytes())
        cls.baseline = json.loads((work/'installation-prepared-r1/baseline.json').read_bytes())
        names = ['scripts/stage8b_p1f_o3_readmission_qualify.py','scripts/stage8b_p1f_o3_native_fixture.py']
        names += ['deploy/stage8b-p1f-o3/'+u+'.in' for u in m.qual.UNITS]
        cls.files = {n:(m.ROOT/n).read_bytes() for n in names}

    def setUp(self):
        self.data = dict(self.original)
        self.result = json.loads(self.data[m.P+'native/result.json'])

    def verify(self):
        self.data[m.P+'native/result.json'] = m.encode(self.result)
        return m.verify_native(self.data.__getitem__,self.files,self.build,self.baseline,self.spec)

    def replace_log_coherently(self, label, value):
        raw = m.encode(value)
        record = next(v for v in self.result['records'] if v['id'] == label)
        record['stdout_sha256'] = m.sha(raw)
        self.data[m.P+'native/logs/'+label+'.json'] = m.encode(record)
        self.data[m.P+'native/logs/'+label+'.stdout'] = raw

    def test_real_retained_positive(self):
        self.assertEqual(self.verify()['signer_exit'],70)

    def test_real_helper_only_build_positive(self):
        self.assertEqual(m.verify_helper(self.data.__getitem__)['production_artifact_count'],0)

    def test_corrected_helper_elf_tamper_rejected(self):
        self.data[m.P+'helper/payload/input-helper'] += b'changed'
        with self.assertRaisesRegex(ValueError,'helper ELF'): m.verify_helper(self.data.__getitem__)

    def test_old_source_cannot_inherit_pass(self):
        self.result['source_ref'] = m.r.REF
        with self.assertRaisesRegex(ValueError,'input binding'): self.verify()

    def test_old_helper_cannot_inherit_pass(self):
        self.result['corrected_helper_build_sha256'] = m.qual.SUPPORT_RECORD
        with self.assertRaisesRegex(ValueError,'corrected helper binding'): self.verify()

    def test_helper_record_change_rejected(self):
        self.data[m.P+'helper/helper-build.json'] += b'\n'
        with self.assertRaisesRegex(ValueError,'corrected helper binding'): self.verify()

    def test_failed_or_prepared_result_rejected(self):
        for value in ('FAILED_NOT_QUALIFIED','PREPARED_ONLY_NOT_QUALIFIED'):
            self.result['status'] = value
            with self.assertRaisesRegex(ValueError,'new native PASS'): self.verify()

    def test_operational_flag_rejected(self):
        self.result['operational_o3'] = True
        with self.assertRaisesRegex(ValueError,'native scope'): self.verify()

    def test_cleanup_incomplete_rejected(self):
        self.result['cleanup_notes'] = ['container removal failed']
        with self.assertRaisesRegex(ValueError,'cleanup incomplete'): self.verify()

    def test_different_elf_rejected(self):
        self.result['payload_sha256'][m.r.BINS[0]] = '0'*64
        with self.assertRaisesRegex(ValueError,'payload binding'): self.verify()

    def test_driver_bytes_rejected(self):
        self.data[m.P+'native/driver.py'] += b'\n# changed\n'
        with self.assertRaisesRegex(ValueError,'executed support source'): self.verify()

    def test_missing_process_proof_rejected(self):
        self.result['records'] = [v for v in self.result['records'] if v['id'] != 'all-recorded-identities-gone']
        with self.assertRaisesRegex(ValueError,'missing native command'): self.verify()

    def test_altered_log_rejected(self):
        self.data[m.P+'native/logs/all-units-stopped.stdout'] = b'{}\n'
        with self.assertRaisesRegex(ValueError,'command log'): self.verify()

    def test_private_digest_rejected(self):
        row = next(v for v in self.result['records'] if v['id'] == 'selected-key-tmpfs')
        row['stdin_sha256'] = '0'*64
        self.data[m.P+'native/logs/selected-key-tmpfs.json'] = m.encode(row)
        with self.assertRaisesRegex(ValueError,'private digest'): self.verify()

    def test_postflight_host_drift_rejected(self):
        value = json.loads(self.data[m.P+'native/logs/protected-after.stdout'])
        value['redis_db15_size'] = 1
        self.replace_log_coherently('protected-after',value)
        with self.assertRaisesRegex(ValueError,'preflight'): self.verify()

    def test_exit_255_is_not_70(self):
        value = json.loads(self.data[m.P+'native/logs/all-units-stopped.stdout'])
        value['signer']['ExecMainStatus'] = '255'
        self.replace_log_coherently('all-units-stopped',value)
        with self.assertRaisesRegex(ValueError,'terminal statuses'): self.verify()

    def test_child_uid_drift_rejected(self):
        value = json.loads(self.data[m.P+'native/logs/active-stable.stdout'])
        unit = value['supervisor']
        child = next(p for pid,p in unit['processes'].items() if pid != unit['MainPID'])
        child['credentials'] = ['Uid:\t0\t0\t0\t0' if s.startswith('Uid:') else s for s in child['credentials']]
        self.replace_log_coherently('active-stable',value)
        with self.assertRaisesRegex(ValueError,'identity/NNP'): self.verify()

    def test_replaced_process_rejected(self):
        value = json.loads(self.data[m.P+'native/logs/active-stable.stdout'])
        next(iter(value['supervisor']['processes'].values()))['start_ticks'] = '-1'
        self.replace_log_coherently('active-stable',value)
        with self.assertRaisesRegex(ValueError,'process replaced'): self.verify()

    def test_surviving_process_rejected(self):
        self.replace_log_coherently('all-recorded-identities-gone',{'123':'S'})
        with self.assertRaisesRegex(ValueError,'process survived'): self.verify()

    def test_missing_credential_deletion_rejected(self):
        self.result['selected_key_removed'] = False
        with self.assertRaisesRegex(ValueError,'native flag'): self.verify()


class InstallationContinuity(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        work = Path(os.environ['O3_READMISSION_EVIDENCE'])/'installation-prepared-r1'
        candidate = Path(os.environ['O3_READMISSION_PACKAGE'])
        cls.original = {}
        for label,directory in (('qualified-installation',work),('installation',candidate)):
            for name in ('spec.json','baseline.json','payload/'+Path(m.install.O3).name,
                         'payload/'+Path(m.install.c.MANIFEST).name):
                cls.original[m.P+label+'/'+name] = (directory/name).read_bytes()
        cls.original[m.P+'qualified-installation/installer.py'] = m.previous.git(
            'show',m.QUALIFIED_REF+':scripts/stage8b_p1f_o3_readmission_install.py')
        cls.original[m.P+'native/result.json'] = (Path(os.environ['O3_READMISSION_NATIVE'])/'result.json').read_bytes()
        cls.files = {'scripts/stage8b_p1f_o3_readmission_install.py':
                     (m.ROOT/'scripts/stage8b_p1f_o3_readmission_install.py').read_bytes()}

    def setUp(self):
        self.data = dict(self.original)
        self.baseline = json.loads(self.data[m.P+'installation/baseline.json'])
        self.spec = json.loads(self.data[m.P+'installation/spec.json'])

    def verify(self):
        return m.verify_installation_continuity(self.data.__getitem__,self.files,self.baseline,self.spec)

    def test_installer_only_successor_positive(self):
        old,bridge = self.verify()
        self.assertNotEqual(old['installer_sha256'],self.spec['installer_sha256'])
        self.assertNotEqual(bridge['qualified_spec_sha256'],bridge['corrected_spec_sha256'])
        self.assertFalse(bridge['native_rerun'])

    def test_cannot_relabel_native_result(self):
        key = m.P+'native/result.json'; row = json.loads(self.data[key])
        row['installation_spec_sha256'] = m.sha(m.install.canonical(self.spec)); self.data[key] = m.encode(row)
        with self.assertRaisesRegex(ValueError,'original native result bytes'): self.verify()

    def test_qualified_spec_tamper_rejected(self):
        self.data[m.P+'qualified-installation/spec.json'] += b'\n'
        with self.assertRaisesRegex(ValueError,'original qualified spec'): self.verify()

    def test_qualified_installer_tamper_rejected(self):
        self.data[m.P+'qualified-installation/installer.py'] += b'\n'
        with self.assertRaisesRegex(ValueError,'original qualified installer'): self.verify()

    def test_baseline_tamper_rejected(self):
        self.baseline['changed'] = True
        with self.assertRaisesRegex(ValueError,'unchanged qualified baseline'): self.verify()

    def test_each_elf_and_other_spec_changes_rejected(self):
        original = copy.deepcopy(self.spec)
        for n in m.install.ELFS:
            self.spec = copy.deepcopy(original); self.spec['updates'][n]['sha256'] = '0'*64
            with self.subTest(n=n),self.assertRaisesRegex(ValueError,'unchanged five ELF'): self.verify()
        self.spec = copy.deepcopy(original); self.spec['activation_authorized'] = True
        with self.assertRaisesRegex(ValueError,'installer-only spec'): self.verify()

    def test_each_manifest_other_changes_rejected(self):
        for n in (m.install.O3,m.install.c.MANIFEST):
            with self.subTest(n=n):
                self.data = dict(self.original); key = m.P+'installation/payload/'+Path(n).name
                row = json.loads(self.data[key]); row['source_ref'] = '0'*40; self.data[key] = m.install.canonical(row)
                with self.assertRaisesRegex(ValueError,'installer-only manifest'): self.verify()

    def test_original_manifest_tamper_rejected(self):
        key = m.P+'qualified-installation/payload/'+Path(m.install.O3).name
        self.data[key] += b'\n'
        with self.assertRaisesRegex(ValueError,'original qualified manifest'): self.verify()


if __name__ == '__main__': unittest.main(verbosity=2)
