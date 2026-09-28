#!/usr/bin/env python3
"""Create one local age recovery identity without exporting private bytes."""

from __future__ import annotations

import hashlib
import os
import shutil
import stat
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
IDENTITY_ENV = "STAGE8B_R2B_G2_BACKUP_IDENTITY_FILE"


def fail(message: str) -> "None":
    raise SystemExit(f"stage8b-generation2-backup-identity: FAIL {message}")


def main() -> None:
    value = os.environ.get(IDENTITY_ENV)
    if not value:
        fail("missing local identity environment")
    identity = Path(value)
    if not identity.is_absolute() or identity.exists() or identity.is_symlink():
        fail("identity must be a new absolute path")
    parent = identity.parent
    try:
        parent_metadata = parent.lstat()
        canonical_parent = parent.resolve(strict=True)
    except OSError as error:
        fail(f"identity parent unavailable: {error}")
    if (
        not stat.S_ISDIR(parent_metadata.st_mode)
        or stat.S_ISLNK(parent_metadata.st_mode)
        or canonical_parent != parent
        or stat.S_IMODE(parent_metadata.st_mode) != 0o700
        or parent_metadata.st_uid != os.geteuid()
        or parent_metadata.st_nlink < 1
    ):
        fail("identity parent custody drift")
    forbidden_roots = (ROOT.resolve(), Path("/tmp"), Path("/private/tmp"), Path("/var/tmp"))
    if any(identity.is_relative_to(root) for root in forbidden_roots):
        fail("identity path is source-tree or ephemeral")
    if identity.is_relative_to(Path("/Volumes")):
        fail("identity must not be stored on backup media")

    age_keygen_text = shutil.which("age-keygen")
    if not age_keygen_text:
        fail("age-keygen unavailable")
    age_keygen = Path(age_keygen_text).resolve(strict=True)
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    descriptor = os.open(identity, flags, 0o600)
    completed = subprocess.run(
        [str(age_keygen)],
        stdin=subprocess.DEVNULL,
        stdout=descriptor,
        stderr=subprocess.PIPE,
        check=False,
        env={"PATH": os.environ.get("PATH", "")},
    )
    os.fsync(descriptor)
    os.close(descriptor)
    if completed.returncode != 0:
        identity.unlink(missing_ok=True)
        fail("age-keygen failed")
    os.chmod(identity, 0o600)
    metadata = identity.lstat()
    if (
        not stat.S_ISREG(metadata.st_mode)
        or stat.S_IMODE(metadata.st_mode) != 0o600
        or metadata.st_uid != os.geteuid()
        or metadata.st_nlink != 1
    ):
        identity.unlink(missing_ok=True)
        fail("created identity custody drift")
    descriptor = os.open(identity, os.O_RDONLY | os.O_CLOEXEC)
    try:
        recipient = subprocess.check_output(
            [str(age_keygen), "-y", f"/dev/fd/{descriptor}"],
            text=True,
            pass_fds=(descriptor,),
            env={"PATH": os.environ.get("PATH", "")},
        ).strip()
    finally:
        os.close(descriptor)
    if not recipient.startswith("age1") or not recipient.isascii():
        identity.unlink(missing_ok=True)
        fail("created identity recipient drift")
    with identity.open("rb") as handle:
        os.fsync(handle.fileno())
    directory_fd = os.open(parent, os.O_RDONLY)
    try:
        os.fsync(directory_fd)
    finally:
        os.close(directory_fd)
    recipient_sha256 = hashlib.sha256((recipient + "\n").encode()).hexdigest()
    print(
        "stage8b-generation2-backup-identity: PASS "
        f"recipient_sha256={recipient_sha256} private_path=false private_value=false"
    )


if __name__ == "__main__":
    main()
