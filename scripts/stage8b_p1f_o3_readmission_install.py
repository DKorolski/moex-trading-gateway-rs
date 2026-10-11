#!/usr/bin/env python3
"""Exact seven-slot stopped O3 successor. No activation, keys or history writes.

Preparation/verification is local. Applying needs separate release acceptance
and owner permission. Reuse existing custody, lock and atomic-fsync primitives.
"""
import argparse
import copy
import json
import os
from pathlib import Path
import stat

import stage8b_p1f_o3_install as prior
import stage8b_p1f_o3_config_binding as config

c, inv, release = prior.c, prior.inv, prior.release
require, sha, canonical, parse = prior.require, prior.sha, prior.canonical, prior.parse
ELFS, O3 = prior.ELFS, prior.O3
CHANGES = (*ELFS, O3, c.MANIFEST)
OLD = 'c2585bb68d67aa8515db15762d61fdd5cf5cd2e36bf75cdec3023339ac937c15'
OLD_O3 = '3075cc2cbf96a70d75684630ca9a61a01e274f96e61bf84c13fe143448ce0ca0'
BASELINE = '3c1b57f833b86256da7946b36f8e66d3d7eb314e52d94263091fe1432abc17b5'
BUILD = 'e82171c248fda803eb973a141642a8510a77490c40dbd75e95bf471f8b54540e'
TX = c.BASE+'/o3-readmission-5df1c88-after-completed-26'
DOMAIN = 'moex.o3.readmission-5df1c88.installation.v1'
CONFIRM = 'INSTALL_O3_5DF1C88_PRESERVE_COMPLETED_26_NO_ACTIVATION'
ROLLBACK = 'ROLLBACK_O3_5DF1C88_PRESERVE_COMPLETED_26'
BASELINE_FIELDS = ('observed','installed','old_installation_transaction','correction_transaction')


def package_file(path):
    s=path.lstat()
    require(stat.S_ISREG(s.st_mode) and s.st_nlink==1 and s.st_size<64*1024*1024,'package file custody')
    with path.open('rb') as f:
        require(os.fstat(f.fileno())==s,'package replaced before read')
        raw=f.read(s.st_size+1)
        require(os.fstat(f.fileno())==s and path.lstat()==s and len(raw)==s.st_size,'package changed during read')
    return raw


def derive(baseline, build_raw, outer_raw, o3_raw, binaries):
    require(sha(canonical(baseline))==BASELINE,'accepted protected baseline')
    require(sha(build_raw)==BUILD,'exact native production build record')
    require(sha(outer_raw)==OLD and sha(o3_raw)==OLD_O3,'exact installed manifests')
    build,old,o3=parse(build_raw),parse(outer_raw),parse(o3_raw)
    require(build['source_ref']==release.READMISSION_REF and build['source_tree']==release.READMISSION_TREE
            and build['status']=='BUILT_NOT_INSTALLED' and build['fixture_features_enabled'] is False,'production build scope')
    require(set(binaries)==set(ELFS) and {b['name'] for b in build['binaries']}==set(release.BINS),'five ELF slots')
    for entry in build['binaries']:
        raw=binaries['/usr/local/libexec/moex/'+entry['name']]
        require(sha(raw)==entry['sha256'] and len(raw)==entry['size'],'compiled ELF binding')
        require(raw[:6]==b'\x7fELF\x02\x01' and int.from_bytes(raw[18:20],'little')==62,'native ELF required')
    for n,e in old['payload'].items():
        observed=baseline['installed'][n]
        require(e['sha256']==observed['sha256'] and e['size']==observed['size'],'installed inventory binding')
    require(baseline['observed']['trees'][inv.prior.CONFIG]['supervisor.json']['sha256']==config.NEW_CONFIG,
            'accepted corrected supervisor config required')
    own=sha(Path(__file__).read_bytes())
    payload=dict(binaries)
    new_o3=copy.deepcopy(o3)
    for n in ELFS:new_o3['payload'][n]=prior.metadata(n,payload[n])
    new_o3.update(source_ref=release.READMISSION_REF,source_tree=release.READMISSION_TREE,
        predecessor_o2_manifest_sha256=OLD,release_build_sha256=BUILD,installer_sha256=own)
    payload[O3]=canonical(new_o3)
    new=copy.deepcopy(old)
    for n,raw in payload.items():new['payload'][n]=prior.metadata(n,raw)
    new.update(artifact_ref=release.READMISSION_REF,artifact_sha256=BUILD,installer_sha256=own,
               predecessor_manifest_sha256=OLD,update_revision='o3-readmission-5df1c88-after-completed-26-v1')
    payload[c.MANIFEST]=canonical(new)
    spec=dict(domain=DOMAIN,source_ref=release.READMISSION_REF,baseline_sha256=BASELINE,
        build_record_sha256=BUILD,installer_sha256=own,old_manifest_sha256=OLD,
        new_manifest_sha256=sha(payload[c.MANIFEST]),installation_authorized=False,activation_authorized=False,
        qualification_status='PENDING_NOT_INSTALL_AUTHORITY',
        updates={n:dict(old_sha256=OLD if n==c.MANIFEST else old['payload'][n]['sha256'],
                        **prior.metadata(n,payload[n])) for n in CHANGES})
    return spec,old,payload


