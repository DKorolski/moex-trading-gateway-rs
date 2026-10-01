#!/usr/bin/env python3
"""Generate the six-file update payload/identity locally, without target access."""
import argparse
import copy
import json
from pathlib import Path
import zipfile

import stage8b_p1f_o2_install as old
import stage8b_p1f_o2_successor_artifact as artifact
import stage8b_p1f_o2_terminal_update as update

ROOT = Path(__file__).resolve().parents[1]

def prepare(new_artifact, old_artifact, output):
    safety = artifact.check(new_artifact)
    _, previous, inventory = old.load_package(old_artifact)
    old.require(old.sha(old.canonical(inventory)) == update.OLD_SHA, 'old installed identity drift')
    new = copy.deepcopy(inventory)
    payload = {}
    with zipfile.ZipFile(new_artifact) as z:
        for name in update.CHANGES[:3]:
            payload[name] = z.read('artifact-evidence/payload/' + Path(name).name)
        payload[update.CHANGES[3]] = z.read('deploy/stage8b-p1e/' + Path(update.CHANGES[3]).name)
    compat = json.loads(previous[old.old.MANIFEST][0])
    compat['binary_sha256'] = old.sha(payload[old.old.BINARY_PATH])
    for name in compat['managed_payload_sha256']:
        if name in payload:
            compat['managed_payload_sha256'][name] = old.sha(payload[name])
    payload[old.old.MANIFEST] = old.canonical(compat)
    for name, raw in payload.items():
        new['payload'][name]['sha256'] = old.sha(raw)
        new['payload'][name]['size'] = len(raw)
    new['artifact_ref'] = artifact.source.SOURCE_REF
    new['artifact_sha256'] = safety['archive_sha256']
    new['installer_sha256'] = old.sha(Path(update.__file__).read_bytes())
    new['predecessor_manifest_sha256'] = update.OLD_SHA
    new['update_revision'] = 'o2-after-terminal-sequence-2-589b801-v1'
    payload[old.MANIFEST] = old.canonical(new)
    spec = dict(schema_version=1, domain='moex.o2.terminal-update.v1', execution_authorized=False,
                target_mutation_performed=False, old_manifest_sha256=update.OLD_SHA,
                new_manifest_sha256=old.sha(payload[old.MANIFEST]), old_inventory=inventory,
                artifact_name=new_artifact.name, artifact_sha256=safety['archive_sha256'],
                compiled_source_ref=artifact.source.SOURCE_REF,
                terminal_evidence_sha256=update.TERMINAL_SHA,
                preserved_installation_id=old.INSTALLATION_ID,
                updates={name: {'old_sha256': old.sha(previous[name][0]), 'new_sha256': old.sha(raw),
                                'size': len(raw)} for name, raw in payload.items()})
    output.mkdir(parents=True, exist_ok=True)
    for name, raw in payload.items():
        (output / Path(name).name).write_bytes(raw)
    update.SPEC.write_text(json.dumps(spec, indent=2, sort_keys=True) + '\n')
    print(json.dumps({'result': 'PREPARED_NOT_INSTALLED', 'new_installation_sha256': spec['new_manifest_sha256'],
                      'write_slots': len(payload), 'preserved_installation_id': old.INSTALLATION_ID}))

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifact', type=Path, required=True)
    parser.add_argument('--old-artifact', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    prepare(args.artifact.resolve(), args.old_artifact.resolve(), args.output.resolve())
