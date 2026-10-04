#!/usr/bin/env python3
"""Exact prepared-input negative cases, without host/Redis/FINAM access."""
import copy
import json
from pathlib import Path
import shutil
import sys
import tempfile

import stage8b_p1f_o2_no_riskgate_install_package as m


def run(source):
    m.validate_prepared(source)
    cases = ['calendar', 'sequence-binding', 'source-plan', 'profile', 'fingerprint',
             'installation-id', 'old-bytes', 'binary', 'boolean-smuggling', 'extra-file']
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
                name = {'source-plan': 'source-plan-v3.json', 'old-bytes': 'prior-installation.zip',
                        'binary': 'payload/stage8b-p1f-o2-materializer', 'extra-file': 'authority.json'}[case]
                (root/name).write_bytes(b'changed')
            try:
                m.validate_prepared(root)
            except (m.update.custody.Error, ValueError):
                print('PASS ' + case, flush=True)
            else:
                raise AssertionError('accepted ' + case)
    print('PASS no-riskgate-install-package negatives=10/10')


if __name__ == '__main__':
    run(Path(sys.argv[1]))
