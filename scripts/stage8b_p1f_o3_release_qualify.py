#!/usr/bin/env python3
"""Local Linux release/installer/systemd qualification. No VPS or private keys.

Real PID1 sandbox: syntax, custody and diagnostic missing-phase service start.
The local x86 emulation fails systemd's executor (also journald) with 255.
This limit is retained as DYNAMIC_UNQUALIFIED, never a runtime negative PASS.
No operational input is issued or manufactured here.
"""
import argparse
import json
from pathlib import Path
import subprocess
import time
import uuid

import stage8b_p1f_o3_release as r
import stage8b_p1f_o3_source_handoff as h

IMAGE='sha256:3cc66c640df0444530a626d2acbcfeda9742039b917a747fd023b315ef2c1526'


def qualify(repo, package, old, out):
    out.mkdir(parents=True,exist_ok=False);(out/'logs').mkdir()
    records=[];container='o3-release-'+uuid.uuid4().hex[:12]
    qualified_paths = [*sorted((repo/'scripts').glob('*stage8b_p1f_o3*.py')),
        *[repo/'scripts'/(n+'.py') for n in ('stage8b_p1e_i1_fixed_install','stage8b_p1f_o2_install',
            'stage8b_p1f_o2_terminal_install','stage8b_p1f_o2_replay_install')],
        *sorted((repo/'deploy/stage8b-p1f-o3').glob('*.in'))]
    qualified = {p.relative_to(repo).as_posix():r.b.sha(p.read_bytes()) for p in qualified_paths}
    def run(label,args,expect=0):
        started=time.monotonic()
        p=subprocess.run(args,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=600)
        r.put(out/'logs'/(label+'.log'),p.stdout)
        record=dict(id=label,command=args,exit_code=p.returncode,expected_exit_code=expect,
                    seconds=round(time.monotonic()-started,3),sha256=r.b.sha(p.stdout))
        records.append(record);r.put(out/'logs'/(label+'.json'),r.encoded(record))
        if p.returncode not in (expect if isinstance(expect,list) else [expect]):
            raise RuntimeError(label+': '+p.stdout.decode(errors='replace')[-2500:])
        print('PASS '+label,flush=True);return p.stdout
    ex=lambda *a:['docker','exec',container,*a]
    try:
        run('create',['docker','run','-d','--name',container,'--network','none','--platform','linux/amd64',
            '--privileged','--cgroupns=private','--tmpfs','/run:rw,nosuid,nodev,mode=0755',
            '--tmpfs','/tmp:rw,nosuid,nodev,mode=1777',
            '--mount',f'type=bind,src={repo},dst=/src,readonly',
            '--mount',f'type=bind,src={package},dst=/package,readonly',
            '--mount',f'type=bind,src={old},dst=/old,readonly',IMAGE,'/lib/systemd/systemd',
            'systemd.mask=systemd-binfmt.service'])
        # Wait only for PID1 bus availability, no effect/market polling.
        for _ in range(100):
            q=subprocess.run(ex('systemctl','show','--property=Version'),capture_output=True)
            if q.returncode==0:break
            time.sleep(.1)
        else:raise RuntimeError('PID1 unavailable')
        run('systemd-version',ex('systemctl','--version'))
        run('installer-tests',['docker','exec','-e','O3_INSTALL_PACKAGE=/package','-e','O3_OLD_ELFS=/old',container,
            'python3','-B','/src/scripts/test_stage8b_p1f_o3_install.py'])
        setup='''
import os,pathlib,shutil,json
p=pathlib.Path
for d,mode in [('/usr/local/libexec/moex',0o755),('/etc/moex-finam-p1-paper',0o750),
 ('/var/lib/moex-finam-p1-paper-control',0o750),('/var/lib/moex-finam-p1-paper/state',0o700),
 ('/var/lib/moex-finam-p1-paper-o3-producer',0o700),('/run/moex-finam-p1-o3-input',0o700),
 ('/run/moex-finam-p1-o3-runtime-credentials',0o700)]:
 p(d).mkdir(parents=True,exist_ok=True);p(d).chmod(mode)
for f in p('/package/payload').glob('stage8b-p1f-*'):
 q=p('/usr/local/libexec/moex')/f.name;shutil.copyfile(f,q);q.chmod(0o755)
for name in ('signer','supervisor'):
 q=p('/etc/systemd/system/moex-finam-p1f-o3-'+name+'.service')
 raw=p('/package/payload/'+q.name+'.in').read_text()
 assert raw.count('@PHASE_SHA256@')==1
 q.write_text(raw.replace('@PHASE_SHA256@','0'*64));q.chmod(0o644)
# Inert test bytes only, never accepted keys. Not part of the handoff payload.
for f in ('/run/moex-finam-p1-o3-input/schedule.ed25519',
          '/run/moex-finam-p1-o3-runtime-credentials/stage8b-p1-lifecycle.key'):
 p(f).write_bytes(b'NOT_A_KEY_LOCAL_CUSTODY_TEST');p(f).chmod(0o400)
os.chown('/run/moex-finam-p1-o3-runtime-credentials',987,987)
os.chown('/run/moex-finam-p1-o3-runtime-credentials/stage8b-p1-lifecycle.key',987,987)
os.chown('/var/lib/moex-finam-p1-paper-o3-producer',987,987)
os.chown('/var/lib/moex-finam-p1-paper/state',987,987)
for d in ('/etc/moex-finam-p1-paper','/var/lib/moex-finam-p1-paper-control'):
 os.chown(d,0,987)
for n in ('.execution.lock','.guardian.lock'):
 q=p('/var/lib/moex-finam-p1-paper-control')/n;q.write_bytes(b'');q.chmod(0o600)
'''
        run('group',ex('groupadd','-g','987','moex-p1-paper'))
        run('user',ex('useradd','-u','987','-g','987','-M','-s','/usr/sbin/nologin','moex-p1-paper'))
        run('synthetic-custody-setup',ex('python3','-c',setup))
        for name in r.BINS:
            # The legacy operator maps argument errors to its existing 70;
            # the four bounded O3 entrypoints return Usage/64.
            run('usage-'+name,ex('/usr/local/libexec/moex/'+name),70 if name=='stage8b-p1f-o2-operator' else 64)
        run('syntax',ex('systemd-analyze','verify','/etc/systemd/system/moex-finam-p1f-o3-signer.service',
                       '/etc/systemd/system/moex-finam-p1f-o3-supervisor.service'))
        run('local-daemon-reload',ex('systemctl','daemon-reload'))
        for role in ('signer','supervisor'):
            raw=run('loaded-'+role,ex('systemctl','show','moex-finam-p1f-o3-'+role+'.service',
                '--property=ActiveState,MainPID,User,Group,Restart,ExecStart,LoadCredential,KillMode,TimeoutStopUSec,CapabilityBoundingSet,InaccessiblePaths,Environment,PrivateNetwork'))
            v=dict(line.split('=',1) for line in raw.decode().splitlines())
            assert v['ActiveState']=='inactive' and v['MainPID']=='0' and v['Restart']=='no' and v['User']=='root'
        base=ex('setpriv','--reuid=987','--regid=987','--clear-groups')
        run('paper-key-unreadable',base+['test','-r','/run/moex-finam-p1-o3-input/schedule.ed25519'],1)
        run('paper-lifecycle-readable',base+['test','-r','/run/moex-finam-p1-o3-runtime-credentials/stage8b-p1-lifecycle.key'])
        for n in r.BINS:
            run('paper-executable-not-writable-'+n,base+['test','-w','/usr/local/libexec/moex/'+n],1)
        # Minimal image has systemd's private control socket, not a system
        # dbus daemon. Type=exec start acknowledges execve only; separately
        # wait for and assert the actual negative child exit, never infer it
        # from the systemctl start return code.
        run('signer-start',ex('systemctl','start','moex-finam-p1f-o3-signer.service'),[0,1])
        for _ in range(100):
            state=subprocess.run(ex('systemctl','show','moex-finam-p1f-o3-signer.service',
                '--property=ActiveState','--value'),capture_output=True,check=True).stdout.strip()
            if state in (b'failed',b'inactive'):break
            time.sleep(.1)
        else:raise RuntimeError('signer did not terminate')
        run('negative-service-journal',ex('journalctl','--no-pager','-u','moex-finam-p1f-o3-signer.service'))
        raw=run('signer-result',ex('systemctl','show','moex-finam-p1f-o3-signer.service',
            '--property=ActiveState,SubState,MainPID,ExecMainStatus,Result,NRestarts'))
        v=dict(line.split('=',1) for line in raw.decode().splitlines())
        assert v['MainPID']=='0' and v['ActiveState']=='failed' and v['ExecMainStatus']=='255' and v['NRestarts']=='0'
        raw=run('journald-platform-diagnostic',ex('systemctl','show','systemd-journald.service',
            '--property=ActiveState,MainPID,ExecMainStatus,Result'))
        j=dict(line.split('=',1) for line in raw.decode().splitlines())
        assert j['ExecMainStatus']=='255' and j['MainPID']=='0' and j['ActiveState']=='failed'
        run('no-signer-credential-left',ex('test','!','-e','/run/credentials/moex-finam-p1f-o3-signer.service'))
        run('supervisor-never-started',ex('systemctl','show','moex-finam-p1f-o3-supervisor.service','--property=MainPID,NRestarts,ActiveState'))
    finally:
        run('remove',['docker','rm','-f',container])
    assert qualified == {p.relative_to(repo).as_posix():r.b.sha(p.read_bytes()) for p in qualified_paths}, 'qualification sources changed'
    result=dict(result='STATIC_CUSTODY_PASS_DYNAMIC_UNQUALIFIED',records=records,qualified_files=qualified,local_disposable_only=True,private_keys_used=False,
        systemd_pid1_real=True,active_phase_positive_systemd_run=False,operational_o3=False,
        dynamic_service_execution_qualified=False,
        boundary='release CLI/ABI, real filesystem transactions, unit loading; dynamic executor failure 255 also in journald, NOT runtime negative proof')
    r.put(out/'result.json',r.encoded(result))


