#!/usr/bin/env python3
"""Install or roll back the non-activating Stage 8B-P1-e service package.

The transaction installs only the binary and public systemd packaging.  It
never installs operator configuration, the first-boot source or the lifecycle
credential, and it never reloads, enables or starts a unit.  Those operational
steps remain outside I1.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
from pathlib import Path


REPO = Path(__file__).resolve().parents[1]
DEPLOY = REPO / "deploy/stage8b-p1e"
DOMAIN = "moex.stage8b.p1e.fixed-install.v1"
MANIFEST = "/usr/local/share/moex/stage8b-p1e/installation-v1.json"
SERVICE_USER = "moex-p1-paper"
SERVICE_GROUP = "moex-p1-paper"
UNITS = (
    "moex-finam-p1-paper.service",
    "moex-finam-p1-paper-bootstrap.service",
    "moex-finam-p1-paper-bootstrap-recover@.service",
)
PUBLIC_PAYLOAD = {
    "/etc/systemd/system/moex-finam-p1-paper.service": (DEPLOY / UNITS[0], 0o644),
    "/etc/systemd/system/moex-finam-p1-paper-bootstrap.service": (DEPLOY / UNITS[1], 0o644),
    "/etc/systemd/system/moex-finam-p1-paper-bootstrap-recover@.service": (
        DEPLOY / UNITS[2],
        0o644,
    ),
    "/usr/lib/sysusers.d/moex-finam-p1-paper.conf": (
        DEPLOY / "moex-finam-p1-paper.sysusers",
        0o644,
    ),
    "/usr/lib/tmpfiles.d/moex-finam-p1-paper.conf": (
        DEPLOY / "moex-finam-p1-paper.tmpfiles",
        0o644,
    ),
}
PERSISTENT_DIRECTORIES = (
    "/etc/moex-finam-p1-paper",
    "/etc/moex-finam-p1-paper/bootstrap",
    "/etc/moex-finam-p1-paper/credentials",
    "/var/lib/moex-finam-p1-paper",
    "/var/lib/moex-finam-p1-paper/state",
    "/var/lib/moex-finam-p1-paper/state/.stage8b-p1-first-boot-quarantine",
)
OPERATOR_FILES = (
    "/etc/moex-finam-p1-paper/supervisor.json",
    "/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json",
    "/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key",
)


class InstallError(RuntimeError):
    pass


def rooted(root: Path, absolute: str) -> Path:
    if not absolute.startswith("/") or ".." in Path(absolute).parts:
        raise InstallError(f"invalid fixed path: {absolute}")
    return root / absolute.removeprefix("/")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_json(value: object) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


def validate_root(raw: Path) -> Path:
    if not raw.is_absolute() or not raw.exists() or not raw.is_dir() or raw.is_symlink():
        raise InstallError("--root must be an existing absolute non-symlink directory")
    root = raw.resolve(strict=True)
    if os.geteuid() != 0:
        raise InstallError("installation requires effective uid 0")
    return root


def validate_binary(path: Path) -> None:
    if not path.is_absolute() or path.is_symlink() or not path.is_file():
        raise InstallError("--binary must be an absolute regular non-symlink file")
    mode = stat.S_IMODE(path.stat().st_mode)
    if not mode & 0o100 or mode & 0o022:
        raise InstallError("--binary must be owner-executable and not group/world-writable")
    if path.stat().st_size == 0:
        raise InstallError("--binary must not be empty")


def ensure_directory_chain(root: Path, directory: Path) -> None:
    try:
        relative = directory.relative_to(root)
    except ValueError as error:
        raise InstallError("target escapes installation root") from error
    cursor = root
    for component in relative.parts:
        cursor = cursor / component
        if cursor.exists() or cursor.is_symlink():
            metadata = cursor.lstat()
            if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISDIR(metadata.st_mode):
                raise InstallError(f"target parent is not a real directory: {cursor}")
        else:
            cursor.mkdir(mode=0o755)


def install_exact(
    root: Path, source: Path, target: Path, mode: int, created: list[Path]
) -> str:
    if source.is_symlink() or not source.is_file():
        raise InstallError(f"invalid package source: {source}")
    source_hash = sha256(source)
    ensure_directory_chain(root, target.parent)
    if target.exists() or target.is_symlink():
        if target.is_symlink() or not target.is_file():
            raise InstallError(f"fixed target is not a regular file: {target}")
        target_stat = target.stat()
        if (
            sha256(target) != source_hash
            or stat.S_IMODE(target_stat.st_mode) != mode
            or target_stat.st_uid != 0
            or target_stat.st_gid != 0
            or target_stat.st_nlink != 1
        ):
            raise InstallError(f"existing fixed target differs: {target}")
        return source_hash
    descriptor, temporary = tempfile.mkstemp(prefix=f".{target.name}.", dir=target.parent)
    temporary_path = Path(temporary)
    try:
        with os.fdopen(descriptor, "wb") as output, source.open("rb") as input_file:
            shutil.copyfileobj(input_file, output)
            output.flush()
            os.fsync(output.fileno())
        os.chmod(temporary_path, mode)
        os.chown(temporary_path, 0, 0)
        os.link(temporary_path, target, follow_symlinks=False)
        created.append(target)
        directory_fd = os.open(target.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    finally:
        temporary_path.unlink(missing_ok=True)
    return source_hash


def run_checked(command: list[str]) -> str:
    result = subprocess.run(
        command,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if result.returncode != 0:
        raise InstallError(f"command failed ({result.returncode}): {' '.join(command)}\n{result.stdout}")
    return result.stdout


def systemd_root_option(root: Path) -> str:
    return f"--root={root}"


def assert_units_not_active(root: Path) -> None:
    if root != Path("/"):
        return
    if shutil.which("systemctl") is None:
        raise InstallError("systemctl is required for installation into /")
    for unit in UNITS:
        result = subprocess.run(
            ["systemctl", "is-active", "--quiet", unit],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        if result.returncode == 0:
            raise InstallError(f"unit must be inactive: {unit}")


def create_service_identity_and_directories(root: Path) -> None:
    sysusers = rooted(root, "/usr/lib/sysusers.d/moex-finam-p1-paper.conf")
    tmpfiles = rooted(root, "/usr/lib/tmpfiles.d/moex-finam-p1-paper.conf")
    run_checked(["systemd-sysusers", systemd_root_option(root), str(sysusers)])
    run_checked(["systemd-tmpfiles", systemd_root_option(root), "--create", str(tmpfiles)])


def account_ids(root: Path) -> tuple[int, int]:
    passwd = rooted(root, "/etc/passwd").read_text(encoding="utf-8").splitlines()
    groups = rooted(root, "/etc/group").read_text(encoding="utf-8").splitlines()
    user_rows = [row.split(":") for row in passwd if row.split(":", 1)[0] == SERVICE_USER]
    group_rows = [row.split(":") for row in groups if row.split(":", 1)[0] == SERVICE_GROUP]
    if len(user_rows) != 1 or len(group_rows) != 1:
        raise InstallError("service identity was not created exactly once")
    try:
        return int(user_rows[0][2]), int(group_rows[0][2])
    except (IndexError, ValueError) as error:
        raise InstallError("service identity database is invalid") from error


def verify_persistent_directories(root: Path) -> None:
    uid, gid = account_ids(root)
    expected = {
        "/etc/moex-finam-p1-paper": (0, gid, 0o750),
        "/etc/moex-finam-p1-paper/bootstrap": (0, gid, 0o750),
        "/etc/moex-finam-p1-paper/credentials": (0, 0, 0o700),
        "/var/lib/moex-finam-p1-paper": (0, gid, 0o750),
        "/var/lib/moex-finam-p1-paper/state": (uid, gid, 0o700),
        "/var/lib/moex-finam-p1-paper/state/.stage8b-p1-first-boot-quarantine": (
            uid,
            gid,
            0o700,
        ),
    }
    for relative, (owner, group, mode) in expected.items():
        path = rooted(root, relative)
        metadata = path.lstat()
        if (
            stat.S_ISLNK(metadata.st_mode)
            or not stat.S_ISDIR(metadata.st_mode)
            or metadata.st_uid != owner
            or metadata.st_gid != group
            or stat.S_IMODE(metadata.st_mode) != mode
        ):
            raise InstallError(f"persistent directory custody drift: {relative}")


def verify_units(root: Path) -> str:
    if shutil.which("systemd-analyze") is None:
        raise InstallError("systemd-analyze is required")
    command = [
        "systemd-analyze",
        "verify",
        "--man=no",
        systemd_root_option(root),
        *UNITS,
    ]
    output = run_checked(command)
    if "Unknown key" in output or "Unknown lvalue" in output:
        raise InstallError(f"systemd parser warning:\n{output}")
    return output


def build_manifest(binary: Path, payload_hashes: dict[str, str]) -> dict[str, object]:
    return {
        "schema_version": 1,
        "domain": DOMAIN,
        "activation_performed": False,
        "daemon_reload_performed": False,
        "redis_contact_performed": False,
        "finam_contact_performed": False,
        "operator_config_installed": False,
        "first_boot_source_installed": False,
        "lifecycle_credential_installed": False,
        "binary_sha256": sha256(binary),
        "managed_payload_sha256": dict(sorted(payload_hashes.items())),
        "persistent_directories": list(PERSISTENT_DIRECTORIES),
        "operator_files_required_before_activation": list(OPERATOR_FILES),
    }


def install(root: Path, binary: Path) -> dict[str, object]:
    assert_units_not_active(root)
    validate_binary(binary)
    created: list[Path] = []
    payload_hashes: dict[str, str] = {}
    binary_target = rooted(root, "/usr/local/libexec/moex/stage8b-p1-paper-supervisor")
    manifest_target = rooted(root, MANIFEST)
    try:
        for destination, (source, mode) in PUBLIC_PAYLOAD.items():
            payload_hashes[destination] = install_exact(
                root, source, rooted(root, destination), mode, created
            )
        create_service_identity_and_directories(root)
        verify_persistent_directories(root)
        payload_hashes["/usr/local/libexec/moex/stage8b-p1-paper-supervisor"] = install_exact(
            root, binary, binary_target, 0o755, created
        )
        verify_units(root)
        manifest = build_manifest(binary, payload_hashes)
        with tempfile.NamedTemporaryFile("wb", delete=False) as handle:
            temporary_manifest = Path(handle.name)
            handle.write(canonical_json(manifest))
            handle.flush()
            os.fsync(handle.fileno())
        try:
            install_exact(root, temporary_manifest, manifest_target, 0o644, created)
        finally:
            temporary_manifest.unlink(missing_ok=True)
    except Exception:
        for path in reversed(created):
            path.unlink(missing_ok=True)
        raise
    return {
        "result": "INSTALLED_OR_ALREADY_EXACT",
        "root": str(root),
        "manifest": str(manifest_target),
        "managed_payload_count": len(payload_hashes),
        "activation_performed": False,
    }


def load_manifest(root: Path) -> dict[str, object]:
    path = rooted(root, MANIFEST)
    if path.is_symlink() or not path.is_file():
        raise InstallError("exact installation manifest is absent")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise InstallError("installation manifest is invalid") from error
    if value.get("schema_version") != 1 or value.get("domain") != DOMAIN:
        raise InstallError("installation manifest identity mismatch")
    return value


def verify_managed_payload(root: Path, manifest: dict[str, object]) -> None:
    expected = manifest.get("managed_payload_sha256")
    if not isinstance(expected, dict):
        raise InstallError("installation manifest payload inventory is invalid")
    for relative, digest in expected.items():
        if not isinstance(relative, str) or not isinstance(digest, str):
            raise InstallError("installation manifest payload entry is invalid")
        path = rooted(root, relative)
        if path.is_symlink() or not path.is_file() or sha256(path) != digest:
            raise InstallError(f"managed payload drift: {relative}")


def rollback(root: Path) -> dict[str, object]:
    assert_units_not_active(root)
    manifest = load_manifest(root)
    verify_managed_payload(root, manifest)
    for operator_file in OPERATOR_FILES:
        if rooted(root, operator_file).exists() or rooted(root, operator_file).is_symlink():
            raise InstallError(f"operator material exists; rollback refused: {operator_file}")
    state = rooted(root, "/var/lib/moex-finam-p1-paper/state")
    allowed = {".stage8b-p1-first-boot-quarantine"}
    if state.is_dir() and {item.name for item in state.iterdir()} - allowed:
        raise InstallError("durable state exists; rollback refused")
    expected = manifest["managed_payload_sha256"]
    assert isinstance(expected, dict)
    rooted(root, MANIFEST).unlink()
    for relative in sorted(expected, reverse=True):
        rooted(root, relative).unlink()
    for directory in (
        "/usr/local/share/moex/stage8b-p1e",
        "/usr/local/libexec/moex",
    ):
        path = rooted(root, directory)
        try:
            path.rmdir()
        except OSError:
            pass
    return {
        "result": "ROLLED_BACK_PUBLIC_PACKAGE",
        "root": str(root),
        "service_identity_retained": True,
        "persistent_directories_retained": True,
        "operator_material_removed": False,
        "durable_state_removed": False,
        "activation_performed": False,
    }


def status(root: Path) -> dict[str, object]:
    try:
        manifest = load_manifest(root)
        verify_managed_payload(root, manifest)
        state = "EXACT_INSTALLED"
    except InstallError:
        state = "ABSENT_OR_DRIFTED"
    return {
        "result": state,
        "root": str(root),
        "units": list(UNITS),
        "activation_performed": False,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("install", "rollback", "status"))
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--binary", type=Path)
    args = parser.parse_args()
    root = validate_root(args.root)
    if args.action == "install":
        if args.binary is None:
            raise InstallError("install requires --binary")
        result = install(root, args.binary.resolve(strict=True))
    elif args.action == "rollback":
        if args.binary is not None:
            raise InstallError("rollback does not accept --binary")
        result = rollback(root)
    else:
        if args.binary is not None:
            raise InstallError("status does not accept --binary")
        result = status(root)
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    try:
        main()
    except (InstallError, OSError, subprocess.SubprocessError) as error:
        print(f"stage8b-p1e-i1-fixed-install: FAIL {error}", file=sys.stderr)
        raise SystemExit(1) from error
