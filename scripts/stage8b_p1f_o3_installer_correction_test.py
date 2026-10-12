#!/usr/bin/env python3
"""Local Linux filesystem regression runner; no VPS, ELF execution or signing."""
import argparse
import json
from pathlib import Path
import subprocess
import time
import uuid

import stage8b_p1f_o3_release as release

ROOT=Path(__file__).resolve().parents[1]
SOURCES=('scripts/stage8b_p1f_o3_readmission_install.py',
         'scripts/test_stage8b_p1f_o3_readmission_install.py')


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--work',type=Path,required=True)
    p.add_argument('--package',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
    old=json.loads((a.work/'readmission-installer-tests-command.json').read_bytes())
    command=old['command'].copy()
    command[command.index('-v')+1]=str(ROOT)+':/src:ro'
    for i,value in enumerate(command):
        if value.endswith(':/package:ro'):command[i]=str(a.package.resolve())+':/package:ro'
    name='o3-installer-filesystem-'+uuid.uuid4().hex[:12]
    command[2:2]=['--name',name,'--label','moex.scope=local-installer-filesystem']
    sources={n:release.b.sha((ROOT/n).read_bytes()) for n in SOURCES}
    start=time.monotonic();result=None
    try:
        result=subprocess.run(command,capture_output=True,timeout=600)
        log=result.stdout+result.stderr
        release.put(out/'tests.log',log)
        print(log.decode(),end='')
    finally:
        # Only this uniquely named local container, including on timeout.
        cleanup=subprocess.run(['docker','rm','-f',name],capture_output=True,timeout=30)
        check=subprocess.run(['docker','ps','-aq','--filter','name=^/'+name+'$'],capture_output=True,timeout=30)
        removed=check.returncode==0 and not check.stdout.strip()
        record=dict(command=command,exit_code=result.returncode if result else None,
            seconds=round(time.monotonic()-start,3),source_sha256=sources,
            source_unchanged=sources=={n:release.b.sha((ROOT/n).read_bytes()) for n in SOURCES},
            candidate_spec_sha256=release.b.sha((a.package/'spec.json').read_bytes()),
            log_sha256=release.b.sha((out/'tests.log').read_bytes()) if result else None,
            container_removed=removed,cleanup_exit_code=cleanup.returncode,
            network='none',remote_writes=False,private_inputs=False,
            scope='local Linux filesystem, real file/rename/fsync/locks; synthetic host/DBSIZE; no ELF execution or power-crash proof')
        release.put(out/'command.json',release.encoded(record))
        if not removed:raise RuntimeError('local test container cleanup incomplete')
    if not record['source_unchanged']:raise RuntimeError('test sources changed while executing')
    raise SystemExit(result.returncode)


if __name__=='__main__':main()
