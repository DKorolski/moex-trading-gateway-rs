#!/usr/bin/env python3
"""Fixed two-slot correction after the October-8 FAILED/1/15 frontier.

Reuse the accepted custody/atomic-write/locking primitives. No daemon-reload,
signing, phase mutation or service start. Retire three generated inputs by rename
into a root-only transaction; never copy the private key into exported evidence.
"""
import argparse
import copy
import fcntl
import json
import os
from pathlib import Path
import stat

import stage8b_p1f_o2_terminal_install as retained
import stage8b_p1f_o2_boot_id_check as contract
c=retained.custody
ROOT=Path('/')
OLD='309ab4fc557f44dcbc56cc081bb32b125abd2dc8470c3062faa88421d20b2974'
HEAD='645ad005c1ce7c6686facfd5ba160f73a91f89b3801d5a5653a3f33911e2c6a6'
UNIT='/etc/systemd/system/moex-finam-p1-paper-o2-bootstrap-runner.service'
MANIFEST=c.MANIFEST
TX=c.BASE+'/o2-boot-id-after-failed-15-20261008'
CONFIG='/etc/moex-finam-p1-paper'
STATE='/var/lib/moex-finam-p1-paper/state'
RETIRED=('/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json',
         '/etc/moex-finam-p1-paper/supervisor.json',
         '/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key')
CONFIRM='INSTALL_BOOT_ID_FIX_PRESERVE_FAILED_15_AND_RETIRE_UNUSED_INPUTS'

def meta(path,secret=False):
    s=path.lstat()
    c.require(stat.S_ISREG(s.st_mode) and s.st_nlink==1 and not path.is_symlink(),'file custody')
    result=dict(uid=s.st_uid,gid=s.st_gid,mode=oct(stat.S_IMODE(s.st_mode)),nlink=s.st_nlink,size=s.st_size,type=stat.S_IFMT(s.st_mode),exists=True)
    if not secret:result['sha256']=c.sha(c.file_bytes(ROOT,str(path),stat.S_IMODE(s.st_mode),s.st_gid))
    return result

def validate_payload(old_raw,new_raw,old_unit,new_unit,installer_sha):
    c.require(c.sha(old_raw)==OLD,'wrong installation predecessor')
    old=retained.strict_json(old_raw);new=retained.strict_json(new_raw)
    contract.unit_properties(new_unit.decode())
    normalized=new_unit.decode().replace('# Production guardian reads /proc/sys/kernel/random/boot_id before admission.\n','').replace('# Keep kernel tunables read-only and all other sandbox restrictions below.\n','').replace('ProcSubset=all','ProcSubset=pid').encode()
    c.require(normalized==old_unit,'unit delta is not solely ProcSubset correction')
    wanted=copy.deepcopy(old)
    wanted['payload'][UNIT].update(sha256=c.sha(new_unit),size=len(new_unit))
    wanted.update(predecessor_manifest_sha256=OLD,installer_sha256=installer_sha,
                  update_revision='boot-id-sandbox-after-failed-15-v1')
    c.require(new==wanted and new_raw==c.canonical(wanted),'unexpected installation delta')
    return old,new

