#!/usr/bin/env python3
"""Manual isolated native qualification of accepted 5df1c88 O3 release bytes.

Preparation is offline and public-only. Running requires the ordinary real
weekday window and explicit selected-key authorization; no waiting, scheduling,
clock override, installation, host DB writes or operational O3. Historical
fb1ceb3 success is not reused. This follows the accepted native test recipe.
"""
import argparse
import datetime as dt
import json
import os
from pathlib import Path
import shlex
import shutil
import stat
import subprocess
import sys
import time
import uuid

import stage8b_p1f_o3_native_fixture as base
import stage8b_p1f_o3_readmission_install as installer
import stage8b_p1f_o3_release as release

REDIS='sha256:858f009f9709ce576febc734aa78b8f6d624b82571f9ddb6bda4377c833b3499'
PUBLIC='432a274889bc2b3a4492b3a1510f7356e1c2878337263e28472335f8600e4d99'
SUPPORT={'input-helper':'20594d8506fb39d123449078d0157965df4224e9c4636ab3b1003f17a9e4cd4b',
         'bootstrap-source-witness':'fe8ed5da2106dfb939313daec2c531ae85b55b7922e17a85e904b79268e3c03e'}
SUPPORT_RECORD='cc6c2fead646157b5cf06507fb20ff1eaca84966b3140b02470dd769285e88b3'
OBSERVER='cb53f7779935169fa30f623576b6cbaf86d05001a382f14caaed9a4f9f166783'
ROOT=Path(__file__).resolve().parents[1]
UNITS=base.UNITS
FIXED='/usr/local/libexec/moex/'
CREDS='/run/moex-finam-p1-o3-runtime-credentials'
sha=base.SHA

def selected_seed(KEY):
    m=KEY.lstat()
    assert KEY.resolve()==KEY and stat.S_ISREG(m.st_mode) and m.st_nlink==1
    assert m.st_uid==os.getuid() and m.st_mode & 0o077==0 and m.st_size in (32,64,65)
    raw=KEY.read_bytes();assert KEY.lstat()==m
    seed=bytearray(raw if len(raw)==32 else bytes.fromhex(raw.decode().strip()))
    assert len(seed)==32
    q=subprocess.run([shutil.which('openssl'),'pkey','-inform','DER','-pubout','-outform','DER'],
        input=bytes.fromhex('302e020100300506032b657004220420')+seed,capture_output=True)
    assert q.returncode==0 and q.stdout==bytes.fromhex('302a300506032b6570032100'+PUBLIC)
    return seed


def credential_seed(path):
    try:
        return selected_seed(path)
    except Exception:
        # No local custody pathname or decoder diagnostic in handoff evidence.
        raise ValueError('selected credential validation failed; private details omitted') from None

EXTRA_SETUP='''
import os,pathlib
p=pathlib.Path
for d in ('/etc/moex-finam-p1-paper/o2','/etc/moex-finam-p1-paper/o3','/etc/moex-finam-p1-paper/bootstrap'):
 p(d).mkdir(mode=0o750,parents=True,exist_ok=True);os.chown(d,0,987)
for d in ('/run/o3-local-lifecycle','/run/o3-public'):
 p(d).mkdir(mode=0o700,exist_ok=False);os.chown(d,987,987)
# Base setup's inert credential placeholders are not reused in this positive run.
p('/run/moex-finam-p1-o3-input/schedule.ed25519').unlink()
p('/run/moex-finam-p1-o3-runtime-credentials/stage8b-p1-lifecycle.key').unlink()
for n in ('input-helper','bootstrap-source-witness'):p('/payload/'+n).chmod(0o755)
'''

