#!/usr/bin/env python3
"""Metadata-only custody preflight for the temporary Generation-2 ceremony."""

from __future__ import annotations

import hashlib
import json
import os
import stat
import subprocess
from pathlib import Path
from typing import Any


TRUST = Path("docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json")
ACCOUNT = Path("docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-account-key-manifest.json")
EXPECTED_PATH = Path("/run/stage8b-g2-ceremony-source")
ROOT_ENTRIES = {
    "package-authorization.ed25519",
    "helper-acceptance.ed25519",
    "account-binding-generation-2.hex",
    "trust-manifest.json",
    "account-key-manifest.json",
    "issuer-private-keys",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    require(isinstance(value, dict), "public manifest shape drift")
    return value


def require_directory(path: Path, expected_uid: int) -> None:
    metadata = os.lstat(path)
    require(stat.S_ISDIR(metadata.st_mode) and not stat.S_ISLNK(metadata.st_mode), "ceremony directory custody drift")
    require(metadata.st_uid == expected_uid and stat.S_IMODE(metadata.st_mode) == 0o700, "ceremony directory owner/mode drift")


def require_private_file(path: Path, expected_uid: int) -> None:
    metadata = os.lstat(path)
    require(stat.S_ISREG(metadata.st_mode) and not stat.S_ISLNK(metadata.st_mode), "private file custody drift")
    require(metadata.st_uid == expected_uid and metadata.st_nlink == 1, "private file owner/link drift")
    require(stat.S_IMODE(metadata.st_mode) == 0o600, "private file mode drift")
    require(metadata.st_size in {32, 64, 65}, "private seed size grammar drift")


def filesystem_type(path: Path) -> str:
    return subprocess.check_output(
        ["findmnt", "-n", "-o", "FSTYPE", "-T", str(path)],
        text=True,
        stderr=subprocess.DEVNULL,
    ).strip()


def check(root: Path, ceremony: Path) -> dict[str, object]:
    require(ceremony == EXPECTED_PATH and ceremony.resolve(strict=True) == ceremony, "ceremony source path drift")
    require(filesystem_type(ceremony) == "tmpfs", "ceremony source must be tmpfs")
    uid = os.geteuid()
    require_directory(ceremony, uid)
    require({path.name for path in ceremony.iterdir()} == ROOT_ENTRIES, "ceremony root inventory drift")

    trust_path = root / TRUST
    account_path = root / ACCOUNT
    trust = load(trust_path)
    account = load(account_path)
    require((ceremony / "trust-manifest.json").read_bytes() == trust_path.read_bytes(), "ceremony trust manifest drift")
    require((ceremony / "account-key-manifest.json").read_bytes() == account_path.read_bytes(), "ceremony account manifest drift")

    for name in ("trust-manifest.json", "account-key-manifest.json"):
        metadata = os.lstat(ceremony / name)
        require(stat.S_ISREG(metadata.st_mode) and metadata.st_uid == uid and metadata.st_nlink == 1, "public manifest custody drift")
        require(stat.S_IMODE(metadata.st_mode) == 0o644, "public manifest mode drift")
    for name in ("package-authorization.ed25519", "helper-acceptance.ed25519", "account-binding-generation-2.hex"):
        require_private_file(ceremony / name, uid)

    source_root = ceremony / "issuer-private-keys"
    require_directory(source_root, uid)
    expected_sources = set(trust["source_keys"])
    require({path.name for path in source_root.iterdir()} == expected_sources, "issuer private-key inventory drift")
    for source in expected_sources:
        directory = source_root / source
        require_directory(directory, uid)
        require({path.name for path in directory.iterdir()} == {"key.ed25519"}, "issuer source inventory drift")
        require_private_file(directory / "key.ed25519", uid)

    require(len(account.get("entries", [])) == 1, "account-key manifest inventory drift")
    return {
        "ceremony_storage": "tmpfs",
        "exact_inventory_verified": True,
        "private_file_metadata_verified": 14,
        "public_manifests_verified": 2,
        "cryptographic_binding_deferred_to_pinned_in_container_verifier": True,
        "private_path_exported": False,
        "private_value_exported": False,
        "trust_manifest_sha256": hashlib.sha256(trust_path.read_bytes()).hexdigest(),
        "account_key_manifest_sha256": hashlib.sha256(account_path.read_bytes()).hexdigest(),
    }
