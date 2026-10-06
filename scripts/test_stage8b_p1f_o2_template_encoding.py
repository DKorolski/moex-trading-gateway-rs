#!/usr/bin/env python3
"""Regression against the exact packaged template, not an in-memory stand-in.

The optional emitted historical-input fixture is NOT an installation successor:
it retains the old FAILED/1/8 descriptor exclusively for offline tests.
"""
import argparse
import io
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

import stage8b_p1f_o2_v4_timestamp_install_package as m

ARCHIVE_SHA = '5dd40622580346c74e1172a1611a42d4cde8aa396b904db345771429d1edd897'
PREFIX = 'installation-v4-timestamp/'
ARCHIVE = m.ROOT / 'reports/handoff/moex-trading-project-836a297-o2-v4-timestamp-installation.zip'

def corrected_fixture(archive):
    raw = archive.read_bytes()
    if m.sha(raw) != ARCHIVE_SHA:
        raise ValueError('accepted installation archive mismatch')
    with zipfile.ZipFile(io.BytesIO(raw)) as z:
        files = {n[len(PREFIX):]: z.read(n) for n in z.namelist() if n.startswith(PREFIX)}
    with zipfile.ZipFile(io.BytesIO(files['prior-installation.zip'])) as z:
        prior = {'/' + n: z.read(n) for n in z.namelist()}
    with zipfile.ZipFile(io.BytesIO(files['binary-artifact.zip'])) as z:
        payload, spec = m.material(prior, z)
    original = {p: files['payload/' + Path(p).name] for p in m.update.CHANGES}
    for path, raw in payload.items():
        files['payload/' + Path(path).name] = raw
    files['spec.json'] = m.canonical(spec)
    return files, original, payload, spec

def emit(files, output):
    output.mkdir(parents=True, exist_ok=False)
    for name, raw in files.items():
        path = output / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(raw)

class Encoding(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.files, cls.original, cls.payload, cls.spec = corrected_fixture(ARCHIVE)

    def test_real_packaged_template_loses_only_final_lf(self):
        path = m.update.CHANGES[5]
        self.assertEqual(len(self.original[path]), 1752)
        self.assertEqual(len(self.payload[path]), 1751)
        self.assertEqual(self.original[path], self.payload[path] + b'\n')
        self.assertEqual(json.loads(self.original[path]), json.loads(self.payload[path]))

    def test_only_template_and_its_binding_manifest_change(self):
        changed = {p for p in self.payload if self.payload[p] != self.original[p]}
        self.assertEqual(changed, {m.update.CHANGES[5], m.update.custody.MANIFEST})

    def test_policy_history_and_inventory_encoding_remain_unchanged(self):
        for path in m.update.CHANGES[:5]:
            self.assertEqual(self.payload[path], self.original[path])
        self.assertTrue(m.canonical({'a': 1}).endswith(b'\n'))

    def test_exact_template_hash_and_size_are_rebound(self):
        path = m.update.CHANGES[5]
        raw = self.payload[path]
        installed = json.loads(self.payload[m.update.custody.MANIFEST])
        compat = json.loads(self.payload[m.update.custody.old.MANIFEST])
        self.assertEqual(installed['payload'][path]['sha256'], m.sha(raw))
        self.assertEqual(installed['payload'][path]['size'], len(raw))
        self.assertNotIn(path, compat['managed_payload_sha256'])
        self.assertEqual(self.payload[m.update.custody.old.MANIFEST], self.original[m.update.custody.old.MANIFEST])
        self.assertEqual(self.spec['updates'][path]['new_sha256'], m.sha(raw))

    def test_regenerated_package_passes_its_exact_checker(self):
        with tempfile.TemporaryDirectory() as t:
            output = Path(t) / 'fixture'
            emit(self.files, output)
            m.validate_prepared(output)

    def test_reintroduced_lf_rejected_even_with_rehashed_metadata(self):
        with tempfile.TemporaryDirectory() as t:
            output = Path(t) / 'fixture'
            emit(self.files, output)
            path = m.update.CHANGES[5]
            raw = self.payload[path] + b'\n'
            (output / 'payload/supervisor.template.json').write_bytes(raw)
            # A consistent local rehash is not authority to change the recipe.
            spec = json.loads(self.files['spec.json'])
            for manifest_path in (m.update.custody.old.MANIFEST, m.update.custody.MANIFEST):
                target = output / 'payload' / Path(manifest_path).name
                value = json.loads(target.read_bytes())
                if manifest_path == m.update.custody.old.MANIFEST:
                    self.assertNotIn(path, value['managed_payload_sha256'])
                else:
                    value['payload'][path].update(sha256=m.sha(raw), size=len(raw))
                target.write_bytes(m.canonical(value))
            for p, change in spec['updates'].items():
                b = (output / 'payload' / Path(p).name).read_bytes()
                change.update(new_sha256=m.sha(b), size=len(b))
            spec['new_manifest_sha256'] = m.sha((output / 'payload/installation-o2-v1.json').read_bytes())
            (output / 'spec.json').write_bytes(m.canonical(spec))
            with self.assertRaises(m.update.custody.Error):
                m.validate_prepared(output)

    def test_utf8_sorted_no_newline_and_nonfinite_rejection(self):
        self.assertEqual(m.supervisor_template_bytes({'z': 'я', 'a': 1}), '{"a":1,"z":"я"}'.encode())
        with self.assertRaises(ValueError):
            m.supervisor_template_bytes({'a': float('nan')})

if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--archive', type=Path, default=m.ROOT / 'reports/handoff/moex-trading-project-836a297-o2-v4-timestamp-installation.zip')
    p.add_argument('--emit', type=Path)
    a = p.parse_args()
    ARCHIVE = a.archive
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Encoding))
    if not result.wasSuccessful():
        raise SystemExit(1)
    if a.emit:
        emit(corrected_fixture(ARCHIVE)[0], a.emit)
        print('EMITTED offline historical-input fixture only; not an installation authorization')