AFTER_PREPARE='''
import os,pathlib,shutil
p=pathlib.Path
for f in ('/etc/moex-finam-p1-paper/supervisor.json','/etc/moex-finam-p1-paper/o3/composition.json',
 '/etc/moex-finam-p1-paper/o3/phase-manifest.signed.json','/etc/moex-finam-p1-paper/o2/authority-public-key.hex',
 '/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json'):
 os.chown(f,0,987)
source=p('/run/o3-local-lifecycle/stage8b-p1-lifecycle.key')
target=p('/run/moex-finam-p1-o3-runtime-credentials/stage8b-p1-lifecycle.key')
assert len(source.read_bytes())==32
shutil.copyfile(source,target);target.chmod(0o400);os.chown(target,987,987);source.unlink()
phase=p('/run/o3-local-phase.sha256').read_text().strip()
assert len(phase)==64 and all(c in '0123456789abcdef' for c in phase)
for role in ('signer','supervisor'):
 name='moex-finam-p1f-o3-'+role+'.service'
 raw=p('/payload/'+name+'.in').read_text();assert raw.count('@PHASE_SHA256@')==1
 dest=p('/etc/systemd/system')/name;dest.write_text(raw.replace('@PHASE_SHA256@',phase));dest.chmod(0o644)
print(phase)
'''

STAGE_KEY='''
import os,sys
# Only this stdin is private; it is never printed or retained in evidence.
os.umask(0o077)
data=bytearray(sys.stdin.buffer.read(33));assert len(data)==32
fd=os.open('/run/moex-finam-p1-o3-input/schedule.ed25519',os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o400)
try:
 assert os.write(fd,data)==32
 os.fsync(fd)
finally:
 os.close(fd)
 for i in range(len(data)):data[i]=0
print('PASS selected credential staged in container tmpfs; content omitted')
'''

PROCESSES='''
import pathlib,json,subprocess
p=pathlib.Path
units={}
for role in ('signer','supervisor'):
 name='moex-finam-p1f-o3-'+role+'.service'
 raw=subprocess.check_output(['systemctl','show',name,'--property=ActiveState,SubState,MainPID,ExecMainCode,ExecMainStatus,Result,NRestarts,ControlGroup']).decode()
 v=dict(line.split('=',1) for line in raw.splitlines());processes={}
 cg=v['ControlGroup']
 if cg:
  root=p('/sys/fs/cgroup'+cg)
  for file in root.rglob('cgroup.procs'):
   for pid in file.read_text().split():
    try:
     status=(p('/proc')/pid/'status').read_text()
     cmd=(p('/proc')/pid/'cmdline').read_bytes().replace(b'\\0',b' ').decode()
     st=(p('/proc')/pid/'stat').read_text();tail=st[st.rfind(')')+2:].split()
     processes[pid]=dict(command=cmd,start_ticks=tail[19],ppid=tail[1],state=tail[0],
       credentials=[line for line in status.splitlines() if line.startswith(('Uid:','Gid:','Groups:','NoNewPrivs:','Cap'))])
    except FileNotFoundError:pass
 v['processes']=processes;units[role]=v
print(json.dumps(units,sort_keys=True))
'''

def check_window(now):
    """Admission only, never pass a synthetic time to the fixture/runtime."""
    if now.tzinfo is None or now.utcoffset()!=dt.timedelta(0):
        raise ValueError('UTC observation required')
    msk=now+dt.timedelta(hours=3)
    # Ordinary helper bounds; retain a preparation freshness margin.
    if msk.weekday()>=5 or not 7<=now.hour<20:
        raise ValueError('weekday real model window required; no remote/key action')
    position=now.timestamp()%600
    if not 360<=position<480:
        raise ValueError('fresh-bootstrap slot required (minute 06..07); no automatic wait')
    return dict(utc=now.isoformat(),slot_second=position)


