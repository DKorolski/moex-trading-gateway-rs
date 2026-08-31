#!/usr/bin/env python3
"""Create, restore, verify, and attest the Generation-2 encrypted backup."""

from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import platform
import shutil
import stat
import subprocess
import tarfile
import tempfile
from pathlib import Path, PurePosixPath
from typing import BinaryIO, Iterable


ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / "tools/stage8b-readonly-preflight"
TRUST = ROOT / "docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json"
ACCOUNT = ROOT / "docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-account-key-manifest.json"
RESTORE_RECEIPT = ROOT / "docs/stage-8/stage8b-p-r2b-generation2-backup-restore-r0-receipt.json"
DESTRUCTION_RECEIPT = ROOT / "docs/stage-8/stage8b-p-r2b-generation2-restore-destruction-r0-receipt.json"
AUTHORITY = ROOT / "docs/stage-8/stage8b-p-r2b-generation2-backup-restore-r0-authority.json"

PRIMARY_ENV = "STAGE8B_R2B_G2_PRIMARY_CEREMONY_DIR"
IDENTITY_ENV = "STAGE8B_R2B_G2_BACKUP_IDENTITY_FILE"
BACKUP_ENV = "STAGE8B_R2B_G2_BACKUP_OUTPUT_FILE"
SOURCE_DIGEST_DOMAIN = b"stage8b-p-r2b-generation2-backup-restore-source-v1\0"
SOURCE_FILES = (
    Path("scripts/stage8b_p_r2b_generation2_backup_identity.py"),
    Path("scripts/stage8b_p_r2b_generation2_backup_restore_r0_operate.py"),
    Path("tools/stage8b-readonly-preflight/src/r2a5.rs"),
    Path(
        "tools/stage8b-readonly-preflight/src/bin/"
        "stage8b-r2b-generation2-backup-restore-attest.rs"
    ),
    Path(
        "tools/stage8b-readonly-preflight/src/bin/"
        "stage8b-r2b-generation2-restore-destruction-attest.rs"
    ),
)
ALLOWED_XATTRS = {"com.apple.provenance"}
ROOT_FILES = {
    "account-binding-generation-2.hex": 0o600,
    "account-key-manifest.json": 0o644,
    "helper-acceptance.ed25519": 0o600,
    "package-authorization.ed25519": 0o600,
    "trust-manifest.json": 0o644,
}


def fail(message: str) -> "None":
    raise RuntimeError(message)


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_text(*arguments: str, environment: dict[str, str] | None = None) -> str:
    return subprocess.check_output(
        arguments,
        cwd=ROOT,
        env=environment,
        text=True,
        stderr=subprocess.DEVNULL,
    ).strip()


def clean_environment() -> dict[str, str]:
    environment = os.environ.copy()
    for name in (
        PRIMARY_ENV,
        IDENTITY_ENV,
        BACKUP_ENV,
        "STAGE8B_R2B_TRUST_REBIND_CEREMONY_DIR",
        "STAGE8B_R2B_TRUST_REBIND_RECEIPT_OUT",
        "STAGE8B_R2B_G2_RESTORED_CEREMONY_DIR",
        "STAGE8B_R2B_G2_RESTORE_PARENT_DIR",
        "STAGE8B_R2B_G2_BACKUP_METADATA_FILE",
        "STAGE8B_R2B_G2_SOURCE_REF",
        "STAGE8B_R2B_G2_VERIFIED_AT_UTC",
        "STAGE8B_R2B_G2_BACKUP_RESTORE_RECEIPT_FILE",
        "STAGE8B_R2B_G2_DESTROYED_AT_UTC",
        "STAGE8B_R2B_G2_RESTORE_FILEVAULT_ENABLED",
    ):
        environment.pop(name, None)
    return environment


