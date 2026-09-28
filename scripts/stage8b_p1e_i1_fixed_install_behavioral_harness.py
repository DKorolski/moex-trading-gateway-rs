#!/usr/bin/env python3
"""Exercise fail-closed installer/status/rollback filesystem states on Linux."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import stat
import subprocess
import sys
from pathlib import Path


if len(sys.argv) != 5:
    raise SystemExit("usage: behavioral_harness.py REPO_ROOT TARGET_ROOT BINARY OUTPUT_JSON")

REPO = Path(sys.argv[1]).resolve()
ROOT = Path(sys.argv[2]).resolve()
BINARY = Path(sys.argv[3])
OUTPUT = Path(sys.argv[4])
INSTALLER = REPO / "scripts/stage8b_p1e_i1_fixed_install.py"
MANIFEST = ROOT / "usr/local/share/moex/stage8b-p1e/installation-v1.json"
BINARY_PARENT = ROOT / "usr/local/libexec/moex"
QUARANTINE = ROOT / "var/lib/moex-finam-p1-paper/state/.stage8b-p1-first-boot-quarantine"
MANAGED = (
    ROOT / "etc/systemd/system/moex-finam-p1-paper.service",
    ROOT / "etc/systemd/system/moex-finam-p1-paper-bootstrap.service",
    ROOT / "etc/systemd/system/moex-finam-p1-paper-bootstrap-recover@.service",
    ROOT / "usr/lib/sysusers.d/moex-finam-p1-paper.conf",
    ROOT / "usr/lib/tmpfiles.d/moex-finam-p1-paper.conf",
    ROOT / "usr/local/libexec/moex/stage8b-p1-paper-supervisor",
)
WATCH = (
    *MANAGED,
    MANIFEST,
    ROOT / "etc/passwd",
    ROOT / "etc/group",
    ROOT / "etc/unrelated-application.conf",
    ROOT / "etc/moex-finam-p1-paper",
    ROOT / "var/lib/moex-finam-p1-paper",
    BINARY_PARENT,
    BINARY_PARENT.with_name("moex.behavioral-real"),
)
RESULTS: list[dict[str, str]] = []


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def describe(path: Path) -> object:
    if not path.exists() and not path.is_symlink():
        return None
    metadata = path.lstat()
    base: dict[str, object] = {
        "mode": stat.S_IMODE(metadata.st_mode),
        "uid": metadata.st_uid,
        "gid": metadata.st_gid,
        "nlink": metadata.st_nlink,
    }
    if stat.S_ISLNK(metadata.st_mode):
        base.update({"type": "symlink", "target": os.readlink(path)})
    elif stat.S_ISREG(metadata.st_mode):
        base.update({"type": "file", "size": metadata.st_size, "sha256": digest(path)})
    elif stat.S_ISDIR(metadata.st_mode):
        base.update(
            {
                "type": "directory",
                "children": {
                    child.name: describe(child)
                    for child in sorted(path.iterdir(), key=lambda candidate: candidate.name)
                },
            }
        )
    else:
        base["type"] = "special"
    return base


def snapshot() -> dict[str, object]:
    return {str(path.relative_to(ROOT)): describe(path) for path in WATCH}


def invoke(action: str, *, binary: Path | None = None) -> subprocess.CompletedProcess[str]:
    command = [sys.executable, str(INSTALLER), action, "--root", str(ROOT)]
    if binary is not None:
        command.extend(("--binary", str(binary)))
    return subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)


def require_unchanged(before: dict[str, object], label: str) -> None:
    after = snapshot()
    if after != before:
        raise RuntimeError(f"{label}: rejected operation changed watched filesystem state")


def expect_status_drift_and_rollback_refusal(label: str) -> None:
    before = snapshot()
    status_result = invoke("status")
    if status_result.returncode != 0 or json.loads(status_result.stdout)["result"] != "ABSENT_OR_DRIFTED":
        raise RuntimeError(f"{label}: status did not report drift: {status_result.stdout}{status_result.stderr}")
    require_unchanged(before, label + " status")
    rollback_result = invoke("rollback")
    if rollback_result.returncode == 0:
        raise RuntimeError(f"{label}: rollback unexpectedly succeeded")
    require_unchanged(before, label + " rollback")
    RESULTS.append({"case": label, "result": "PASS"})


def expect_install_refusal(label: str, source: Path) -> None:
    before = snapshot()
    result = invoke("install", binary=source)
    if result.returncode == 0:
        raise RuntimeError(f"{label}: install unexpectedly succeeded")
    require_unchanged(before, label)
    RESULTS.append({"case": label, "result": "PASS"})


def expect_rollback_refusal(label: str) -> None:
    before = snapshot()
    rollback_result = invoke("rollback")
    if rollback_result.returncode == 0:
        raise RuntimeError(f"{label}: rollback unexpectedly succeeded")
    require_unchanged(before, label + " rollback")
    RESULTS.append({"case": label, "result": "PASS"})


def write_manifest(value: dict[str, object]) -> None:
    MANIFEST.write_text(
        json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    os.chown(MANIFEST, 0, 0)
    os.chmod(MANIFEST, 0o644)


original_manifest = MANIFEST.read_bytes()
manifest_value = json.loads(original_manifest)

# Arbitrary or missing manifest paths cannot become root deletion authority.
unrelated = ROOT / "etc/unrelated-application.conf"
unrelated.write_text("retained\n", encoding="utf-8")
os.chown(unrelated, 0, 0)
os.chmod(unrelated, 0o644)
extra = json.loads(original_manifest)
extra["managed_payload_sha256"]["/etc/unrelated-application.conf"] = digest(unrelated)
write_manifest(extra)
expect_status_drift_and_rollback_refusal("manifest-extra-path")
MANIFEST.write_bytes(original_manifest)
unrelated.unlink()

missing = json.loads(original_manifest)
missing["managed_payload_sha256"].pop("/usr/lib/tmpfiles.d/moex-finam-p1-paper.conf")
write_manifest(missing)
expect_status_drift_and_rollback_refusal("manifest-missing-path")
MANIFEST.write_bytes(original_manifest)

os.chmod(MANIFEST, 0o666)
expect_status_drift_and_rollback_refusal("manifest-custody-mode")
os.chmod(MANIFEST, 0o644)

# File and ancestor custody are part of exact status and rollback authority.
binary_target = MANAGED[-1]
os.chmod(binary_target, 0o777)
expect_status_drift_and_rollback_refusal("binary-mode-drift")
os.chmod(binary_target, 0o755)

os.chmod(BINARY_PARENT, 0o777)
expect_status_drift_and_rollback_refusal("binary-parent-writable")
os.chmod(BINARY_PARENT, 0o755)

service_uid = (ROOT / "etc/passwd").read_text(encoding="utf-8").split("moex-p1-paper:", 1)[1].split(":", 3)[1]
service_gid = (ROOT / "etc/group").read_text(encoding="utf-8").split("moex-p1-paper:", 1)[1].split(":", 2)[1]
os.chown(MANAGED[0], int(service_uid), 0)
expect_status_drift_and_rollback_refusal("managed-file-owner-drift")
os.chown(MANAGED[0], 0, 0)

real_parent = BINARY_PARENT.with_name("moex.behavioral-real")
BINARY_PARENT.rename(real_parent)
BINARY_PARENT.symlink_to(real_parent.name)
expect_status_drift_and_rollback_refusal("managed-ancestor-symlink")
BINARY_PARENT.unlink()
real_parent.rename(BINARY_PARENT)

# Source validation occurs before canonicalization and binds a stable single-link descriptor.
source_symlink = OUTPUT.parent / "accepted-binary-symlink"
source_symlink.unlink(missing_ok=True)
source_symlink.symlink_to(BINARY)
expect_install_refusal("source-binary-symlink", source_symlink)
source_symlink.unlink()

source_hardlink = OUTPUT.parent / "accepted-binary-hardlink"
source_hardlink_base = OUTPUT.parent / "accepted-binary-hardlink-base"
source_hardlink.unlink(missing_ok=True)
source_hardlink_base.unlink(missing_ok=True)
shutil.copy2(BINARY, source_hardlink_base)
os.link(source_hardlink_base, source_hardlink)
expect_install_refusal("source-binary-hardlink", source_hardlink)
source_hardlink.unlink()
source_hardlink_base.unlink()

# An existing root-equivalent service account is a conflict, not an identity to adopt.
passwd = ROOT / "etc/passwd"
group = ROOT / "etc/group"
original_passwd = passwd.read_bytes()
original_group = group.read_bytes()
passwd.write_text(
    "\n".join(
        "moex-p1-paper:x:0:0:MOEX FINAM Stage 8B P1 paper supervisor:/:/usr/sbin/nologin"
        if line.startswith("moex-p1-paper:") else line
        for line in original_passwd.decode().splitlines()
    ) + "\n",
    encoding="utf-8",
)
group.write_text(
    "\n".join("moex-p1-paper:x:0:" if line.startswith("moex-p1-paper:") else line for line in original_group.decode().splitlines()) + "\n",
    encoding="utf-8",
)
expect_install_refusal("conflicting-root-service-identity", BINARY)
passwd.write_bytes(original_passwd)
group.write_bytes(original_group)

# Quarantine is durable history: only an exact, empty directory permits rollback.
history = QUARANTINE / "history-record"
history.write_text("retained\n", encoding="utf-8")
expect_rollback_refusal("quarantine-nonempty")
history.unlink()

quarantine_real = QUARANTINE.with_name(QUARANTINE.name + ".behavioral-real")
QUARANTINE.rename(quarantine_real)
QUARANTINE.symlink_to(quarantine_real.name)
expect_status_drift_and_rollback_refusal("quarantine-symlink")
QUARANTINE.unlink()
quarantine_real.rename(QUARANTINE)

QUARANTINE.rmdir()
QUARANTINE.write_text("not-a-directory\n", encoding="utf-8")
os.chown(QUARANTINE, int(service_uid), int(service_uid))
os.chmod(QUARANTINE, 0o600)
expect_status_drift_and_rollback_refusal("quarantine-wrong-type")
QUARANTINE.unlink()
QUARANTINE.mkdir(mode=0o700)
os.chown(QUARANTINE, int(service_uid), int(service_gid))

# The restored baseline must be exact before the rehearsal performs the positive rollback.
status_result = invoke("status")
if status_result.returncode != 0 or json.loads(status_result.stdout)["result"] != "EXACT_INSTALLED":
    raise RuntimeError("behavioral matrix did not restore the exact baseline")
RESULTS.append({"case": "restored-exact-baseline", "result": "PASS"})

OUTPUT.write_text(
    json.dumps(
        {
            "schema_version": 1,
            "domain": "moex.stage8b.p1e.fixed-install.behavioral-matrix.v1",
            "case_count": len(RESULTS),
            "all_passed": True,
            "cases": RESULTS,
        },
        sort_keys=True,
        separators=(",", ":"),
    ) + "\n",
    encoding="utf-8",
)
print(f"stage8b-p1e-i1-fixed-install-behavioral-harness: PASS {len(RESULTS)}/{len(RESULTS)}")
