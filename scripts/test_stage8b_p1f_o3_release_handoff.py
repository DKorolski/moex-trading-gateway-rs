#!/usr/bin/env python3
"""Offline negative controls; regenerated outer index cannot hide inner drift."""
import json
import os
from pathlib import Path
import tempfile
import unittest
import zipfile

import stage8b_p1f_o3_release_handoff as m


class Handoff(unittest.TestCase):
    def setUp(self):
        self.archive=Path(os.environ['O3_RELEASE_ZIP'])
        self.tmp=tempfile.TemporaryDirectory()

    def tearDown(self):self.tmp.cleanup()

    def mutate(self,fn,mode_file=None):
        with zipfile.ZipFile(self.archive) as z:
            infos={i.filename:i for i in z.infolist()};data={n:z.read(n) for n in infos}
        fn(data)
        data[m.P+'files-sha256.json']=m.encode({n:m.sha(raw) for n,raw in data.items() if n!=m.P+'files-sha256.json'})
        path=Path(self.tmp.name)/'mutated.zip'
        with zipfile.ZipFile(path,'w',zipfile.ZIP_DEFLATED) as z:
            for n,raw in data.items():
                i=infos[n]
                if n==mode_file:i.external_attr=(0o100755<<16)
                z.writestr(i,raw)
        with self.assertRaises((ValueError,RuntimeError,KeyError)):
            m.verify(path)

    def test_positive(self):self.assertEqual(m.verify(self.archive)['status'],'PASS')

    def test_elf_changed(self):
        self.mutate(lambda d:d.__setitem__(m.P+'installation/payload/stage8b-p1f-o3-signer',
            d[m.P+'installation/payload/stage8b-p1f-o3-signer']+b'changed'))

    def test_commit_forgery(self):
        self.mutate(lambda d:d.__setitem__(m.P+'package-commit.raw',d[m.P+'package-commit.raw']+b'x'))

    def test_cargo_source_drift(self):
        self.mutate(lambda d:d.__setitem__('Cargo.toml',d['Cargo.toml']+b'\n# drift\n'))

    def test_source_mode_drift(self):self.mutate(lambda d:None,mode_file='Cargo.toml')

    def test_qualification_inventory_reduction(self):
        def alter(d):
            n=m.P+'qualification/result.json';v=json.loads(d[n]);v['records'].pop();d[n]=m.encode(v)
        self.mutate(alter)

    def test_forged_negative_service_success(self):
        def alter(d):
            n=m.P+'qualification/logs/signer-result.log';d[n]=d[n].replace(b'ExecMainStatus=255',b'ExecMainStatus=0')
            q=m.P+'qualification/result.json';v=json.loads(d[q])
            next(x for x in v['records'] if x['id']=='signer-result')['sha256']=m.sha(d[n]);d[q]=m.encode(v)
        self.mutate(alter)

    def test_template_execution_changed(self):
        self.mutate(lambda d:d.__setitem__(m.P+'installation/payload/moex-finam-p1f-o3-signer.service.in',
            d[m.P+'installation/payload/moex-finam-p1f-o3-signer.service.in'].replace(b'Restart=no',b'Restart=always')))

    def test_install_scope_opened(self):
        def alter(d):
            n=m.P+'installation/spec.json';v=json.loads(d[n]);v['activation_authorized']=True;d[n]=m.encode(v)
        self.mutate(alter)

    def require_native(self):
        with zipfile.ZipFile(self.archive) as z:
            if m.native.PREFIX+'run/result.json' not in z.namelist():
                self.skipTest('historical partial archive has no native witness')

    def test_native_evidence_cannot_be_stripped(self):
        self.require_native()
        def alter(d):
            for n in list(d):
                if n.startswith(m.native.PREFIX):del d[n]
        self.mutate(alter)

    def test_native_result_flags_and_bindings(self):
        self.require_native()
        for key,value in [('operational_o3',True),('installation_performed',True),
                          ('rust_cargo_changed',True),('selected_key_removed',False),
                          ('containers_removed',False),('host_state_used',True),
                          ('supervisor_stop_seconds',45),('driver_sha256','0'*64)]:
            with self.subTest(key=key):
                def alter(d):
                    n=m.native.PREFIX+'run/result.json';v=json.loads(d[n]);v[key]=value;d[n]=m.encode(v)
                self.mutate(alter)

    def test_native_semantics_with_rehashed_command_records(self):
        self.require_native()
        def child(v):
            u=v['supervisor'];return next(p for pid,p in u['processes'].items() if pid!=u['MainPID'])
        def credential(v,key,value):
            p=child(v);p['credentials']=[key+':\t'+value if s.startswith(key+':') else s for s in p['credentials']]
        cases=[
            ('active-stable',lambda v:credential(v,'Uid','0\t0\t0\t0')),
            ('active-stable',lambda v:credential(v,'CapEff','0000000000000080')),
            ('active-stable',lambda v:credential(v,'CapInh','0000000000000080')),
            ('active-stable',lambda v:credential(v,'NoNewPrivs','0')),
            ('active-stable',lambda v:child(v).__setitem__('start_ticks','1')),
            ('active-stable',lambda v:v['supervisor'].__setitem__('NRestarts','1')),
            ('all-units-stopped',lambda v:v['signer'].__setitem__('ExecMainStatus','0')),
            ('all-units-stopped',lambda v:v['supervisor'].__setitem__('MainPID','123')),
            ('all-recorded-identities-gone',lambda v:v.__setitem__('233',{'state':'S'})),
            ('isolation',lambda v:v[0]['HostConfig'].__setitem__('NetworkMode','host')),
            ('host-global-after',lambda v:v.__setitem__('containers',['foreign-container'])),
        ]
        for idx,(ident,fn) in enumerate(cases):
            with self.subTest(case=idx,record=ident):
                def alter(d):
                    prefix=m.native.PREFIX+'run/';path=prefix+'logs/'+ident+'.stdout'
                    v=json.loads(d[path]);fn(v);d[path]=m.encode(v)
                    result=json.loads(d[prefix+'result.json'])
                    record=next(v for v in result['records'] if v['id']==ident)
                    record['stdout_sha256']=m.sha(d[path])
                    d[prefix+'logs/'+ident+'.json']=m.encode(record)
                    d[prefix+'result.json']=m.encode(result)
                self.mutate(alter)


if __name__=='__main__':unittest.main(verbosity=2)
