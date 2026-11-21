#!/usr/bin/env python3
"""Stopped, exact O3 release overlay; no reload/start/signing/Redis/bootstrap.

Reuse accepted O2 secure reads, atomic replacement, fsync and both authority
locks. The only transaction extension is explicit absent-before slots, whose
rollback removes only our exact installed bytes. Operational permission is
separate from this installation candidate.
"""
import argparse
import copy
import json
import os
from pathlib import Path
import stat
import sys

import stage8b_p1f_o3_install_inventory as inv
import stage8b_p1f_o3_release as release

c = inv.c
require, sha, canonical, parse = c.require, c.sha, c.canonical, inv.prior.parse
O3 = c.BASE+'/installation-o3-v1.json'
OPERATOR = '/usr/local/libexec/moex/stage8b-p1f-o2-operator'
ELFS = tuple('/usr/local/libexec/moex/'+n for n in release.BINS)
TEMPLATES = tuple(c.BASE+'/moex-finam-p1f-o3-'+n+'.service.in' for n in ('supervisor','signer'))
CHANGES = (*ELFS, *TEMPLATES, O3, c.MANIFEST)
TX = c.BASE+'/o3-fb1ceb3-after-completed-26'
CONFIRM = 'INSTALL_O3_FB1CEB3_PRESERVE_COMPLETED_26_WITHOUT_ACTIVATION'
ROLLBACK = 'ROLLBACK_O3_FB1CEB3_PRESERVE_COMPLETED_26'


def metadata(n, raw):
    return dict(sha256=sha(raw), size=len(raw), mode='0755' if n in ELFS else '0644', owner='root', group='root')


def replacements(snapshot, build, binaries, templates):
    require(build['source_ref'] == release.REF and build['source_tree'] == release.TREE
            and build['fixture_features_enabled'] is False and build['installation_performed'] is False, 'release identity')
    raw = snapshot['old_public_payload'][c.MANIFEST].encode()
    require(sha(raw) == inv.OLD, 'O2 predecessor')
    old = parse(raw)
    require(set(binaries) == set(ELFS) and set(templates) == set(TEMPLATES), 'fixed slot inventory')
    require({b['name'] for b in build['binaries']} == set(release.BINS), 'build binary inventory')
    for b in build['binaries']:
        raw = binaries['/usr/local/libexec/moex/'+b['name']]
        require(sha(raw) == b['sha256'] and len(raw) == b['size'], 'release ELF binding')
        require(raw[:6] == b'\x7fELF\x02\x01' and int.from_bytes(raw[18:20],'little') == 62, 'ELF architecture')
    payload = dict(binaries) | templates
    # The bootstrap compatibility manifest does not own the O2 operator.
    # Preserve it byte-for-byte; only the outer installation owns that slot.
    compat_raw = snapshot['old_public_payload'][c.old.MANIFEST].encode()
    require(sha(compat_raw) == old['payload'][c.old.MANIFEST]['sha256']
            and OPERATOR not in parse(compat_raw)['managed_payload_sha256'], 'compat boundary changed')
    payload[O3] = canonical(dict(schema_version=1, domain='moex.stage8b.p1f.o3.installation.v1',
        source_ref=release.REF, source_tree=release.TREE, predecessor_o2_manifest_sha256=inv.OLD,
        release_build_sha256=sha(canonical(build)), installer_sha256=sha(Path(__file__).read_bytes()),
        payload={n:metadata(n,payload[n]) for n in (*ELFS,*TEMPLATES)},
        operational_inputs_issued=False, units_loaded=False, activation_performed=False))
    new = copy.deepcopy(old)
    for n, raw in payload.items():
        new['payload'][n] = metadata(n, raw)
    new.update(artifact_ref=release.REF, artifact_sha256=sha(canonical(build)),
        installer_sha256=sha(Path(__file__).read_bytes()), predecessor_manifest_sha256=inv.OLD,
        update_revision='o3-fb1ceb3-after-completed-26-v1')
    payload[c.MANIFEST] = canonical(new)
    return old, payload


