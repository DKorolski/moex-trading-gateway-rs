#!/usr/bin/env python3
"""Exact O1 -> accepted O2 bytes, with pre-activation rollback. No activation APIs.

Operational use requires separate acceptance/permission. The accepted artifact
is an input, never rebuilt here. Reuse the accepted fixed-install custody helpers.
"""
from __future__ import annotations

import argparse
import copy
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import stat
import subprocess
import sys
import tempfile
import zipfile

import stage8b_p1e_i1_fixed_install as old

ROOT = Path(__file__).resolve().parents[1]
ARTIFACT_NAME = "moex-trading-project-7196aaa-stage8b-p1f-o2-execution-artifact.zip"
ARTIFACT_SHA = "1069cbeb597e902126ebdb0b2c01ef52dca45420dfe2a69a6963b939de33ac4a"
ARTIFACT_REF = "7196aaac7c0bf45a03d90742d8ef483078649de6"
OLD_MANIFEST_SHA = "dab3e1426e40acb5101a8743cce5aac5aec8d3ae54d81fb73ac33c9bb18d464f"
INSTALLATION_ID = "stage8b-p1f-o2-baseline07-7196aaa-v1"
BASE = "/usr/local/share/moex/stage8b-p1e"
MANIFEST = BASE + "/installation-o2-v1.json"
TRANSACTION = BASE + "/o2-replacement-7196aaa"
O2DIR = "/etc/moex-finam-p1-paper/o2"
CONTROL = "/var/lib/moex-finam-p1-paper-control"
STAGING = "/var/lib/moex-finam-p1-paper-o2-staging"
DIRECTORIES = {O2DIR: ("service", 0o750), CONTROL: ("service", 0o750), STAGING: ("root", 0o700)}
CONFIRMATION = "INSTALL_ACCEPTED_O2_7196AAA_WITHOUT_ACTIVATION"
ROLLBACK_CONFIRMATION = "ROLLBACK_O2_7196AAA_BEFORE_ACTIVATION"
NEW_UNITS = ("moex-finam-p1f-o2-materializer.service", "moex-finam-p1-paper-o2-bootstrap-runner.service")
P0_UNITS = ("moex-finam-paper-runtime.service", "moex-finam-paper-ws.service")
HOST_KEY = "SHA256:8fOAPkfvZ61LYmldUBs6A5ZC+BFr6O3yCvUrBAWZsAo"
Error = old.InstallError


