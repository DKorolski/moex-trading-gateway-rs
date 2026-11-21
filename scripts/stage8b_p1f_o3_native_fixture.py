"""Public synthetic container setup reused from the accepted O3 recipe.

Not a historical evidence verifier, installed runtime or host configuration.
No executable module entrypoint and no private inputs.
"""
import hashlib
from stage8b_p1f_o3_collect_readonly import SSH
from stage8b_p1f_o3_release import BINS

IMAGE = 'sha256:3cc66c640df0444530a626d2acbcfeda9742039b917a747fd023b315ef2c1526'
UNITS = ('moex-finam-p1f-o3-signer.service', 'moex-finam-p1f-o3-supervisor.service')
SHA = lambda b: hashlib.sha256(b).hexdigest()

SETUP = '''
import os,pathlib,shutil
p=pathlib.Path
for d,mode in [('/usr/local/libexec/moex',0o755),('/etc/moex-finam-p1-paper',0o750),
 ('/var/lib/moex-finam-p1-paper-control',0o750),('/var/lib/moex-finam-p1-paper/state',0o700),
 ('/var/lib/moex-finam-p1-paper-o3-producer',0o700),('/run/moex-finam-p1-o3-input',0o700),
 ('/run/moex-finam-p1-o3-runtime-credentials',0o700)]:
 p(d).mkdir(parents=True,exist_ok=True);p(d).chmod(mode)
for f in p('/payload').glob('stage8b-p1f-*'):
 q=p('/usr/local/libexec/moex')/f.name;shutil.copyfile(f,q);q.chmod(0o755)
for name in ('signer','supervisor'):
 q=p('/etc/systemd/system/moex-finam-p1f-o3-'+name+'.service')
 raw=p('/payload/'+q.name+'.in').read_text()
 assert raw.count('@PHASE_SHA256@')==1
 q.write_text(raw.replace('@PHASE_SHA256@','0'*64));q.chmod(0o644)
for f in ('/run/moex-finam-p1-o3-input/schedule.ed25519',
          '/run/moex-finam-p1-o3-runtime-credentials/stage8b-p1-lifecycle.key'):
 p(f).write_bytes(b'NOT_A_KEY_NATIVE_CUSTODY_TEST');p(f).chmod(0o400)
for d in ('/run/moex-finam-p1-o3-runtime-credentials',
          '/run/moex-finam-p1-o3-runtime-credentials/stage8b-p1-lifecycle.key',
          '/var/lib/moex-finam-p1-paper/state','/var/lib/moex-finam-p1-paper-o3-producer'):
 os.chown(d,987,987)
for d in ('/etc/moex-finam-p1-paper','/var/lib/moex-finam-p1-paper-control'):
 os.chown(d,0,987)
for n in ('.execution.lock','.guardian.lock'):
 q=p('/var/lib/moex-finam-p1-paper-control')/n;q.write_bytes(b'');q.chmod(0o600)
'''

GLOBAL_STATE = '''
import json,pathlib,hashlib,subprocess
p=pathlib.Path('/proc/sys/fs/binfmt_misc')
v={f.name:f.read_text() for f in sorted(p.iterdir()) if f.name!='register' and f.is_file()}
print(json.dumps(dict(binfmt=v,containers=subprocess.check_output(
 ['docker','ps','-aq','--no-trunc']).decode().splitlines()),sort_keys=True))
'''
