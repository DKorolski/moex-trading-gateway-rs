#!/usr/bin/env python3
"""Verify Generation-2 private/public bindings without exporting private data."""

from __future__ import annotations

import hashlib
import json
import os
import stat
import subprocess
import tempfile
from pathlib import Path
from typing import Any


TRUST = Path("docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json")
ACCOUNT = Path("docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-account-key-manifest.json")
ROOT_PRIVATE = {
    "package-authorization.ed25519": "authorization_key",
    "helper-acceptance.ed25519": "helper_acceptance_key",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def private_bytes(path: Path) -> bytes:
    metadata = os.lstat(path)
    require(stat.S_ISREG(metadata.st_mode) and not stat.S_ISLNK(metadata.st_mode), "private file custody drift")
    require(metadata.st_uid == os.geteuid() and metadata.st_nlink == 1, "private file ownership drift")
    require(stat.S_IMODE(metadata.st_mode) & 0o077 == 0, "private file permissions too broad")
    raw = path.read_bytes()
    stripped = raw.strip()
    if len(stripped) == 64:
        try:
            return bytes.fromhex(stripped.decode("ascii"))
        except (UnicodeDecodeError, ValueError):
            pass
    require(len(raw) == 32, "private seed grammar drift")
    return raw


def public_from_seed(seed: bytes) -> bytes:
    require(len(seed) == 32, "Ed25519 seed length drift")
    private_der = bytes.fromhex("302e020100300506032b657004220420") + seed
    with tempfile.TemporaryDirectory(prefix="stage8b-g2-public-derive-") as temporary:
        source = Path(temporary) / "private.der"
        source.write_bytes(private_der)
        os.chmod(source, 0o600)
        result = subprocess.check_output(
            ["openssl", "pkey", "-inform", "DER", "-in", str(source), "-pubout", "-outform", "DER"],
            stderr=subprocess.DEVNULL,
        )
    require(len(result) == 44 and result.startswith(bytes.fromhex("302a300506032b6570032100")), "derived public-key grammar drift")
    return result[-32:]


def load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    require(isinstance(value, dict), "public manifest shape drift")
    return value


def check(root: Path, ceremony: Path) -> dict[str, object]:
    require(ceremony.is_absolute() and ceremony.resolve(strict=True) == ceremony, "ceremony path must be canonical absolute")
    metadata = os.lstat(ceremony)
    require(stat.S_ISDIR(metadata.st_mode) and not stat.S_ISLNK(metadata.st_mode), "ceremony directory custody drift")
    require(metadata.st_uid == os.geteuid(), "ceremony directory ownership drift")
    require(stat.S_IMODE(metadata.st_mode) & 0o077 == 0, "ceremony directory permissions too broad")
    trust = load(root / TRUST)
    account = load(root / ACCOUNT)

    bindings: list[tuple[Path, dict[str, Any]]] = []
    for filename, key_name in ROOT_PRIVATE.items():
        bindings.append((ceremony / filename, trust[key_name]))
    source_root = ceremony / "issuer-private-keys"
    require(source_root.is_dir() and not source_root.is_symlink(), "issuer private-key root missing")
    require(set(trust["source_keys"]) == {path.name for path in source_root.iterdir() if path.is_dir()}, "issuer private-key inventory drift")
    for source, public in trust["source_keys"].items():
        bindings.append((source_root / source / "key.ed25519", public))

    for path, public in bindings:
        derived = public_from_seed(private_bytes(path))
        require(derived.hex() == public["public_key_ed25519_hex"], "private/public key binding mismatch")
        require(hashlib.sha256(derived).hexdigest() == public["public_key_sha256"], "public-key hash mismatch")

    account_key = private_bytes(ceremony / "account-binding-generation-2.hex")
    require(len(account["entries"]) == 1, "account-key manifest inventory drift")
    require(hashlib.sha256(account_key).hexdigest() == account["entries"][0]["key_sha256"], "account-key binding mismatch")
    require((ceremony / "trust-manifest.json").read_bytes() == (root / TRUST).read_bytes(), "ceremony trust manifest drift")
    require((ceremony / "account-key-manifest.json").read_bytes() == (root / ACCOUNT).read_bytes(), "ceremony account manifest drift")
    return {
        "signing_seed_bindings_verified": len(bindings),
        "account_key_bindings_verified": 1,
        "trust_manifest_sha256": hashlib.sha256((root / TRUST).read_bytes()).hexdigest(),
        "account_key_manifest_sha256": hashlib.sha256((root / ACCOUNT).read_bytes()).hexdigest(),
        "private_path_exported": False,
        "private_value_exported": False,
    }
