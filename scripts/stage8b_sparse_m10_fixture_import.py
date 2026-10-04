#!/usr/bin/env python3
"""Import exact public diagnostic data; no broker requests or synthetic bars."""
import hashlib
import io
import json
from pathlib import Path
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = '5ada885a1b087b2cc74775a1c51a0f8bd6231d69f654ead364f53485f1ce6993'
EXPECTED_FILES = {
    'finam-short-response.json': 'd2e7d6b57a47ab921e42904a967a80aa86854e359c35b32aaa095831298b2ea9',
    'finam-short-response-meta.json': 'c90b4e9338754d842cd18c725a9a3e2840d81546a2de4b04b9d5d56df1870e66',
    'finam-long-response.json': '8dac45b26cb1e07f11bf73f7b2032abfc6a4362aae4a2a7e297401f7dced7383',
    'finam-long-response-meta.json': '591906ccde78e9d78a4f5e5bd292565bc10199fc44280704af5f31f91da73d40',
    'alor-native-m10.jsonl': '54d75e8e7e92390257cc84f14cc7700b480795ebc957952e524fdb65d72bc632',
}


def verify(out):
    manifest = json.loads((out/'manifest.json').read_text())
    if manifest != {'source_compact_archive_sha256': EXPECTED, 'synthetic': False, 'files': EXPECTED_FILES}:
        raise ValueError('diagnostic fixture lineage mismatch')
    if {p.name for p in out.iterdir()} != set(EXPECTED_FILES) | {'manifest.json'}:
        raise ValueError('unexpected or missing diagnostic fixture')
    for name, expected in EXPECTED_FILES.items():
        if hashlib.sha256((out/name).read_bytes()).hexdigest() != expected:
            raise ValueError('diagnostic fixture digest mismatch: '+name)
    print('PASS immutable diagnostic fixtures: 5/5')


def main():
    out = ROOT/'crates/broker-finam/tests/fixtures/sparse-m10'
    if sys.argv[1:] == ['--verify-only']:
        verify(out)
        return
    raw = Path(sys.argv[1]).read_bytes()
    if hashlib.sha256(raw).hexdigest() != EXPECTED:
        raise ValueError('source archive SHA-256 mismatch')
    out.mkdir(parents=True, exist_ok=True)
    files = {}
    with zipfile.ZipFile(io.BytesIO(raw)) as z:
        for label in ('short', 'long'):
            for suffix in ('response.json', 'response-meta.json'):
                name = label+'-'+suffix
                files['finam-'+name] = z.read('inputs/finam/responses/'+name)
        with zipfile.ZipFile(io.BytesIO(z.read('inputs/alor-export.zip'))) as a:
            files['alor-native-m10.jsonl'] = a.read('alor_imoexf_finam_export_2026_09_28_10_01/IMOEXF_M10_raw.jsonl')
    manifest = {'source_compact_archive_sha256': EXPECTED, 'synthetic': False,
                'files': {n: hashlib.sha256(b).hexdigest() for n,b in files.items()}}
    if manifest['files'] != EXPECTED_FILES:
        raise ValueError('unexpected archive member bytes')
    files['manifest.json'] = (json.dumps(manifest, indent=2)+'\n').encode()
    for name, data in files.items():
        if (out/name).exists():
            if (out/name).read_bytes() != data:
                raise ValueError('refusing to overwrite changed fixture: '+name)
        else:
            with (out/name).open('xb') as f:
                f.write(data)
    print(json.dumps(manifest, indent=2))
    verify(out)


if __name__ == '__main__':
    main()
