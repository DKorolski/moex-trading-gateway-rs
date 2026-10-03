#!/usr/bin/env python3
"""Tamper controls against the actual sparse release ZIP, including rehashes."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import zipfile

import stage8b_p1f_o2_sparse_artifact as a


def main(path):
    a.check(path)
    with zipfile.ZipFile(path) as z:
        files = {i.filename: (z.read(i), i.external_attr) for i in z.infolist()}
    def change_json(data, name, key, value):
        obj = json.loads(data[name][0])
        obj[key] = value
        data[name] = (a.encoded(obj), data[name][1])
    def rehash(data):
        name = a.P+'descriptor.json'
        meta = json.loads(data[name][0])
        meta['generated_sha256'] = {n:a.sha(data[n][0]) for n in meta['generated_sha256'] if n in data}
        data[name] = (a.encoded(meta), data[name][1])
    q = a.P+'qualification/'
    cases = [
        ('source-blob', lambda d: d.__setitem__('Cargo.toml',(b'[bad]',d['Cargo.toml'][1]))),
        ('ELF-byte', lambda d: d.__setitem__(a.P+'payload/'+a.source.builder.BINS[0],
                                         (b'bad-elf',0o100644 << 16))),
        ('build-ref', lambda d: change_json(d,a.P+'build.json','implementation_ref','0'*40)),
        ('release-log-missing', lambda d: d.pop(a.P+'build.log')),
        ('raw-commit', lambda d: d.__setitem__(a.P+'build-commit.raw',(b'forged',0o100644 << 16))),
        ('policy-schema-rehashed', lambda d: change_json(d,q+'materialization-policy-fixture.json','schema_version',2)),
        ('source-v3-rehashed', lambda d: change_json(d,q+'source-v4-fixture.json','schema_version',3)),
        ('foreign-profile-rehashed', lambda d: change_json(d,q+'supervisor-fixture.json','runtime_profile_sha256','a'*64)),
        ('boolean-count-rehashed', lambda d: change_json(d,q+'probe-result.json','history_m10_count',True)),
        ('forged-qualification', lambda d: change_json(d,q+'result.json','result','FAIL')),
        ('activation-enabled', lambda d: change_json(d,a.P+'descriptor.json','execution_authorized',True)),
        ('missing-negative-fixture', lambda d: d.pop(q+'mixed-schema1.json')),
        ('symlink', lambda d: d.__setitem__('README.md',(d['README.md'][0],0o120777 << 16))),
        ('path-traversal', lambda d: d.__setitem__('../escape',(b'x',0o100644 << 16))),
        ('absolute-path', lambda d: d.__setitem__('/escape',(b'x',0o100644 << 16))),
        ('noncanonical-path', lambda d: d.__setitem__('docs//escape',(b'x',0o100644 << 16))),
    ]
    with tempfile.TemporaryDirectory(prefix='sparse-artifact-neg-') as tmp:
        target=Path(tmp)/path.name
        for label, mutate in cases:
            data=copy.deepcopy(files)
            mutate(data)
            rehash(data)
            with zipfile.ZipFile(target,'w',zipfile.ZIP_STORED) as z:
                for name,(raw,mode) in data.items():
                    info=zipfile.ZipInfo(name); info.external_attr=mode
                    z.writestr(info,raw)
            try:
                a.check(target)
            except (ValueError,RuntimeError,KeyError,UnicodeError):
                print('PASS '+label,flush=True)
            else:
                raise SystemExit('FAIL escaped negative: '+label)
    print(f'PASS sparse-artifact-negative {len(cases)}/{len(cases)}')


if __name__=='__main__':
    main(Path(sys.argv[1]).resolve())
