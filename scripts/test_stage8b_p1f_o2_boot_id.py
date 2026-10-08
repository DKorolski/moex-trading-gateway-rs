import unittest
from pathlib import Path
from unittest.mock import patch
import stage8b_p1f_o2_boot_id_check as check

class BootIdContract(unittest.TestCase):
    def setUp(self):self.raw=(check.ROOT/check.UNIT).read_text()

    def test_corrected_unit(self):
        self.assertEqual(check.unit_properties(self.raw),check.REQUIRED)

    def test_old_proc_subset_refused(self):
        with self.assertRaises(ValueError):check.unit_properties(self.raw.replace('ProcSubset=all','ProcSubset=pid'))

    def test_other_sandbox_changes_refused(self):
        for key,value in [('PrivateNetwork','no'),('ProtectKernelTunables','no'),('ProtectProc','default'),
                          ('CapabilityBoundingSet','CAP_SYS_ADMIN'),('RestrictAddressFamilies','AF_UNIX AF_INET'),
                          ('ReadWritePaths','/'),('NoNewPrivileges','no'),('ExecStart','/bin/true')]:
            with self.subTest(key=key),self.assertRaises(ValueError):
                check.unit_properties(self.raw.replace(key+'='+check.REQUIRED[key],key+'='+value))

    def test_no_runtime_start_in_probe(self):
        with patch.object(check.sys,'platform','linux'),patch.object(check.os,'geteuid',return_value=0),\
             patch.object(Path,'read_bytes',return_value=b'boot'),patch.object(check.subprocess,'run') as run:
            import hashlib,json
            from types import SimpleNamespace
            run.side_effect=[SimpleNamespace(returncode=0,stdout=b'{"subset":"pid"}',stderr=b''),
                SimpleNamespace(returncode=0,stdout=json.dumps({'subset':'all','boot_id_sha256':hashlib.sha256(b'boot').hexdigest()}).encode(),stderr=b'')]
            check.native_probe(check.REQUIRED)
            for call in run.call_args_list:
                args=call.args[0]
                self.assertFalse(any('ExecStart=' in arg or 'ExecStopPost=' in arg for arg in args))
                self.assertTrue(any('TimeoutStartSec=15s' in arg for arg in args))
                self.assertEqual(args[-4:-2],['-B','-c'])
                self.assertNotIn('guardian-',args[-2])
                self.assertNotIn('systemctl',args[-2])

if __name__=='__main__':unittest.main()
