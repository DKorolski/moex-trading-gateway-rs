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

    def test_fixed_install_checker_tracks_only_bootstrap_exception(self):
        import stage8b_p1e_i1_fixed_install_check as fixed
        bootstrap=fixed.parse_unit(c.ROOT/fixed.DEPLOY/fixed.BOOTSTRAP)
        fixed.check_shared(bootstrap,bootstrap=True)
        with self.assertRaises(fixed.CheckError):fixed.check_shared(bootstrap)
        for name in (fixed.MAIN,fixed.RECOVERY):
            unit=fixed.parse_unit(c.ROOT/fixed.DEPLOY/name)
            fixed.check_shared(unit)
            with self.assertRaises(fixed.CheckError):fixed.check_shared(unit,bootstrap=True)

if __name__=='__main__':unittest.main()
