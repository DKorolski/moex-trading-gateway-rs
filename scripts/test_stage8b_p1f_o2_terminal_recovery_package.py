"""Unit controls for artifact evidence binding; synthetic bytes are never delivered."""
import copy
import unittest

import stage8b_p1f_o2_terminal_recovery_package as package


class BuildBinding(unittest.TestCase):
    def setUp(self):
        self.raw_commit = b"tree " + b"1" * 40 + b"\n\nsynthetic unit fixture\n"
        self.ref = package.handoff.git_objects.git_object_id("commit", self.raw_commit)
        self.tree = "1" * 40
        self.binary = b"\x7fELF\x02\x01" + bytes(12) + b"\x3e\x00" + bytes(80)
        self.log = b"COMMAND cargo build --locked --release -p runtime-durable-service --bin stage8b-p1f-o2-operator\nFinished `release`\n"
        self.build = {
            "implementation_ref": self.ref, "source_tree": self.tree,
            "rust_image": package.builder.IMAGE, "platform": "linux/amd64",
            "cargo_args": package.CARGO_ARGS, "network": "none", "cargo_offline": True,
            "execution_authorized": False, "target_mutation_performed": False,
            "build_log_sha256": package.sha(self.log), "source_commit_raw_sha256": package.sha(self.raw_commit),
            "binaries": [{"name": package.OPERATOR, "sha256": package.sha(self.binary), "size": len(self.binary), "elf_machine": "x86-64"}],
        }

    def validate(self, value=None, **kwargs):
        package.validate_build(value or self.build, self.ref, self.tree,
                               kwargs.get("binary", self.binary), kwargs.get("log", self.log),
                               kwargs.get("raw_commit", self.raw_commit))

    def test_exact_binding(self):
        self.validate()

    def test_wrong_build_controls(self):
        mutations = {
            "implementation_ref": "0" * 40, "source_tree": "0" * 40,
            "rust_image": "rust:latest", "platform": "linux/arm64",
            "cargo_args": package.CARGO_ARGS + ["--all-features"], "network": "host",
            "cargo_offline": False, "execution_authorized": True, "target_mutation_performed": True,
            "build_log_sha256": "0" * 64, "source_commit_raw_sha256": "0" * 64,
            "binaries": self.build["binaries"] * 2,
        }
        for key, value in mutations.items():
            with self.subTest(key=key), self.assertRaises(SystemExit):
                changed = copy.deepcopy(self.build)
                changed[key] = value
                self.validate(changed)

    def test_changed_blob_log_or_raw_commit(self):
        for key, value in (("binary", self.binary + b"changed"), ("log", self.log + b"changed"),
                           ("raw_commit", self.raw_commit + b"changed")):
            with self.subTest(key=key), self.assertRaises(SystemExit):
                self.validate(**{key: value})

    def test_wrong_elf_with_consistent_hash_rejects(self):
        binary = self.binary[:18] + b"\xb7\x00" + self.binary[20:]
        changed = copy.deepcopy(self.build)
        changed["binaries"][0]["sha256"] = package.sha(binary)
        with self.assertRaises(SystemExit):
            self.validate(changed, binary=binary)

    def test_protected_inventory_boundary(self):
        for name in ("Cargo.toml", "Cargo.lock", "crates/a/src/lib.rs", "deploy/x.service", ".github/workflows/ci.yml", "config/a.json"):
            self.assertTrue(package.protected(name), name)
        self.assertFalse(package.protected("docs/current-status.md"))


if __name__ == "__main__":
    unittest.main()