def inputs(package,support,observer):
    spec,baseline,old,payload=installer.load(package)
    raw=installer.package_file(support/'support-build.json')
    if sha(raw)!=SUPPORT_RECORD:raise ValueError('exact separate support record required')
    record=json.loads(raw)
    if (record['source_ref']!=release.READMISSION_REF or record['source_tree']!=release.READMISSION_TREE
            or record['status']!='SUPPORT_BUILT_NOT_RELEASE'
            or not record['container_removed'] or not record['remote_staging_removed']):
        raise ValueError('support provenance/scope drift')
    result={Path(n).name:payload[n] for n in installer.ELFS}
    old_o3=json.loads(installer.package_file(package/'before-o3.json'))
    for n in installer.prior.TEMPLATES:
        raw=installer.package_file(ROOT/'deploy/stage8b-p1f-o3'/Path(n).name)
        if sha(raw)!=old_o3['payload'][n]['sha256']:raise ValueError('accepted template drift')
        result[Path(n).name]=raw
    for name,digest in SUPPORT.items():
        raw=installer.package_file(support/'payload'/name)
        if sha(raw)!=digest:raise ValueError('qualification support ELF drift')
        result[name]=raw
    raw=installer.package_file(observer)
    if sha(raw)!=OBSERVER:raise ValueError('exact read-only host collector required')
    compile(raw,'host-collector.py','exec')
    return spec,baseline,result,raw


def check_host(value,baseline):
    if not (value['status']=='FRESH_PREFLIGHT_PASS' and value['redis_db15_size']==0
            and value['ntp_synchronized'] is True and value['remote_writes'] is False
            and value['private_bytes_read'] is False):
        raise ValueError('read-only stopped host preflight failed')
    if {k:value[k] for k in installer.BASELINE_FIELDS}!=baseline:
        raise ValueError('accepted host baseline drift')


def private_output(process):
    expected=b'PASS selected credential staged in container tmpfs; content omitted\n'
    valid=process.returncode==0 and process.stdout==expected and not process.stderr
    # Unexpected bytes are neither displayed nor persisted; never fabricate PASS.
    return (expected if valid else b'PRIVATE_TRANSFER_OUTPUT_OMITTED\n', b'', valid)