def prepare(directory, snapshot, build_dir, manifests):
    observed=parse(package_file(snapshot))
    require(observed['status']=='FRESH_PREFLIGHT_PASS' and observed['remote_writes'] is False
            and observed['private_bytes_read'] is False,'read-only snapshot required')
    baseline={k:observed[k] for k in BASELINE_FIELDS}
    raw=package_file(build_dir/'build.json')
    outer=package_file(manifests/Path(c.MANIFEST).name)
    o3=package_file(manifests/Path(O3).name)
    spec,_,payload=derive(baseline,raw,outer,o3,{n:package_file(build_dir/'payload'/Path(n).name) for n in ELFS})
    directory.mkdir(parents=True,exist_ok=False)
    for name,data in {'baseline.json':canonical(baseline),'build.json':raw,'before-outer.json':outer,
                      'before-o3.json':o3,'spec.json':canonical(spec)}.items():release.put(directory/name,data)
    for n,data in payload.items():release.put(directory/'payload'/Path(n).name,data)
    load(directory)
    return spec


def load(directory):
    require(directory.is_dir() and not directory.is_symlink(),'package directory')
    require({p.name for p in directory.iterdir()}=={'baseline.json','build.json','before-outer.json','before-o3.json','spec.json','payload'},'package inventory')
    require((directory/'payload').is_dir() and not (directory/'payload').is_symlink(),'payload directory')
    require({p.name for p in (directory/'payload').iterdir()}=={Path(n).name for n in CHANGES},'seven payload slots')
    read=lambda name:package_file(directory/name)
    baseline=parse(read('baseline.json'))
    spec,old,payload=derive(baseline,read('build.json'),read('before-outer.json'),read('before-o3.json'),
                          {n:read('payload/'+Path(n).name) for n in ELFS})
    require(read('spec.json')==canonical(spec),'exact spec required')
    require(all(read('payload/'+Path(n).name)==raw for n,raw in payload.items()),'derived payload mismatch')
    return spec,baseline,old,payload