def prepare(directory, snapshot_dir, build_dir):
    directory.mkdir(parents=True, exist_ok=False)
    snapshot = parse((snapshot_dir/'stdout.json').read_bytes())
    build = parse((build_dir/'build.json').read_bytes())
    require(snapshot['status']=='PASS_READ_ONLY' and snapshot['mutation_performed'] is False
            and snapshot['private_bytes_read'] is False, 'snapshot scope')
    templates = {n:(Path(__file__).resolve().parents[1]/'deploy/stage8b-p1f-o3'/Path(n).name).read_bytes() for n in TEMPLATES}
    old, payload = replacements(snapshot, build, {n:(build_dir/'payload'/Path(n).name).read_bytes() for n in ELFS}, templates)
    release.put(directory/'snapshot.json', canonical(snapshot))
    release.put(directory/'build.json', canonical(build))
    for n, raw in payload.items():
        release.put(directory/'payload'/Path(n).name, raw)
    spec = dict(schema_version=1, source_ref=release.REF, snapshot_sha256=sha(canonical(snapshot)),
        build_sha256=sha(canonical(build)), installer_sha256=sha(Path(__file__).read_bytes()),
        installation_authorized=False, activation_authorized=False, old_manifest_sha256=inv.OLD,
        new_manifest_sha256=sha(payload[c.MANIFEST]),
        updates={n:dict(old_sha256=inv.OLD if n==c.MANIFEST else old['payload'].get(n,{}).get('sha256'),
                        **metadata(n,payload[n])) for n in CHANGES})
    release.put(directory/'spec.json', canonical(spec))
    load(directory)
    return spec


def load(directory):
    require(not directory.is_symlink() and not any(p.is_symlink() for p in directory.rglob('*')), 'package symlink')
    spec = parse((directory/'spec.json').read_bytes())
    snapshot = parse((directory/'snapshot.json').read_bytes()); build = parse((directory/'build.json').read_bytes())
    require(spec['snapshot_sha256'] == sha(canonical(snapshot)) and spec['build_sha256'] == sha(canonical(build)), 'input hashes')
    require(spec['installer_sha256'] == sha(Path(__file__).read_bytes()) and spec['source_ref']==release.REF
            and spec['old_manifest_sha256']==inv.OLD and spec['installation_authorized'] is False
            and spec['activation_authorized'] is False, 'scope/installer binding')
    require({p.name for p in (directory/'payload').iterdir()} == {Path(n).name for n in CHANGES}, 'payload inventory')
    read = lambda n:(directory/'payload'/Path(n).name).read_bytes()
    templates={n:(Path(__file__).resolve().parents[1]/'deploy/stage8b-p1f-o3'/Path(n).name).read_bytes() for n in TEMPLATES}
    old, payload = replacements(snapshot,build,{n:read(n) for n in ELFS},templates)
    require(set(spec['updates'])==set(CHANGES) and spec['new_manifest_sha256']==sha(payload[c.MANIFEST]), 'manifest binding')
    for n in CHANGES:
        expected=dict(old_sha256=inv.OLD if n==c.MANIFEST else old['payload'].get(n,{}).get('sha256'), **metadata(n,payload[n]))
        require(read(n)==payload[n] and spec['updates'][n]==expected, 'exact old/new slot: '+n)
    return spec, snapshot['expected'], old, payload