def main(argv=None):
    if not __debug__:raise RuntimeError('qualification checks require non-optimized Python')
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('action',choices=('prepare','run'));ap.add_argument('package',type=Path)
    ap.add_argument('output',type=Path);ap.add_argument('--support',type=Path,required=True)
    ap.add_argument('--observer',type=Path,required=True)
    ap.add_argument('--authorized-selected-key-native-test',action='store_true')
    ap.add_argument('--schedule-key',type=Path,help='only for run; never recorded in evidence')
    a=ap.parse_args(argv)
    if a.action=='run':
        if not a.authorized_selected_key_native_test or a.schedule_key is None:
            raise ValueError('explicit isolated-test/key authorization required')
        check_window(dt.datetime.now(dt.timezone.utc))  # Before SSH, mkdir or key read.
    elif a.schedule_key is not None or a.authorized_selected_key_native_test:
        raise ValueError('offline preparation must not receive a private key argument')
    spec,baseline,payload,collector=inputs(a.package,a.support,a.observer)
    out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
    (out/'logs').mkdir();(out/'payload').mkdir()
    release.put(out/'driver.py',Path(__file__).read_bytes())
    release.put(out/'fixture-setup.py',Path(base.__file__).read_bytes())
    release.put(out/'host-collector.py',collector)
    for name,raw in payload.items():release.put(out/'payload'/name,raw)
    names=tuple(payload);hashes={name:sha(raw) for name,raw in payload.items()}
    if a.action=='prepare':
        record=dict(status='PREPARED_ONLY_NOT_QUALIFIED',source_ref=release.READMISSION_REF,
            production_build_sha256=installer.BUILD,support_build_sha256=SUPPORT_RECORD,
            installation_spec_sha256=sha(installer.canonical(spec)),payload_sha256=hashes,
            driver_sha256=sha(Path(__file__).read_bytes()),observer_sha256=OBSERVER,
            fixture_setup_sha256=sha(Path(base.__file__).read_bytes()),
            remote_contact=False,private_key_read=False,signature_issued=False,
            automatic_launch_scheduled=False,operational_o3=False,installation_performed=False)
        release.put(out/'preparation.json',release.encoded(record))
        print(json.dumps(record,sort_keys=True));return
    name='moex-o3-readmission-qual-'+uuid.uuid4().hex[:12];redis=name+'-redis';remote='/var/tmp/'+name
    ssh=base.SSH.copy();ssh[-1:-1]=['-o','ControlMaster=auto','-o','ControlPath=/tmp/'+name,'-o','ControlPersist=60']
    records=[]
    result=dict(status='INCOMPLETE',source_ref=release.READMISSION_REF,
        production_build_sha256=installer.BUILD,support_build_sha256=SUPPORT_RECORD,
        installation_spec_sha256=sha(installer.canonical(spec)),observer_sha256=OBSERVER,
        fixture_setup_sha256=sha(Path(base.__file__).read_bytes()),
        driver_sha256=sha(Path(__file__).read_bytes()),payload_sha256=hashes,support_not_release=list(SUPPORT),
        systemd_image=base.IMAGE,redis_image=REDIS,native=True,operational_o3=False,
        installation_performed=False,host_state_used=False,rust_cargo_changed=False,
        automatic_launch_scheduled=False,
        selected_schedule_public_key=PUBLIC,schedule_key_transferred=False,
        credential_staging='SSH stdin -> private container /run tmpfs only',
        positive_phase_run=False,live_child_stop_proven=False,containers_removed=False)
    def run(label,args,expect=(0,),stdin=None,private=False,timeout=90):
        command=[*ssh,shlex.join(args)];started=time.monotonic()
        try:
            q=subprocess.run(command,input=stdin,capture_output=True,timeout=timeout)
        except subprocess.TimeoutExpired as exc:
            q=subprocess.CompletedProcess(command,124,exc.stdout or b'',exc.stderr or b'')
        # Private transfer diagnostics/input digests are never retained.
        private_valid=None
        if private:
            stdout,stderr,private_valid=private_output(q)
            q=subprocess.CompletedProcess(command,q.returncode,stdout,stderr)
        (out/'logs'/(label+'.stdout')).write_bytes(q.stdout)
        (out/'logs'/(label+'.stderr')).write_bytes(q.stderr)
        r=dict(id=label,command=command,exit_code=q.returncode,expected=list(expect),
            seconds=round(time.monotonic()-started,3),stdout_sha256=sha(q.stdout),stderr_sha256=sha(q.stderr),
            stdin_sha256=sha(stdin) if stdin is not None and not private else None,private_stdin_omitted=private,
            private_output_valid=private_valid)
        records.append(r);(out/'logs'/(label+'.json')).write_text(json.dumps(r,indent=2)+'\n')
        if q.returncode not in expect or (private and not private_valid):
            raise RuntimeError(label+': exit '+str(q.returncode)+'; see public logs')
        print(label+': exit '+str(q.returncode),flush=True);return q.stdout
    ex=lambda *v:['docker','exec',name,*v]
    py=lambda label,code:run(label,ex('python3','-c',code))
    uid=lambda *v:ex('setpriv','--reuid=987','--regid=987','--clear-groups',*v)
    rc=lambda label,*v:run(label,['docker','exec',redis,'redis-cli','--json','-n','15',*v])
    created=redis_created=staged=False;before=None;protected_before=None
    try:
        protected_before=json.loads(run('protected-before',['python3','-B','-'],stdin=collector))
        check_host(protected_before,baseline)
        before=json.loads(run('host-global-before',['python3','-c',base.GLOBAL_STATE]))
        remote_now=int(run('real-clock',['date','-u','+%s']).strip())
        if abs(dt.datetime.now(dt.timezone.utc).timestamp()-remote_now)>10:raise ValueError('clock disagreement')
        check_window(dt.datetime.fromtimestamp(remote_now,dt.timezone.utc))
        assert run('machine',['uname','-m']).strip()==b'x86_64'
        assert len(run('swap-disabled',['cat','/proc/swaps']).splitlines())==1
        run('images',['docker','image','inspect',base.IMAGE,REDIS])
        key=credential_seed(a.schedule_key)
        for i in range(len(key)):key[i]=0
        # Mark owned resources attempted before effects: ambiguous SSH exits still clean them.
        staged=True;run('staging',['mkdir','-m','0700',remote])
        run('public-payload-directory',['mkdir','-m','0700',remote+'/payload'])
        for filename,raw in payload.items():
            code="import os,sys; fd=os.open("+repr(remote+'/payload/'+filename)+",os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600); w=os.fdopen(fd,'wb'); w.write(sys.stdin.buffer.read()); w.flush(); os.fsync(w.fileno()); w.close()"
            run('public-transfer-'+filename,['python3','-c',code],stdin=raw,timeout=120)
        created=True
        run('create',['docker','create','--pull=never','--name',name,'--hostname','o3-native-positive','--network','none',
            '--label','moex.scope=o3-native-positive','--cgroupns','private','--cpus','1','--memory','768m',
            '--memory-swap','768m','--pids-limit','128','--ulimit','core=0','--cap-add','SYS_ADMIN','--cap-add','NET_ADMIN',
            '--cap-add','BPF','--cap-add','PERFMON','--security-opt','seccomp=unconfined',
            '--security-opt','apparmor=unconfined','--tmpfs','/run:rw,nosuid,nodev,mode=0755,size=64m',
            '--tmpfs','/tmp:rw,nosuid,nodev,mode=1777,size=32m',base.IMAGE,'/bin/sh','-c',
            # First detach all propagation from the host; then establish shared
            # mounts only inside this no-bind container for systemd credentials.
            # Diagnosed with a dummy credential, not a production qualification.
            'mount --make-rprivate / && mount --make-rshared / && '
            'mount -o remount,rw /sys/fs/cgroup && exec /lib/systemd/systemd '
            'systemd.mask=systemd-binfmt.service systemd.mask=systemd-modules-load.service systemd.mask=systemd-udevd.service'])
        created=True;run('copy-public',['docker','cp',remote+'/payload',name+':/payload'])
        run('start-container',['docker','start',name])
        v=json.loads(run('isolation',['docker','inspect',name]))[0]
        h=v['HostConfig'];assert not h['Privileged'] and not h['Binds'] and h['NetworkMode']=='none' and h['CgroupnsMode']=='private'
        assert h['NanoCpus']==1000000000 and h['Memory']==h['MemorySwap']==805306368
        assert h['RestartPolicy']['Name']=='no' and all(m['Type']=='tmpfs' for m in v['Mounts'])
        for i in range(20):
            if run('ready-'+str(i),ex('systemctl','show','--property=Version'),expect=(0,1)).startswith(b'Version='):break
            time.sleep(.2)
        else:raise RuntimeError('PID1 not ready')
        # Independent upper bound if Mac/SSH disappears: ephemeral container PID1 exits.
        # No host timer/service is installed. Normal test removes this watchdog too.
        run('six-minute-backstop',['docker','exec','-d',name,'sh','-c','sleep 360; systemctl poweroff --force --force'])
        run('group',ex('groupadd','-g','987','moex-p1-paper'))
        run('user',ex('useradd','-u','987','-g','987','-M','-s','/usr/sbin/nologin','moex-p1-paper'))
        py('base-setup',base.SETUP);py('extra-setup',EXTRA_SETUP)
        actual=run('exact-release-hashes',ex('sha256sum',*(FIXED+n for n in base.BINS))).decode().splitlines()
        assert {Path(line.split()[1]).name:line.split()[0] for line in actual}=={n:hashes[n] for n in base.BINS}
        redis_created=True
        run('redis-create',['docker','run','--pull=never','-d','--name',redis,'--network','container:'+name,
            '--cpus','0.25','--memory','128m','--memory-swap','128m','--pids-limit','32','--ulimit','core=0','--tmpfs','/data:rw,nosuid,nodev,size=32m',
            REDIS,'redis-server','--bind','127.0.0.1','--save','','--appendonly','no'])
        redis_created=True
        for i in range(30):
            if rc('redis-ready-'+str(i),'PING').strip()==b'"PONG"':break
            time.sleep(.1)
        else:raise RuntimeError('fresh isolated Redis unavailable')
        assert json.loads(rc('redis-empty','DBSIZE'))==0
        # Recheck actual native time before minting fresh synthetic inputs.
        remote_now=int(run('fixture-real-clock',['date','-u','+%s']).strip())
        check_window(dt.datetime.fromtimestamp(remote_now,dt.timezone.utc))
        # Local fixture-only input generator, never writes the accepted VPS root.
        run('fixture-inputs',ex('/payload/input-helper','prepare-linked'))
        phase=py('fixture-public-custody',AFTER_PREPARE).decode().strip()
        result['synthetic_phase_sha256']=phase
        py('fixture-provenance',"import pathlib; print(pathlib.Path('/run/o3-public/local-fixture-provenance.json').read_text())")
        run('production-bootstrap',uid('env','CREDENTIALS_DIRECTORY='+CREDS,'/payload/bootstrap-source-witness',
            'bootstrap','/etc/moex-finam-p1-paper/supervisor.json','CREATE_NEW_STAGE8B_P1_DURABLE_ROOT'))
        actual=run('pre-start-exact-release-hashes',ex('sha256sum',*(FIXED+n for n in base.BINS))).decode().splitlines()
        assert {Path(line.split()[1]).name:line.split()[0] for line in actual}=={n:hashes[n] for n in base.BINS}
        key=credential_seed(a.schedule_key)
        result['schedule_key_transfer_attempted']=True
        try:
            run('selected-key-tmpfs',['docker','exec','-i',name,'python3','-c',STAGE_KEY],stdin=key,private=True)
            result['schedule_key_transferred']=True
        finally:
            for i in range(len(key)):key[i]=0
        run('input-schedule-unreadable',uid('test','-r','/run/moex-finam-p1-o3-input/schedule.ed25519'),expect=(1,))
        run('lifecycle-readable',uid('test','-r',CREDS+'/stage8b-p1-lifecycle.key'))
        run('unit-syntax',ex('systemd-analyze','verify',*('/etc/systemd/system/'+u for u in UNITS)))
        run('container-reload',ex('systemctl','daemon-reload'))
        run('loaded-units',ex('systemctl','show',*UNITS,'--property=User,Group,ExecStart,LoadCredential,Restart,KillMode,TimeoutStopUSec,PrivateNetwork,IPAddressDeny,IPAddressAllow'))
        run('start-signer',ex('systemctl','start',UNITS[0]))
        for i in range(30):
            ready=json.loads(py('signer-ready-'+str(i),"import pathlib,json,subprocess; print(json.dumps(dict(socket=pathlib.Path('/run/moex-finam-p1-o3/schedule-signer.sock').is_socket(),status=subprocess.check_output(['systemctl','show','"+UNITS[0]+"','--property=ActiveState','--value']).decode().strip())))"))
            if ready['socket'] and ready['status']=='active':break
            if ready['status'] in ('failed','inactive'):raise RuntimeError('signer startup refused')
            time.sleep(.1)
        else:raise RuntimeError('signer readiness timeout')
        initial=run('real-signer-production-checkpoint',uid(FIXED+'stage8b-p1f-o3-composition',
            'prepare-initial-schedule',phase,'PREPARE-FRESH-O3-SCHEDULE'))
        # Public signed envelope only. HMAC/schedule credential never copied out.
        code="import os,sys; b=sys.stdin.buffer.read(); fd=os.open('/etc/moex-finam-p1-paper/o3/initial-schedule.json',os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o440); os.fchown(fd,0,987); assert os.write(fd,b)==len(b); os.close(fd)"
        run('install-public-envelope',['docker','exec','-i',name,'python3','-c',code],stdin=initial)
        run('provision-fresh-redis',ex(FIXED+'stage8b-p1f-o3-provisioner','initialize-fixed',phase,'INITIALIZE-FRESH-O3-DB15'))
        assert json.loads(rc('redis-provisioned','DBSIZE'))==12
        run('start-supervisor',ex('systemctl','start',UNITS[1]))
        for i in range(30):
            active=json.loads(py('active-processes-'+str(i),PROCESSES))
            s=active['supervisor'];children=[v for pid,v in s['processes'].items() if pid!=s['MainPID']]
            if active['signer']['ActiveState']=='active' and s['ActiveState']=='active' and children:break
            if s['ActiveState'] in ('failed','inactive'):raise RuntimeError('supervisor refused before positive readiness')
            time.sleep(.1)
        else:raise RuntimeError('no live composition child')
        assert len(children)==1 and 'stage8b-p1f-o3-composition run '+phase in children[0]['command']
        assert any(line.startswith('Uid:') and line.split()[1:]==['987']*4 for line in children[0]['credentials'])
        child_status=dict(line.split(':',1) for line in children[0]['credentials'])
        assert all(int(child_status[n],16)==0 for n in ('CapEff','CapPrm','CapAmb','CapInh'))
        assert child_status['NoNewPrivs'].strip()=='1'
        # Give actual runtime/producer startup time to fail; mere execve is not readiness.
        time.sleep(8)
        active2=json.loads(py('active-stable',PROCESSES))
        for role in ('signer','supervisor'):
            assert active2[role]['ActiveState']=='active' and active2[role]['NRestarts']=='0'
            assert set(active2[role]['processes'])==set(active[role]['processes'])
        result['positive_phase_run']=True
        rc('redis-observation-before-stop','XINFO','GROUPS','finam_imoexf_paper:{finam-imoexf-p1}:market-data:m10')
        started=time.monotonic()
        run('bounded-supervisor-stop',ex('systemctl','stop',UNITS[1]),timeout=45)
        elapsed=time.monotonic()-started;assert elapsed<42
        stopped=json.loads(py('stopped-processes',PROCESSES))
        s=stopped['supervisor'];assert s['MainPID']=='0' and not s['processes'] and s['ExecMainStatus']=='0'
        run('bounded-signer-stop',ex('systemctl','stop',UNITS[0]),timeout=10)
        stopped=json.loads(py('all-units-stopped',PROCESSES))
        assert all(v['MainPID']=='0' and not v['processes'] and v['NRestarts']=='0' for v in stopped.values())
        assert stopped['signer']['ExecMainCode']=='1' and stopped['signer']['ExecMainStatus']=='70'
        journal=run('supervisor-journal',ex('journalctl','--no-pager','-o','cat','-u',UNITS[1]))
        signer_journal=run('signer-journal',ex('journalctl','--no-pager','-o','cat','-u',UNITS[0]))
        assert b'proceeding without' not in (journal+signer_journal).lower(), 'unit sandbox degraded'
        reports=[]
        for line in journal.splitlines():
            try:v=json.loads(line)
            except ValueError:continue
            if isinstance(v,dict) and v.get('domain')=='stage8b-p1f-local-supervision-result-v1':reports.append(v)
        assert len(reports)==1
        report=reports[0]
        assert report['disposition']=='GRACEFUL_STOP' and not report['force_killed'] and report['child_starts']==1 and report['last_exit_code']==0
        result.update(live_child_stop_proven=True,supervisor_stop_seconds=round(elapsed,3),supervision_result=report)
        # Verify specific pre-stop process identities disappeared, not just MainPID.
        identities={pid:v['start_ticks'] for role in active2.values() for pid,v in role['processes'].items()}
        code="import pathlib,json; ids="+repr(identities)+"; survivors={};\nfor pid,ticks in ids.items():\n p=pathlib.Path('/proc')/pid/'stat'\n if p.exists():\n  raw=p.read_text(); tail=raw[raw.rfind(')')+2:].split()\n  if tail[19]==ticks: survivors[pid]=tail[0]\nprint(json.dumps(survivors)); assert not survivors"
        py('all-recorded-identities-gone',code)
        # Destroy both tmpfs credential copies before removing the container.
        py('delete-selected-input',"import pathlib; pathlib.Path('/run/moex-finam-p1-o3-input/schedule.ed25519').unlink(); assert not pathlib.Path('/run/credentials/moex-finam-p1f-o3-signer.service').exists(); print('PASS no staged schedule credential or systemd credential directory')")
        result['selected_key_removed']=True
        result['status']='NATIVE_SYSTEMD_START_STOP_PASS_NOT_OPERATIONAL_O3'
    except Exception as exc:
        result['status']='FAILED_NOT_QUALIFIED'
        result['error']=str(exc);raise
    finally:
        cleanup=[]
        if created:
            for role in ('signer','supervisor'):
                try:run('final-journal-'+role,ex('journalctl','--no-pager','-o','cat','-u','moex-finam-p1f-o3-'+role+'.service'),expect=(0,1))
                except Exception as exc:cleanup.append('journal: '+str(exc))
        # Independent cleanup actions: a failed diagnostic must not prevent key destruction.
        for container,exists in ((redis,redis_created),(name,created)):
            if exists:
                try:
                    run('remove-'+container,['docker','rm','-f',container],expect=(0,1))
                    listed=run('confirm-removed-'+container,['docker','ps','-aq','--filter','name=^/'+container+'$'])
                    assert not listed.strip(),'task container remains'
                except Exception as exc:cleanup.append(str(exc))
        result['containers_removed']=not cleanup or all(x.startswith('journal:') for x in cleanup)
        if staged:
            try:
                code="import pathlib,re,shutil; p=pathlib.Path("+repr(remote)+"); assert p.parent==pathlib.Path('/var/tmp') and re.fullmatch(r'moex-o3-readmission-qual-[0-9a-f]{12}',p.name); assert not p.is_symlink(); shutil.rmtree(p) if p.exists() else None; assert not p.exists()"
                run('remove-public-staging',['python3','-c',code])
                result['remote_public_staging_removed']=True
            except Exception as exc:cleanup.append(str(exc))
        try:
            after=json.loads(run('host-global-after',['python3','-c',base.GLOBAL_STATE]))
            result['host_binfmt_and_container_inventory_unchanged']=before==after
            assert before==after
        except Exception as exc:cleanup.append('global comparison: '+str(exc))
        try:
            after=json.loads(run('protected-after',['python3','-B','-'],stdin=collector))
            check_host(after,baseline)
            assert protected_before is not None
            for k in (*installer.BASELINE_FIELDS,'public_seal'):assert after[k]==protected_before[k]
            result['protected_host_unchanged']=True
        except Exception as exc:cleanup.append('protected comparison: '+str(exc))
        if cleanup:result['status']='CLEANUP_OR_POSTFLIGHT_FAILED_NOT_QUALIFIED'
        result['cleanup_notes']=cleanup;result['records']=records
        (out/'result.json').write_text(json.dumps(result,indent=2)+'\n')
        if cleanup:raise RuntimeError('inspect cleanup_notes; no clean-completion claim')
    print(json.dumps({k:v for k,v in result.items() if k!='records'},indent=2))

if __name__=='__main__':main()
