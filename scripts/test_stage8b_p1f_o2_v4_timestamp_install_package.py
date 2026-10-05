#!/usr/bin/env python3
"""Exact prepared-input negative cases, without host/Redis/FINAM access."""
import copy
import json
from pathlib import Path
import shutil
import sys
import tempfile
import zipfile

import stage8b_p1f_o2_v4_timestamp_install_package as m


def run(source):
    m.validate_prepared(source)
    cases = ['calendar', 'sequence-binding', 'source-plan', 'profile', 'fingerprint',
             'installation-id', 'old-bytes', 'binary', 'boolean-smuggling', 'extra-file',
             'policy-schema', 'bootstrap-schema', 'policy-digest', 'operational-identity',
             'terminal-observation', 'terminal-receipt', 'root-migration', 'installation-scope',
             'old-supervisor-mixed-set', 'old-operator-mixed-set']
    for case in cases:
        with tempfile.TemporaryDirectory(prefix='nrg-install-negative-') as temp:
            root = Path(temp)/'package'
            shutil.copytree(source, root)
            spec = json.loads((root/'spec.json').read_bytes())
            if case in ('calendar', 'boolean-smuggling', 'sequence-binding'):
                if case == 'calendar':
                    spec['calendar']['prior_dates'][0] = '2026-04-10'
                elif case == 'boolean-smuggling':
                    spec['execution_authorized'] = 0
                else:
                    spec['terminal_evidence_sha256'] = '0'*64
                (root/'spec.json').write_bytes(m.canonical(spec))
            elif case in ('root-migration', 'installation-scope'):
                spec['root_migration_authorized' if case == 'root-migration' else 'installation_authorized'] = True
                (root/'spec.json').write_bytes(m.canonical(spec))
            elif case in ('old-supervisor-mixed-set', 'old-operator-mixed-set'):
                prior, _ = m.load_predecessor(root/'retained-terminal.zip', root/'prior-installation.zip')
                target = m.update.CHANGES[2 if case.startswith('old-supervisor') else 1]
                (root/'payload'/Path(target).name).write_bytes(prior[target])
            elif case in ('terminal-observation', 'terminal-receipt'):
                path = root/'retained-terminal.zip'
                files = m.read_compact(path)
                name = 'snapshot.stdout'
                value = json.loads(files[name])
                if case == 'terminal-observation':
                    value['expected']['observation']['p0'] = {}
                else:
                    value['expected']['authority_inventory'].pop(next(n for n in value['expected']['authority_inventory'] if 'terminal-receipt-' in n))
                files[name] = m.canonical(value)
                path.unlink()
                m.write_compact(path, files)
            elif case in ('policy-schema', 'bootstrap-schema', 'policy-digest', 'operational-identity'):
                path = root/'payload'/('supervisor.template.json' if case == 'bootstrap-schema' else 'materialization-policy.json')
                value = json.loads(path.read_bytes())
                if case == 'bootstrap-schema':
                    value['bootstrap']['schema_version'] = 1
                elif case == 'policy-schema':
                    value['schema_version'] = 2
                elif case == 'policy-digest':
                    value['market_data_policy_sha256'] = '0'*64
                else:
                    value['operational_identity_sha256'] = '9b3572618c2540b32e7fd2fb256b5604fd48aec74b7de63a1e01d498b4b962c0'
                path.write_bytes(m.canonical(value))
            elif case in ('profile', 'fingerprint', 'installation-id'):
                name = 'source-template.json' if case == 'profile' else 'supervisor.template.json' if case == 'fingerprint' else 'installation-o2-v1.json'
                path = root/'payload'/name
                data = json.loads(path.read_bytes())
                if case == 'profile':
                    data['runtime_profile_sha256'] = '0'*64
                elif case == 'fingerprint':
                    data['bootstrap']['runtime_config_fingerprint_sha256'] = '0'*64
                else:
                    data['installation_id'] = 'new-genesis'
                path.write_bytes(m.canonical(data))
                # A local rehash is not a new accepted installation recipe.
                for key in spec['updates']:
                    if Path(key).name == name:
                        spec['updates'][key]['new_sha256'] = m.sha(path.read_bytes())
                (root/'spec.json').write_bytes(m.canonical(spec))
            else:
                name = {'source-plan': 'sparse-policy-contract.rs', 'old-bytes': 'prior-installation.zip',
                        'binary': 'payload/stage8b-p1f-o2-materializer', 'extra-file': 'authority.json'}[case]
                (root/name).write_bytes(b'changed')
            try:
                m.validate_prepared(root)
            except (m.update.custody.Error, ValueError, KeyError, zipfile.BadZipFile):
                print('PASS ' + case, flush=True)
            else:
                raise AssertionError('accepted ' + case)
    print('PASS v4-timestamp-install-package negatives=20/20')


if __name__ == '__main__':
    run(Path(sys.argv[1]))
