#!/usr/bin/env python3
"""Immutable config-only review ZIP; offline verification, no deployment."""
import argparse
import json
from pathlib import Path
import stat
import zipfile

import stage8b_p1f_o3_config_binding as m
import stage8b_p1f_o3_release_handoff as archive
import stage8b_p1f_o3_source_handoff as safety

P = 'o3-config/'
BASE = '0e89208c1e6235eab8b5ea0e115d7899cbafd223'
ALLOWED = {
    'docs/current-status.md', 'docs/roadmap.md', 'docs/stage-8/gov-ci-1-authority.json',
    'docs/stage-8/stage8b-p1f-o3-config-binding-correction.md',
    'docs/stage-8/reviews/REVIEW_0e89208_O3_INSTALLATION_EVIDENCE_ACCEPT_20261009_RU.txt',
    'docs/stage-8/reviews/REVIEW_0e89208_O3_RELEASE_INSTALLATION_ACCEPT_20261009_RU.txt',
    'scripts/fixtures/o3-accepted-o2-supervisor.json',
    'scripts/fixtures/stage8b-o3-config-binding-probe.rs',
    'scripts/stage8b_p1f_o3_config_binding.py', 'scripts/test_stage8b_p1f_o3_config_binding.py',
    'scripts/stage8b_p1f_o3_config_handoff.py',
}
GATES = ('filesystem-tests', 'production-library-build', 'probe-compile', 'config-regression',
         'package-inputs', 'fmt', 'diff', 'authority', 'authority-negative')


def encode(v): return (json.dumps(v,sort_keys=True,indent=2)+'\n').encode()


def verify(path):
    with zipfile.ZipFile(path) as z:
        names=z.namelist();m.require(len(names)==len(set(names)) and z.testzip() is None,'ZIP duplicates/CRC')
        m.require(sum(i.file_size for i in z.infolist())<150*1024*1024,'expanded size')
        modes={}
        for i in z.infolist():
            safety.scan(i.filename,z.read(i));mode=i.external_attr>>16
            m.require(stat.S_ISREG(mode) and stat.S_IMODE(mode) in (0o644,0o755),'member mode')
            modes[i.filename]='100755' if mode&0o111 else '100644'
        js=lambda n: m.parse(z.read(n))
        inventory=js(P+'files-sha256.json')
        m.require(set(inventory)==set(names)-{P+'files-sha256.json'},'complete inventory')
        for n,sha in inventory.items():m.require(m.sha(z.read(n))==sha,'hash '+n)
        descriptor=js(P+'descriptor.json');index=js(P+'source-index.json')
        m.require(set(index)=={n for n in names if not n.startswith(P)},'complete source')
        files={n:z.read(P+'original-handoff-commit.txt') if n=='handoff-commit.txt' else z.read(n) for n in index}
        m.require(all(m.sha(files[n])==e['sha256'] and modes[n]==e['mode'] for n,e in index.items()),'source bytes/modes')
        tree=safety.tree_hash(files,{n:e['mode'] for n,e in index.items()})
        raw=z.read(P+'commit.raw')
        m.require(raw.startswith(('tree '+tree+'\n').encode()) and archive.commit_hash(raw)==descriptor['commit'],'commit/tree')
        base_index=js(P+'accepted-source-index.json')
        base={n:files[n] if n in files and m.sha(files[n])==e['sha256'] else z.read(P+'preimages/'+n) for n,e in base_index.items()}
        m.require(all(m.sha(base[n])==e['sha256'] for n,e in base_index.items()),'accepted source bytes')
        base_tree=safety.tree_hash(base,{n:e['mode'] for n,e in base_index.items()})
        raw=z.read(P+'accepted-commit.raw')
        m.require(archive.commit_hash(raw)==BASE and raw.startswith(('tree '+base_tree+'\n').encode()),'accepted commit/tree')
        changes={n for n in set(files)|set(base) if files.get(n)!=base.get(n) or index.get(n,{}).get('mode')!=base_index.get(n,{}).get('mode')}
        m.require(changes==set(descriptor['changed_paths']) and changes<=ALLOWED,'config-only source scope')
        m.require(descriptor['baseline']==BASE and descriptor['installation_performed'] is False and descriptor['operational_o3'] is False,'scope')
        m.require(('full_sha='+descriptor['commit']) in z.read('handoff-commit.txt').decode(),'handoff SHA')
        expected=m.expected_files(z.read(P+'config-correction/baseline.json'))
        m.require({n[len(P+'config-correction/'):] for n in names if n.startswith(P+'config-correction/')}==set(expected),'correction inventory')
        for n,b in expected.items():m.require(z.read(P+'config-correction/'+n)==b,'correction bytes '+n)
        for n in GATES:
            record=js(P+'gates/'+n+'.json');log=z.read(P+'gates/'+n+'.log')
            m.require(record['exit_code']==0 and m.sha(log)==record['log_sha256'],'gate '+n)
        log=z.read(P+'gates/filesystem-tests.log')
        m.require(b'Ran 18 tests in ' in log and b'\nOK\n' in log,'filesystem test result')
        for where in ('before','after'):
            for n in range(1,5):m.require(f'PASS crash-{where}-atomic-{n}'.encode() in log,'crash boundary')
        probe=z.read(P+'gates/config-regression.log')
        for marker in ('exact-one-field-config-change','production-constructor-old-reject-new-match-boot-1-same-root-source',
            'production-constructor-old-reject-new-match-boot-2-same-root-source','foreign-generation-not-bound-by-new-pin',
            'DB0-rejected','CONFIG_ONLY_REGRESSION_NO_RUNTIME_OR_REDIS_EXECUTION'):
            m.require(('PASS '+marker).encode() in probe,'production probe result')
        artifacts=[json.loads(l) for l in z.read(P+'gates/production-library-build.log').splitlines() if l.startswith(b'{')]
        artifacts=[a for a in artifacts if a.get('reason')=='compiler-artifact']
        m.require(artifacts and not any('fixture' in f for a in artifacts for f in a['features']),'production feature graph')
        m.require(js(P+'gates/library-binding.json')['fixture_features_enabled'] is False,'library binding')
        m.require(b'current-tree-authority-negative: PASS cases=45/45' in z.read(P+'gates/authority-negative.log'),'negative inventory')
        return dict(status='PASS',commit=descriptor['commit'],sha256=m.sha(path.read_bytes()),
                    members=len(names),duplicates=0,unsafe_paths=0,symlinks=0,production_code_changes=0,
                    configuration_fields_changed=1,installation_performed=False,operational_o3=False)


