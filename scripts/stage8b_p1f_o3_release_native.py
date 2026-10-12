#!/usr/bin/env python3
"""Owner-authorized, temporary native build of accepted O3 readmission source.

No installed project paths, credentials, networked Cargo, service actions or
operational phase. Only public source/cache cross SSH. Every remote write is
inside a unique temporary build directory or its labelled build container.
"""
import argparse
import gzip
import io
import json
from pathlib import Path, PurePosixPath
import re
import shlex
import stat
import subprocess
import tarfile
import time
import uuid

import stage8b_p1f_o3_release as r
import stage8b_p1f_o3_source_handoff as h
from stage8b_p1f_o3_collect_readonly import SSH


class TransportFailure(RuntimeError):
    """Only SSH/observer timeout, not a compiler or Docker exit code."""


def read_container_state(run, name, number):
    # Retry a read only. Never replay docker create/start, transfers or signing.
    for attempt in range(3):
        label = 'state-'+str(number)+('-retry-'+str(attempt) if attempt else '')
        try:
            return json.loads(run(label, ['docker', 'inspect', name,
                                         '--format', '{{json .State}}'], timeout=30))
        except TransportFailure:
            if attempt == 2:
                raise
            time.sleep(5)


def tar_members(archive):
    members = archive.getmembers()
    names = set()
    for m in members:
        p = PurePosixPath(m.name)
        if (p.is_absolute() or '..' in p.parts or '\\' in m.name
                or m.name in names or not (m.isfile() or m.isdir())):
            raise ValueError('unsafe or duplicate public archive member')
        names.add(m.name)
    return members


HELPER_SOURCE = 'crates/finam-gateway/examples/o3_local_signer_witness.rs'


def helper_source(ref):
    full = r.b.git('rev-parse', ref+'^{commit}').decode().strip()
    changed = r.b.git('diff','--name-only',r.READMISSION_REF,full,'--',
                      'crates','Cargo.toml','Cargo.lock').decode().splitlines()
    if changed != [HELPER_SOURCE]:
        raise ValueError('helper rebuild permits only the disposable example delta')
    return full, r.b.git('rev-parse',full+'^{tree}').decode().strip()


def public_inputs(out, cache, source_ref=None, source_tree=None):
    source_ref = source_ref or r.READMISSION_REF
    source_tree = source_tree or r.READMISSION_TREE
    if r.b.git('rev-parse', source_ref+'^{tree}').decode().strip() != source_tree:
        raise ValueError('accepted source/tree binding')
    source = r.b.git('archive', '--format=tar', source_ref)
    files, modes = {}, {}
    with tarfile.open(fileobj=io.BytesIO(source)) as archive:
        for m in tar_members(archive):
            if not m.isfile():
                continue
            h.safe_name(m.name)
            files[m.name] = archive.extractfile(m).read()
            modes[m.name] = '100755' if m.mode & 0o111 else '100644'
    if h.tree_hash(files, modes) != source_tree:
        raise ValueError('exported source tree differs from acceptance')
    r.put(out/'source.tar.gz', gzip.compress(source, mtime=0))
    r.put(out/'source-commit.raw', r.b.git('cat-file', 'commit', source_ref))
    index = {n:dict(sha256=r.b.sha(b), size=len(b), mode=modes[n]) for n,b in files.items()}
    r.put(out/'source-index.json', r.encoded(index))
    # Never include Cargo home config, credentials or a previous target tree.
    registry = cache/'cargo-home/registry'
    if registry.is_symlink() or not registry.is_dir():
        raise ValueError('missing/linked public registry')
    paths = sorted(registry.rglob('*'))
    for p in paths:
        s = p.lstat()
        if not (stat.S_ISREG(s.st_mode) or stat.S_ISDIR(s.st_mode)):
            raise ValueError('nonregular registry node')
        parts = p.relative_to(registry).parts
        if parts[0] not in ('src', 'cache', 'index', 'CACHEDIR.TAG'):
            raise ValueError('nonregistry Cargo data')
        if len(parts) > 1 and not parts[1].startswith('index.crates.io-'):
            raise ValueError('non-public Cargo registry')
    with tarfile.open(out/'registry.tar.gz', 'x:gz', dereference=False) as archive:
        for p in paths:
            info = archive.gettarinfo(str(p), arcname='registry/'+p.relative_to(registry).as_posix())
            info.uid = info.gid = 0
            info.uname = info.gname = ''
            if info.isfile():
                with p.open('rb') as f:
                    archive.addfile(info, f)
            else:
                archive.addfile(info)
    pins = {n:r.b.sha((out/n).read_bytes()) for n in ('source.tar.gz','source-index.json','registry.tar.gz')}
    r.put(out/'inputs.json', r.encoded(dict(source_ref=source_ref,
        source_tree=source_tree, public_input_sha256=pins,
        source_files=len(files), private_inputs=False)))
    return pins


