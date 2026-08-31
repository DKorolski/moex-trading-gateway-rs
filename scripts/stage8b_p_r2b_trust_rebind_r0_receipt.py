#!/usr/bin/env python3
"""Validate the public-only signed Trust Rebind R0-R1 ceremony receipt."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
TRUST = Path("docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json")
ACCOUNT = Path("docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-account-key-manifest.json")
VERIFIER_SOURCES = (
    Path("tools/stage8b-readonly-preflight/src/r2a5.rs"),
    Path(
        "tools/stage8b-readonly-preflight/src/bin/"
        "stage8b-r2b-trust-rebind-key-ceremony-verify.rs"
    ),
)
SOURCE_DIGEST_DOMAIN = b"stage8b-p-r2b-trust-rebind-verifier-source-v1\0"
SIGNATURE_DOMAIN = "stage8b-p-r2b-trust-rebind-verification-receipt-v1"
STAGE = "Stage 8B-P R2B Trust Rebind R0-R1"
RECEIPT_KEYS = {
    "schema_version",
    "stage",
    "generation",
    "verification_status",
    "verified_at_utc",
    "source_ref",
    "verifier_source_sha256",
    "trust_manifest_sha256",
    "public_key_set_sha256",
    "authorization_public_key_sha256",
    "helper_acceptance_public_key_sha256",
    "account_key_manifest_sha256",
    "source_key_count",
    "signing_seed_count",
    "account_key_count",
    "exact_inventory_verified",
    "owner_verified",
    "directory_modes_verified",
    "file_modes_verified",
    "single_link_verified",
    "symlink_rejection_verified",
    "private_public_bindings_verified",
    "account_key_binding_verified",
    "private_path_recorded",
    "private_values_exported",
    "backup_status",
    "signature_domain",
    "authorization_key_id",
    "authorization_key_generation",
    "signature_ed25519_hex",
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def lower_hex(value: object, length: int) -> bool:
    return (
        isinstance(value, str)
        and len(value) == length
        and all(character in "0123456789abcdef" for character in value)
    )


def require(value: bool, message: str) -> None:
    if not value:
        raise ValueError(message)


def verifier_source_sha256(root: Path) -> str:
    digest = hashlib.sha256(SOURCE_DIGEST_DOMAIN)
    for relative in VERIFIER_SOURCES:
        name = relative.as_posix().encode()
        data = (root / relative).read_bytes()
        digest.update(len(name).to_bytes(8, "big"))
        digest.update(name)
        digest.update(len(data).to_bytes(8, "big"))
        digest.update(data)
    return digest.hexdigest()


def signature_preimage(receipt: dict[str, Any]) -> bytes:
    unsigned = copy.deepcopy(receipt)
    unsigned["signature_ed25519_hex"] = ""
    body = json.dumps(unsigned, ensure_ascii=False, separators=(",", ":")).encode()
    return SIGNATURE_DOMAIN.encode() + b"\0" + body


def verify_signature(receipt: dict[str, Any], public_key_hex: str) -> None:
    # RFC 8410 SubjectPublicKeyInfo prefix for a raw Ed25519 public key.
    public_der = bytes.fromhex("302a300506032b6570032100" + public_key_hex)
    with tempfile.TemporaryDirectory(prefix="stage8b-trust-rebind-receipt-") as temporary:
        root = Path(temporary)
        key = root / "public.der"
        message = root / "receipt.preimage"
        signature = root / "receipt.signature"
        key.write_bytes(public_der)
        message.write_bytes(signature_preimage(receipt))
        signature.write_bytes(bytes.fromhex(receipt["signature_ed25519_hex"]))
        result = subprocess.run(
            [
                "openssl",
                "pkeyutl",
                "-verify",
                "-pubin",
                "-keyform",
                "DER",
                "-inkey",
                str(key),
                "-rawin",
                "-in",
                str(message),
                "-sigfile",
                str(signature),
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    require(result.returncode == 0, "receipt signature drift")


def validate_receipt(receipt: dict[str, Any], root: Path, source_ref: str) -> None:
    require(set(receipt) == RECEIPT_KEYS, "receipt keyset drift")
    require(lower_hex(source_ref, 40), "source-ref grammar drift")
    require(receipt["schema_version"] == 1 and type(receipt["schema_version"]) is int, "receipt schema drift")
    require(receipt["stage"] == STAGE, "receipt stage drift")
    require(receipt["generation"] == 2 and type(receipt["generation"]) is int, "receipt generation drift")
    require(receipt["verification_status"] == "PASS", "receipt verification status drift")
    require(
        isinstance(receipt["verified_at_utc"], str)
        and re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", receipt["verified_at_utc"]),
        "receipt timestamp grammar drift",
    )
    require(receipt["source_ref"] == source_ref, "receipt source-ref drift")
    expected_source_hash = verifier_source_sha256(root)
    require(receipt["verifier_source_sha256"] == expected_source_hash, "verifier source hash drift")

    trust_bytes = (root / TRUST).read_bytes()
    account_bytes = (root / ACCOUNT).read_bytes()
    trust = json.loads(trust_bytes)
    account = json.loads(account_bytes)
    authorization = trust["authorization_key"]
    helper = trust["helper_acceptance_key"]
    require(receipt["trust_manifest_sha256"] == sha256(trust_bytes), "receipt trust hash drift")
    require(receipt["public_key_set_sha256"] == trust["public_key_set_sha256"], "receipt key-set drift")
    require(
        receipt["authorization_public_key_sha256"] == authorization["public_key_sha256"],
        "receipt authorization-key drift",
    )
    require(
        receipt["helper_acceptance_public_key_sha256"] == helper["public_key_sha256"],
        "receipt helper-key drift",
    )
    require(receipt["account_key_manifest_sha256"] == sha256(account_bytes), "receipt account hash drift")
    require(receipt["source_key_count"] == 11 and type(receipt["source_key_count"]) is int, "source count drift")
    require(receipt["signing_seed_count"] == 13 and type(receipt["signing_seed_count"]) is int, "seed count drift")
    require(receipt["account_key_count"] == 1 and type(receipt["account_key_count"]) is int, "account count drift")
    for key in (
        "exact_inventory_verified",
        "owner_verified",
        "directory_modes_verified",
        "file_modes_verified",
        "single_link_verified",
        "symlink_rejection_verified",
        "account_key_binding_verified",
    ):
        require(receipt[key] is True, f"receipt positive proof drift: {key}")
    require(
        receipt["private_public_bindings_verified"] == 13
        and type(receipt["private_public_bindings_verified"]) is int,
        "private/public binding count drift",
    )
    require(receipt["private_path_recorded"] is False, "private path exported")
    require(receipt["private_values_exported"] is False, "private values exported")
    require(receipt["backup_status"] == "REQUIRED_NOT_VERIFIED", "backup status drift")
    require(receipt["signature_domain"] == SIGNATURE_DOMAIN, "receipt signature domain drift")
    require(receipt["authorization_key_id"] == authorization["key_id"], "receipt signer identity drift")
    require(
        receipt["authorization_key_generation"] == 2
        and type(receipt["authorization_key_generation"]) is int,
        "receipt signer generation drift",
    )
    require(lower_hex(receipt["signature_ed25519_hex"], 128), "receipt signature grammar drift")
    require(lower_hex(authorization["public_key_ed25519_hex"], 64), "authorization public-key grammar drift")
    verify_signature(receipt, authorization["public_key_ed25519_hex"])


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--source-ref", required=True)
    arguments = parser.parse_args()
    try:
        receipt = json.loads(arguments.receipt.read_text(encoding="utf-8"))
        require(isinstance(receipt, dict), "receipt is not an object")
        validate_receipt(receipt, arguments.root.resolve(), arguments.source_ref)
    except (KeyError, OSError, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2b-trust-rebind-r0-receipt: FAIL {error}") from error
    print(
        "stage8b-p-r2b-trust-rebind-r0-receipt: PASS "
        f"generation=2 source_ref={arguments.source_ref} signature=true private_path=false"
    )


if __name__ == "__main__":
    main()
