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
BINARY_PATH = "/usr/local/libexec/moex/stage8b-p1-paper-supervisor"
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
MANAGED_FILE_MODES = {
    **{path: mode for path, (_source, mode) in PUBLIC_PAYLOAD.items()},
    BINARY_PATH: 0o755,
}
MANIFEST_KEYS = {
    "schema_version",
    "domain",
    "activation_performed",
    "daemon_reload_performed",
    "redis_contact_performed",
    "finam_contact_performed",
    "operator_config_installed",
    "first_boot_source_installed",
    "lifecycle_credential_installed",
    "binary_sha256",
    "managed_payload_sha256",
    "persistent_directories",
    "operator_files_required_before_activation",
}


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


def sha256_fd(descriptor: int) -> str:
    digest = hashlib.sha256()
    os.lseek(descriptor, 0, os.SEEK_SET)
    while chunk := os.read(descriptor, 1024 * 1024):
        digest.update(chunk)
    os.lseek(descriptor, 0, os.SEEK_SET)
    return digest.hexdigest()


def canonical_json(value: object) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


def validate_root(raw: Path) -> Path:
    if not raw.is_absolute() or not raw.exists() or not raw.is_dir() or raw.is_symlink():
        raise InstallError("--root must be an existing absolute non-symlink directory")
    root = raw.resolve(strict=True)
    if os.geteuid() != 0:
        raise InstallError("installation requires effective uid 0")
    metadata = root.lstat()
    if metadata.st_uid != 0 or metadata.st_gid != 0 or stat.S_IMODE(metadata.st_mode) & 0o022:
        raise InstallError("--root custody must be root:root and not group/world-writable")
    return root


def open_validated_binary(path: Path) -> tuple[int, str]:
    if not path.is_absolute():
        raise InstallError("--binary must be an absolute regular non-symlink file")
    try:
        metadata = path.lstat()
    except OSError as error:
        raise InstallError("--binary must be an absolute regular non-symlink file") from error
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
        raise InstallError("--binary must be an absolute regular non-symlink file")
    if metadata.st_nlink != 1:
        raise InstallError("--binary must be single-link")
    mode = stat.S_IMODE(metadata.st_mode)
    if not mode & 0o100 or mode & 0o022:
        raise InstallError("--binary must be owner-executable and not group/world-writable")
    if metadata.st_size == 0:
        raise InstallError("--binary must not be empty")
    flags = os.O_RDONLY | os.O_CLOEXEC
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    descriptor = os.open(path, flags)
    opened = os.fstat(descriptor)
    if (
        not stat.S_ISREG(opened.st_mode)
        or opened.st_dev != metadata.st_dev
        or opened.st_ino != metadata.st_ino
        or opened.st_nlink != 1
        or stat.S_IMODE(opened.st_mode) != mode
        or opened.st_size != metadata.st_size
    ):
        os.close(descriptor)
        raise InstallError("--binary changed during validation")
    return descriptor, sha256_fd(descriptor)


def require_secure_directory(path: Path, label: str) -> None:
    metadata = path.lstat()
    if (
        stat.S_ISLNK(metadata.st_mode)
        or not stat.S_ISDIR(metadata.st_mode)
        or metadata.st_uid != 0
        or stat.S_IMODE(metadata.st_mode) & 0o022
    ):
        raise InstallError(f"protected directory custody drift: {label}")


def verify_secure_directory_chain(root: Path, directory: Path) -> None:
    try:
        relative = directory.relative_to(root)
    except ValueError as error:
        raise InstallError("target escapes installation root") from error
    require_secure_directory(root, "/")
    cursor = root
    for component in relative.parts:
        cursor = cursor / component
        if cursor.exists() or cursor.is_symlink():
            require_secure_directory(cursor, "/" + str(cursor.relative_to(root)))
        else:
            break