class Install:
    def __init__(self,directory):
        self.directory=directory.resolve(strict=True)
        self.spec=retained.strict_json((directory/'spec.json').read_bytes())
        self.expected=retained.strict_json((directory/'before.json').read_bytes())
        self.payload={UNIT:(directory/'runner.service').read_bytes(),MANIFEST:(directory/'installation-o2-v1.json').read_bytes()}
        self.old_unit=(directory/'runner.before.service').read_bytes()
        self.old_manifest=(directory/'installation.before.json').read_bytes()
        c.require(set(self.spec)=={'domain','source_ref','before_sha256','new_manifest_sha256','installer_sha256'},'spec schema')
        c.require(self.spec['domain']=='o2-boot-id-correction-v1' and len(self.spec['source_ref'])==40,'spec identity')
        c.require(c.sha((directory/'before.json').read_bytes())==self.spec['before_sha256'],'before binding')
        own=c.sha(Path(__file__).read_bytes())
        c.require(own==self.spec['installer_sha256'],'installer binding')
        self.old,self.new=validate_payload(self.old_manifest,self.payload[MANIFEST],self.old_unit,self.payload[UNIT],own)
        c.require(c.sha(self.payload[MANIFEST])==self.spec['new_manifest_sha256'],'new manifest binding')
        e=self.expected
        c.require(e['history_head']['state']=='FAILED' and e['history_head']['authority_generation']==1
                  and e['history_head']['latest_sequence']==15 and e['history_head']['latest_event_sha256']==HEAD
                  and e['history_head']['active_manifest_sha256'] is None,'wrong terminal frontier')
        self.retired_meta={p:e['config_inventory'][str(Path(p).relative_to(CONFIG))] for p in RETIRED}
        receipt=json.loads(e['public_phase_files']['materialized-set-receipt.json']['text'])
        c.require(self.retired_meta[RETIRED[0]]['sha256']==receipt['source_sha256']
                  and self.retired_meta[RETIRED[1]]['sha256']==receipt['final_config_sha256'],'generated inputs not bound')
        key=self.retired_meta[RETIRED[2]]
        c.require(key['uid']==key['gid']==0 and key['mode']=='0o400' and key['size']==32,'unused key custody')
        c.require(e['durable_inventory']=={k:v for k,v in e['durable_inventory'].items() if k in ('.','.stage8b-p1-first-boot-quarantine')},'bootstrap already used state')

    def paths(self,i):return Path(RETIRED[i]),Path(TX)/('retired-'+str(i))

    def invariants(self):
        e=self.expected
        c.require(retained.inventory(Path(c.CONTROL))==e['authority_inventory'],'authority drift')
        c.require(retained.inventory(Path(STATE))==e['durable_inventory'],'state drift')
        c.require(retained.inventory(Path(c.STAGING))==e['staging_inventory'],'staging drift')
        for path,key in (*retained.RETAINED_DIRS,(retained.TRANSACTION,'transaction_inventory')):
            c.require(retained.inventory(Path(path))==e['retained_inventories'][key],'retained tree drift: '+key)
        for unit,observed in e['units'].items():
            keys=list(observed);keys.remove('NeedDaemonReload')
            raw=c.command(['systemctl','show',unit,'--property='+','.join(keys)])
            current=dict(line.split('=',1) for line in raw.splitlines())
            c.require(current=={k:v for k,v in observed.items() if k!='NeedDaemonReload'},'unit/P0 drift: '+unit)
        c.require(not c.command(['systemctl','list-jobs','--no-legend','--no-pager']).strip(),'systemd jobs')
        for name,key in (('moex-finam-p1-o2-once-20261008.service','scheduler_service'),
                         ('moex-finam-p1-o2-once-20261008.timer','scheduler_timer')):
            props=e[key]
            raw=c.command(['systemctl','show',name,'--property='+','.join(props)])
            c.require(dict(line.split('=',1) for line in raw.splitlines())==props,'current exhausted caller drift')
        for name,expected in e['exhausted_units'].items():
            raw=c.command(['systemctl','show',name,'--property='+','.join(expected)])
            c.require(dict(line.split('=',1) for line in raw.splitlines())==expected,'old schedule drift')
        for name,digest in e['unit_file_sha256'].items():
            if '/etc/systemd/system/'+name!=UNIT:
                c.require(c.sha(c.file_bytes(ROOT,'/etc/systemd/system/'+name,0o644,0))==digest,'retained unit changed')
        for name in ('/root/moex-o2-bounded-20261008/auth.private.json','/run/moex-finam-p1f-o2-input',
                     '/run/credentials/moex-finam-p1f-o2-materializer.service','/run/credentials/moex-finam-p1-paper-bootstrap.service'):
            c.require(not os.path.lexists(name),'transient credentials')
        for path,entry in self.old['payload'].items():
            digest=c.sha(c.file_bytes(ROOT,path,int(entry['mode'],8),987 if entry['group']=='service' else 0))
            allowed={entry['sha256']}
            if path==UNIT:allowed.add(self.new['payload'][UNIT]['sha256'])
            c.require(digest in allowed,'foreign installed bytes: '+path)
        c.require(c.sha(c.file_bytes(ROOT,MANIFEST,0o644,0)) in {OLD,self.spec['new_manifest_sha256']},'manifest drift')
        for i in range(3):
            source,dest=self.paths(i)
            present=[p for p in (source,dest) if os.path.lexists(p)]
            c.require(len(present)==1,'retired input missing/duplicated')
            c.require(meta(present[0],i==2)==self.retired_meta[RETIRED[i]],'retired input drift')
        actual_paths={str(p.relative_to(CONFIG)) for p in [Path(CONFIG),*Path(CONFIG).rglob('*')]}
        expected_paths=set(e['config_inventory'])
        for i in range(3):
            source,dest=self.paths(i)
            if dest.exists():expected_paths.remove(str(source.relative_to(CONFIG)))
        c.require(actual_paths==expected_paths,'foreign config path')
        for name in actual_paths:
            p=Path(CONFIG)/name;expected=e['config_inventory'][name]
            if p.is_file():c.require(meta(p,'credentials' in p.parts)==expected,'config bytes/custody drift')
            else:
                s=p.lstat()
                c.require(stat.S_ISDIR(s.st_mode) and s.st_uid==expected['uid'] and s.st_gid==expected['gid']
                          and oct(stat.S_IMODE(s.st_mode))==expected['mode'],'config directory custody')

    def journal(self,state):
        c.atomic_write(ROOT,TX+'/journal.json',c.canonical(dict(domain='o2-boot-id-correction-v1',state=state,
            old_manifest_sha256=OLD,new_manifest_sha256=self.spec['new_manifest_sha256'],history_head_sha256=HEAD)),0o600,0)

    def apply(self):
        c.require(os.geteuid()==0 and __debug__,'normal root Python required')
        c.require(not os.path.lexists(TX),'transaction exists; no automatic retry')
        fds=[]
        try:
            for name in ('.execution.lock','.guardian.lock'):
                p=Path(c.CONTROL)/name
                c.file_bytes(ROOT,str(p),0o600,0)
                fd=os.open(p,os.O_RDWR|os.O_NOFOLLOW);fds.append(fd)
                fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB)
            self.invariants()
            c.require(c.sha(c.file_bytes(ROOT,UNIT,0o644,0))==c.sha(self.old_unit)
                      and c.sha(c.file_bytes(ROOT,MANIFEST,0o644,0))==OLD,'already changed')
            c.old.verify_secure_directory_chain(ROOT,Path(TX).parent)
            Path(TX).mkdir(mode=0o700);c.sync_dir(Path(TX).parent)
            for i,raw in enumerate((self.old_unit,self.old_manifest)):
                c.atomic_write(ROOT,TX+'/before-'+str(i),raw,0o600,0)
            self.journal('PREPARED')
            for i in range(3):
                self.invariants()
                source,dest=self.paths(i)
                c.require(not os.path.lexists(dest),'archive exists')
                os.rename(source,dest);c.sync_dir(source.parent);c.sync_dir(dest.parent)
                self.invariants()
            for path in (UNIT,MANIFEST):
                self.invariants();c.atomic_write(ROOT,path,self.payload[path],0o644,0);self.invariants()
            self.journal('APPLIED')
            print(json.dumps(dict(status='APPLIED_NOT_RELOADED_NOT_ACTIVATED',changed_files=[UNIT,MANIFEST],
                 generated_inputs_retired=3,private_key_exported=False,history_unchanged=True,
                 new_manifest_sha256=self.spec['new_manifest_sha256'])))
        finally:
            for fd in reversed(fds):os.close(fd)

def main():
    p=argparse.ArgumentParser();p.add_argument('--input',type=Path,required=True);p.add_argument('--confirm',required=True)
    a=p.parse_args();c.require(a.confirm==CONFIRM,'explicit owner authorization required')
    Install(a.input).apply()

if __name__=='__main__':main()