def require(value, message):
    if not value:
        raise Error(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def canonical(value):
    return old.canonical_json(value)


def load_package(path):
    raw = path.read_bytes()
    require(sha(raw) == ARTIFACT_SHA, "accepted artifact SHA mismatch")
    import io
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        require(len(archive.namelist()) == len(set(archive.namelist())), "duplicate artifact entries")
        artifact = json.loads(archive.read("docs/stage-8/stage8b-p1f-o2-execution-artifact.json"))
        prior = json.loads(archive.read("docs/stage-8/stage8b-p1f-o1-operational-evidence.json"))
        old_manifest = prior["post_install"]["installation_manifest_content"]
        require(sha(canonical(old_manifest)) == OLD_MANIFEST_SHA, "O1 manifest pin drift")
        # destination -> (bytes, mode, group). Unchanged O1 payloads are retained
        # in the complete inventory and also verified before the first mutation.
        payload = {}
        for destination, (source, mode) in old.PUBLIC_PAYLOAD.items():
            payload[destination] = (archive.read(source.relative_to(old.REPO).as_posix()), mode, "root")
        for binary in artifact["build"]["binaries"]:
            data = archive.read("payload/" + binary["name"])
            require(sha(data) == binary["sha256"] and len(data) == binary["size"], "ELF digest drift")
            payload["/usr/local/libexec/moex/" + binary["name"]] = (data, 0o755, "root")
        for unit in artifact["units"]:
            payload[unit["install_path"]] = (archive.read(unit["path"]), 0o644, "root")
        for name, filename in (
            ("authority_public_key", "authority-public-key.hex"),
            ("materialization_policy", "materialization-policy.json"),
            ("source_template", "source-template.json"),
            ("supervisor_template", "supervisor.template.json"),
        ):
            item = artifact["public_inputs"][name]
            data = archive.read(item["path"])
            require(sha(data) == item["sha256"], "public input digest drift")
            payload[O2DIR + "/" + filename] = (data, 0o440, "service")
    compat = copy.deepcopy(old_manifest)
    compat["binary_sha256"] = sha(payload[old.BINARY_PATH][0])
    compat["managed_payload_sha256"] = {name: sha(payload[name][0]) for name in old_manifest["managed_payload_sha256"]}
    payload[old.MANIFEST] = (canonical(compat), 0o644, "root")
    inventory = {
        "schema_version": 1, "domain": "moex.stage8b.p1f.o2.installation.v1",
        "installation_id": INSTALLATION_ID, "artifact_ref": ARTIFACT_REF,
        "installer_sha256": sha(Path(__file__).read_bytes()),
        "custody_helper_sha256": sha(Path(old.__file__).read_bytes()),
        "artifact_sha256": ARTIFACT_SHA, "predecessor_manifest_sha256": OLD_MANIFEST_SHA,
        "target_id": "stage8b-p1f-isolated-vps-1",
        "activation_performed": False, "daemon_reload_performed": False,
        "redis_contact_performed": False, "finam_contact_performed": False,
        "payload": {name: {"sha256": sha(data), "size": len(data), "mode": f"{mode:04o}", "owner": "root", "group": group}
                    for name, (data, mode, group) in sorted(payload.items())},
        "directories": {name: {"owner": "root", "group": group, "mode": f"{mode:04o}"}
                        for name, (group, mode) in sorted(DIRECTORIES.items())},
    }
    payload[MANIFEST] = (canonical(inventory), 0o644, "root")
    return old_manifest, payload, inventory


def file_bytes(root, name, mode, gid):
    path = old.rooted(root, name)
    old.verify_secure_directory_chain(root, path.parent)
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode) and before.st_uid == 0 and before.st_gid == gid
            and stat.S_IMODE(before.st_mode) == mode and before.st_nlink == 1, "file custody drift: " + name)
    fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        opened = os.fstat(fd)
        require((opened.st_dev, opened.st_ino, opened.st_mode, opened.st_nlink) ==
                (before.st_dev, before.st_ino, before.st_mode, before.st_nlink), "file replaced during read")
        require(before.st_size <= 64 * 1024 * 1024, "file too large")
        with os.fdopen(os.dup(fd), "rb") as handle:
            data = handle.read(before.st_size + 1)
        after = os.fstat(fd)
        require(len(data) == before.st_size and
                (before.st_mtime_ns, before.st_ctime_ns, before.st_uid, before.st_gid) ==
                (after.st_mtime_ns, after.st_ctime_ns, after.st_uid, after.st_gid), "file changed during read")
        return data
    finally:
        os.close(fd)


def exists(root, name):
    path = old.rooted(root, name)
    return path.exists() or path.is_symlink()