def require_clean_source() -> tuple[str, str]:
    if run_text("git", "status", "--porcelain", "--untracked-files=all"):
        fail("source worktree must be clean before custody operation")
    source_ref = run_text("git", "rev-parse", "HEAD")
    source_tree = run_text("git", "rev-parse", "HEAD^{tree}")
    if len(source_ref) != 40 or len(source_tree) != 40:
        fail("source identity drift")
    for output in (RESTORE_RECEIPT, DESTRUCTION_RECEIPT, AUTHORITY):
        if output.exists() or output.is_symlink():
            fail("public evidence output already exists")
    return source_ref, source_tree


def exact_source_digest() -> str:
    digest = hashlib.sha256(SOURCE_DIGEST_DOMAIN)
    for relative in SOURCE_FILES:
        data = (ROOT / relative).read_bytes()
        name = relative.as_posix().encode()
        digest.update(len(name).to_bytes(8, "big"))
        digest.update(name)
        digest.update(len(data).to_bytes(8, "big"))
        digest.update(data)
    return digest.hexdigest()


def require_owned_directory(path: Path, mode: int) -> None:
    metadata = path.lstat()
    if (
        not stat.S_ISDIR(metadata.st_mode)
        or stat.S_ISLNK(metadata.st_mode)
        or stat.S_IMODE(metadata.st_mode) != mode
        or metadata.st_uid != os.geteuid()
    ):
        fail("directory custody drift")


def require_owned_file(path: Path, mode: int) -> None:
    metadata = path.lstat()
    if (
        not stat.S_ISREG(metadata.st_mode)
        or stat.S_ISLNK(metadata.st_mode)
        or stat.S_IMODE(metadata.st_mode) != mode
        or metadata.st_uid != os.geteuid()
        or metadata.st_nlink != 1
    ):
        fail("file custody drift")


def acl_absent(path: Path) -> bool:
    first = subprocess.check_output(
        ["/bin/ls", "-lde", str(path)], text=True, stderr=subprocess.DEVNULL
    ).splitlines()[0]
    return not first.split()[0].endswith("+")


def flags_absent(path: Path) -> bool:
    return run_text("stat", "-f", "%Sf", str(path)) == "-"


def metadata_profile(paths: Iterable[Path]) -> tuple[bool, bool, bool]:
    acl_ok = True
    flags_ok = True
    xattrs_ok = True
    for path in paths:
        acl_ok = acl_ok and acl_absent(path)
        flags_ok = flags_ok and flags_absent(path)
        names = set(
            subprocess.check_output(
                ["xattr", str(path)], text=True, stderr=subprocess.DEVNULL
            ).splitlines()
        )
        xattrs_ok = xattrs_ok and names.issubset(ALLOWED_XATTRS)
    return acl_ok, flags_ok, xattrs_ok


def expected_inventory(ceremony: Path) -> tuple[dict[str, int], set[str], list[Path]]:
    trust = json.loads((ceremony / "trust-manifest.json").read_text(encoding="utf-8"))
    source_names = sorted(trust["source_keys"])
    expected_files = dict(ROOT_FILES)
    expected_directories = {".", "issuer-private-keys"}
    for name in source_names:
        expected_directories.add(f"issuer-private-keys/{name}")
        expected_files[f"issuer-private-keys/{name}/key.ed25519"] = 0o600
    paths = [ceremony]
    for relative in sorted(expected_directories - {"."}):
        path = ceremony / relative
        require_owned_directory(path, 0o700)
        paths.append(path)
    for relative, mode in sorted(expected_files.items()):
        path = ceremony / relative
        require_owned_file(path, mode)
        if path.stat().st_size > 128 * 1024:
            fail("ceremony file size drift")
        paths.append(path)
    actual = {
        path.relative_to(ceremony).as_posix() if path != ceremony else "."
        for path in ceremony.rglob("*")
    } | {"."}
    if actual != set(expected_files) | expected_directories:
        fail("ceremony exact inventory drift")
    return expected_files, expected_directories, paths


