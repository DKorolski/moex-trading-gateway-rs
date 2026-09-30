#!/usr/bin/env python3
"""Bounded archive negatives for the full terminal-successor artifact."""
import json
from pathlib import Path
import sys
import tempfile
import warnings
import zipfile

import stage8b_p1f_o2_successor_artifact as m

def run(path):
    m.check(path)
    with zipfile.ZipFile(path) as z:
        infos = z.infolist()
        original = {i.filename: z.read(i) for i in infos}
    cases = ['binary-bytes', 'build-commit', 'authority-opened', 'lock-negative-removed',
             'source-unit', 'duplicate-member', 'unsafe-member', 'symlink-member']
    for case in cases:
        files = dict(original)
        if case == 'binary-bytes':
            files['artifact-evidence/payload/stage8b-p1f-o2-materializer'] += b'tamper'
        elif case == 'build-commit':
            files['artifact-evidence/source-commit.raw'] += b'tamper'
        elif case in ('authority-opened', 'lock-negative-removed'):
            d = json.loads(files['artifact-evidence/descriptor.json'])
            if case == 'authority-opened':
                d['execution_authorized'] = True
            else:
                name = 'artifact-evidence/lock-proof/result.json'
                p = json.loads(files[name]); p['cases'].pop()
                files[name] = json.dumps(p).encode()
                d['extras_sha256'][name] = m.sha(files[name])
            files['artifact-evidence/descriptor.json'] = json.dumps(d).encode()
        elif case == 'source-unit':
            files['deploy/stage8b-p1e/moex-finam-p1f-o2-materializer.service'] += b'\nReadWritePaths=/\n'
        with tempfile.TemporaryDirectory() as directory:
            mutated = Path(directory) / path.name
            with zipfile.ZipFile(mutated, 'w', zipfile.ZIP_DEFLATED) as out:
                for i in infos:
                    out.writestr(i, files[i.filename])
                if case == 'duplicate-member':
                    with warnings.catch_warnings():
                        warnings.simplefilter('ignore')
                        out.writestr(infos[0], files[infos[0].filename])
                elif case in ('unsafe-member', 'symlink-member'):
                    info = zipfile.ZipInfo('../escape' if case == 'unsafe-member' else 'link')
                    info.external_attr = (0o100644 if case == 'unsafe-member' else 0o120777) << 16
                    out.writestr(info, b'fake')
            try:
                m.check(mutated)
            except (m.old.Error, KeyError, ValueError):
                print('PASS ' + case)
            else:
                raise AssertionError('mutation accepted: ' + case)
    print('PASS successor-artifact negatives=8/8')

if __name__ == '__main__':
    run(Path(sys.argv[1]))