def ensure_directory_chain(root: Path, directory: Path) -> None:
    try:
        relative = directory.relative_to(root)
    except ValueError as error:
        raise InstallError("target escapes installation root") from error
    cursor = root
    require_secure_directory(root, "/")
    for component in relative.parts:
        cursor = cursor / component
        if cursor.exists() or cursor.is_symlink():
            require_secure_directory(cursor, "/" + str(cursor.relative_to(root)))
        else:
            cursor.mkdir(mode=0o755)
            os.chown(cursor, 0, 0)
            os.chmod(cursor, 0o755)


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


def install_exact_fd(
    root: Path,
    source_descriptor: int,
    source_hash: str,
    target: Path,
    mode: int,
    created: list[Path],
) -> str:
    ensure_directory_chain(root, target.parent)
    if target.exists() or target.is_symlink():
        verify_exact_file(root, target, mode, source_hash)
        return source_hash
    descriptor, temporary = tempfile.mkstemp(prefix=f".{target.name}.", dir=target.parent)
    temporary_path = Path(temporary)
    try:
        os.lseek(source_descriptor, 0, os.SEEK_SET)
        with os.fdopen(descriptor, "wb") as output, os.fdopen(
            os.dup(source_descriptor), "rb"
        ) as input_file:
            shutil.copyfileobj(input_file, output)
            output.flush()
            os.fsync(output.fileno())
        os.chmod(temporary_path, mode)
        os.chown(temporary_path, 0, 0)
        if sha256(temporary_path) != source_hash:
            raise InstallError("installed binary copy hash mismatch")
        os.link(temporary_path, target, follow_symlinks=False)
        created.append(target)
        directory_fd = os.open(target.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    finally:
        temporary_path.unlink(missing_ok=True)
    verify_exact_file(root, target, mode, source_hash)
    return source_hash


def verify_exact_file(root: Path, path: Path, mode: int, digest: str) -> None:
    verify_secure_directory_chain(root, path.parent)
    metadata = path.lstat()
    if (
        stat.S_ISLNK(metadata.st_mode)
        or not stat.S_ISREG(metadata.st_mode)
        or metadata.st_uid != 0
        or metadata.st_gid != 0
        or stat.S_IMODE(metadata.st_mode) != mode
        or metadata.st_nlink != 1
        or sha256(path) != digest
    ):
        raise InstallError(f"managed payload custody or content drift: {path}")


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


def read_identity_database(root: Path, relative: str, fields: int) -> list[list[str]]:
    path = rooted(root, relative)
    if not path.exists() and not path.is_symlink():
        return []
    verify_secure_directory_chain(root, path.parent)
    metadata = path.lstat()
    if (
        stat.S_ISLNK(metadata.st_mode)
        or not stat.S_ISREG(metadata.st_mode)
        or metadata.st_uid != 0
        or metadata.st_gid != 0
        or stat.S_IMODE(metadata.st_mode) & 0o022
        or metadata.st_nlink != 1
    ):
        raise InstallError(f"identity database custody drift: {relative}")
    rows = [row.split(":") for row in path.read_text(encoding="utf-8").splitlines() if row]
    if any(len(row) != fields for row in rows):
        raise InstallError(f"identity database is invalid: {relative}")
    return rows


def account_ids(root: Path, *, allow_absent: bool = False) -> tuple[int, int] | None:
    passwd = read_identity_database(root, "/etc/passwd", 7)
    groups = read_identity_database(root, "/etc/group", 4)
    user_rows = [row for row in passwd if row[0] == SERVICE_USER]
    group_rows = [row for row in groups if row[0] == SERVICE_GROUP]
    if not user_rows and not group_rows and allow_absent:
        return None
    if len(user_rows) != 1 or len(group_rows) != 1:
        raise InstallError("service identity must be absent or present exactly once as a pair")
    try:
        uid = int(user_rows[0][2])
        primary_gid = int(user_rows[0][3])
        gid = int(group_rows[0][2])
    except ValueError as error:
        raise InstallError("service identity database is invalid") from error
    if uid <= 0 or gid <= 0 or primary_gid != gid:
        raise InstallError("service identity must be non-root with its exact primary group")
    if user_rows[0][6] != "/usr/sbin/nologin" or group_rows[0][3] != "":
        raise InstallError("service identity account contract drift")
    if sum(row[2] == str(uid) for row in passwd) != 1:
        raise InstallError("service uid is not unique")
    if sum(row[2] == str(gid) for row in groups) != 1:
        raise InstallError("service gid is not unique")
    return uid, gid


def verify_persistent_directories(root: Path) -> None:
    identity = account_ids(root)
    assert identity is not None
    uid, gid = identity
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


def preflight_target_layout(root: Path) -> None:
    protected = {
        *(rooted(root, path).parent for path in MANAGED_FILE_MODES),
        rooted(root, MANIFEST).parent,
        *(rooted(root, path).parent for path in OPERATOR_FILES),
        rooted(root, "/etc"),
        rooted(root, "/var/lib"),
    }
    for directory in protected:
        verify_secure_directory_chain(root, directory)
    account_ids(root, allow_absent=True)


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


def build_manifest(binary_hash: str, payload_hashes: dict[str, str]) -> dict[str, object]:
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
        "binary_sha256": binary_hash,
        "managed_payload_sha256": dict(sorted(payload_hashes.items())),
        "persistent_directories": list(PERSISTENT_DIRECTORIES),
        "operator_files_required_before_activation": list(OPERATOR_FILES),
    }


