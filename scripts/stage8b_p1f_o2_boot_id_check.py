#!/usr/bin/env python3
"""O2 runner boot-ID/sandbox regression. Native mode runs only read probes.

No guardian/operator, FINAM, Redis, claim or bootstrap invocation. Native mode
uses two transient, collected systemd units with the runner's sandbox settings,
but substitutes a bounded Python read probe for ExecStart and omits ExecStopPost.
"""
import argparse
import configparser
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[1]
UNIT='deploy/stage8b-p1e/moex-finam-p1-paper-o2-bootstrap-runner.service'
BOOT='/proc/sys/kernel/random/boot_id'
REQUIRED={
    'Type':'oneshot','User':'root','Group':'moex-p1-paper','UMask':'0027',
    'ExecStart':'/usr/local/libexec/moex/stage8b-p1f-o2-operator runner-fixed',
    'ExecStopPost':'/usr/local/libexec/moex/stage8b-p1f-o2-operator cleanup-fixed',
    'TimeoutStartSec':'infinity','TimeoutStopSec':'45s',
    'KillSignal':'SIGTERM','FinalKillSignal':'SIGKILL','KillMode':'mixed',
    'RemainAfterExit':'no','RestrictAddressFamilies':'AF_UNIX','PrivateNetwork':'yes',
    'NoNewPrivileges':'yes','PrivateDevices':'yes','PrivateTmp':'yes',
    'ProtectClock':'yes','ProtectControlGroups':'yes','ProtectHome':'yes',
    'ProtectHostname':'yes','ProtectKernelLogs':'yes','ProtectKernelModules':'yes',
    'ProtectKernelTunables':'yes','ProtectSystem':'strict','ProtectProc':'invisible',
    'ProcSubset':'all','RestrictRealtime':'yes','RestrictSUIDSGID':'yes',
    'LockPersonality':'yes','MemoryDenyWriteExecute':'yes',
    'CapabilityBoundingSet':'','AmbientCapabilities':'',
    'SystemCallArchitectures':'native','LimitCORE':'0',
    'ReadOnlyPaths':'/etc/moex-finam-p1-paper',
    'ReadWritePaths':'/var/lib/moex-finam-p1-paper-control',
}

def unit_properties(raw):
    config=configparser.ConfigParser(interpolation=None,strict=True)
    config.optionxform=str
    config.read_string(raw)
    if set(config.sections())!={'Unit','Service'} or dict(config['Service'])!=REQUIRED:
        raise ValueError('runner sandbox/command drift')
    if dict(config['Unit'])!={
        'Description':'MOEX FINAM Stage 8B P1F O2 guarded bootstrap runner',
        'After':'local-fs.target',
        'ConditionPathIsReadWrite':'/var/lib/moex-finam-p1-paper-control',
        'ConditionPathExists':'/etc/moex-finam-p1-paper/o2/active-manifest.sha256',
    }:raise ValueError('runner conditions drift')
    return dict(config['Service'])

def bootstrap_properties(raw):
    config=configparser.ConfigParser(interpolation=None,strict=True)
    config.optionxform=str
    config.read_string(raw)
    wanted={k:v for k,v in REQUIRED.items() if k not in (
        'ExecStopPost','TimeoutStartSec','TimeoutStopSec','KillSignal','FinalKillSignal','KillMode')}
    wanted.update(User='moex-p1-paper',UMask='0077',
        LoadCredential='stage8b-p1-lifecycle.key:/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key',
        ExecStart='/usr/local/libexec/moex/stage8b-p1-paper-supervisor bootstrap /etc/moex-finam-p1-paper/supervisor.json CREATE_NEW_STAGE8B_P1_DURABLE_ROOT',
        ReadWritePaths='/var/lib/moex-finam-p1-paper/state')
    if set(config.sections())!={'Unit','Service'} or dict(config['Service'])!=wanted:
        raise ValueError('bootstrap sandbox/command drift')
    if dict(config['Unit'])!={'Description':'MOEX FINAM Stage 8B P1 paper first boot','After':'local-fs.target'}:
        raise ValueError('bootstrap conditions drift')
    return dict(config['Service'])

