#!/usr/bin/env python3
"""Small offline packaging regression suite. No runtime/keys/network."""
import argparse
import io
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

import stage8b_p1f_o3_source_handoff as h


class PackagingTests(unittest.TestCase):
    def templates(self):
        return {h.TEMPLATES + n + ".service.in":
                (h.ROOT / h.TEMPLATES / (n + ".service.in")).read_bytes() for n in h.UNITS}

    def test_templates_positive(self):
        h.validate_templates(self.templates())

    def test_templates_do_not_gain_retry_or_initialization(self):
        for unit in h.UNITS:
            name = h.TEMPLATES + unit + ".service.in"
            for old, new in ((b"Restart=no", b"Restart=on-failure"),
                             (b"@PHASE_SHA256@", b"arbitrary"),
                             (b"ExecStart=", b"ExecStartPre="),
                             (b"Type=exec", b"Type=exec\n[Install]"),
                             (b"Type=exec", b"Type=exec\nSuccessExitStatus=70")):
                with self.subTest(unit=unit, new=new):
                    files = self.templates()
                    files[name] = files[name].replace(old, new)
                    with self.assertRaises(ValueError):
                        h.validate_templates(files)

    def test_render_is_local_new_directory_only(self):
        with tempfile.TemporaryDirectory(dir="/tmp") as tmp:
            out = Path(tmp) / "units"
            args = argparse.Namespace(output=out, phase="a" * 64)
            h.render(args)
            self.assertEqual(len(list(out.iterdir())), 2)
            self.assertNotIn(b"@PHASE_SHA256@", next(out.iterdir()).read_bytes())
            with self.assertRaises(FileExistsError):
                h.render(args)
            for phase in ("A" * 64, "x" * 64, "a" * 63, "a" * 64 + "\nExecStart=bad"):
                with self.subTest(phase=phase), self.assertRaises(ValueError):
                    h.render(argparse.Namespace(output=Path(tmp) / "bad", phase=phase))
        with self.assertRaises(ValueError):
            h.render(argparse.Namespace(output=Path("/etc/o3-must-not-exist"), phase="a" * 64))

    def test_unsafe_names_and_private_payload(self):
        for name in ("../bad", "/etc/file", "a/../b", "a//b", "a\\b",
                     "a/./b", ".git/config", "target/debug/a", ".env",
                     "evidence/key.ed25519", "issuer-private-keys/x", "a\nb"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                h.safe_name(name)
        h.safe_name(".github/workflows/ci.yml")
        h.safe_name(".env.example")
        with self.assertRaises(ValueError):
            h.scan("innocent.txt", b"-----BEGIN OPENSSH " + b"PRIVATE KEY-----")

    def test_source_drift_rejected_even_if_result_stays_pass(self):
        old = {"crates/test/src/lib.rs": h.sha(b"qualified"), "Cargo.lock": h.sha(b"lock")}
        read = lambda _: h.encode(old)
        files = {"crates/test/src/lib.rs": b"qualified", "Cargo.lock": b"lock"}
        h.bind_source(files, read)
        for name in old:
            with self.subTest(name=name), self.assertRaises(ValueError):
                h.bind_source(dict(files, **{name: b"changed"}), read)
        with self.assertRaises(ValueError):
            h.bind_source(dict(files, **{"crates/new/build.rs": b"new"}), read)
        with self.assertRaises(ValueError):
            h.bind_source({"Cargo.lock": b"lock"}, read)
        changed = dict(files, **{"docs/stage-8/stage8b-p1f-o3-source-handoff.md": b"packaging"})
        self.assertIn("docs/stage-8/stage8b-p1f-o3-source-handoff.md",
                      h.bind_source(changed, read)["production"]["post_proof_packaging_only_changes"])

    def test_result_mutation_rejected(self):
        with self.assertRaises(ValueError):
            h.evidence_check(lambda _: b'{"all_component_checks_passed": true}')

    def test_git_tree_reconstructed_including_modes(self):
        files, modes = {}, {}
        with zipfile.ZipFile(io.BytesIO(h.git("archive", "--format=zip", "HEAD"))) as z:
            for item in z.infolist():
                if item.is_dir():
                    continue
                files[item.filename] = z.read(item)
                modes[item.filename] = "100755" if item.external_attr >> 16 & 0o111 else "100644"
        actual = h.git("rev-parse", "HEAD^{tree}").decode().strip()
        self.assertEqual(h.tree_hash(files, modes), actual)
        name = sorted(files)[0]
        altered = dict(modes, **{name: "100755" if modes[name] == "100644" else "100644"})
        self.assertNotEqual(h.tree_hash(files, altered), actual)


if __name__ == "__main__":
    unittest.main(verbosity=2)