def require_primary(path_text: str | None) -> tuple[Path, dict[str, int], set[str]]:
    if not path_text:
        fail("missing primary ceremony environment")
    primary = Path(path_text)
    if not primary.is_absolute() or primary.resolve(strict=True) != primary:
        fail("primary ceremony path drift")
    if primary.is_relative_to(ROOT.resolve()) or primary.is_relative_to(Path("/Volumes")):
        fail("primary ceremony custody boundary drift")
    require_owned_directory(primary, 0o700)
    files, directories, paths = expected_inventory(primary)
    if metadata_profile(paths) != (True, True, True):
        fail("primary access metadata drift")
    return primary, files, directories


def require_identity(path_text: str | None) -> tuple[Path, Path, str, str]:
    if not path_text:
        fail("missing backup identity environment")
    identity = Path(path_text)
    if not identity.is_absolute() or identity.resolve(strict=True) != identity:
        fail("backup identity path drift")
    if identity.is_relative_to(ROOT.resolve()) or identity.is_relative_to(Path("/Volumes")):
        fail("backup identity custody boundary drift")
    require_owned_directory(identity.parent, 0o700)
    require_owned_file(identity, 0o600)
    age_keygen_text = shutil.which("age-keygen")
    age_text = shutil.which("age")
    if not age_keygen_text or not age_text:
        fail("age toolchain unavailable")
    age_keygen = Path(age_keygen_text).resolve(strict=True)
    age = Path(age_text).resolve(strict=True)
    flags = os.O_RDONLY | os.O_CLOEXEC
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    descriptor = os.open(identity, flags)
    try:
        recipient = subprocess.check_output(
            [str(age_keygen), "-y", f"/dev/fd/{descriptor}"],
            cwd=ROOT,
            env=clean_environment(),
            text=True,
            stderr=subprocess.DEVNULL,
            pass_fds=(descriptor,),
        ).strip()
    finally:
        os.close(descriptor)
    if not recipient.startswith("age1") or not recipient.isascii():
        fail("age recipient grammar drift")
    recipient_sha256 = sha256_bytes((recipient + "\n").encode())
    return identity, age, recipient, recipient_sha256


def diskutil_values(device: str) -> dict[str, str]:
    result: dict[str, str] = {}
    output = subprocess.check_output(["diskutil", "info", device], text=True)
    for line in output.splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        result[key.strip()] = value.strip()
    return result


def require_external_backup(path_text: str | None, source_ref: str, identity: Path, primary: Path) -> Path:
    if not path_text:
        fail("missing external backup output environment")
    output = Path(path_text)
    expected_name = f"stage8b-p-r2b-generation2-{source_ref[:7]}.tar.age"
    if not output.is_absolute() or output.name != expected_name or output.exists() or output.is_symlink():
        fail("backup output path drift")
    if len(output.parts) < 4 or output.parts[:2] != ("/", "Volumes"):
        fail("backup output is not on a mounted external volume")
    volume_root = Path("/").joinpath(*output.parts[1:3])
    if volume_root.resolve(strict=True) != volume_root:
        fail("external volume path drift")
    parent = output.parent
    if parent.name != "moex-trading-offline-backup" or parent.parent != volume_root:
        fail("dedicated backup directory drift")
    if not parent.exists():
        parent.mkdir(mode=0o700)
    if parent.resolve(strict=True) != parent or parent.is_symlink():
        fail("backup directory path drift")
    device = run_text("df", str(parent)).splitlines()[-1].split()[0]
    info = diskutil_values(device)
    if (
        info.get("Device Location") != "External"
        or info.get("Removable Media") != "Removable"
        or info.get("File System Personality") != "MS-DOS FAT32"
        or info.get("Read-Only Volume", "No") == "Yes"
    ):
        fail("external removable media qualification failed")
    if parent.stat().st_dev in {identity.stat().st_dev, primary.stat().st_dev}:
        fail("encryption identity is not on a separate device from backup")
    if shutil.disk_usage(parent).free < 16 * 1024 * 1024:
        fail("insufficient external media space")
    return output