class Update:
    locks=inv.prior.previous.Update.locks

    def __init__(self,root,directory):
        self.root=c.old.validate_root(root)
        self.spec,self.baseline,self.old,self.payload=load(directory)
        self.uid,self.gid=c.old.account_ids(root)
        require((self.uid,self.gid)==(987,987),'service identity drift')

    def path(self,n):return c.old.rooted(self.root,n)

    def read(self,n):
        e=self.old['payload'].get(n,dict(mode='0644',group='root'))
        return c.file_bytes(self.root,n,int(e['mode'],8),987 if e['group']=='service' else 0)

    def invariants(self):
        require(inv.observe(self.root)==self.baseline['observed'],'protected state/P0/stopped state drift')
        for tx,key in ((prior.TX,'old_installation_transaction'),(config.TX,'correction_transaction')):
            require(inv.inventory(self.path(tx))==self.baseline[key],'prior transaction drift')
        require(c.command(['redis-cli','-h','127.0.0.1','-p','6379','-n','15','DBSIZE']).strip()=='0',
                'DB15 not empty; no flush or repair')

    def verify_files(self,state):
        for n in (*self.old['payload'],c.MANIFEST):
            raw=self.read(n)
            if n in CHANGES:
                e=self.spec['updates'][n]
                allowed={'old':{e['old_sha256']},'new':{e['sha256']},'mixed':{e['old_sha256'],e['sha256']}}[state]
                require(sha(raw) in allowed,'foreign/missing replacement: '+n)
            else:
                e=self.old['payload'][n]
                require(sha(raw)==e['sha256'] and len(raw)==e['size'],'retained payload drift: '+n)

    def write_journal(self,state):
        c.atomic_write(self.root,TX+'/journal.json',canonical(dict(domain=DOMAIN,
            spec_sha256=sha(canonical(self.spec)),state=state)),0o600,0)

    def journal(self):
        p=self.path(TX);s=p.lstat()
        require(stat.S_ISDIR(s.st_mode) and s.st_uid==s.st_gid==0 and stat.S_IMODE(s.st_mode)==0o700,'transaction custody')
        names={q.name for q in p.iterdir()}
        backups={f'before-{i}' for i in range(len(CHANGES))}
        require(names<=backups|{'journal.json'},'foreign transaction file')
        for i,n in enumerate(CHANGES):
            if f'before-{i}' in names:
                require(sha(c.file_bytes(self.root,TX+f'/before-{i}',0o600,0))==self.spec['updates'][n]['old_sha256'],'backup drift')
        if 'journal.json' not in names:
            self.verify_files('old')
            return None
        require(names==backups|{'journal.json'},'prepared journal missing backups')
        raw=c.file_bytes(self.root,TX+'/journal.json',0o600,0);value=parse(raw)
        require(type(value) is dict and 'state' in value,'malformed transaction journal')
        state=value['state']
        require(state in ('PREPARED','APPLIED','ROLLING_BACK','ROLLED_BACK') and raw==canonical(
            dict(domain=DOMAIN,spec_sha256=sha(canonical(self.spec)),state=state)),'journal binding')
        return state

    def transition(self,action):
        require(action in ('preflight','apply','resume','rollback'),'unknown action')
        with self.locks():
            self.invariants()
            if not c.exists(self.root,TX):
                self.verify_files('old')
                if action=='preflight':return dict(state='EXACT_OLD_COMPLETED_26',installation_performed=False)
                require(action=='apply','no prior transaction')
                c.old.verify_secure_directory_chain(self.root,self.path(TX).parent)
                self.path(TX).mkdir(mode=0o700);c.sync_dir(self.path(TX).parent)
            state=self.journal()
            self.verify_files({'APPLIED':'new','ROLLED_BACK':'old'}.get(state,'mixed'))
            if action=='preflight':return dict(state=state or 'INCOMPLETE_PREPARATION',installation_performed=False)
            if state is None:
                for i,n in enumerate(CHANGES):
                    if not c.exists(self.root,TX+f'/before-{i}'):
                        c.atomic_write(self.root,TX+f'/before-{i}',self.read(n),0o600,0)
                self.write_journal('PREPARED');state='PREPARED'
            if action in ('apply','resume'):
                require(state in ('PREPARED','APPLIED'),'cannot reapply rolled-back transaction')
                desired,terminal='new','APPLIED'
            else:
                desired,terminal='old','ROLLED_BACK'
                if state!='ROLLED_BACK':self.write_journal('ROLLING_BACK')
            for i,n in enumerate(CHANGES):
                self.invariants();self.verify_files('mixed')
                raw=self.payload[n] if desired=='new' else c.file_bytes(self.root,TX+f'/before-{i}',0o600,0)
                if self.read(n)!=raw:c.atomic_write(self.root,n,raw,int(self.spec['updates'][n]['mode'],8),0)
            self.verify_files(desired);self.invariants();self.write_journal(terminal)
            return dict(state=terminal,history_preserved=True,config_templates_preserved=True,
                activation_performed=False,daemon_reload_performed=False,redis_writes=False,finam_contact=False)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('action',choices=('prepare','verify','preflight','apply','resume','rollback'))
    p.add_argument('directory',type=Path)
    p.add_argument('--snapshot',type=Path);p.add_argument('--build',type=Path);p.add_argument('--manifests',type=Path)
    p.add_argument('--confirm')
    a=p.parse_args()
    if a.action=='prepare':
        require(a.confirm is None,'local preparation has no operational confirmation')
        prepare(a.directory,a.snapshot,a.build,a.manifests)
    if a.action in ('prepare','verify'):
        spec,_,_,_=load(a.directory)
        print(json.dumps(dict(status='PREPARED_NOT_INSTALLED',write_slots=len(spec['updates']),
            new_manifest_sha256=spec['new_manifest_sha256'],operational_o3=False),sort_keys=True));return
    require(os.geteuid()==0,'root required')
    require(a.confirm==(None if a.action=='preflight' else ROLLBACK if a.action=='rollback' else CONFIRM),'separate owner authorization required')
    print(json.dumps(Update(Path('/'),a.directory).transition(a.action),sort_keys=True))


if __name__=='__main__':main()
