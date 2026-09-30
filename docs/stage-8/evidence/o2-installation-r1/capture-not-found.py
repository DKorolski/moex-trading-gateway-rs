"""Capture only missing-unit public properties; verify no installation occurred."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess
import sys

root = Path('/root/stage8b-p1f-o2-install-304cd56/bundle')
sys.path.insert(0, str(root / 'scripts'))
import stage8b_p1f_o2_install as accepted

r = accepted.Replacement(Path('/'), root / 'accepted-artifact' / accepted.ARTIFACT_NAME)
r.verify_state('old')
accepted.old.verify_operator_material_absent(Path('/'))
accepted.old.verify_empty_state_for_rollback(Path('/'))
assert not accepted.exists(Path('/'), accepted.TRANSACTION)
assert not accepted.exists(Path('/'), accepted.MANIFEST)
keys = ('LoadState', 'ActiveState', 'SubState', 'MainPID', 'ControlPID', 'Job',
        'FragmentPath', 'DropInPaths', 'ExecStart', 'ControlGroup')
records = []
for unit in accepted.NEW_UNITS:
    args = ['systemctl', 'show', unit, '--no-pager', *['--property=' + key for key in keys]]
    result = subprocess.run(args, capture_output=True, text=True, timeout=10)
    parsed = dict(line.split('=', 1) for line in result.stdout.splitlines())
    assert result.returncode == 0 and set(parsed) == set(keys) - {'ExecStart'}
    assert parsed['LoadState'] == 'not-found'
    records.append({'unit': unit, 'argv': args, 'returncode': result.returncode,
                    'stdout': result.stdout, 'stderr': result.stderr})
print(json.dumps({'schema_version': 1, 'observed_at_utc': datetime.now(timezone.utc).isoformat(),
                  'target': '45.150.11.252', 'accepted_installer_ref': '304cd56bd33e2145f327c5f1ea02f56837cc3e62',
                  'managed_state': 'EXACT_O1_UNCHANGED', 'transaction_present': False,
                  'new_installation_manifest_present': False, 'observations': records}, indent=2))
