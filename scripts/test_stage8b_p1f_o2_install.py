#!/usr/bin/env python3
"""Linux filesystem transaction tests; manager observations explicitly mocked.

Use only in the isolated network-none test container, never on a target host.
"""
import copy
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import stage8b_p1f_o2_install as m
NATIVE_OBSERVER = m.observe

ARTIFACT = Path("/inputs/" + m.ARTIFACT_NAME)
OLD_ARCHIVE = Path("/inputs/old-o1.zip")
OBSERVATION = {"host_key": m.HOST_KEY, "platform": "Linux-x86_64", "glibc": "2.39", "p1_stopped": True, "p0": {"fixture": "unchanged"}}


def fixture(root):
    raw = OLD_ARCHIVE.read_bytes()
    assert m.sha(raw) == "d8f9695bdb7a29b220dfe1396e31856fa7e71fb8a932dea126cd13e79c78e985"
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        names = [name for name in archive.namelist() if name.startswith("handoff-evidence/") and name.endswith(".zip")]
        assert len(names) == 1
        with zipfile.ZipFile(io.BytesIO(archive.read(names[0]))) as bundle:
            binary = bundle.read("payload/stage8b-p1-paper-supervisor")
    with zipfile.ZipFile(ARTIFACT) as archive:
        evidence = json.loads(archive.read("docs/stage-8/stage8b-p1f-o1-operational-evidence.json"))
        prior = evidence["post_install"]["installation_manifest_content"]
        for name, (source, mode) in m.old.PUBLIC_PAYLOAD.items():
            data = archive.read(source.relative_to(m.old.REPO).as_posix())
            if name.endswith("tmpfiles.d/moex-finam-p1-paper.conf"):
                data = b"".join(line for line in data.splitlines(keepends=True) if b"/o2 " not in line and b"paper-o2-staging " not in line)
            path = m.old.rooted(root, name)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            path.chmod(mode)
            assert m.sha(data) == prior["managed_payload_sha256"][name]
    for name, data in ((m.old.BINARY_PATH, binary), (m.old.MANIFEST, m.canonical(prior))):
        path = m.old.rooted(root, name)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        path.chmod(0o755 if name == m.old.BINARY_PATH else 0o644)
    (root / "etc/passwd").write_text("root:x:0:0:root:/root:/bin/bash\nmoex-p1-paper:x:65000:65000::/:/usr/sbin/nologin\n")
    (root / "etc/group").write_text("root:x:0:\nmoex-p1-paper:x:65000:\n")
    for name in m.old.PERSISTENT_DIRECTORIES:
        path = m.old.rooted(root, name)
        path.mkdir(parents=True, exist_ok=True)
        is_state = "/state" in name
        private = is_state or name.endswith("/credentials")
        os.chown(path, 65000 if is_state else 0, 0 if name.endswith("/credentials") else 65000)
        path.chmod(0o700 if private else 0o750)


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="o2-replace-test-")
        self.root = Path(self.tmp.name)
        fixture(self.root)
        self.observer = patch.object(m, "observe", return_value=copy.deepcopy(OBSERVATION))
        self.observer.start()
        self.r = m.Replacement(self.root, ARTIFACT)

    def tearDown(self):
        self.observer.stop()
        self.tmp.cleanup()

    def test_install_exact_replay_and_rollback(self):
        self.assertEqual(self.r.status()["result"], "EXACT_O1_READY_FOR_REPLACEMENT")
        result = self.r.transition("install")
        self.assertEqual(result["result"], "EXACT_O2_INSTALLED_NOT_ACTIVATED")
        self.assertEqual(self.r.transition("resume"), result)
        manifest = json.loads(self.r.read(m.old.MANIFEST))
        self.assertNotEqual(manifest["binary_sha256"], self.r.prior["binary_sha256"])
        self.assertEqual(self.r.read(m.MANIFEST), m.canonical(self.r.inventory))
        self.assertEqual(self.r.transition("rollback")["result"], "ROLLED_BACK")
        self.assertEqual(self.r.transition("rollback")["result"], "ROLLED_BACK")
        self.assertEqual(m.sha(self.r.read(m.old.MANIFEST)), m.OLD_MANIFEST_SHA)
        with self.assertRaises(m.Error):
            self.r.transition("install")

    def test_old_hash_and_custody_conflicts(self):
        path = m.old.rooted(self.root, m.old.BINARY_PATH)
        original = path.read_bytes()
        for kind in ("bytes", "mode", "owner", "hardlink", "symlink"):
            with self.subTest(kind=kind):
                if kind == "bytes": path.write_bytes(b"foreign")
                elif kind == "mode": path.chmod(0o777)
                elif kind == "owner": os.chown(path, 65000, 0)
                elif kind == "hardlink": os.link(path, path.with_name("extra"))
                else:
                    path.rename(path.with_name("extra"))
                    path.symlink_to("extra")
                with self.assertRaises(m.Error): self.r.preflight()
                self.assertFalse(m.exists(self.root, m.TRANSACTION))
                if kind == "symlink":
                    path.unlink()
                    path.with_name("extra").rename(path)
                path.with_name("extra").unlink(missing_ok=True)
                path.write_bytes(original)
                path.chmod(0o755)
                os.chown(path, 0, 0)

    def test_empty_directory_is_not_implicit_authorization(self):
        path = m.old.rooted(self.root, m.CONTROL)
        path.mkdir(mode=0o750)
        os.chown(path, 0, 65000)
        with self.assertRaisesRegex(m.Error, "directory remains"): self.r.preflight()

    def test_unexpected_state_or_material_rejected(self):
        names = [*m.old.OPERATOR_FILES, "/var/lib/moex-finam-p1-paper/state/unexpected", "/var/lib/moex-finam-p1-paper/state/.stage8b-p1-first-boot-quarantine/event"]
        for name in names:
            with self.subTest(path=name):
                path = m.old.rooted(self.root, name)
                path.write_bytes(b"not-authority")
                with self.assertRaises(m.Error): self.r.transition("install")
                self.assertFalse(m.exists(self.root, m.TRANSACTION))
                path.unlink()

    def test_resume_each_durable_file_frontier(self):
        # Inject a process interruption after each committed target-file write,
        # not simulated OS SIGKILL or hardware power loss.
        count = sum(name not in self.r.old_hashes or m.sha(value[0]) != self.r.old_hashes[name] for name, value in self.r.payload.items())
        for frontier in range(1, count + 1):
            with self.subTest(frontier=frontier), tempfile.TemporaryDirectory(prefix="o2-frontier-") as directory:
                root = Path(directory); fixture(root)
                r = m.Replacement(root, ARTIFACT)
                original_write = m.atomic_write
                writes = 0
                def fail_after_write(root, name, data, mode, gid):
                    nonlocal writes
                    original_write(root, name, data, mode, gid)
                    if name in r.payload:
                        writes += 1
                        if writes == frontier: raise InterruptedError("injected after durable write")
                with patch.object(m, "atomic_write", side_effect=fail_after_write):
                    with self.assertRaises(InterruptedError): r.transition("install")
                self.assertEqual(r.journal()["state"], "PREPARED")
                restored = m.Replacement(root, ARTIFACT)
                self.assertEqual(restored.transition("resume")["result"], "EXACT_O2_INSTALLED_NOT_ACTIVATED")
                restored.transition("rollback")
                print(f"PASS replacement durable-file-frontier {frontier}/{count}", flush=True)

    def test_partial_rollback_and_restart(self):
        self.r.transition("install")
        original_write = m.atomic_write
        def fail_after_manifest(root, name, data, mode, gid):
            original_write(root, name, data, mode, gid)
            if name == m.old.MANIFEST: raise InterruptedError("rollback frontier")
        with patch.object(m, "atomic_write", side_effect=fail_after_manifest):
            with self.assertRaises(InterruptedError): self.r.transition("rollback")
        with self.assertRaises(m.Error): self.r.transition("resume")
        self.assertEqual(m.Replacement(self.root, ARTIFACT).transition("rollback")["result"], "ROLLED_BACK")

    def test_activation_prevents_rollback_without_deletion(self):
        self.r.transition("install")
        for name in (m.CONTROL + "/genesis.json", m.O2DIR + "/active-manifest.sha256", m.STAGING + "/retained-source.json"):
            path = m.old.rooted(self.root, name)
            path.write_bytes(b"retained")
            with self.assertRaises(m.Error): self.r.transition("rollback")
            self.assertEqual(path.read_bytes(), b"retained")
            path.unlink()

    def test_new_payload_or_backup_drift_refuses_rollback(self):
        self.r.transition("install")
        for name in (m.old.BINARY_PATH, m.TRANSACTION + "/before-0"):
            path = m.old.rooted(self.root, name)
            raw = path.read_bytes()
            path.write_bytes(b"foreign")
            with self.assertRaises(m.Error): self.r.transition("rollback")
            path.write_bytes(raw)
        self.r.transition("rollback")

    def test_observation_change_never_yields_success(self):
        current = copy.deepcopy(OBSERVATION)
        self.r.transition("install")
        current["p0"] = {"changed": True}
        with patch.object(m, "observe", return_value=current):
            with self.assertRaises(m.Error): self.r.status()
            with self.assertRaises(m.Error): self.r.transition("rollback")

    def test_active_or_unknown_manager_rejected(self):
        with patch.object(m, "observe", side_effect=m.Error("unknown manager")):
            with self.assertRaises(m.Error): self.r.transition("install")
        self.assertFalse(m.exists(self.root, m.TRANSACTION))

    def test_lock_conflict(self):
        import fcntl
        path = m.old.rooted(self.root, m.BASE) / ".o2-replacement.lock"
        fd = os.open(path, os.O_CREAT | os.O_RDWR, 0o600)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaises(BlockingIOError): self.r.transition("install")
        finally: os.close(fd)

    def test_status_rejects_missing_control_directory(self):
        self.r.transition("install")
        m.old.rooted(self.root, m.CONTROL).rmdir()
        with self.assertRaises(m.Error): self.r.status()

    def test_manager_parser_fail_closed(self):
        with patch.object(m, "command", return_value="LoadState=loaded\n"):
            with self.assertRaises(m.Error): m.properties("unit")
        with patch.object(m, "command", side_effect=m.Error("query failed")):
            with self.assertRaises(m.Error): m.properties("unit")

    def test_native_observer_command_responses(self):
        import contextlib
        units = set(m.old.UNITS) | set(m.NEW_UNITS)
        def response(args):
            if args[0] == "ssh-keygen": return "256 " + m.HOST_KEY + " host (ED25519)\n"
            if args[1] == "list-units": return ""
            if args[1] == "list-unit-files": return "".join(name + " static -\n" for name in sorted(units))
            value = {"LoadState": "loaded", "ActiveState": "inactive", "SubState": "dead", "MainPID": "0", "ControlPID": "0", "Job": "", "FragmentPath": "/etc/systemd/system/" + args[2], "DropInPaths": "", "ExecStart": "pinned executable", "ControlGroup": ""}
            if args[2] in m.P0_UNITS: value.update(ActiveState="active", SubState="running", MainPID="999")
            return "".join(key + "=" + item + "\n" for key, item in value.items())
        with contextlib.ExitStack() as stack:
            stack.enter_context(patch.object(m.old, "account_ids", return_value=(65000, 65000)))
            stack.enter_context(patch.object(m, "assert_no_p1_processes"))
            stack.enter_context(patch.object(m, "file_bytes", return_value=b"unit"))
            stack.enter_context(patch.object(Path, "exists", return_value=True))
            with patch.object(m, "command", side_effect=response):
                self.assertTrue(NATIVE_OBSERVER(Path("/"))["p1_stopped"])
            for kind in ("active", "pid", "job", "load-error", "drop-in", "enabled", "missing-row", "unexpected-instance", "timer", "query-error", "host-key"):
                def mutated(args):
                    text = response(args)
                    if kind == "query-error": raise m.Error("command failed")
                    if kind == "host-key" and args[0] == "ssh-keygen": return "256 wrong host\n"
                    if kind == "enabled" and args[1] == "list-unit-files": return text.replace(" static ", " enabled ")
                    if kind == "missing-row" and args[1] == "list-unit-files": return ""
                    if kind == "unexpected-instance" and args[1] == "list-units": return "moex-finam-p1-paper-bootstrap-recover@unexpected.service loaded inactive dead description\n"
                    if kind == "timer" and args[1] == "list-unit-files": return text + "moex-finam-p1-paper.timer enabled enabled\n"
                    if args[1] == "show" and args[2] not in m.P0_UNITS:
                        for label, old, new in (("active", "ActiveState=inactive", "ActiveState=active"), ("pid", "MainPID=0", "MainPID=42"), ("job", "Job=\n", "Job=5\n"), ("load-error", "LoadState=loaded", "LoadState=error"), ("drop-in", "DropInPaths=\n", "DropInPaths=/foreign\n")):
                            if kind == label: return text.replace(old, new)
                    return text
                with self.subTest(kind=kind), patch.object(m, "command", side_effect=mutated):
                    with self.assertRaises(m.Error): NATIVE_OBSERVER(Path("/"))

    def test_process_inventory_service_and_root_binary(self):
        proc = self.root / "proc"; proc.mkdir()
        process = proc / "42"; process.mkdir()
        status = process / "status"
        status.write_text("Uid:\t65000\t65000\t65000\t65000\n")
        with self.assertRaises(m.Error): m.assert_no_p1_processes(proc, 65000)
        status.write_text("Uid:\t0\t0\t0\t0\n")
        (process / "exe").symlink_to("/usr/local/libexec/moex/stage8b-p1f-o2-operator (deleted)")
        with self.assertRaises(m.Error): m.assert_no_p1_processes(proc, 65000)
        (process / "exe").unlink()
        m.assert_no_p1_processes(proc, 65000)


if __name__ == "__main__":
    assert os.geteuid() == 0 and Path("/inputs").is_dir()
    unittest.main(verbosity=2)
