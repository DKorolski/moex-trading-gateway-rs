#!/usr/bin/env python3
"""Offline release build of the accepted O3 source; no deployment/credentials.

Only immutable Git blobs and public Cargo caches enter the networkless build.
Production build records are separate from any test/fixture compilation.
"""
import argparse
import json
from pathlib import Path
import subprocess
import time

import stage8b_p1f_o2_build_linux as b

REF = 'fb1ceb3dac40b365d0d23983af788ca1c9c72907'
TREE = 'bf7b2353a8a972ffd5af0ba1fc7e86e7eeb06b31'
BINS = ('stage8b-p1f-o2-operator', 'stage8b-p1f-local-supervisor',
        'stage8b-p1f-o3-composition', 'stage8b-p1f-o3-signer', 'stage8b-p1f-o3-provisioner')


def put(path, raw):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('xb') as f:
        f.write(raw)


def encoded(value):
    return (json.dumps(value, indent=2, sort_keys=True)+'\n').encode()


def capture(out, name, command):
    print('RUN '+name, flush=True)
    start = time.monotonic()
    with (out/'logs'/(name+'.log')).open('xb') as f:
        r = subprocess.run(command, stdout=f, stderr=subprocess.STDOUT)
    raw = (out/'logs'/(name+'.log')).read_bytes()
    put(out/'logs'/(name+'.json'), encoded(dict(command=command, exit_code=r.returncode,
        seconds=round(time.monotonic()-start, 3), log_sha256=b.sha(raw))))
    if r.returncode:
        raise RuntimeError(name+' failed; evidence retained')
    print('PASS '+name, flush=True)
    return raw


def source_check(out):
    expected = set()
    for row in b.git('ls-tree', '-rz', REF).split(b'\0'):
        if not row:
            continue
        metadata, name = row.split(b'\t', 1)
        mode, kind, oid = metadata.split()
        if mode not in (b'100644', b'100755') or kind != b'blob':
            raise ValueError('nonregular source')
        p = out/'source'/name.decode(); expected.add(p)
        if p.is_symlink() or p.read_bytes() != b.git('cat-file', 'blob', oid.decode()):
            raise ValueError('source export changed')
    if expected != {p for p in (out/'source').rglob('*') if not p.is_dir()}:
        raise ValueError('source inventory changed')


def build(out, cache):
    if b.git('rev-parse', REF+'^{tree}').decode().strip() != TREE:
        raise ValueError('accepted source tree mismatch')
    if {p.name for p in (cache/'cargo-home').iterdir()} - {
            'registry', '.global-cache', '.package-cache', '.package-cache-mutate'}:
        raise ValueError('only public Cargo cache permitted')
    if any(p.is_symlink() for p in (cache/'cargo-home').rglob('*')):
        raise ValueError('cache symlink')
    out.mkdir(parents=True, exist_ok=False)
    (out/'source').mkdir(); (out/'logs').mkdir()
    subprocess.run(['tar', '-xf', '-', '-C', str(out/'source')],
                   input=b.git('archive', '--format=tar', REF), check=True)
    put(out/'source-commit.raw', b.git('cat-file', 'commit', REF))
    source_check(out)
    docker = ['docker', 'run', '--rm', '--network', 'none', '--platform', 'linux/amd64',
        '--ulimit', 'core=0', '--mount', f'type=bind,src={out / "source"},dst=/src,readonly',
        '--mount', f'type=bind,src={cache / "target"},dst=/target',
        '--mount', f'type=bind,src={cache / "cargo-home"},dst=/cargo-home',
        '-w', '/src', '-e', 'CARGO_HOME=/cargo-home', '-e', 'CARGO_TARGET_DIR=/target',
        '-e', 'CARGO_NET_OFFLINE=true', '-e', 'CARGO_INCREMENTAL=0',
        '-e', 'RUST_MIN_STACK=33554432', '-e', 'CARGO_TERM_COLOR=never', b.IMAGE]
    capture(out, 'toolchain', [*docker, 'rustc', '--version', '--verbose'])
    args = ['cargo', 'build', '--release', '--locked', '--offline', '--no-default-features',
        '--message-format=json', '-p', 'finam-gateway', '-p', 'runtime-durable-service']
    for name in BINS:
        args += ['--bin', name]
    raw = capture(out, 'production-build', [*docker, *args])
    artifacts = []
    for line in raw.splitlines():
        if line.startswith(b'{'):
            item = json.loads(line)
            if item.get('reason') == 'compiler-artifact':
                artifacts.append(item)
    selected = [a for a in artifacts if a.get('executable') and a['target']['name'] in BINS]
    if {a['target']['name'] for a in selected} != set(BINS) or len(selected) != len(BINS):
        raise ValueError('missing/duplicate production executables')
    for a in artifacts:
        if any('fixture' in f for f in a['features']):
            raise ValueError('fixture features in production graph')
        if a['profile']['test']:
            raise ValueError('test artifact in production graph')
    entries = []
    for name in BINS:
        raw = (cache/'target/release'/name).read_bytes()
        if raw[:6] != b'\x7fELF\x02\x01' or int.from_bytes(raw[18:20], 'little') != 62:
            raise ValueError('not Linux x86-64 ELF')
        put(out/'payload'/name, raw); (out/'payload'/name).chmod(0o555)
        entries.append(dict(name=name, size=len(raw), sha256=b.sha(raw), elf_machine='x86-64'))
        capture(out, 'identity-'+name, [*docker, 'readelf', '-h', '-d', '-V', '/target/release/'+name])
    source_check(out)
    put(out/'build.json', encoded(dict(schema_version=1, status='BUILT_NOT_INSTALLED',
        source_ref=REF, source_tree=TREE, image=b.IMAGE, cargo_args=args,
        source_commit_raw_sha256=b.sha((out/'source-commit.raw').read_bytes()),
        binaries=entries, production_artifacts=selected, production_artifact_count=len(artifacts),
        fixture_features_enabled=False, cargo_offline=True, network='none',
        source_tree_reverified=True, installation_performed=False, operational_o3_invoked=False)))
    print('PASS release build: five fixed slots', flush=True)


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('output', type=Path); p.add_argument('--cache', type=Path, required=True)
    a = p.parse_args(); build(a.output.resolve(), a.cache.resolve())