def retain_partial(attempt,out):
    """Preserve an INCOMPLETE attempt, never promote its failure to a PASS.

    Raw command records include executed Python setup and exact binary paths.
    No claim that the revised runner completed, or that its current bytes were
    the driver for the old attempt. This is diagnostic evidence collation.
    """
    out.mkdir(parents=True,exist_ok=False)
    records=[]
    for p in sorted((attempt/'logs').glob('*.json')):
        v=json.loads(p.read_bytes());raw=p.with_suffix('.log').read_bytes()
        assert r.b.sha(raw)==v['sha256']
        r.put(out/'logs'/p.name,p.read_bytes());r.put(out/'logs'/p.with_suffix('.log').name,raw)
        records.append(v)
    assert (attempt/'result.json').exists() is False
    assert b'ExecMainStatus=255' in (out/'logs/signer-result.log').read_bytes()
    result=dict(result='PARTIAL_DYNAMIC_UNQUALIFIED',records=records,
        evidence_kind='retained_incomplete_attempt_command_records',
        driver_source_binding_claimed=False,local_disposable_only=True,private_keys_used=False,
        systemd_pid1_real=True,active_phase_positive_systemd_run=False,operational_o3=False,
        dynamic_service_execution_qualified=False,
        limitation='No completed qualification run; actual signer setup exit 255, not runtime refusal 70. Native qualification required.')
    r.put(out/'result.json',r.encoded(result))


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('output',type=Path)
    p.add_argument('--package',type=Path);p.add_argument('--old-elfs',type=Path)
    p.add_argument('--retain-partial',type=Path)
    a=p.parse_args()
    if a.retain_partial:retain_partial(a.retain_partial.resolve(),a.output.resolve())
    else:qualify(Path(__file__).resolve().parents[1],a.package.resolve(),a.old_elfs.resolve(),a.output.resolve())