def add_directory(archive: tarfile.TarFile, name: str) -> None:
    info = tarfile.TarInfo(name)
    info.type = tarfile.DIRTYPE
    info.mode = 0o700
    info.uid = 0
    info.gid = 0
    info.uname = ""
    info.gname = ""
    info.mtime = 0
    archive.addfile(info)


def add_file(archive: tarfile.TarFile, source: Path, name: str, mode: int) -> None:
    before = source.lstat()
    flags = os.O_RDONLY | os.O_CLOEXEC
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    descriptor = os.open(source, flags)
    try:
        opened = os.fstat(descriptor)
        if (
            before.st_dev != opened.st_dev
            or before.st_ino != opened.st_ino
            or before.st_size != opened.st_size
            or before.st_nlink != 1
            or stat.S_IMODE(opened.st_mode) != mode
        ):
            fail("ceremony file changed before archive read")
        info = tarfile.TarInfo(name)
        info.type = tarfile.REGTYPE
        info.mode = mode
        info.uid = 0
        info.gid = 0
        info.uname = ""
        info.gname = ""
        info.mtime = 0
        info.size = opened.st_size
        with os.fdopen(os.dup(descriptor), "rb") as handle:
            archive.addfile(info, handle)
        after = os.fstat(descriptor)
        if (opened.st_dev, opened.st_ino, opened.st_size) != (
            after.st_dev,
            after.st_ino,
            after.st_size,
        ):
            fail("ceremony file changed during archive read")
    finally:
        os.close(descriptor)


def create_encrypted_backup(
    primary: Path,
    expected_files: dict[str, int],
    expected_directories: set[str],
    age: Path,
    recipient: str,
    output: Path,
) -> None:
    partial = output.with_name(f".{output.name}.partial")
    if partial.exists() or partial.is_symlink():
        fail("stale partial backup exists")
    process = subprocess.Popen(
        [str(age), "-r", recipient, "-o", str(partial)],
        stdin=subprocess.PIPE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        env=clean_environment(),
    )
    if process.stdin is None or process.stderr is None:
        fail("age stream unavailable")
    try:
        with tarfile.open(fileobj=process.stdin, mode="w|", format=tarfile.PAX_FORMAT) as archive:
            add_directory(archive, ".")
            for relative in sorted(expected_directories - {"."}):
                add_directory(archive, relative)
            for relative, mode in sorted(expected_files.items()):
                add_file(archive, primary / relative, relative, mode)
        process.stdin.close()
        stderr = process.stderr.read()
        return_code = process.wait()
        if return_code != 0:
            fail(f"age encryption failed with exit code {return_code}")
        if stderr and b"warning" in stderr.lower():
            fail("age encryption emitted a warning")
        with partial.open("rb") as handle:
            os.fsync(handle.fileno())
        os.replace(partial, output)
        with output.open("rb") as handle:
            os.fsync(handle.fileno())
        subprocess.run(["sync"], check=True)
    except BaseException:
        process.kill()
        process.wait()
        partial.unlink(missing_ok=True)
        raise


def normalize_member(name: str) -> str:
    while name.startswith("./"):
        name = name[2:]
    if name in ("", "."):
        return "."
    pure = PurePosixPath(name)
    if pure.is_absolute() or ".." in pure.parts or str(pure) != name:
        fail("unsafe archive member path")
    return name


