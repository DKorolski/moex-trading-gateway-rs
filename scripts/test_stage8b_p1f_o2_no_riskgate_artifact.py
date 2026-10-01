#!/usr/bin/env python3
"""Focused negatives, including rehashed evidence; no service or broker access."""
import json
from pathlib import Path
import sys
import tempfile
import warnings
import zipfile

import stage8b_p1f_o2_no_riskgate_artifact as m


def run(path):
    m.check(path)
    with zipfile.ZipFile(path) as z:
        infos = z.infolist()
        original = {i.filename: z.read(i) for i in infos}
    cases = ['binary', 'build-ref', 'source', 'profile-fixture', 'open-execution',
             'open-installation', 'smoke-negative-removed', 'probe-network', 'toolchain',
             'duplicate', 'traversal', 'symlink']
    for case in cases:
        files = dict(original)
        descriptor = json.loads(files[m.P + 'descriptor.json'])
        def replace(name, document):
            files[name] = m.encoded(document)
            descriptor['generated_sha256'][name] = m.sha(files[name])
        if case == 'binary':
            files[m.P + 'payload/' + m.source.builder.BINS[0]] += b'changed'
        elif case == 'build-ref':
            descriptor['compiled_ref'] = '0' * 40
        elif case == 'source':
            files['Cargo.toml'] += b'\n#changed\n'
        elif case == 'profile-fixture':
            name = m.P + 'qualification/supervisor-fixture.json'
            value = json.loads(files[name]); value['runtime_profile_id'] = 'legacy'
            replace(name, value)
        elif case.startswith('open-'):
            descriptor[case[5:] + '_authorized'] = True
        elif case == 'smoke-negative-removed':
            name = m.P + 'qualification/smoke.log'
            files[name] = files[name].replace(b'PASS no-riskgate exact-elf mixed-runtime_profile_sha256 exit=64', b'')
            descriptor['generated_sha256'][name] = m.sha(files[name])
            name2 = m.P + 'qualification/commands.json'
            value = json.loads(files[name2]); value[-1]['stdout'] = files[name].decode()
            replace(name2, value)
        elif case == 'probe-network':
            name = m.P + 'qualification/commands.json'
            value = json.loads(files[name]); value[0]['command'][value[0]['command'].index('--network') + 1] = 'host'
            replace(name, value)
        elif case == 'toolchain':
            name = m.P + 'build.json'
            value = json.loads(files[name]); value['rust_image'] = 'rust:latest'
            replace(name, value)
        files[m.P + 'descriptor.json'] = m.encoded(descriptor)
        with tempfile.TemporaryDirectory(prefix='nrg-artifact-negative-') as directory:
            mutated = Path(directory) / path.name
            with zipfile.ZipFile(mutated, 'w', zipfile.ZIP_DEFLATED) as out:
                for info in infos:
                    out.writestr(info, files[info.filename])
                if case == 'duplicate':
                    with warnings.catch_warnings():
                        warnings.simplefilter('ignore')
                        out.writestr(infos[0], files[infos[0].filename])
                elif case in ('traversal', 'symlink'):
                    info = m.common.zip_info('../escape' if case == 'traversal' else 'link',
                                             '120777' if case == 'symlink' else '100644')
                    out.writestr(info, b'test')
            try:
                m.check(mutated)
            except ValueError:
                print('PASS ' + case)
            else:
                raise AssertionError('accepted mutation: ' + case)
    print('PASS no-riskgate-artifact negatives=12/12')


if __name__ == '__main__':
    run(Path(sys.argv[1]).resolve())
