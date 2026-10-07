import copy
import unittest

import stage8b_p1f_o2_abort_artifact as a


class ArtifactChecks(unittest.TestCase):
    def fixture(self):
        tree = "1" * 40
        raw_commit = f"tree {tree}\nparent {a.ACCEPTED}\n\nfixture\n".encode()
        ref = a.handoff.git_objects.git_object_id("commit", raw_commit)
        elf = b"\x7fELF\x02\x01" + bytes(12) + b"\x3e\x00" + bytes(40)
        binaries = {n: elf for n in a.builder.BINS}
        log = b"COMMAND cargo build --locked --release\nFinished `release`\n"
        info = dict(implementation_ref=ref, source_tree=tree, rust_image=a.builder.IMAGE,
            cargo_args=a.builder.CARGO_ARGS, platform="linux/amd64", network="none", cargo_offline=True,
            execution_authorized=False, target_mutation_performed=False, build_log_sha256=a.sha(log),
            source_commit_raw_sha256=a.sha(raw_commit), binaries=[dict(name=n, sha256=a.sha(elf),
            size=len(elf), elf_machine="x86-64") for n in binaries])
        return info, ref, tree, raw_commit, log, binaries

    def test_valid_build(self):
        a.validate_build(*self.fixture())

    def test_build_mutations(self):
        for field, bad in [("implementation_ref", "0" * 40), ("source_tree", "0" * 40),
            ("network", "host"), ("cargo_offline", False), ("rust_image", "rust:latest"),
            ("execution_authorized", True), ("target_mutation_performed", True),
            ("build_log_sha256", "0" * 64), ("source_commit_raw_sha256", "0" * 64),
            ("cargo_args", ["build"]), ("platform", "linux/arm64")]:
            with self.subTest(field=field):
                args = list(copy.deepcopy(self.fixture()))
                args[0][field] = bad
                with self.assertRaises(SystemExit):
                    a.validate_build(*args)

    def test_elf_inventory_and_bytes(self):
        for case in range(4):
            args = list(copy.deepcopy(self.fixture()))
            if case == 0:
                args[0]["binaries"].pop()
            elif case == 1:
                args[0]["binaries"][1] = args[0]["binaries"][0]
            elif case == 2:
                args[-1][a.builder.BINS[0]] += b"changed"
            else:
                args[0]["binaries"][0]["elf_machine"] = "arm64"
            with self.subTest(case=case), self.assertRaises(SystemExit):
                a.validate_build(*args)

    def test_abort_log_selection(self):
        raw = b"QUALIFICATION uid=0 gid=987 umask=0077\n"
        raw += b"".join(f"PASS abort nonmutating negative {n}\n".encode() for n in range(28))
        raw += b"".join(f"PASS abort reopen frontier {n}: ACTIVE/1/11 -> EXPIRED/1/12\n".encode()
                        for n in [0, 2, 40, 41, 42, 43, 44, 45, 46, 5, 47, 52, 48, 51, 49])
        raw += b"test result: ok. 3 passed; 0 failed; 0 ignored;\n"
        raw += b"PASS Linux root:987 abort archive/consume/reopen under umask0077\n"
        a.validate_abort_log(raw)
        for old, new in [(b"gid=987", b"gid=0"), (b"umask=0077", b"umask=0022"),
                         (b"3 passed", b"0 passed"), (b"0 ignored", b"3 ignored"),
                         (b"PASS abort reopen frontier", b"skipped frontier")]:
            with self.subTest(old=old), self.assertRaises(SystemExit):
                a.validate_abort_log(raw.replace(old, new))


if __name__ == "__main__":
    unittest.main()