def extract_file(member: tarfile.TarInfo, source: BinaryIO, destination: Path, mode: int) -> None:
    if member.size > 128 * 1024 or member.mode & 0o777 != mode:
        fail("archive file metadata drift")
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    descriptor = os.open(destination, flags, mode)
    try:
        remaining = member.size
        while remaining:
            chunk = source.read(min(remaining, 64 * 1024))
            if not chunk:
                fail("truncated archive member")
            os.write(descriptor, chunk)
            remaining -= len(chunk)
        os.fchmod(descriptor, mode)
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def restore_encrypted_backup(
    backup: Path,
    identity: Path,
    age: Path,
    expected_files: dict[str, int],
    expected_directories: set[str],
) -> tuple[Path, Path]:
    raw_parent = Path(tempfile.mkdtemp(prefix="stage8b-generation2-restore-"))
    os.chmod(raw_parent, 0o700)
    restore_parent = raw_parent.resolve(strict=True)
    restored = restore_parent / "ceremony"
    restored.mkdir(mode=0o700)
    flags = os.O_RDONLY | os.O_CLOEXEC
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    identity_descriptor = os.open(identity, flags)
    try:
        process = subprocess.Popen(
            [
                str(age),
                "--decrypt",
                "-i",
                f"/dev/fd/{identity_descriptor}",
                str(backup),
            ],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=clean_environment(),
            pass_fds=(identity_descriptor,),
        )
    finally:
        os.close(identity_descriptor)
    if process.stdout is None or process.stderr is None:
        fail("age restore stream unavailable")
    seen: set[str] = set()
    try:
        with tarfile.open(fileobj=process.stdout, mode="r|*") as archive:
            for member in archive:
                name = normalize_member(member.name)
                if name in seen:
                    fail("duplicate archive member")
                seen.add(name)
                if name in expected_directories:
                    if not member.isdir() or member.mode & 0o777 != 0o700:
                        fail("archive directory metadata drift")
                    if name != ".":
                        destination = restored / name
                        destination.mkdir(mode=0o700)
                    continue
                mode = expected_files.get(name)
                if mode is None or not member.isfile() or member.linkname:
                    fail("unexpected archive member")
                source = archive.extractfile(member)
                if source is None:
                    fail("archive file stream missing")
                extract_file(member, source, restored / name, mode)
        process.stdout.close()
        stderr = process.stderr.read()
        return_code = process.wait()
        if return_code != 0:
            fail(f"age decryption failed with exit code {return_code}")
        if stderr and b"warning" in stderr.lower():
            fail("age decryption emitted a warning")
        if seen != set(expected_files) | expected_directories:
            fail("restored archive inventory drift")
        expected_inventory(restored)
        return restore_parent, restored
    except BaseException:
        process.kill()
        process.wait()
        shutil.rmtree(raw_parent, ignore_errors=True)
        raise


