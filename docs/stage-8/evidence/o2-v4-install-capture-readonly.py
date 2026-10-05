"""Read-only native inventory. No deployment, service action, Redis or FINAM."""
import datetime
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent
WT = ROOT / 'worktree'
code = 'import sys, types\n'
for name in ('stage8b_p1e_i1_fixed_install', 'stage8b_p1f_o2_install', 'stage8b_p1f_o2_sparse_install'):
    source = (WT / 'scripts' / (name + '.py')).read_text()
    code += f'm=types.ModuleType({name!r});m.__file__="/readonly-input/scripts/{name}.py";sys.modules[{name!r}]=m\nexec({source!r},m.__dict__)\n'
code += (ROOT / 'snapshot_readonly.py').read_text()
command = ['ssh', '-i', '/Users/denisq/.ssh/id_rsa_lightnode', '-o', 'BatchMode=yes',
           '-o', 'IdentitiesOnly=yes', '-o', 'StrictHostKeyChecking=yes', '-o', 'HostKeyAlgorithms=ssh-ed25519',
           '-o', 'ConnectTimeout=10', '-o', 'ServerAliveInterval=15', '-o', 'ServerAliveCountMax=2',
           'root@45.150.11.252', 'python3', '-B', '-']
started = datetime.datetime.now(datetime.timezone.utc).isoformat()
done = subprocess.run(command, input=code.encode(), capture_output=True, timeout=90)
for name, raw in [('snapshot.stdout', done.stdout), ('snapshot.stderr', done.stderr)]:
    with (ROOT/name).open('xb') as f:
        f.write(raw)
record = dict(command=command, stdin_sha256=hashlib.sha256(code.encode()).hexdigest(),
              started_at_utc=started, finished_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
              exit_code=done.returncode, stdout_sha256=hashlib.sha256(done.stdout).hexdigest(),
              stderr_sha256=hashlib.sha256(done.stderr).hexdigest())
with (ROOT/'snapshot.record.json').open('x') as f:
    json.dump(record, f, indent=2)
print(json.dumps(record))
raise SystemExit(done.returncode)