def install(root: Path, binary: Path) -> dict[str, object]:
    assert_units_not_active(root)
    preflight_target_layout(root)
    binary_descriptor, binary_hash = open_validated_binary(binary)
    created: list[Path] = []
    payload_hashes: dict[str, str] = {}
    binary_target = rooted(root, BINARY_PATH)
    manifest_target = rooted(root, MANIFEST)
    try:
        for destination, (source, mode) in PUBLIC_PAYLOAD.items():
            payload_hashes[destination] = install_exact(
                root, source, rooted(root, destination), mode, created
            )
        account_ids(root, allow_absent=True)
        create_service_identity_and_directories(root)
        verify_persistent_directories(root)
        payload_hashes[BINARY_PATH] = install_exact_fd(
            root, binary_descriptor, binary_hash, binary_target, 0o755, created
        )
        verify_units(root)
        manifest = build_manifest(binary_hash, payload_hashes)
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
    finally:
        os.close(binary_descriptor)
    return {
        "result": "INSTALLED_OR_ALREADY_EXACT",
        "root": str(root),
        "manifest": str(manifest_target),
        "managed_payload_count": len(payload_hashes),
        "activation_performed": False,
    }


def reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    value: dict[str, object] = {}
    for key, item in pairs:
        if key in value:
            raise InstallError(f"installation manifest duplicate key: {key}")
        value[key] = item
    return value


def load_manifest(root: Path) -> dict[str, object]:
    path = rooted(root, MANIFEST)
    verify_secure_directory_chain(root, path.parent)
    if not path.exists() and not path.is_symlink():
        raise InstallError("exact installation manifest is absent")
    metadata = path.lstat()
    if (
        stat.S_ISLNK(metadata.st_mode)
        or not stat.S_ISREG(metadata.st_mode)
        or metadata.st_uid != 0
        or metadata.st_gid != 0
        or stat.S_IMODE(metadata.st_mode) != 0o644
        or metadata.st_nlink != 1
    ):
        raise InstallError("installation manifest custody drift")
    try:
        raw = path.read_bytes()
        value = json.loads(raw, object_pairs_hook=reject_duplicate_keys)
    except (OSError, json.JSONDecodeError) as error:
        raise InstallError("installation manifest is invalid") from error
    if not isinstance(value, dict) or set(value) != MANIFEST_KEYS:
        raise InstallError("installation manifest schema mismatch")
    if value.get("schema_version") != 1 or value.get("domain") != DOMAIN:
        raise InstallError("installation manifest identity mismatch")
    for key in (
        "activation_performed",
        "daemon_reload_performed",
        "redis_contact_performed",
        "finam_contact_performed",
        "operator_config_installed",
        "first_boot_source_installed",
        "lifecycle_credential_installed",
    ):
        if value[key] is not False:
            raise InstallError(f"installation manifest false boundary drift: {key}")
    if value["persistent_directories"] != list(PERSISTENT_DIRECTORIES):
        raise InstallError("installation manifest persistent directory inventory drift")
    if value["operator_files_required_before_activation"] != list(OPERATOR_FILES):
        raise InstallError("installation manifest operator inventory drift")
    if raw != canonical_json(value):
        raise InstallError("installation manifest is not canonical")
    return value