PROBE=r'''
import errno,hashlib,json,os,socket,sys,uuid
from pathlib import Path
path=Path('/proc/sys/kernel/random/boot_id')
expected=sys.argv[1]
try:
    raw=path.read_bytes()
except FileNotFoundError as e:
    assert expected=='pid' and e.errno==errno.ENOENT
    print(json.dumps({'subset':'pid','read':'NotFound','errno':e.errno}))
    raise SystemExit(0)
assert expected=='all'
assert str(uuid.UUID(raw.decode().strip()))==raw.decode().strip()
status=dict(line.split(':',1) for line in Path('/proc/self/status').read_text().splitlines() if ':' in line)
assert int(status['CapEff'].strip(),16)==0 and int(status['CapBnd'].strip(),16)==0
assert status['NoNewPrivs'].strip()=='1'
try:
    s=socket.socket(socket.AF_INET,socket.SOCK_STREAM)
except OSError as e:
    assert e.errno in (errno.EAFNOSUPPORT,errno.EPERM,errno.EACCES)
else:
    s.close();raise AssertionError('INET socket unexpectedly allowed')
# Inspect mount flags rather than attempting any write to kernel tunables.
rows=[line.split() for line in Path('/proc/self/mountinfo').read_text().splitlines()]
mount=max((r for r in rows if str(path)==r[4] or str(path).startswith(r[4].rstrip('/')+'/')),key=lambda r:len(r[4]))
assert 'ro' in mount[5].split(',')
print(json.dumps({'subset':'all','read':'OK','boot_id_sha256':hashlib.sha256(raw).hexdigest(),
                 'kernel_path_readonly':True,'inet_socket_refused':True,'capabilities_empty':True,'no_new_privileges':True}))
'''

def native_probe(properties):
    if sys.platform!='linux' or os.geteuid()!=0:raise ValueError('native root systemd host required')
    host=hashlib.sha256(Path(BOOT).read_bytes()).hexdigest()
    results=[]
    for subset in ('pid','all'):
        # Only executable/lifetime differs. This is not the O2 runner and cannot
        # call cleanup, write authority or start the actual bootstrap service.
        # No lifecycle credential is loaded by the read-only probe. Production
        # LoadCredential is validated above and is not removed from the unit.
        props={k:v for k,v in properties.items() if k not in ('ExecStart','ExecStopPost','LoadCredential')}
        props.update(ProcSubset=subset,TimeoutStartSec='15s',TimeoutStopSec='5s')
        args=['systemd-run','--quiet','--wait','--pipe','--collect',
              '--unit=moex-o2-boot-id-probe-'+str(os.getpid())+'-'+subset]
        args.extend('--property='+k+'='+v for k,v in props.items())
        args.extend(['/usr/bin/python3','-B','-c',PROBE,subset])
        run=subprocess.run(args,capture_output=True,timeout=30)
        if run.returncode!=0:
            raise RuntimeError('native probe failed: '+subset+' '+run.stderr.decode(errors='replace'))
        result=json.loads(run.stdout)
        if subset=='all' and result['boot_id_sha256']!=host:raise ValueError('wrong boot identity')
        results.append(result)
    return results

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--unit',type=Path,default=ROOT/UNIT)
    parser.add_argument('--native-read-probe',action='store_true')
    args=parser.parse_args()
    properties=unit_properties(args.unit.read_text())
    result={'status':'PASS','runner_unit_sha256':hashlib.sha256(args.unit.read_bytes()).hexdigest(),
            'native_systemd_tested':args.native_read_probe,'o2_invoked':False}
    if args.native_read_probe:result['probe']=native_probe(properties)
    print(json.dumps(result,indent=2))

if __name__=='__main__':main()
