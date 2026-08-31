#!/usr/bin/env python3
"""Validate an immutable Trust Rebind R0 handoff without private material."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

import stage8b_p_r2b_trust_rebind_r0_receipt as receipt_contract


EVIDENCE = "handoff-evidence/stage8b-p-r2b-trust-rebind-r0-evidence.json"
GATE = "handoff-evidence/stage8b-p-r2b-trust-rebind-r0-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
RECEIPT = "handoff-evidence/stage8b-p-r2b-trust-rebind-r0-r1-primary-ceremony-verification-receipt.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST, RECEIPT}
AUTHORITY = "docs/stage-8/stage8b-p-r2b-trust-rebind-r0-authority.json"
SUPERSESSION = "docs/stage-8/stage8b-p-r2b-trust-rebind-r0-supersession.json"
TRUST = "docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json"
ACCOUNT = "docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-account-key-manifest.json"
REQUIRED = GENERATED | {
    AUTHORITY,
    SUPERSESSION,
    TRUST,
    ACCOUNT,
    "docs/stage-8/STAGE8B_P_R2B_TRUST_REBIND_R0_2026-08-30.md",
    "docs/stage-8/STAGE8B_P_R2B_TRUST_REBIND_R0_ACCEPTANCE_MATRIX_2026-08-30.csv",
    "docs/stage-8/STAGE8B_P_R2B_TRUST_REBIND_R0_R1_2026-08-31.md",
    "docs/stage-8/STAGE8B_P_R2B_TRUST_REBIND_R0_R1_ACCEPTANCE_MATRIX_2026-08-31.csv",
    "scripts/stage8b_p_r2b_trust_rebind_r0_check.py",
    "scripts/stage8b_p_r2b_trust_rebind_r0_negative_harness.py",
    "scripts/stage8b_p_r2b_trust_rebind_r0_gate.sh",
    "scripts/stage8b_p_r2b_trust_rebind_r0_handoff_safety_check.py",
    "scripts/stage8b_p_r2b_trust_rebind_r0_receipt.py",
    "scripts/stage8b_p_r2b_trust_rebind_r0_actual_ceremony_verify.py",
    "scripts/stage8b_p_r2b_trust_rebind_r0_handoff_negative_harness.py",
    "scripts/make_stage8b_p_r2b_trust_rebind_r0_handoff.py",
    "tools/stage8b-readonly-preflight/src/r2a5.rs",
    "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-trust-rebind-key-ceremony.rs",
    "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-trust-rebind-key-ceremony-verify.rs",
}
SECRET_NAMES = {
    "package-authorization.ed25519",
    "helper-acceptance.ed25519",
    "account-binding-generation-2.hex",
    "key.ed25519",
}
PUBLIC = {
    "authorization_public_key_sha256": "c3160a41e54fbeb9de4afe2163260f383fefa3fb531613d9754fc6b911a37c88",
    "trust_manifest_sha256": "dfe61ddb944df042cdf9514f56c14131e4a45bc732435ff89658ceaceb92d4ee",
    "public_key_set_sha256": "a1094751e25613d1a9f10b54436f3229fc73774d9135812577978c22a7bb7465",
    "account_key_manifest_sha256": "206bb41415f5edd9c59aa0d256dea63219fa6e28def2e436b676a4de3d1b52ec",
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def mode(item: zipfile.ZipInfo) -> str:
    return f"{(item.external_attr >> 16) & 0o177777:06o}"


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        members = {item.filename: item for item in infos}
        if len(names) != len(set(names)):
            raise ValueError("duplicate members")
        if missing := REQUIRED - set(names):
            raise ValueError(f"missing members: {sorted(missing)}")
        for item in infos:
            member = PurePosixPath(item.filename)
            item_mode = (item.external_attr >> 16) & 0o177777
            if member.is_absolute() or ".." in member.parts or "" in member.parts:
                raise ValueError(f"unsafe path: {item.filename}")
            if item_mode & 0o170000 == 0o120000:
                raise ValueError(f"symlink: {item.filename}")
            if item_mode & 0o170000 not in {0, 0o100000, 0o040000}:
                raise ValueError(f"special file: {item.filename}")
            if member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden root: {item.filename}")
            if member.name in SECRET_NAMES:
                raise ValueError(f"private ceremony member: {item.filename}")
            if ".env" in member.parts or item.filename.endswith((".log", ".pem", ".key", ".sqlite", ".sqlite3")):
                raise ValueError(f"secret/runtime artifact: {item.filename}")

        marker = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        receipt_bytes = archive.read(RECEIPT)
        receipt = json.loads(receipt_bytes)
        authority = json.loads(archive.read(AUTHORITY))
        supersession = json.loads(archive.read(SUPERSESSION))
        trust = json.loads(archive.read(TRUST))
        account = json.loads(archive.read(ACCOUNT))
        manifest_bytes = archive.read(MANIFEST)
        manifest = json.loads(manifest_bytes)
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("source_tree") != evidence.get("source_tree"):
            raise ValueError("source-tree binding mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name or evidence.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive-name binding mismatch")
        if evidence.get("accepted_predecessor_ref") != "a2586c428cd97349956efb12409ff37aea1fbe78":
            raise ValueError("accepted predecessor drift")
        if evidence.get("generation") != 2 or evidence.get("public_fingerprints") != PUBLIC:
            raise ValueError("generation-2 evidence drift")
        if (
            evidence.get("private_signing_seed_count_verified") != receipt.get("signing_seed_count")
            or evidence.get("private_account_key_count_verified") != receipt.get("account_key_count")
            or evidence.get("private_public_bindings_verified")
            != receipt.get("private_public_bindings_verified")
        ):
            raise ValueError("private binding inventory drift")
        if sha256(receipt_bytes) != evidence.get("ceremony_verification_receipt_sha256"):
            raise ValueError("ceremony receipt binding drift")
        if evidence.get("actual_ceremony_verifier_run") is not True or evidence.get("receipt_signature_verified") is not True:
            raise ValueError("actual ceremony verification evidence missing")
        if evidence.get("backup_status") != "REQUIRED_NOT_VERIFIED" or evidence.get("backup_attestation_present") is not False:
            raise ValueError("backup gate drift")
        if evidence.get("authorization") != "NOT_ISSUED" or authority.get("authorization") != "NOT_ISSUED":
            raise ValueError("authorization opened")
        for key in (
            "private_material_in_handoff",
            "public_authority_selection_changed",
            "production_binaries_rebuilt",
            "helper_acceptance_reissued",
            "production_credentials_installed",
            "package_issued",
            "container_created",
            "finam_network",
            "http_post_delete",
            "broker_dispatch",
            "redis_live",
            "runtime_live",
            "real_orders",
        ):
            if evidence.get(key) is not False:
                raise ValueError(f"closed handoff surface opened: {key}")
        if authority.get("status") != "GENERATION_2_PRIMARY_VERIFIED_BACKUP_REQUIRED_NOT_ACTIVE":
            raise ValueError("authority status drift")
        if authority.get("candidate_generation_2", {}).get("active") is not False:
            raise ValueError("generation 2 activated")
        if supersession.get("status") != "CANDIDATE_REBIND_RECORDED_NOT_ACTIVATED":
            raise ValueError("supersession status drift")
        if any(supersession.get("transition_state", {}).values()):
            raise ValueError("supersession transition opened")
        if sha256(archive.read(TRUST)) != PUBLIC["trust_manifest_sha256"]:
            raise ValueError("trust-manifest bytes drift")
        if sha256(archive.read(ACCOUNT)) != PUBLIC["account_key_manifest_sha256"]:
            raise ValueError("account-manifest bytes drift")
        if trust.get("authorization_key", {}).get("public_key_sha256") != PUBLIC["authorization_public_key_sha256"]:
            raise ValueError("authorization public-key drift")
        if trust.get("public_key_set_sha256") != PUBLIC["public_key_set_sha256"]:
            raise ValueError("public-key-set drift")
        if account.get("entries", [{}])[0].get("generation_id") != "2":
            raise ValueError("account generation drift")
        gate = archive.read(GATE)
        if (
            b"stage8b-p-r2b-trust-rebind-r0-gate: PASS" not in gate
            or b"actual_ceremony_verifier=PASS" not in gate
            or sha256(gate) != evidence.get("gate_sha256")
        ):
            raise ValueError("gate evidence mismatch")
        if sha256(manifest_bytes) != evidence.get("manifest_sha256"):
            raise ValueError("manifest digest mismatch")

        with tempfile.TemporaryDirectory(prefix="stage8b-trust-rebind-public-receipt-") as temporary:
            public_root = Path(temporary)
            for relative in (
                TRUST,
                ACCOUNT,
                *(item.as_posix() for item in receipt_contract.VERIFIER_SOURCES),
            ):
                destination = public_root / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(archive.read(relative))
            receipt_contract.validate_receipt(receipt, public_root, source_ref)

        tracked: set[str] = set()
        entries = manifest.get("entries", [])
        if manifest.get("entry_count") != len(entries):
            raise ValueError("manifest count mismatch")
        for entry in entries:
            name = entry["path"]
            if name in tracked or name not in members:
                raise ValueError(f"source member mismatch: {name}")
            tracked.add(name)
            data = archive.read(name)
            if len(data) != entry["size"] or sha256(data) != entry["sha256"] or mode(members[name]) != entry["mode"]:
                raise ValueError(f"source content mismatch: {name}")
        if set(names) - tracked != GENERATED:
            raise ValueError("generated member inventory mismatch")
        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "private_ceremony_members": 0,
            "actual_ceremony_verifier_run": True,
            "receipt_signature_verified": True,
            "source_ref": source_ref,
            "generation": 2,
            "backup_status": "REQUIRED_NOT_VERIFIED",
            "authorization": "NOT_ISSUED",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p_r2b_trust_rebind_r0_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (KeyError, OSError, ValueError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2b-trust-rebind-r0-handoff-safety: FAIL {error}") from error
    print("stage8b-p-r2b-trust-rebind-r0-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
