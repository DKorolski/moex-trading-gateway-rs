"""Read-only diagnostic: report keys and safe scalar values, never ExecStart."""
import hashlib
import json
import subprocess
from datetime import datetime, timezone

keys = ('LoadState', 'ActiveState', 'SubState', 'MainPID', 'ControlPID', 'Job',
        'FragmentPath', 'DropInPaths', 'ExecStart', 'ControlGroup')
units = ('moex-finam-p1-paper.service', 'moex-finam-p1-paper-bootstrap.service',
         'moex-finam-p1f-o2-materializer.service',
         'moex-finam-p1-paper-o2-bootstrap-runner.service',
         'moex-finam-paper-runtime.service', 'moex-finam-paper-ws.service')
observations = []
for unit in units:
    for include_empty in (False, True):
        args = ['systemctl', 'show', unit, '--no-pager']
        if include_empty:
            args.append('--all')
        args.extend('--property=' + key for key in keys)
        result = subprocess.run(args, capture_output=True, text=True, timeout=10)
        rows = [line.split('=', 1) for line in result.stdout.splitlines()]
        parsed = {key: value for key, value in rows if len((key, value)) == 2}
        scalars = {key: value for key, value in parsed.items() if key in
                   ('LoadState', 'ActiveState', 'SubState', 'MainPID', 'ControlPID', 'Job')}
        observations.append({'unit': unit, 'include_empty': include_empty,
                             'argv': args, 'returncode': result.returncode,
                             'keys': sorted(parsed), 'missing': sorted(set(keys) - set(parsed)),
                             'line_count': len(rows), 'safe_scalars': scalars,
                             'stdout_sha256': hashlib.sha256(result.stdout.encode()).hexdigest(),
                             'stderr_sha256': hashlib.sha256(result.stderr.encode()).hexdigest()})
print(json.dumps({'observed_at_utc': datetime.now(timezone.utc).isoformat(),
                  'observations': observations, 'remote_mutation_performed': False}, indent=2))
