"""Offline tests. Never run SSH/Docker/OpenSSL or open a ceremony credential."""
import contextlib
import copy
import datetime as dt
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import stage8b_p1f_o3_readmission_qualify as m


class Qualification(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.evidence=Path(os.environ['O3_READMISSION_EVIDENCE'])
        cls.package=cls.evidence/'installation-prepared-r1'
        cls.support=cls.evidence/'native-support'
        cls.observer=cls.evidence/'commands/support-build-after/remote.py'

    def args(self,action,out):
        return [action,str(self.package),str(out),'--support',str(self.support),'--observer',str(self.observer)]

    def test_weekend_and_outside_window_rejected(self):
        for stamp in ('2026-10-10T07:06:00+00:00','2026-10-11T07:06:00+00:00',
                      '2026-10-12T06:56:00+00:00','2026-10-12T20:06:00+00:00',
                      '2026-10-12T07:05:59+00:00','2026-10-12T07:08:00+00:00',
                      '2026-10-12T07:00:00','2026-10-12T10:06:00+03:00'):
            with self.subTest(stamp=stamp),self.assertRaises(ValueError):
                m.check_window(dt.datetime.fromisoformat(stamp))

    def test_weekday_real_window_boundaries(self):
        for stamp in ('2026-10-12T07:06:00+00:00','2026-10-12T07:07:59.999+00:00',
                      '2026-10-16T19:57:59+00:00'):
            self.assertEqual(m.check_window(dt.datetime.fromisoformat(stamp))['utc'],dt.datetime.fromisoformat(stamp).isoformat())

    def test_weekend_run_rejects_before_inputs_key_ssh_or_output(self):
        class Sunday(dt.datetime):
            @classmethod
            def now(cls,tz=None):return cls(2026,10,11,7,6,tzinfo=dt.timezone.utc)
        with tempfile.TemporaryDirectory() as tmp:
            out=Path(tmp)/'not-created'
            args=self.args('run',out)+['--authorized-selected-key-native-test','--schedule-key','/NOT-OPENED']
            with patch.object(m.dt,'datetime',Sunday),patch.object(m,'inputs') as inputs,\
                 patch.object(m,'selected_seed') as key,patch.object(m.subprocess,'run') as run:
                with self.assertRaisesRegex(ValueError,'weekday'):m.main(args)
                inputs.assert_not_called();key.assert_not_called();run.assert_not_called()
            self.assertFalse(out.exists())

    def test_run_needs_explicit_authorization(self):
        with tempfile.TemporaryDirectory() as tmp,patch.object(m,'inputs') as inputs,\
             patch.object(m,'selected_seed') as key,patch.object(m.subprocess,'run') as run:
            with self.assertRaisesRegex(ValueError,'authorization'):m.main(self.args('run',Path(tmp)/'out'))
            inputs.assert_not_called();key.assert_not_called();run.assert_not_called()

    def test_prepare_cannot_receive_private_argument(self):
        with tempfile.TemporaryDirectory() as tmp,patch.object(m,'inputs') as inputs:
            for flag in (['--schedule-key','/NOT-OPENED'],['--authorized-selected-key-native-test']):
                with self.assertRaisesRegex(ValueError,'private key argument'):
                    m.main(self.args('prepare',Path(tmp)/'out')+flag)
            inputs.assert_not_called()

    def test_prepare_public_only_exact_nine_payloads(self):
        with tempfile.TemporaryDirectory() as tmp,patch.object(m.subprocess,'run',side_effect=AssertionError('no process allowed')),\
             patch.object(m,'selected_seed',side_effect=AssertionError('no key read')),contextlib.redirect_stdout(io.StringIO()):
            out=Path(tmp)/'out';m.main(self.args('prepare',out))
            v=json.loads((out/'preparation.json').read_bytes())
            self.assertEqual(v['status'],'PREPARED_ONLY_NOT_QUALIFIED')
            self.assertEqual(set(v['payload_sha256']),set(m.base.BINS)|{u+'.in' for u in m.UNITS}|set(m.SUPPORT))
            self.assertEqual(len(v['payload_sha256']),9)
            for flag in ('remote_contact','private_key_read','signature_issued','automatic_launch_scheduled',
                         'operational_o3','installation_performed'):self.assertIs(v[flag],False)
            for n,digest in v['payload_sha256'].items():self.assertEqual(m.sha((out/'payload'/n).read_bytes()),digest)
            self.assertEqual(m.sha((out/'host-collector.py').read_bytes()),m.OBSERVER)
            self.assertFalse((out/'result.json').exists())

    def test_tampered_support_record_payload_collector_or_template_rejected(self):
        read=m.installer.package_file
        cases=[
            self.support/'support-build.json',self.support/'payload/input-helper',self.observer,
            m.ROOT/'deploy/stage8b-p1f-o3'/(m.UNITS[0]+'.in')]
        for target in cases:
            with self.subTest(file=target.name):
                def altered(path):return read(path)+b'changed' if path==target else read(path)
                with patch.object(m.installer,'package_file',side_effect=altered):
                    with self.assertRaises(ValueError):m.inputs(self.package,self.support,self.observer)

    def test_protected_host_validation(self):
        baseline=json.loads((self.package/'baseline.json').read_bytes())
        v=json.loads((self.evidence/'commands/support-build-after/stdout').read_bytes())
        m.check_host(v,baseline)
        for field,value in [('redis_db15_size',1),('remote_writes',True),('ntp_synchronized',False),
                            ('observed',{}),('installed',{}),('old_installation_transaction',{}),
                            ('correction_transaction',{})]:
            changed=copy.deepcopy(v);changed[field]=value
            with self.subTest(field=field),self.assertRaises(ValueError):m.check_host(changed,baseline)

    def test_private_transfer_output_not_leaked_or_forged(self):
        expected=b'PASS selected credential staged in container tmpfs; content omitted\n'
        self.assertEqual(m.private_output(subprocess.CompletedProcess([],0,expected,b'')),(expected,b'',True))
        for code,stdout,stderr in ((0,b'SENSITIVE_FAKE',b''),(0,expected,b'SENSITIVE_FAKE'),
                                   (124,b'SENSITIVE_FAKE',b'SENSITIVE_FAKE'),(1,b'',b'')):
            out,err,valid=m.private_output(subprocess.CompletedProcess([],code,stdout,stderr))
            self.assertFalse(valid);self.assertNotIn(b'SENSITIVE_FAKE',out+err);self.assertNotIn(b'PASS',out)

    def test_private_validation_error_omits_custody_path(self):
        with patch.object(m,'selected_seed',side_effect=FileNotFoundError('/FAKE_PRIVATE_PATH')):
            with self.assertRaisesRegex(ValueError,'private details omitted') as error:
                m.credential_seed(Path('/not-opened'))
            self.assertNotIn('FAKE_PRIVATE_PATH',str(error.exception))

    def test_embedded_container_programs_parse(self):
        for name,code in [('setup',m.base.SETUP),('globals',m.base.GLOBAL_STATE),('extra',m.EXTRA_SETUP),
                          ('after',m.AFTER_PREPARE),('stage',m.STAGE_KEY),('processes',m.PROCESSES)]:
            compile(code,name,'exec')

    def test_public_support_directory_traversable_not_writable(self):
        # docker cp preserves the staged directory's 0700; bootstrap drops UID.
        with tempfile.TemporaryDirectory() as tmp:
            directory=Path(tmp)/'payload';directory.mkdir(mode=0o700)
            for name in m.SUPPORT:
                (directory/name).write_bytes(b'public test placeholder')
                (directory/name).chmod(0o600)
            def mapped(name):
                self.assertTrue(name=='/payload' or name in ('/payload/'+n for n in m.SUPPORT))
                return directory if name=='/payload' else directory/name.rsplit('/',1)[1]
            with patch('pathlib.Path',side_effect=mapped):
                exec(m.PUBLIC_SUPPORT_SETUP,{})
            for entry in (directory,*(directory/n for n in m.SUPPORT)):
                self.assertEqual(entry.stat().st_mode & 0o777,0o755)


if __name__=='__main__':unittest.main(verbosity=2)