def atomic_public_json(path: Path, value: dict[str, object]) -> bytes:
    encoded = (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode()
    temporary = path.with_name(f".{path.name}.partial")
    if temporary.exists():
        fail("stale public evidence partial exists")
    with temporary.open("xb") as handle:
        handle.write(encoded)
        handle.flush()
        os.fsync(handle.fileno())
    os.chmod(temporary, 0o644)
    os.replace(temporary, path)
    return encoded


def invoke_json_binary(binary: Path, environment: dict[str, str]) -> tuple[dict[str, object], bytes]:
    completed = subprocess.run(
        [str(binary)],
        cwd=ROOT,
        env=environment,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        fail(f"signed attestor rejected operation with exit code {completed.returncode}")
    try:
        value = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        fail(f"signed attestor emitted invalid JSON: {error}")
    if not isinstance(value, dict):
        fail("signed attestor receipt is not an object")
    encoded = (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode()
    return value, encoded


def build_authority(
    source_ref: str,
    source_tree: str,
    restore_receipt: dict[str, object],
    restore_bytes: bytes,
    destruction_receipt: dict[str, object],
    destruction_bytes: bytes,
) -> dict[str, object]:
    return {
        "schema_version": 1,
        "stage": "Stage 8B-P R2B Generation 2 Encrypted Backup Restore R0",
        "status": "INDEPENDENT_REVIEW_REQUIRED",
        "source_ref": source_ref,
        "source_tree": source_tree,
        "lineage": {
            "accepted_trust_rebind_r0_r1": "d8c71154d7407358b638af9e0c690578050d1640",
            "ceremony_path_hardening": "b5352fb33e69b4113fe2a8e65d3a0ceed55cce57",
            "merged_main_predecessor": "dd1af77efab89cc66f523bbe96821751465e12aa",
        },
        "backup": {
            "generation": 2,
            "status": "VERIFIED",
            "encrypted_backup_file_name": restore_receipt["encrypted_backup_file_name"],
            "encrypted_backup_sha256": restore_receipt["encrypted_backup_sha256"],
            "encrypted_backup_size_bytes": restore_receipt["encrypted_backup_size_bytes"],
            "encryption_format": "age-encryption.org/v1/X25519",
            "archive_format": "POSIX_PAX_STREAM",
            "media_class": "REMOVABLE_EXTERNAL_MEDIA",
            "media_filesystem": "FAT32",
            "encryption_identity_separate_device_verified": True,
            "plaintext_archive_written": False,
            "backup_ciphertext_in_git_or_handoff": False,
            "private_key_in_git_or_handoff": False,
            "private_path_recorded": False,
        },
        "restore": {
            "verification_status": "PASS",
            "public_fingerprints_identical": True,
            "signing_seed_bindings": 13,
            "account_key_bindings": 1,
            "restore_receipt_sha256": sha256_bytes(restore_bytes),
            "destruction_receipt_sha256": sha256_bytes(destruction_bytes),
            "disposable_restore_deleted": True,
            "logical_deletion_only": True,
            "restore_volume_filevault_enabled": True,
        },
        "public_fingerprints": {
            "trust_manifest_sha256": restore_receipt["trust_manifest_sha256"],
            "public_key_set_sha256": restore_receipt["public_key_set_sha256"],
            "authorization_public_key_sha256": restore_receipt[
                "authorization_public_key_sha256"
            ],
            "helper_acceptance_public_key_sha256": restore_receipt[
                "helper_acceptance_public_key_sha256"
            ],
            "account_key_manifest_sha256": restore_receipt["account_key_manifest_sha256"],
            "encryption_recipient_sha256": restore_receipt["encryption_recipient_sha256"],
        },
        "toolchain": {
            "verifier_source_sha256": restore_receipt["verifier_source_sha256"],
            "verifier_binary_sha256": restore_receipt["verifier_binary_sha256"],
            "destruction_attestor_binary_sha256": restore_receipt[
                "destruction_attestor_binary_sha256"
            ],
            "cargo_lock_sha256": restore_receipt["cargo_lock_sha256"],
            "rustc_version": restore_receipt["rustc_version"],
            "cargo_version": restore_receipt["cargo_version"],
            "python_version": restore_receipt["python_version"],
            "age_version": restore_receipt["age_version"],
            "age_binary_sha256": restore_receipt["age_binary_sha256"],
            "age_keygen_binary_sha256": restore_receipt["age_keygen_binary_sha256"],
            "clean_cargo_target_dir": True,
        },
        "receipts": {
            "backup_restore_signature_domain": restore_receipt["signature_domain"],
            "destruction_signature_domain": destruction_receipt["signature_domain"],
            "authorization_key_generation": 2,
            "package_authorization_domain_reused": False,
        },
        "activation": {
            "generation_2_active": False,
            "generation_2_public_authority_selected": False,
            "production_binaries_rebuilt": False,
            "helper_acceptance_reissued": False,
            "phase6_rehearsal_rebound": False,
            "production_credentials_installed": False,
            "controlled_installation": False,
            "package_authorization": "NOT_ISSUED",
        },
        "closed_surfaces": {
            "finam_network": False,
            "auth_service": False,
            "broker_get": False,
            "http_post_delete": False,
            "broker_dispatch": False,
            "redis_live": False,
            "runtime_live": False,
            "real_orders": False,
        },
    }


def main() -> None:
    if platform.system() != "Darwin":
        fail("operator custody run requires macOS")
    source_ref, source_tree = require_clean_source()
    primary, expected_files, expected_directories = require_primary(os.environ.get(PRIMARY_ENV))
    identity, age, recipient, recipient_sha256 = require_identity(os.environ.get(IDENTITY_ENV))
    backup = require_external_backup(
        os.environ.get(BACKUP_ENV), source_ref, identity, primary
    )
    age_keygen = Path(shutil.which("age-keygen") or "").resolve(strict=True)
    age_version = run_text(str(age), "--version", environment=clean_environment())
    if age_version != run_text(str(age_keygen), "--version", environment=clean_environment()):
        fail("age tool version drift")
    rustc_version = run_text("rustc", "--version", environment=clean_environment())
    cargo_version = run_text("cargo", "--version", environment=clean_environment())
    python_version = run_text("python3", "--version", environment=clean_environment())

    restore_parent: Path | None = None
    public_outputs: list[Path] = []
    backup_created = False
    try:
        with tempfile.TemporaryDirectory(prefix="stage8b-generation2-build-") as build_root_text:
            build_root = Path(build_root_text).resolve(strict=True)
            environment = clean_environment()
            environment["CARGO_TARGET_DIR"] = str(build_root / "target")
            subprocess.run(
                [
                    "cargo",
                    "build",
                    "--locked",
                    "--release",
                    "--manifest-path",
                    str(TOOL / "Cargo.toml"),
                    "--bin",
                    "stage8b-r2b-generation2-backup-restore-attest",
                    "--bin",
                    "stage8b-r2b-generation2-restore-destruction-attest",
                ],
                cwd=ROOT,
                env=environment,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=True,
            )
            verifier = build_root / "target/release/stage8b-r2b-generation2-backup-restore-attest"
            destructor = (
                build_root
                / "target/release/stage8b-r2b-generation2-restore-destruction-attest"
            )
            create_encrypted_backup(
                primary,
                expected_files,
                expected_directories,
                age,
                recipient,
                backup,
            )
            backup_created = True
            backup_sha256 = sha256_file(backup)
            backup_size = backup.stat().st_size
            restore_parent, restored = restore_encrypted_backup(
                backup, identity, age, expected_files, expected_directories
            )
            _, _, restored_paths = expected_inventory(restored)
            primary_metadata = metadata_profile(expected_inventory(primary)[2])
            restored_metadata = metadata_profile(restored_paths)
            if primary_metadata != (True, True, True) or restored_metadata != (True, True, True):
                fail("primary or restored access metadata drift")

            metadata = {
                "verifier_source_sha256": exact_source_digest(),
                "verifier_binary_sha256": sha256_file(verifier),
                "destruction_attestor_binary_sha256": sha256_file(destructor),
                "cargo_lock_sha256": sha256_file(TOOL / "Cargo.lock"),
                "rustc_version": rustc_version,
                "cargo_version": cargo_version,
                "python_version": python_version,
                "age_version": age_version,
                "age_binary_sha256": sha256_file(age),
                "age_keygen_binary_sha256": sha256_file(age_keygen),
                "archive_format": "POSIX_PAX_STREAM",
                "encryption_format": "age-encryption.org/v1/X25519",
                "encrypted_backup_file_name": backup.name,
                "encrypted_backup_sha256": backup_sha256,
                "encrypted_backup_size_bytes": backup_size,
                "encryption_recipient_sha256": recipient_sha256,
                "media_class": "REMOVABLE_EXTERNAL_MEDIA",
                "media_filesystem": "FAT32",
                "external_removable_media_verified": True,
                "encryption_identity_separate_device_verified": True,
                "plaintext_archive_written": False,
                "extended_acl_absent": True,
                "unexpected_file_flags_absent": True,
                "unexpected_extended_attributes_absent": True,
            }
            metadata_path = build_root / "public-metadata.json"
            metadata_path.write_text(json.dumps(metadata, separators=(",", ":")), encoding="utf-8")
            verified_at = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace(
                "+00:00", "Z"
            )
            attest_environment = clean_environment()
            attest_environment.update(
                {
                    "STAGE8B_R2B_G2_PRIMARY_CEREMONY_DIR": str(primary),
                    "STAGE8B_R2B_G2_RESTORED_CEREMONY_DIR": str(restored),
                    "STAGE8B_R2B_G2_RESTORE_PARENT_DIR": str(restore_parent),
                    "STAGE8B_R2B_G2_BACKUP_METADATA_FILE": str(metadata_path),
                    "STAGE8B_R2B_G2_SOURCE_REF": source_ref,
                    "STAGE8B_R2B_G2_VERIFIED_AT_UTC": verified_at,
                }
            )
            restore_receipt, restore_bytes = invoke_json_binary(verifier, attest_environment)
            restore_bytes = atomic_public_json(RESTORE_RECEIPT, restore_receipt)
            public_outputs.append(RESTORE_RECEIPT)

            restored_path_for_receipt = restored
            shutil.rmtree(restore_parent)
            restore_parent = None
            if restored_path_for_receipt.exists() or restored_path_for_receipt.is_symlink():
                fail("disposable restore deletion failed")
            filevault = run_text("fdesetup", "status") == "FileVault is On."
            if not filevault:
                fail("restore volume FileVault is not enabled")
            destroyed_at = (
                dt.datetime.now(dt.timezone.utc)
                .replace(microsecond=0)
                .isoformat()
                .replace("+00:00", "Z")
            )
            destruction_environment = clean_environment()
            destruction_environment.update(
                {
                    "STAGE8B_R2B_G2_PRIMARY_CEREMONY_DIR": str(primary),
                    "STAGE8B_R2B_G2_RESTORED_CEREMONY_DIR": str(restored_path_for_receipt),
                    "STAGE8B_R2B_G2_BACKUP_RESTORE_RECEIPT_FILE": str(RESTORE_RECEIPT),
                    "STAGE8B_R2B_G2_DESTROYED_AT_UTC": destroyed_at,
                    "STAGE8B_R2B_G2_RESTORE_FILEVAULT_ENABLED": "true",
                }
            )
            destruction_receipt, destruction_bytes = invoke_json_binary(
                destructor, destruction_environment
            )
            destruction_bytes = atomic_public_json(DESTRUCTION_RECEIPT, destruction_receipt)
            public_outputs.append(DESTRUCTION_RECEIPT)
            authority = build_authority(
                source_ref,
                source_tree,
                restore_receipt,
                restore_bytes,
                destruction_receipt,
                destruction_bytes,
            )
            atomic_public_json(AUTHORITY, authority)
            public_outputs.append(AUTHORITY)
    except BaseException:
        if restore_parent is not None:
            shutil.rmtree(restore_parent, ignore_errors=True)
        for output in public_outputs:
            output.unlink(missing_ok=True)
        if backup_created:
            backup.unlink(missing_ok=True)
        raise
    finally:
        for name in (PRIMARY_ENV, IDENTITY_ENV, BACKUP_ENV):
            os.environ.pop(name, None)

    print(
        "stage8b-generation2-backup-restore-r0: PASS "
        f"backup_file={backup.name} backup_sha256={sha256_file(backup)} "
        "bindings=13+1 restored_deleted=true private_path=false generation_active=false "
        "authorization=NOT_ISSUED"
    )


if __name__ == "__main__":
    try:
        main()
    except (KeyError, OSError, RuntimeError, subprocess.CalledProcessError, tarfile.TarError) as error:
        raise SystemExit(f"stage8b-generation2-backup-restore-r0: FAIL {error}") from error
    except Exception as error:  # Keep custody paths out of unexpected tracebacks.
        raise SystemExit(
            "stage8b-generation2-backup-restore-r0: FAIL "
            f"internal_error={type(error).__name__}"
        ) from None