def sync_dir(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def atomic_write(root, name, data, mode, gid):
    path = old.rooted(root, name)
    old.verify_secure_directory_chain(root, path.parent)
    # Root-owned secure parent excludes service-UID path substitution. This
    # transaction is serialized with flock; privileged external edits are not
    # authorized during the separately scheduled installation window.
    fd, temporary = tempfile.mkstemp(prefix=".o2-copy-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as handle:
            handle.write(data)
            os.fchown(handle.fileno(), 0, gid)
            os.fchmod(handle.fileno(), mode)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        sync_dir(path.parent)
        require(file_bytes(root, name, mode, gid) == data, "write reread mismatch")
    finally:
        Path(temporary).unlink(missing_ok=True)


def command(args):
    process = subprocess.run(args, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10, check=False)
    require(process.returncode == 0, "read-only command failed: " + args[0])
    return process.stdout


def properties(unit):
    keys = ("LoadState", "ActiveState", "SubState", "MainPID", "ControlPID", "Job", "FragmentPath", "DropInPaths", "ExecStart", "ControlGroup")
    text = command(["systemctl", "show", unit, "--no-pager", *["--property=" + key for key in keys]])
    rows = [row.split("=", 1) for row in text.splitlines()]
    require(all(len(row) == 2 for row in rows), "malformed systemd response")
    result = dict(rows)
    require(len(result) == len(rows), "duplicate systemd property")
    if set(result) == set(keys) - {"ExecStart"}:
        # Native systemd 255 omits this service-specific property for not-found
        # units, even with --all. Only the two new O2 units may use this shape.
        # Do not synthesize an ExecStart value or infer absence from a query error.
        require(unit in NEW_UNITS and result["LoadState"] == "not-found"
                and result["ActiveState"] == "inactive" and result["SubState"] == "dead"
                and result["MainPID"] == result["ControlPID"] == "0"
                and result["Job"] in {"", "0"}
                and result["FragmentPath"] == result["DropInPaths"] == result["ControlGroup"] == "",
                "incomplete systemd response is not an exact absent O2 unit")
    else:
        require(set(result) == set(keys), "systemd property inventory drift")
    return result


def assert_no_p1_processes(proc, service_uid):
    for item in proc.iterdir():
        if not item.name.isdecimal():
            continue
        try:
            lines = (item / "status").read_text().splitlines()
            uids = next(line.split()[1:] for line in lines if line.startswith("Uid:"))
            require(len(uids) == 4, "unknown process UID shape")
            require(str(service_uid) not in uids, "P1 service UID has a process")
            try:
                executable = os.readlink(item / "exe")
            except FileNotFoundError:
                continue  # Kernel thread or a process that has just exited.
            require(not any(name in executable for name in ("stage8b-p1-paper-supervisor", "stage8b-p1f-o2-materializer", "stage8b-p1f-o2-operator")), "P1 executable is running")
        except FileNotFoundError:
            continue


def observe(root):
    """Only native / is allowed by the CLI; tests replace this observer explicitly."""
    require(root == Path("/") and platform.system() == "Linux" and platform.machine() == "x86_64", "native Linux x86-64 / target required")
    libc = os.confstr("CS_GNU_LIBC_VERSION").split()
    require(libc[0] == "glibc" and tuple(int(part) for part in libc[1].split(".")) >= (2, 34), "GLIBC 2.34 required")
    key = command(["ssh-keygen", "-lf", "/etc/ssh/ssh_host_ed25519_key.pub", "-E", "sha256"]).split()
    require(len(key) >= 2 and key[1] == HOST_KEY, "target host key mismatch")
    names = set(old.UNITS) | set(NEW_UNITS)
    # Include timers/sockets too: an unexpected P1 trigger is not stopped proof.
    unit_rows = command(["systemctl", "list-units", "--all", "--no-legend", "--no-pager", "--plain"])
    file_rows = command(["systemctl", "list-unit-files", "--no-legend", "--no-pager"])
    present = set()
    for row in unit_rows.splitlines():
        columns = row.split()
        require(bool(columns), "invalid unit inventory")
        if columns[0].startswith("moex-finam-p1"):
            require(columns[0] in names, "unexpected P1 instance/unit")
    for row in file_rows.splitlines():
        columns = row.split()
        require(len(columns) >= 2, "invalid unit-file inventory")
        if columns[0].startswith("moex-finam-p1"):
            require(columns[0] in names and columns[1] == "static", "P1 unit is enabled or unexpected")
            require(columns[0] not in present, "duplicate P1 unit-file row")
            present.add(columns[0])
    expected_files = {name for name in names if Path("/etc/systemd/system", name).exists()}
    require(set(old.UNITS) <= present and present == expected_files, "incomplete P1 unit-file inventory")
    for unit in (*old.UNITS[:2], *NEW_UNITS):
        data = properties(unit)
        require(data["LoadState"] in {"loaded", "not-found"} and data["ActiveState"] in {"inactive", "failed"}, "P1 unit is not stopped")
        require(data["MainPID"] == data["ControlPID"] == "0" and data["Job"] in {"", "0"}, "P1 unit has a PID/job")
        require(data["DropInPaths"] == "", "P1 unit drop-in exists")
        group = data["ControlGroup"]
        if group:
            require(group.startswith("/") and ".." not in Path(group).parts, "invalid cgroup path")
            cgroup = Path("/sys/fs/cgroup") / group.lstrip("/")
            if cgroup.exists():
                require(not cgroup.is_symlink(), "cgroup symlink")
                for file in cgroup.rglob("cgroup.procs"):
                    require(not file.read_text().strip(), "P1 cgroup is not empty")
    uid, _ = old.account_ids(root)
    assert_no_p1_processes(Path("/proc"), uid)
    # No Redis connection, no P0 action: compare only unit identity/config/PIDs.
    p0 = {}
    for unit in P0_UNITS:
        value = properties(unit)
        require(value["LoadState"] == "loaded" and value["ActiveState"] == "active" and value["SubState"] == "running", "P0 health changed")
        require(value["FragmentPath"] == "/etc/systemd/system/" + unit and value["DropInPaths"] == "", "P0 unit boundary drift")
        p0[unit] = {"properties_sha256": sha(canonical(value)), "fragment_sha256": sha(file_bytes(root, value["FragmentPath"], 0o644, 0))}
    return {"host_key": HOST_KEY, "platform": "Linux-x86_64", "glibc": libc[1], "p0": p0, "p1_stopped": True}


class Replacement:
    def __init__(self, root, artifact):
        self.root = old.validate_root(root)
        self.prior, self.payload, self.inventory = load_package(artifact)
        self.uid, self.gid = old.account_ids(root)
        self.old_hashes = dict(self.prior["managed_payload_sha256"])
        self.old_hashes[old.MANIFEST] = OLD_MANIFEST_SHA

    def gid_for(self, group):
        return self.gid if group == "service" else 0

    def inert(self):
        old.verify_persistent_directories(self.root)
        old.verify_operator_material_absent(self.root)
        old.verify_empty_state_for_rollback(self.root)
        for name in ("/etc/moex-finam-p1-paper/bootstrap", "/etc/moex-finam-p1-paper/credentials"):
            require(not any(old.rooted(self.root, name).iterdir()), "operator material directory not empty")
        for name, allowed in (("/etc/moex-finam-p1-paper", {"bootstrap", "credentials", "o2"}),
                              ("/var/lib/moex-finam-p1-paper", {"state"})):
            require({p.name for p in old.rooted(self.root, name).iterdir()} <= allowed, "unexpected P1 root entry")
        require(not exists(self.root, "/run/moex-finam-p1f-o2-input"), "transient credentials/input present")
        # These are the sole permitted additions; selector/genesis/source output
        # or any other entry means activation has begun and replacement is closed.
        for name, (group, mode) in DIRECTORIES.items():
            if not exists(self.root, name):
                continue
            path = old.rooted(self.root, name)
            old.verify_secure_directory_chain(self.root, path)
            info = path.lstat()
            require(stat.S_IMODE(info.st_mode) == mode and info.st_gid == self.gid_for(group), "directory custody drift")
            allowed = {Path(p).name for p in self.payload if str(Path(p).parent) == name}
            require({p.name for p in path.iterdir()} <= allowed, "unexpected authority/materialized state")
        return observe(self.root)

    def read(self, name):
        _, mode, group = self.payload[name]
        return file_bytes(self.root, name, mode, self.gid_for(group))

    def verify_state(self, wanted):
        for name in DIRECTORIES:
            if wanted == "new":
                require(exists(self.root, name), "installed directory missing: " + name)
            elif wanted == "old":
                require(not exists(self.root, name), "replacement directory remains: " + name)
        for name, (data, _mode, _group) in self.payload.items():
            if not exists(self.root, name):
                require(wanted != "new" and name not in self.old_hashes, "required file missing: " + name)
                continue
            actual = self.read(name)
            is_old = name in self.old_hashes and sha(actual) == self.old_hashes[name]
            is_new = actual == data
            require((wanted == "old" and is_old) or (wanted == "new" and is_new) or
                    (wanted == "mixed" and (is_old or is_new)), "unexpected managed bytes: " + name)

    def preflight(self):
        evidence = self.inert()
        self.verify_state("old")
        require(not exists(self.root, TRANSACTION), "retained transaction exists; use status/resume/rollback")
        require(not any(old.rooted(self.root, BASE).glob(".o2-prepare-*")), "incomplete backup staging retained; manual inspection required")
        for name in DIRECTORIES:
            require(not exists(self.root, name), "unexpected pre-existing replacement directory")
        return evidence

    def journal(self):
        raw = file_bytes(self.root, TRANSACTION + "/journal.json", 0o600, 0)
        value = json.loads(raw, object_pairs_hook=old.reject_duplicate_keys)
        require(raw == canonical(value) and set(value) == {"schema_version", "installation_sha256", "state", "preflight"}
                and type(value["schema_version"]) is int and value["schema_version"] == 1
                and value["installation_sha256"] == sha(canonical(self.inventory))
                and value["state"] in {"PREPARED", "APPLIED", "ROLLING_BACK", "ROLLED_BACK"}, "transaction identity/state drift")
        for index, name in enumerate(sorted(self.old_hashes)):
            data = file_bytes(self.root, TRANSACTION + f"/before-{index}", 0o600, 0)
            require(sha(data) == self.old_hashes[name], "before-image drift")
        require({p.name for p in old.rooted(self.root, TRANSACTION).iterdir()} ==
                {"journal.json", *(f"before-{index}" for index in range(len(self.old_hashes)))}, "transaction inventory drift")
        return value

    def write_journal(self, journal, state):
        journal["state"] = state
        atomic_write(self.root, TRANSACTION + "/journal.json", canonical(journal), 0o600, 0)

    def prepare(self, observation):
        parent = old.rooted(self.root, BASE)
        old.verify_secure_directory_chain(self.root, parent)
        staging = Path(tempfile.mkdtemp(prefix=".o2-prepare-", dir=parent))
        os.chmod(staging, 0o700)
        # An interrupted pre-prepare leaves a forensic staging directory, never
        # a valid transaction. No target payload has changed at that frontier.
        for index, name in enumerate(sorted(self.old_hashes)):
            data = self.read(name)
            require(sha(data) == self.old_hashes[name], "preimage changed")
            atomic_write(self.root, "/" + str((staging / f"before-{index}").relative_to(self.root)), data, 0o600, 0)
        journal = {"schema_version": 1, "installation_sha256": sha(canonical(self.inventory)), "state": "PREPARED", "preflight": observation}
        atomic_write(self.root, "/" + str((staging / "journal.json").relative_to(self.root)), canonical(journal), 0o600, 0)
        os.rename(staging, old.rooted(self.root, TRANSACTION))
        sync_dir(parent)
        return self.journal()

    def status(self):
        observation = self.inert()
        if not exists(self.root, TRANSACTION):
            self.preflight()
            state = "EXACT_O1_READY_FOR_REPLACEMENT"
        else:
            journal = self.journal()
            state = journal["state"]
            self.verify_state({"APPLIED": "new", "ROLLED_BACK": "old"}.get(state, "mixed"))
            if state == "APPLIED":
                require(observation == journal["preflight"], "post-install observation differs")
                state = "EXACT_O2_INSTALLED_NOT_ACTIVATED"
        return {"result": state, "installation_id": INSTALLATION_ID,
                "installation_sha256": sha(canonical(self.inventory)), "observation": observation,
                "activation_performed": False, "daemon_reload_performed": False,
                "redis_contact_performed": False, "finam_contact_performed": False}

    def transition(self, action):
        self.inert()
        base = old.rooted(self.root, BASE)
        old.verify_secure_directory_chain(self.root, base)
        fd = os.open(base / ".o2-replacement.lock", os.O_CREAT | os.O_RDWR | os.O_CLOEXEC | os.O_NOFOLLOW, 0o600)
        try:
            metadata = os.fstat(fd)
            require(stat.S_ISREG(metadata.st_mode) and metadata.st_uid == metadata.st_gid == 0 and
                    stat.S_IMODE(metadata.st_mode) == 0o600 and metadata.st_nlink == 1, "installer lock custody drift")
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            if exists(self.root, TRANSACTION):
                journal = self.journal()
            else:
                require(action == "install", "no transaction to recover/rollback")
                observation = self.preflight()
                journal = self.prepare(observation)
            require(self.inert() == journal["preflight"], "preflight observation changed")
            state = journal["state"]
            self.verify_state({"APPLIED": "new", "ROLLED_BACK": "old"}.get(state, "mixed"))
            if action in {"install", "resume"}:
                require(state in {"PREPARED", "APPLIED"}, "installation is terminal/rolling back")
                for name, (group, mode) in DIRECTORIES.items():
                    if not exists(self.root, name):
                        path = old.rooted(self.root, name)
                        old.verify_secure_directory_chain(self.root, path.parent)
                        path.mkdir(mode=mode)
                        os.chown(path, 0, self.gid_for(group))
                        os.chmod(path, mode)
                        sync_dir(path.parent)
                # Compatibility manifest follows all payloads; full O2 identity
                # is the final installed file. PREPARED blocks claiming success.
                ordered = [name for name in sorted(self.payload) if name not in {old.MANIFEST, MANIFEST}]
                for name in (*ordered, old.MANIFEST, MANIFEST):
                    require(self.inert() == journal["preflight"], "observation changed before mutation")
                    self.verify_state("mixed")
                    data, mode, group = self.payload[name]
                    if not exists(self.root, name) or self.read(name) != data:
                        atomic_write(self.root, name, data, mode, self.gid_for(group))
                self.verify_state("new")
                require(self.inert() == journal["preflight"], "post-install observation changed")
                self.write_journal(journal, "APPLIED")
            else:
                if state != "ROLLED_BACK":
                    self.write_journal(journal, "ROLLING_BACK")
                    for name in reversed(list(self.payload)):
                        require(self.inert() == journal["preflight"], "activation/observation prevents rollback")
                        self.verify_state("mixed")
                        if name in self.old_hashes:
                            index = sorted(self.old_hashes).index(name)
                            data = file_bytes(self.root, TRANSACTION + f"/before-{index}", 0o600, 0)
                            if self.read(name) != data:
                                atomic_write(self.root, name, data, self.payload[name][1], 0)
                        elif exists(self.root, name):
                            old.unlink_fixed(self.root, name)
                    for name in reversed(list(DIRECTORIES)):
                        if exists(self.root, name):
                            path = old.rooted(self.root, name)
                            require(not any(path.iterdir()), "rollback directory not empty")
                            path.rmdir()
                            sync_dir(path.parent)
                    self.verify_state("old")
                    self.write_journal(journal, "ROLLED_BACK")
            return self.status()
        finally:
            os.close(fd)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("preflight", "status", "install", "resume", "rollback"))
    parser.add_argument("--confirm")
    args = parser.parse_args()
    if args.action in {"install", "resume", "rollback"}:
        expected = ROLLBACK_CONFIRMATION if args.action == "rollback" else CONFIRMATION
        require(args.confirm == expected, "exact non-activation confirmation required")
    else:
        require(args.confirm is None, "read-only operation does not accept confirmation")
    replacement = Replacement(Path("/"), ROOT / "accepted-artifact" / ARTIFACT_NAME)
    result = replacement.status() if args.action in {"preflight", "status"} else replacement.transition(args.action)
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (Error, OSError, ValueError, KeyError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        print(f"stage8b-p1f-o2-install: FAIL {error}", file=sys.stderr)
        raise SystemExit(1) from error