def verify_managed_payload(root: Path, manifest: dict[str, object]) -> None:
    expected = manifest.get("managed_payload_sha256")
    if not isinstance(expected, dict) or set(expected) != set(MANAGED_FILE_MODES):
        raise InstallError("installation manifest payload inventory is invalid")
    for relative, digest in expected.items():
        if (
            not isinstance(relative, str)
            or not isinstance(digest, str)
            or len(digest) != 64
            or any(character not in "0123456789abcdef" for character in digest)
        ):
            raise InstallError("installation manifest payload entry is invalid")
        path = rooted(root, relative)
        verify_exact_file(root, path, MANAGED_FILE_MODES[relative], digest)
    if manifest.get("binary_sha256") != expected[BINARY_PATH]:
        raise InstallError("installation manifest binary digest drift")


def verify_exact_installation(root: Path) -> dict[str, object]:
    preflight_target_layout(root)
    manifest = load_manifest(root)
    verify_managed_payload(root, manifest)
    verify_persistent_directories(root)
    return manifest


def verify_operator_material_absent(root: Path) -> None:
    for operator_file in OPERATOR_FILES:
        path = rooted(root, operator_file)
        verify_secure_directory_chain(root, path.parent)
        if path.exists() or path.is_symlink():
            raise InstallError(f"operator material exists; rollback refused: {operator_file}")


def verify_empty_state_for_rollback(root: Path) -> None:
    identity = account_ids(root)
    assert identity is not None
    uid, gid = identity
    state = rooted(root, "/var/lib/moex-finam-p1-paper/state")
    quarantine = rooted(root, PERSISTENT_DIRECTORIES[-1])
    state_metadata = state.lstat()
    quarantine_metadata = quarantine.lstat()
    for label, metadata in (("state", state_metadata), ("quarantine", quarantine_metadata)):
        if (
            stat.S_ISLNK(metadata.st_mode)
            or not stat.S_ISDIR(metadata.st_mode)
            or metadata.st_uid != uid
            or metadata.st_gid != gid
            or stat.S_IMODE(metadata.st_mode) != 0o700
        ):
            raise InstallError(f"durable {label} custody drift; rollback refused")
    state_entries = {item.name for item in state.iterdir()}
    if state_entries != {quarantine.name}:
        raise InstallError("durable state exists; rollback refused")
    if any(quarantine.iterdir()):
        raise InstallError("durable quarantine history exists; rollback refused")


def unlink_fixed(root: Path, absolute: str) -> None:
    path = rooted(root, absolute)
    verify_secure_directory_chain(root, path.parent)
    descriptor = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.unlink(path.name, dir_fd=descriptor)
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def rollback(root: Path) -> dict[str, object]:
    assert_units_not_active(root)
    verify_exact_installation(root)
    verify_operator_material_absent(root)
    verify_empty_state_for_rollback(root)
    # Repeat the complete preflight immediately before the first mutation.
    verify_exact_installation(root)
    verify_operator_material_absent(root)
    verify_empty_state_for_rollback(root)
    unlink_fixed(root, MANIFEST)
    for relative in sorted(MANAGED_FILE_MODES, reverse=True):
        unlink_fixed(root, relative)
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
        verify_exact_installation(root)
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
        result = install(root, args.binary)
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