# Executed in the network-none, resource-limited build container, not on host.
BUILD_TASK = '''import json,subprocess,time,pathlib,hashlib,sys
p=pathlib.Path
out=p('/target/evidence');out.mkdir()
records=[]
def run(name,args,timeout):
 start=time.monotonic()
 with (out/(name+'.log')).open('xb') as f:
  try:code=subprocess.run(args,stdout=f,stderr=subprocess.STDOUT,timeout=timeout).returncode
  except subprocess.TimeoutExpired:code=124
 raw=(out/(name+'.log')).read_bytes()
 record=dict(command=args,exit_code=code,seconds=round(time.monotonic()-start,3),log_sha256=hashlib.sha256(raw).hexdigest())
 (out/(name+'.json')).write_text(json.dumps(record,sort_keys=True,indent=2)+'\\n')
 records.append(record)
 if code:raise RuntimeError(name+' exit '+str(code))
try:
 run('toolchain',['rustc','--version','--verbose'],30)
 run('cargo-version',['cargo','--version'],30)
 steps=json.loads(p('/build-steps.json').read_text())
 for step in steps:run(step['name'],step['command'],3600)
 names=json.loads(p('/build-bins.json').read_text())
 for name,path in names.items():run('identity-'+name,['readelf','-h','-d','-V',path],30)
except Exception as exc:
 (out/'status.json').write_text(json.dumps(dict(status='FAILED',error=str(exc),records=records)))
 sys.exit(1)
(out/'status.json').write_text(json.dumps(dict(status=('FIXTURE_HELPER_BUILT_NOT_RELEASE' if steps[0]['name']=='fixture-regression' else 'SUPPORT_BUILT_NOT_RELEASE' if steps[0]['name']=='production-support-build' else 'BUILT_NOT_INSTALLED'),records=records)))
'''


def build_plan(support=False, fixture_only=False):
    common=['cargo','build','--release','--locked','--offline','--no-default-features',
            '--jobs','1','--message-format=json']
    if fixture_only:
        if support: raise ValueError('helper-only and historical support are separate')
        test=['cargo','test','--release','--locked','--offline','--no-default-features','--jobs','1',
              '-p','finam-gateway','--example','o3_local_signer_witness','marker_regression','--','--nocapture']
        return [dict(name='fixture-regression',command=test),
                dict(name='fixture-input-build',command=common+['-p','finam-gateway','--example','o3_local_signer_witness'])], {
                    'input-helper':'/target/release/examples/o3_local_signer_witness'}
    if support:
        return [dict(name='production-support-build',command=common+[
            '-p','runtime-durable-service','--bin','stage8b-p1-paper-supervisor']),
            dict(name='fixture-input-build',command=common+[
                '-p','finam-gateway','--example','o3_local_signer_witness'])], {
            'bootstrap-source-witness':'/target/release/stage8b-p1-paper-supervisor',
            'input-helper':'/target/release/examples/o3_local_signer_witness'}
    cargo=common+['-p','finam-gateway','-p','runtime-durable-service']
    for name in r.BINS:cargo+=['--bin',name]
    return [dict(name='production-build',command=cargo)], {n:'/target/release/'+n for n in r.BINS}


