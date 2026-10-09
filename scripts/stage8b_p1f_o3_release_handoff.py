#!/usr/bin/env python3
"""Fixed O3 release/install handoff with offline tree/commit/evidence verification."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import stat
import subprocess
import zipfile

import stage8b_p1f_o3_release as r
import stage8b_p1f_o3_source_handoff as h
import stage8b_p1f_o3_install as install

P='o3-release/'
ROOT=Path(__file__).resolve().parents[1]
sha,encode,require=h.sha,h.encode,h.require


def git(*args):return subprocess.check_output(['git',*args],cwd=ROOT)


def snapshot(ref):
    modes={}
    for row in git('ls-tree','-rz',ref).split(b'\0'):
        if row:
            fields,n=row.split(b'\t',1);mode,kind,_=fields.split()
            require(kind==b'blob' and mode in (b'100644',b'100755'),'nonregular source')
            modes[n.decode()]=mode.decode()
    with zipfile.ZipFile(io.BytesIO(git('archive','--format=zip',ref))) as z:
        files={n:z.read(n) for n in modes}
    return files,modes


def commit_hash(raw):return hashlib.sha1(b'commit '+str(len(raw)).encode()+b'\0'+raw).hexdigest()


def verify(path):
    with zipfile.ZipFile(path) as z:
        infos=z.infolist();names=[i.filename for i in infos]
        require(len(names)==len(set(names)),'duplicate member')
        require(sum(i.file_size for i in infos)<300*1024*1024,'archive expanded size')
        modes={}
        for i in infos:
            h.safe_name(i.filename)
            mode=i.external_attr>>16
            require(stat.S_ISREG(mode) and stat.S_IMODE(mode) in (0o644,0o755),'member type/mode')
            modes[i.filename]='100755' if mode&0o111 else '100644'
        require(z.testzip() is None,'CRC failure')
        read=z.read;js=lambda n:json.loads(read(n))
        idx=js(P+'files-sha256.json')
        require(set(idx)==set(names)-{P+'files-sha256.json'},'complete file inventory')
        for n,digest in idx.items():
            raw=read(n);require(sha(raw)==digest,'file hash: '+n);h.scan(n,raw)
        d=js(P+'descriptor.json');index=js(P+'source-index.json')
        require(set(index)=={n for n in names if not n.startswith(P)
                and (n!='handoff-commit.txt' or d['handoff_was_tracked'])},'complete source inventory')
        files={n:read(P+'original-handoff-commit.txt') if n=='handoff-commit.txt' else read(n) for n in index}
        require(all(sha(files[n])==e['sha256'] and modes[n]==e['mode'] for n,e in index.items()),'source blob/mode')
        tree=h.tree_hash(files,{n:e['mode'] for n,e in index.items()})
        raw=read(P+'package-commit.raw')
        require(tree==d['package_tree'] and raw.startswith(('tree '+tree+'\n').encode())
                and commit_hash(raw)==d['package_ref'],'packaging commit/tree')
        require(d['source_ref']==r.REF and d['source_tree']==r.TREE,'accepted source binding')
        base_index=js(P+'accepted-source-index.json')
        base={n:files[n] if n in files and sha(files[n])==e['sha256'] else read(P+'source-preimages/'+n) for n,e in base_index.items()}
        require(all(sha(base[n])==e['sha256'] for n,e in base_index.items()),'accepted source blob')
        require(h.tree_hash(base,{n:e['mode'] for n,e in base_index.items()})==r.TREE,'accepted source tree')
        raw=read(P+'accepted-source-commit.raw')
        require(commit_hash(raw)==r.REF and raw.startswith(('tree '+r.TREE+'\n').encode()),'accepted commit object')
        changed={n for n in set(base)|set(files) if base.get(n)!=files.get(n) or base_index.get(n,{}).get('mode')!=index.get(n,{}).get('mode')}
        require(changed==set(d['packaging_delta']),'packaging delta')
        for n in changed:
            require(n in {'docs/current-status.md','docs/roadmap.md','docs/stage-8/gov-ci-1-authority.json',
                'docs/stage-8/stage8b-p1f-o3-release-installation.md',
                'docs/stage-8/reviews/REVIEW_fb1ceb3_O3_SOURCE_ACCEPT_20261009_RU.txt'}
                or n.startswith(('scripts/stage8b_p1f_o3_','scripts/test_stage8b_p1f_o3_')),'unexpected source delta: '+n)
        h.validate_templates(files)
        handoff=read('handoff-commit.txt').decode()
        require('full_sha='+d['package_ref'] in handoff and 'archive='+d['archive'] in handoff,'handoff provenance')
        build=js(P+'build/build.json')
        require(build['source_ref']==r.REF and build['source_tree']==r.TREE
                and build['image']==r.b.IMAGE and build['fixture_features_enabled'] is False
                and build['installation_performed'] is False and build['operational_o3_invoked'] is False,'build scope')
        require(set(b['name'] for b in build['binaries'])==set(r.BINS) and len(build['binaries'])==5,'five release slots')
        for b in build['binaries']:
            raw=read(P+'installation/payload/'+b['name'])
            require(sha(raw)==b['sha256'] and len(raw)==b['size'] and raw[:6]==b'\x7fELF\x02\x01'
                    and int.from_bytes(raw[18:20],'little')==62,'ELF identity')
        log=read(P+'build/logs/production-build.log')
        artifacts=[json.loads(line) for line in log.splitlines() if line.startswith(b'{')]
        artifacts=[a for a in artifacts if a.get('reason')=='compiler-artifact']
        require(len(artifacts)==build['production_artifact_count'] and artifacts,'production graph')
        require(not any('fixture' in f for a in artifacts for f in a['features'])
                and not any(a['profile']['test'] for a in artifacts),'fixture/test production graph')
        selected=[a for a in artifacts if a.get('executable') and a['target']['name'] in r.BINS]
        require(selected==build['production_artifacts'] and len(selected)==5,'compiler artifact selection')
        for n in names:
            if n.startswith(P+'build/logs/') and n.endswith('.json'):
                v=js(n);require(v['exit_code']==0 and sha(read(n[:-5]+'.log'))==v['log_sha256'],'build log')
        q=js(P+'qualification/result.json')
        require(q['result']=='PARTIAL_DYNAMIC_UNQUALIFIED' and q['systemd_pid1_real'] is True
                and q['dynamic_service_execution_qualified'] is False
                and q['active_phase_positive_systemd_run'] is False and q['private_keys_used'] is False
                and q['operational_o3'] is False,'qualification scope')
        require(q['evidence_kind']=='retained_incomplete_attempt_command_records'
                and q['driver_source_binding_claimed'] is False,'honest partial evidence boundary')
        records=q['records'];ids=[v['id'] for v in records]
        expected={'create','systemd-version','installer-tests','group','user','synthetic-custody-setup','syntax',
            'local-daemon-reload','loaded-signer','loaded-supervisor','paper-key-unreadable','paper-lifecycle-readable',
            'signer-start','signer-result','negative-service-journal',
            'remove'}|{'usage-'+n for n in r.BINS}|{'paper-executable-not-writable-'+n for n in r.BINS}
        require(len(ids)==len(expected) and set(ids)==expected,'qualification case inventory')
        for v in records:
            expected_exit=(70 if v['id']=='usage-stage8b-p1f-o2-operator' else 64 if v['id'].startswith('usage-') else
                           1 if v['id']=='paper-key-unreadable' or v['id'].startswith('paper-executable-not-writable-') else 0)
            if v['id']=='signer-start':
                require(v['expected_exit_code']==[0,1] and v['exit_code'] in (0,1),'Type=exec acknowledgement')
            else:require(v['exit_code']==v['expected_exit_code']==expected_exit,'qualification exit')
            require(sha(read(P+'qualification/logs/'+v['id']+'.log'))==v['sha256'],'qualification log')
        require(b'Ran 14 tests in ' in read(P+'qualification/logs/installer-tests.log')
                and read(P+'qualification/logs/installer-tests.log').rstrip().endswith(b'OK'),'installer test inventory')
        signer=dict(line.split('=',1) for line in read(P+'qualification/logs/signer-result.log').decode().splitlines())
        require(signer['MainPID']=='0' and signer['ActiveState']=='failed' and signer['ExecMainStatus']=='255'
                and signer['NRestarts']=='0','retain actual unqualified systemd exit, not runtime PASS')
        spec=js(P+'installation/spec.json');snap=js(P+'installation/snapshot.json')
        require(spec['installer_sha256']==sha(files['scripts/stage8b_p1f_o3_install.py'])
                and spec['snapshot_sha256']==sha(install.canonical(snap))
                and spec['build_sha256']==sha(install.canonical(build))
                and spec['old_manifest_sha256']==install.inv.OLD
                and spec['installation_authorized'] is False and spec['activation_authorized'] is False,'installation contract')
        old=install.parse(snap['old_public_payload'][install.c.MANIFEST].encode())
        require(sha(snap['old_public_payload'][install.c.MANIFEST].encode())==install.inv.OLD,'predecessor inventory')
        require(set(spec['updates'])==set(install.CHANGES),'fixed nine writes')
        _,expected_payload=install.replacements(snap,build,
            {n:read(P+'installation/payload/'+Path(n).name) for n in install.ELFS},
            {n:files['deploy/stage8b-p1f-o3/'+Path(n).name] for n in install.TEMPLATES})
        for n,e in spec['updates'].items():
            raw=read(P+'installation/payload/'+Path(n).name)
            require(raw==expected_payload[n],'derived exact payload: '+n)
            require(e==dict(old_sha256=install.inv.OLD if n==install.c.MANIFEST else old['payload'].get(n,{}).get('sha256'),
                           **install.metadata(n,raw)),'old/new inventory: '+n)
        require(spec['new_manifest_sha256']==sha(read(P+'installation/payload/installation-o2-v1.json')),'successor manifest')
        require(js(P+'snapshot/stdout.json')==snap,'snapshot continuity')
        sr=js(P+'snapshot/record.json')
        require(sr['exit_code']==0 and sr['stdout_sha256']==sha(read(P+'snapshot/stdout.json'))
                and sr['stdin_sha256']==sha(read(P+'snapshot/collector.py'))
                and sr['stderr_sha256']==sha(read(P+'snapshot/stderr.txt')),'readonly collection records')
        authority=json.loads(files['docs/stage-8/gov-ci-1-authority.json'])
        old_authority=json.loads(base['docs/stage-8/gov-ci-1-authority.json'])
        inv_fields={'production_code_manifest','governance_control_plane_manifest'}
        require({k:v for k,v in authority.items() if k not in inv_fields}==
                {k:v for k,v in old_authority.items() if k not in inv_fields},'authority scope drift')
        gates=js(P+'gates/result.json')
        require(set(gates)=={'authority','authority-negative','diff'},'gate inventory')
        for n,g in gates.items():
            require(g['exit_code']==0 and sha(read(P+'gates/'+n+'.log'))==g['sha256'],'gate log')
        require(b'45 cases passed' in read(P+'gates/authority-negative.log') or
                b'45/45' in read(P+'gates/authority-negative.log'),'authority negative inventory')
        return dict(status='PASS',archive_sha256=sha(path.read_bytes()),package_ref=d['package_ref'],
            package_tree=tree,compiled_source_ref=r.REF,compiled_source_tree=r.TREE,
            members=len(names),source_files=len(index),duplicates=0,symlinks=0,unsafe_paths=0,
            release_elfs=5,write_slots=9,installation_performed=False,operational_o3=False,
            dynamic_systemd_qualified=False,verdict='INTEGRITY_PASS_NOT_INSTALLATION_OR_EXECUTION_ACCEPTANCE')


def package(work, qualification, out):
    require(git('status','--porcelain')==b'','commit complete package sources first')
    ref=git('rev-parse','HEAD').decode().strip();files,modes=snapshot('HEAD');base,bm=snapshot(r.REF)
    payload=dict(files);name='moex-trading-project-'+ref[:7]+'-o3-release-installation.zip'
    if 'handoff-commit.txt' in files:
        payload[P+'original-handoff-commit.txt']=files['handoff-commit.txt']
    payload[P+'package-commit.raw']=git('cat-file','commit',ref)
    payload[P+'accepted-source-commit.raw']=git('cat-file','commit',r.REF)
    payload['handoff-commit.txt']=f'short_sha={ref[:7]}\nfull_sha={ref}\nsource_ref={r.REF}\narchive={name}\n'.encode()
    payload[P+'source-index.json']=encode({n:dict(sha256=sha(raw),mode=modes[n]) for n,raw in files.items()})
    payload[P+'accepted-source-index.json']=encode({n:dict(sha256=sha(raw),mode=bm[n]) for n,raw in base.items()})
    changed={n for n in set(base)|set(files) if base.get(n)!=files.get(n) or bm.get(n)!=modes.get(n)}
    for n in changed:
        if n in base:payload[P+'source-preimages/'+n]=base[n]
    for src,dest in [(work/'installation-r1','installation'),(work/'build/logs','build/logs'),
                     (work/qualification,'qualification'),(work/'snapshot-r1','snapshot')]:
        for p in sorted(src.rglob('*')):
            require(not p.is_symlink(),'evidence symlink')
            if p.is_file():payload[P+dest+'/'+p.relative_to(src).as_posix()]=p.read_bytes()
    payload[P+'build/build.json']=(work/'build/build.json').read_bytes()
    commands={'authority':['python3','-B','scripts/current_tree_authority_check.py'],
        'authority-negative':['python3','-B','scripts/current_tree_authority_negative_harness.py'],
        'diff':['git','diff','--check',r.REF,'HEAD']}
    results={}
    for n,cmd in commands.items():
        p=subprocess.run(cmd,cwd=ROOT,capture_output=True,timeout=600)
        raw=p.stdout+p.stderr;require(p.returncode==0,'package gate: '+n)
        results[n]=dict(command=cmd,exit_code=p.returncode,sha256=sha(raw));payload[P+'gates/'+n+'.log']=raw
    payload[P+'gates/result.json']=encode(results)
    payload[P+'descriptor.json']=encode(dict(schema_version=1,archive=name,package_ref=ref,
        handoff_was_tracked='handoff-commit.txt' in files,
        package_tree=git('rev-parse','HEAD^{tree}').decode().strip(),source_ref=r.REF,source_tree=r.TREE,
        repository='https://github.com/DKorolski/moex-trading-gateway-rs',
        branch=git('branch','--show-current').decode().strip(),packaging_delta=sorted(changed),
        source_acceptance='SOURCE ACCEPT / P1-O301 CLOSED',installation_performed=False,operational_o3=False))
    for n,raw in payload.items():h.scan(n,raw)
    payload[P+'files-sha256.json']=encode({n:sha(raw) for n,raw in payload.items()})
    out.mkdir(parents=True,exist_ok=True);path=out/name
    for suffix in ('','.partial','.sha256','.safety.json','.review.txt'):require(not Path(str(path)+suffix).exists(),'output exists')
    tmp=Path(str(path)+'.partial')
    with zipfile.ZipFile(tmp,'w',zipfile.ZIP_DEFLATED,compresslevel=6) as z:
        for n,raw in sorted(payload.items()):
            i=zipfile.ZipInfo(n,(2026,10,9,0,0,0));i.create_system=3
            i.external_attr=(stat.S_IFREG|(0o755 if modes.get(n)=='100755' else 0o644))<<16
            i.compress_type=zipfile.ZIP_DEFLATED;z.writestr(i,raw)
    safety=verify(tmp);tmp.rename(path)
    r.put(Path(str(path)+'.sha256'),f'{safety["archive_sha256"]}  {name}\n'.encode())
    r.put(Path(str(path)+'.safety.json'),encode(safety))
    r.put(Path(str(path)+'.review.txt'),f'''Repository: https://github.com/DKorolski/moex-trading-gateway-rs
Branch: {git('branch','--show-current').decode().strip()}
Package commit: {ref}
Compiled accepted source: {r.REF}
Archive: {name}
SHA-256: {safety['archive_sha256']}

O3 unified RELEASE / AUTHORITY / STOPPED INSTALLATION candidate.
SOURCE ACCEPT inherited; five production Linux amd64 release ELFs, no fixture features.
Nine exact old-to-new slots; accepted installer custody/locks/fsync retained.
Real Linux filesystem tests; real local systemd syntax/loading/custody.
Qualification evidence is an INCOMPLETE attempt, not a completed aggregate PASS.
LIMITATION: dynamic systemd unqualified; local signer executor exit 255.
This is NOT the expected runtime refusal 70. Native x86 systemd check remains required.
No positive active-phase systemd run claimed; operational O3 NOT_RUN (13 rows).
VPS only read, not changed. No private key bytes read. No phase inputs issued.
No push/merge/CI invocation; stopped installation and O3 need separate authorization.
Start at docs/stage-8/stage8b-p1f-o3-release-installation.md and o3-release/descriptor.json.
Verify: python3 -B scripts/stage8b_p1f_o3_release_handoff.py verify ARCHIVE
Next: package review -> permitted stopped install -> installation evidence -> permitted bounded O3 -> O4/ALOR.
'''.encode())
    print(json.dumps(safety,indent=2))


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('action',choices=('package','verify'))
    p.add_argument('path',type=Path);p.add_argument('--work',type=Path);p.add_argument('--qualification',default='qualification-r3')
    a=p.parse_args()
    if a.action=='verify':print(json.dumps(verify(a.path.resolve()),indent=2))
    else:package(a.work.resolve(),a.qualification,a.path.resolve())