def package(work,out):
    m.require(archive.git('status','--porcelain')==b'','commit complete correction first')
    ref=archive.git('rev-parse','HEAD').decode().strip()
    files,modes=archive.snapshot(ref);base,bm=archive.snapshot(BASE)
    changed={n for n in set(files)|set(base) if files.get(n)!=base.get(n) or modes.get(n)!=bm.get(n)}
    m.require(changed<=ALLOWED,'unexpected source delta')
    payload=dict(files);payload[P+'original-handoff-commit.txt']=files['handoff-commit.txt']
    name='moex-trading-project-'+ref[:7]+'-o3-config-binding-correction.zip'
    payload['handoff-commit.txt']=f'short_sha={ref[:7]}\nfull_sha={ref}\nsource_ref={ref}\narchive={name}\n'.encode()
    for prefix,f,mo,commit in (('',files,modes,ref),('accepted-',base,bm,BASE)):
        payload[P+prefix+'source-index.json']=encode({n:dict(sha256=m.sha(b),mode=mo[n]) for n,b in f.items()})
        payload[P+prefix+'commit.raw']=archive.git('cat-file','commit',commit)
    for n in changed:
        if n in base:payload[P+'preimages/'+n]=base[n]
    for folder in ('config-correction','gates'):
        for p in sorted((work/folder).iterdir()):
            m.require(p.is_file() and not p.is_symlink(),'evidence file')
            payload[P+folder+'/'+p.name]=p.read_bytes()
    payload[P+'qualify.py']=(work/'qualify.py').read_bytes()
    payload[P+'descriptor.json']=encode(dict(commit=ref,baseline=BASE,changed_paths=sorted(changed),
        branch=archive.git('branch','--show-current').decode().strip(),
        repository='https://github.com/DKorolski/moex-trading-gateway-rs',
        installation_performed=False,operational_o3=False))
    payload[P+'files-sha256.json']=encode({n:m.sha(b) for n,b in payload.items()})
    out.mkdir(parents=True,exist_ok=True);path=out/name
    with zipfile.ZipFile(path,'x',zipfile.ZIP_DEFLATED,compresslevel=6) as z:
        for n,b in sorted(payload.items()):
            safety.scan(n,b);i=zipfile.ZipInfo(n,(2026,10,9,0,0,0));i.create_system=3
            i.external_attr=(stat.S_IFREG|(0o755 if modes.get(n)=='100755' else 0o644))<<16
            i.compress_type=zipfile.ZIP_DEFLATED;z.writestr(i,b)
    result=verify(path)
    for suffix,b in (('.sha256',(result['sha256']+'  '+name+'\n').encode()),('.safety.json',encode(result)),
                     ('.review.txt',f'Config-only correction review candidate. Commit {ref}.\n18 filesystem tests including 8 interruption cases; production pure-constructor regression.\nNo production Rust/Cargo/ELF changes. NOT INSTALLED; O3 NOT_RUN.\nVerify: python3 -B scripts/stage8b_p1f_o3_config_handoff.py verify ARCHIVE\n'.encode())):
        with Path(str(path)+suffix).open('xb') as f:f.write(b)
    return result


if __name__=='__main__':
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('action',choices=('package','verify'))
    ap.add_argument('path',type=Path);ap.add_argument('--out',type=Path);a=ap.parse_args()
    print(json.dumps(verify(a.path) if a.action=='verify' else package(a.path,a.out),indent=2))
