import unittest
from pathlib import Path
import stage8b_p1f_o2_boot_id_check as c

class Bootstrap(unittest.TestCase):
    def setUp(self):
        self.raw=(c.ROOT/'deploy/stage8b-p1e/moex-finam-p1-paper-bootstrap.service').read_text()

    def test_bootstrap_keeps_policy_and_real_boot_id(self):
        p=c.bootstrap_properties(self.raw)
        self.assertEqual(p['ProcSubset'],'all')
        self.assertEqual(p['User'],'moex-p1-paper')
        self.assertIn('LoadCredential',p)

    def test_conflict_and_boundary_changes_refused(self):
        for old,new in [('ProcSubset=all','ProcSubset=pid'),
                        ('PrivateNetwork=yes','PrivateNetwork=no'),
                        ('ProtectKernelTunables=yes','ProtectKernelTunables=no'),
                        ('CapabilityBoundingSet=','CapabilityBoundingSet=CAP_SYS_ADMIN'),
                        ('User=moex-p1-paper','User=root'),
                        ('RestrictAddressFamilies=AF_UNIX','RestrictAddressFamilies=AF_UNIX AF_INET')]:
            with self.subTest(old=old),self.assertRaises(ValueError):
                c.bootstrap_properties(self.raw.replace(old,new))

if __name__=='__main__':unittest.main()
