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

    def test_old_source_cannot_inherit_pass(self):
        self.result['source_ref'] = m.r.REF
        with self.assertRaisesRegex(ValueError,'input binding'): self.verify()

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


if __name__ == '__main__': unittest.main(verbosity=2)