def build(out, cache, support=False, fixture_ref=None):
    if support and fixture_ref: raise ValueError('choose one support profile')
    fixture_only = fixture_ref is not None
    source_ref,source_tree = helper_source(fixture_ref) if fixture_only else (r.READMISSION_REF,r.READMISSION_TREE)
    out.mkdir(parents=True, exist_ok=False)
    (out/'logs').mkdir()
    r.put(out/'driver.py', Path(__file__).read_bytes())
    pins = public_inputs(out, cache, source_ref, source_tree)
    name = 'moex-o3-readmission-build-'+uuid.uuid4().hex[:12]
    remote = '/var/tmp/'+name
    records = []
    staged = created = False
    steps,binary_paths=build_plan(support,fixture_only)
    result = dict(status='INCOMPLETE',source_ref=source_ref,source_tree=source_tree,
        build_kind='fixture_helper_correction' if fixture_only else 'qualification_support' if support else 'production_release',
        image=r.b.IMAGE,remote_directory=remote,container=name,private_inputs=False,
        installation_performed=False,operational_o3_invoked=False,network='none',
        cpu_limit=1,memory_limit_bytes=2147483648,memory_swap_limit_bytes=2147483648,
        remote_staging_removed=False,container_removed=False)

    def run(label,args,stdin=None,timeout=180,expected=(0,)):
        command=[*SSH,shlex.join(args)]
        started=time.monotonic()
        try:
            q=subprocess.run(command,input=stdin,capture_output=True,timeout=timeout)
            stdout,stderr,code=q.stdout,q.stderr,q.returncode
        except subprocess.TimeoutExpired as exc:
            stdout,stderr,code=exc.stdout or b'',exc.stderr or b'',124
        r.put(out/'logs'/(label+'.stdout'),stdout)
        r.put(out/'logs'/(label+'.stderr'),stderr)
        record=dict(id=label,command=command,exit_code=code,seconds=round(time.monotonic()-started,3),
            stdin_sha256=r.b.sha(stdin) if stdin is not None else None,
            stdout_sha256=r.b.sha(stdout),stderr_sha256=r.b.sha(stderr))
        records.append(record);r.put(out/'logs'/(label+'.json'),r.encoded(record))
        if code not in expected:
            failure = TransportFailure if code in (255, 124) else RuntimeError
            raise failure(label+' failed, exit '+str(code)+'; public logs retained')
        print('PASS '+label,flush=True)
        return stdout

    def py(label,code,timeout=180):
        r.put(out/'logs'/(label+'.py'),code.encode())
        return run(label,['python3','-B','-'],code.encode(),timeout)

    def collect(label):
        raw=py(label,f'''import pathlib,tarfile,io,sys
root=pathlib.Path({remote!r})/'target'
b=io.BytesIO()
with tarfile.open(fileobj=b,mode='w:gz') as t:
 for p in sorted((root/'evidence').glob('*')):
  assert p.is_file() and not p.is_symlink();t.add(p,arcname='evidence/'+p.name,recursive=False)
 for name,path in {binary_paths!r}.items():
  p=root/path.removeprefix('/target/')
  if p.is_file() and not p.is_symlink():t.add(p,arcname='payload/'+name,recursive=False)
sys.stdout.buffer.write(b.getvalue())
''',timeout=300)
        r.put(out/'outputs.tar.gz',raw)
        with tarfile.open(fileobj=io.BytesIO(raw)) as a:
            for m in tar_members(a):
                if not m.isfile() or len(PurePosixPath(m.name).parts)!=2:
                    raise ValueError('unexpected build output member')
                if not (m.name.startswith('evidence/') or m.name in {'payload/'+n for n in binary_paths}):
                    raise ValueError('unexpected output path')
                r.put(out/m.name,a.extractfile(m).read())

    r.put(out/'build-task.py',BUILD_TASK.encode())
    r.put(out/'build-steps.json',r.encoded(steps))
    r.put(out/'build-bins.json',r.encoded(binary_paths))
    error = None
    try:
        assert run('machine',['uname','-m']).strip()==b'x86_64'
        run('image',['docker','image','inspect',r.b.IMAGE,'--format','{{.Id}}'])
        py('create-staging',f'import os\nos.mkdir({remote!r},0o700)\n')
        staged=True
        names=(*pins,'build-task.py','build-steps.json','build-bins.json')
        # Public paths only. Use SSH stdin, retaining SHA/exit per transferred file.
        for n in names:
            run('transfer-'+n,['python3','-c',
                'import sys,os; p='+repr(remote+'/'+n)+'; '
                'f=os.open(p,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600); '
                'w=os.fdopen(f,"wb"); '
                'import shutil; shutil.copyfileobj(sys.stdin.buffer,w); w.flush(); os.fsync(w.fileno()); w.close()'],
                stdin=(out/n).read_bytes(),timeout=900)
        extract=f'''import pathlib,tarfile,hashlib,json,stat
root=pathlib.Path({remote!r})
pins={pins!r}
for n,sha in pins.items():assert hashlib.sha256((root/n).read_bytes()).hexdigest()==sha
for folder in ('source','cargo-home','target'):(root/folder).mkdir(mode=0o700)
for filename,destination in [('source.tar.gz','source'),('registry.tar.gz','cargo-home')]:
 with tarfile.open(root/filename) as a:
  names=set()
  for m in a.getmembers():
   p=pathlib.PurePosixPath(m.name)
   assert not p.is_absolute() and '..' not in p.parts and '\\\\' not in m.name
   assert m.name not in names and (m.isfile() or m.isdir()); names.add(m.name)
  a.extractall(root/destination,filter='data')
index=json.loads((root/'source-index.json').read_bytes());source=root/'source'
assert set(index)=={{p.relative_to(source).as_posix() for p in source.rglob('*') if p.is_file()}}
for n,e in index.items():
 p=source/n;assert not p.is_symlink() and hashlib.sha256(p.read_bytes()).hexdigest()==e['sha256']
 assert p.stat().st_size==e['size'] and ('100755' if p.stat().st_mode&0o111 else '100644')==e['mode']
print('PASS exact exported source and public Cargo cache')
'''
        py('extract-verify',extract)
        cmd=['docker','create','--name',name,'--label','moex.scope=o3-readmission-build',
            '--network','none','--cpus','1','--cpu-shares','64','--memory','2g','--memory-swap','2g',
            '--pids-limit','128','--cap-drop','ALL','--security-opt','no-new-privileges',
            '--ulimit','core=0','--workdir','/src']
        for src,dst,ro in [('source','/src',True),('cargo-home','/cargo-home',False),('target','/target',False)]:
            cmd += ['--mount','type=bind,src='+remote+'/'+src+',dst='+dst+(',readonly' if ro else '')]
        for key,value in dict(CARGO_HOME='/cargo-home',CARGO_TARGET_DIR='/target',CARGO_NET_OFFLINE='true',
                              CARGO_INCREMENTAL='0',RUST_MIN_STACK='33554432',CARGO_TERM_COLOR='never').items():
            cmd+=['--env',key+'='+value]
        cmd += [r.b.IMAGE,'python3','-B','/build-task.py']
        run('create-container',cmd);created=True
        for n in ('build-task.py','build-steps.json','build-bins.json'):
            run('copy-'+n,['docker','cp',remote+'/'+n,name+':/'+n])
        observed=json.loads(run('isolation',['docker','inspect',name]))[0]
        hc=observed['HostConfig']
        assert hc['NetworkMode']=='none' and hc['NanoCpus']==1000000000
        assert hc['Memory']==hc['MemorySwap']==2147483648 and not hc['Privileged']
        assert all(m['Source'].startswith(remote+'/') for m in observed['Mounts'])
        assert hc['CapDrop']==['ALL'] and 'no-new-privileges' in hc['SecurityOpt']
        run('start',['docker','start',name])
        started=time.monotonic()
        number=0
        while True:
            state=read_container_state(run,name,number)
            number+=1
            if not state['Running']:break
            if time.monotonic()-started>3750:raise RuntimeError('native build exceeded bounded duration')
            time.sleep(20)
        # Obtain public compiler logs even on build failure, before cleanup.
        collect('collect-evidence')
        if state['ExitCode']!=0 or state['OOMKilled']:
            raise RuntimeError('native compiler failed; see collected evidence')
        artifacts=[]
        graph_step='fixture-input-build' if fixture_only else steps[0]['name']
        for line in (out/'evidence'/(graph_step+'.log')).read_bytes().splitlines():
            if line.startswith(b'{'):
                item=json.loads(line)
                if item.get('reason')=='compiler-artifact':artifacts.append(item)
        production_names=set() if fixture_only else {'stage8b-p1-paper-supervisor'} if support else set(r.BINS)
        selected=[v for v in artifacts if v.get('executable') and v['target']['name'] in production_names]
        if len(selected)!=len(production_names) or {v['target']['name'] for v in selected}!=production_names:
            raise ValueError('production binary graph inventory')
        if not fixture_only and any(v['profile']['test'] or any('fixture' in f for f in v['features']) for v in artifacts):
            raise ValueError('fixture/test production contamination')
        fixture_artifact=None
        fixture_libraries={}
        if support or fixture_only:
            fixture=[json.loads(line) for line in (out/'evidence/fixture-input-build.log').read_bytes().splitlines() if line.startswith(b'{')]
            fixture_libraries={v['target']['name']:v['features'] for v in fixture if v.get('reason')=='compiler-artifact' and 'lib' in v['target']['kind']}
            if not any('fixture' in f for features in fixture_libraries.values() for f in features):
                raise ValueError('input helper fixture graph not separately evidenced')
            fixture=[v for v in fixture if v.get('reason')=='compiler-artifact' and v.get('executable')==binary_paths['input-helper']]
            if len(fixture)!=1 or fixture[0]['target']['kind']!=['example'] or fixture[0]['profile']['test']:
                raise ValueError('missing separate input-generator example')
            fixture_artifact=fixture[0]
        binaries=[]
        for n in binary_paths:
            raw=(out/'payload'/n).read_bytes()
            if raw[:6]!=b'\x7fELF\x02\x01' or int.from_bytes(raw[18:20],'little')!=62:
                raise ValueError('not Linux x86-64 ELF')
            binaries.append(dict(name=n,size=len(raw),sha256=r.b.sha(raw),elf_machine='x86-64'))
            (out/'payload'/n).chmod(0o555)
        verify=extract[extract.index('index=json.loads'):]
        py('source-after','import pathlib,json,hashlib\nroot=pathlib.Path('+repr(remote)+')\n'+verify)
        result.update(status='FIXTURE_HELPER_BUILT_NOT_RELEASE' if fixture_only else 'SUPPORT_BUILT_NOT_RELEASE' if support else 'BUILT_NOT_INSTALLED',binaries=binaries,production_artifacts=selected,
            production_artifact_count=0 if fixture_only else len(artifacts),fixture_features_enabled=support or fixture_only,
            fixture_input_artifact=fixture_artifact,fixture_library_features=fixture_libraries,
            build_steps=steps,cargo_args=steps[0]['command'],
            source_commit_raw_sha256=r.b.sha((out/'source-commit.raw').read_bytes()),
            cargo_offline=True,source_tree_reverified=True)
    except Exception as exc:
        error=exc;result.update(status='FAILED',error=str(exc))
        if created and not (out/'outputs.tar.gz').exists():
            try:
                run('stop-failed-build',['docker','stop','--time','10',name],timeout=30)
                collect('collect-failure-evidence')
            except Exception as collection_error:
                result['failure_evidence_error']=str(collection_error)
    finally:
        cleanup=[]
        if created:
            try:
                run('remove-container',['docker','rm','-f',name]);result['container_removed']=True
            except Exception as exc:cleanup.append(str(exc))
        if staged:
            try:
                py('remove-staging',f'''import pathlib,shutil,stat,re
p=pathlib.Path({remote!r});s=p.lstat()
assert p.parent==pathlib.Path('/var/tmp') and re.fullmatch(r'moex-o3-readmission-build-[0-9a-f]{{12}}',p.name)
assert stat.S_ISDIR(s.st_mode) and s.st_uid==0 and not p.is_symlink()
shutil.rmtree(p);assert not p.exists()
''')
                result['remote_staging_removed']=True
            except Exception as exc:cleanup.append(str(exc))
        result.update(cleanup_errors=cleanup,records=records)
        if cleanup:result['status']='CLEANUP_INCOMPLETE'
        r.put(out/'result.json',r.encoded(result))
    if error:raise error
    if result['cleanup_errors']:raise RuntimeError('cleanup incomplete')
    r.put(out/('helper-build.json' if fixture_only else 'support-build.json' if support else 'build.json'),r.encoded(result))
    print('PASS '+('one fixture helper, NOT release' if fixture_only else 'two qualification helpers, NOT release' if support else 'five production ELFs')+'; not installed',flush=True)


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('output',type=Path);p.add_argument('--cache',type=Path,required=True)
    p.add_argument('--authorized-native-build',action='store_true',required=True)
    p.add_argument('--qualification-support',action='store_true',help='separate fixture input helper and production bootstrap; never release payload')
    p.add_argument('--fixture-helper-ref',help='committed example-only correction; never a production release build')
    a=p.parse_args();build(a.output.resolve(),a.cache.resolve(),a.qualification_support,a.fixture_helper_ref)
