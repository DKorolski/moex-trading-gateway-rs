#!/usr/bin/env python3
"""Offline integration gate; immutable released code and retained diagnostic input.

Does not install a successor, generate operational authorization or contact VPS.
The retained staged JSON is confidential input and must NOT enter handoff ZIPs.
"""
import argparse
import io
import json
from pathlib import Path
import subprocess
import sys
import uuid
import zipfile

import stage8b_p1f_o2_v4_timestamp_install_package as prep
import test_stage8b_p1f_o2_template_encoding as fixture

ROOT = prep.ROOT
STAGED_SHA = '5c7ba133f673d9f46916ee092781613e342044be7d5381c37aeb011bc2e307b7'

def gate(archive, build, retained, output):
    if prep.sha(retained.read_bytes()) != STAGED_SHA:
        raise ValueError('retained staged input mismatch')
    output.mkdir(parents=True, exist_ok=False)
    prepared = output / 'fixture'
    files, original, payload, _ = fixture.corrected_fixture(archive)
    fixture.emit(files, prepared)
    prep.validate_prepared(prepared)
    (output / 'original-template.json').write_bytes(original[prep.update.CHANGES[5]])
    records = []
    def run(args, timeout=180, expected_exit=0):
        idx = f'{len(records):02d}'
        print('RUN ' + idx + ' ' + args[0], flush=True)
        p = subprocess.run(args, cwd=ROOT, capture_output=True, timeout=timeout)
        (output / (idx + '.stdout')).write_bytes(p.stdout)
        (output / (idx + '.stderr')).write_bytes(p.stderr)
        records.append(dict(command=args, exit_code=p.returncode, expected_exit=expected_exit,
                            stdout_sha256=prep.sha(p.stdout), stderr_sha256=prep.sha(p.stderr)))
        (output / 'commands.json').write_bytes(prep.canonical(records))
        if p.returncode != expected_exit:
            raise RuntimeError('gate failed; retained diagnostic logs: ' + idx)
        print('PASS ' + idx, flush=True)
    run([sys.executable, '-B', 'scripts/test_stage8b_p1f_o2_template_encoding.py', '--archive', str(archive)])
    run([sys.executable, '-B', 'scripts/test_stage8b_p1f_o2_v4_timestamp_install_package.py', str(prepared)])
    run(['rustfmt', '--edition', '2021', '--check', 'scripts/fixtures/stage8b-o2-canonical-materialization-probe.rs'])
    with zipfile.ZipFile(io.BytesIO(files['binary-artifact.zip'])) as z:
        pins = json.loads(z.read(prep.artifact.P + 'qualification/result.json'))['rlib_sha256']
    deps = build / 'target/release/deps'
    externs, linked = [], {}
    for name in ('runtime_durable_service', 'finam_gateway', 'serde_json', 'chrono', 'sha2'):
        matches = [n for n in pins if n.startswith('lib' + name + '-') and n.endswith('.rlib')]
        if len(matches) != 1 or prep.sha((deps / matches[0]).read_bytes()) != pins[matches[0]]:
            raise ValueError('accepted rlib mismatch: ' + name)
        linked[matches[0]] = pins[matches[0]]
        externs += ['--extern', name + '=/deps/' + matches[0]]
    operator = prepared / 'payload/stage8b-p1f-o2-operator'
    if prep.sha(operator.read_bytes()) != '81e88b129896287e2b6b849043d2dfb09b860ddcd3861d1094f7e76a857b40e7':
        raise ValueError('accepted operator ELF mismatch')
    # Extraction writes files as data; only the test container copies/chmods ELF.
    def mount(path, target, ro=True):
        return ['--mount', f'type=bind,src={path},dst={target}' + (',readonly' if ro else '')]
    docker = ['docker', 'run', '--rm', '--network', 'none', '--platform', 'linux/amd64',
              '--ulimit', 'core=0', '--cap-drop=ALL', '--security-opt=no-new-privileges']
    image = prep.qualified.source.IMAGE
    probe = ROOT / 'scripts/fixtures/stage8b-o2-canonical-materialization-probe.rs'
    run(docker + mount(deps, '/deps') + mount(probe, '/probe.rs') + mount(output, '/proof', False) +
        [image, 'rustc', '--edition=2021', '-D', 'warnings', '/probe.rs', '-L', 'dependency=/deps',
         *externs, '-o', '/proof/materialization-probe'], 300)
    name = 'o2-canonical-probe-' + uuid.uuid4().hex[:12]
    # Only output/probe and public payload are exposed, not Docker socket, SSH,
    # host production dirs or the ceremony folder. Raw source is a read-only file.
    command = docker + ['--name', name, '-e', 'O2_DISPOSABLE_PROBE=1'] + mount(prepared, '/package') + \
        mount(retained, '/private-input/retained-staged.json') + mount(operator, '/operator-input') + \
        mount(output / 'materialization-probe', '/probe') + mount(output / 'original-template.json', '/original-template.json') + \
        [image, 'sh', '-ec', 'cp /operator-input /accepted-operator; chmod 0555 /accepted-operator; exec /probe']
    try:
        run(command, 120)
    finally:
        subprocess.run(['docker', 'rm', '-f', name], capture_output=True, timeout=30)
    existing_probe = ROOT / 'scripts/fixtures/stage8b-o2-sparse-install-probe.rs'
    run(docker + mount(deps, '/deps') + mount(existing_probe, '/probe.rs') + mount(output, '/proof', False) +
        [image, 'rustc', '--edition=2021', '-D', 'warnings', '/probe.rs', '-L', 'dependency=/deps',
         *externs, '-o', '/proof/input-probe'], 300)
    run(docker + mount(prepared, '/package') + mount(output / 'input-probe', '/probe') + [image, '/probe'])
    run(docker + mount(prepared, '/package') + mount(output / 'input-probe', '/probe') +
        mount(output / 'original-template.json', '/package/payload/supervisor.template.json') + [image, '/probe'],
        expected_exit=101)
    if b'packaged supervisor template must be canonical serde_json without LF' not in (output / '07.stderr').read_bytes():
        raise ValueError('old-LF rejection did not reach the canonical boundary')
    # Release binaries and exact linked libraries remain untouched.
    for name, digest in linked.items():
        if prep.sha((deps / name).read_bytes()) != digest:
            raise ValueError('release rlib changed')
    result = dict(result='PASS', installed_source_ref=prep.artifact.source.SOURCE_REF,
        retained_staged_sha256=STAGED_SHA, retained_source_sha256='e669f6a1c6632e953a5292af1c958fb14f7a3cc99c54c6ac462bf3a30185adaa',
        original_template_sha256=prep.sha(original[prep.update.CHANGES[5]]),
        corrected_template_sha256=prep.sha(payload[prep.update.CHANGES[5]]),
        linked_rlib_sha256=linked, operator_elf_sha256=prep.sha(operator.read_bytes()),
        commands=len(records), records=records, historical_clock='2026-10-06T04:15:05.318837Z',
        fixture_authority_only=True, source_bytes_unchanged=True, network='none',
        vps_mutations=False, production_keys_used=False, redis_contact=False, finam_contact=False,
        operational_successor_prepared=False, installation_authorized=False, execution_authorized=False,
        o2_verdict='HOLD', source_tree_probe_sha256=prep.sha(probe.read_bytes()),
        tested_source_sha256={p.relative_to(ROOT).as_posix(): prep.sha(p.read_bytes()) for p in (
            Path(__file__).resolve(), ROOT / 'scripts/test_stage8b_p1f_o2_template_encoding.py',
            ROOT / 'scripts/stage8b_p1f_o2_v4_timestamp_install_package.py', existing_probe, probe)})
    (output / 'result.json').write_bytes(prep.canonical(result))
    print('PASS offline materialization chain; O2 remains HOLD; no operational install/phase')

if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--archive', type=Path, default=ROOT / 'reports/handoff/moex-trading-project-836a297-o2-v4-timestamp-installation.zip')
    p.add_argument('--build', type=Path, required=True)
    p.add_argument('--retained', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    gate(a.archive.resolve(), a.build.resolve(), a.retained.resolve(), a.output.resolve())