class Update(inv.prior.previous.Update):
    def __init__(self, root, payload, spec, expected, old):
        self.root=c.old.validate_root(root); self.payload=payload; self.spec=spec
        self.expected=expected; self.old=old; self.uid,self.gid=c.old.account_ids(root)
        require((self.uid,self.gid)==(987,987), 'service account changed')

    def invariants(self):
        require(inv.observe(self.root)==self.expected, 'O2/root/P0/stopped state changed')

    def read(self,n):
        if not c.exists(self.root,n):
            require(n in inv.ADDED, 'existing slot missing')
            return None
        e=self.old['payload'].get(n,metadata(n,b''))
        return c.file_bytes(self.root,n,int(e['mode'],8),987 if e['group']=='service' else 0)

    def verify_files(self,state):
        for n,e in self.old['payload'].items():
            if n not in CHANGES:
                require(sha(self.read(n))==e['sha256'], 'retained payload changed: '+n)
        for n in CHANGES:
            raw=self.read(n); value=sha(raw) if raw is not None else None
            item=self.spec['updates'][n]
            allowed={'old':{item['old_sha256']},'new':{item['sha256']},
                     'mixed':{item['old_sha256'],item['sha256']}}[state]
            require(value in allowed,'foreign/missing slot: '+n)

    def write_journal(self,state):
        c.atomic_write(self.root,TX+'/journal.json',canonical(dict(domain='o3-install-fb1ceb3-v1',
            spec_sha256=sha(canonical(self.spec)), state=state)),0o600,0)

    def journal(self):
        s=self.path(TX).lstat()
        require(stat.S_ISDIR(s.st_mode) and s.st_uid==s.st_gid==0 and stat.S_IMODE(s.st_mode)==0o700, 'transaction custody')
        require({p.name for p in self.path(TX).iterdir()}=={'journal.json', *[f'before-{i}' for i,n in enumerate(CHANGES)
                if self.spec['updates'][n]['old_sha256'] is not None]}, 'transaction inventory')
        j=parse(c.file_bytes(self.root,TX+'/journal.json',0o600,0))
        require(j==dict(domain='o3-install-fb1ceb3-v1',spec_sha256=sha(canonical(self.spec)),state=j['state'])
                and j['state'] in ('PREPARED','APPLIED','ROLLING_BACK','ROLLED_BACK'), 'transaction binding/state')
        for i,n in enumerate(CHANGES):
            expected=self.spec['updates'][n]['old_sha256']
            if expected is not None:
                require(sha(c.file_bytes(self.root,TX+f'/before-{i}',0o600,0))==expected, 'backup corruption')
        return j

    def transition(self,action):
        require(action in ('preflight','apply','resume','rollback'), 'unknown action')
        with self.locks():
            self.invariants()
            if not c.exists(self.root,TX):
                self.verify_files('old')
                if action=='preflight':
                    return dict(state='EXACT_OLD_COMPLETED_26',activation_performed=False)
                require(action=='apply','no prior transaction')
                c.old.verify_secure_directory_chain(self.root,self.path(TX).parent)
                self.path(TX).mkdir(mode=0o700); c.sync_dir(self.path(TX).parent)
                for i,n in enumerate(CHANGES):
                    raw=self.read(n)
                    if raw is not None:
                        c.atomic_write(self.root,TX+f'/before-{i}',raw,0o600,0)
                self.write_journal('PREPARED')
            j=self.journal()
            self.verify_files({'APPLIED':'new','ROLLED_BACK':'old'}.get(j['state'],'mixed'))
            if action=='preflight':
                return dict(state=j['state'],activation_performed=False)
            if action in ('apply','resume'):
                require(j['state'] in ('PREPARED','APPLIED'), 'rolled back transaction cannot be reapplied')
                desired,terminal='new','APPLIED'
            else:
                desired,terminal='old','ROLLED_BACK'
                if j['state']!='ROLLED_BACK':self.write_journal('ROLLING_BACK')
            for i,n in enumerate(CHANGES):
                self.invariants(); self.verify_files('mixed')
                before=self.spec['updates'][n]['old_sha256']
                raw=(self.payload[n] if desired=='new' else
                     c.file_bytes(self.root,TX+f'/before-{i}',0o600,0) if before is not None else None)
                if self.read(n)!=raw:
                    if raw is None:
                        # verify_files('mixed') proves this is our exact new file,
                        # never an arbitrary operator-created slot.
                        self.path(n).unlink(); c.sync_dir(self.path(n).parent)
                    else:
                        c.atomic_write(self.root,n,raw,int(self.spec['updates'][n]['mode'],8),0)
            self.verify_files(desired); self.invariants(); self.write_journal(terminal)
            return dict(state=terminal,history_preserved=True,activation_performed=False,
                        daemon_reload_performed=False,redis_contact=False,finam_contact=False)


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('action',choices=('prepare','preflight','apply','resume','rollback'))
    p.add_argument('package',type=Path); p.add_argument('--snapshot',type=Path)
    p.add_argument('--build',type=Path); p.add_argument('--confirm')
    a=p.parse_args()
    if a.action=='prepare':
        result=prepare(a.package.resolve(),a.snapshot.resolve(),a.build.resolve())
    else:
        require(os.geteuid()==0,'root required')
        require(a.confirm==(None if a.action=='preflight' else ROLLBACK if a.action=='rollback' else CONFIRM), 'exact confirmation required')
        spec,expected,old,payload=load(a.package.resolve())
        result=Update(Path('/'),payload,spec,expected,old).transition(a.action)
    print(json.dumps(result,sort_keys=True))
