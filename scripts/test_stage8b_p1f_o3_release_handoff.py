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


if __name__=='__main__':unittest.main(verbosity=2)
