#!/usr/bin/env python3
"""Fail-closed source/public-projection checker for Trust Rebind R0."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
BASE = Path("docs/stage-8")
AUTHORITY = BASE / "stage8b-p-r2b-trust-rebind-r0-authority.json"
SUPERSESSION = BASE / "stage8b-p-r2b-trust-rebind-r0-supersession.json"
TRUST = BASE / "stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json"
ACCOUNT = BASE / "stage8b-p-r2b-trust-rebind-generation-2-account-key-manifest.json"
MATRIX = BASE / "STAGE8B_P_R2B_TRUST_REBIND_R0_ACCEPTANCE_MATRIX_2026-08-30.csv"
DESIGN = BASE / "STAGE8B_P_R2B_TRUST_REBIND_R0_2026-08-30.md"
RUST = Path("tools/stage8b-readonly-preflight/src/r2a5.rs")
GENERATOR = Path("tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-trust-rebind-key-ceremony.rs")
VERIFIER = Path("tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-trust-rebind-key-ceremony-verify.rs")

OLD_FILES = {
    "docs/stage-8/stage8b-p-r2a5-authority.json": "81df156d3d8a1633f6301b2873e0ead98c409bcb29a31fc1c0286427ab8d33bb",
    "docs/stage-8/stage8b-p-r2a5-production-trust-manifest.json": "8014eea21ebe0b619122e0c7a332b50d173ff31d1cb2ea91e2505551dd547ef8",
    "docs/stage-8/stage8b-p-r2a5-production-account-key-manifest.json": "e40ea1d12ef5ebe4faf8ebaf6897056b9ac45d5efd0bb4c68eb4ff85f8bc7cd7",
    "docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-canary-ceremony.json": "a67e2c276f7d51e05de964686551fd73de64c46b748509c8f01b5779ce85393e",
    "docs/stage-8/stage8b-p-r2b-implementation-r0-r1-authority.json": "2410d88d150a77bdfa77b5e608cfcf40fe158695bd258b947754991d82172b9c",
    "docs/stage-8/stage8b-p-r2b-preproduction-supersession.json": "40f962a60cc721512bd07134e641e2ce69bb37f27dbea975fc552640ea3bd7b5",
    "docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-preflight-authority.json": "bafa59e0b76eb323b6f4be02f32200c48958854e63fde9ad9aaca9cb0b1f2db1",
}
OLD = {
    "authorization": "9149e9620ec0ea7ad3dab389542acf308471aaa0282e4b9020f75de7c13781af",
    "trust": "8014eea21ebe0b619122e0c7a332b50d173ff31d1cb2ea91e2505551dd547ef8",
    "key_set": "2e609dcbb6b6e7eb12fabebe4eb5ce62712aea91c2971a4e247194484f23da24",
    "account": "e40ea1d12ef5ebe4faf8ebaf6897056b9ac45d5efd0bb4c68eb4ff85f8bc7cd7",
}
NEW = {
    "authorization": "c3160a41e54fbeb9de4afe2163260f383fefa3fb531613d9754fc6b911a37c88",
    "trust": "dfe61ddb944df042cdf9514f56c14131e4a45bc732435ff89658ceaceb92d4ee",
    "key_set": "a1094751e25613d1a9f10b54436f3229fc73774d9135812577978c22a7bb7465",
    "account": "206bb41415f5edd9c59aa0d256dea63219fa6e28def2e436b676a4de3d1b52ec",
}
MATRIX_SHA256 = "90992731053a89076faa0ffcf5cae718d0b10fe890c1516f78252209ed3775f7"
SOURCES = {
    "ambiguity_orphan_unresolved_lifecycle",
    "composite_readiness",
    "durable_micro_budget",
    "instrument_specification",
    "kill_switch_run_allowed",
    "schedule",
    "single_finam_ownership",
    "stage6_exact_dispatch_ready_command",
    "stage7b_current_recovery_seal",
    "stage8a_root_config_policy_control",
    "trusted_clock",
}
SECRET_NAMES = {
    "package-authorization.ed25519",
    "helper-acceptance.ed25519",
    "account-binding-generation-2.hex",
    "key.ed25519",
}


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def exact_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    require(set(value) == expected, f"{label} keyset drift: {sorted(set(value) ^ expected)}")


def load(root: Path, relative: Path) -> dict[str, Any]:
    value = json.loads((root / relative).read_text(encoding="utf-8"))
    require(isinstance(value, dict), f"{relative} is not an object")
    return value


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def lower_sha256(value: object) -> bool:
    return isinstance(value, str) and len(value) == 64 and all(character in "0123456789abcdef" for character in value)


def digest_parts(domain: str, parts: list[str]) -> str:
    digest = hashlib.sha256(domain.encode())
    for part in parts:
        encoded = part.encode()
        digest.update(len(encoded).to_bytes(8, "big"))
        digest.update(encoded)
    return digest.hexdigest()


def validate_public_key(key: dict[str, Any], key_id: str) -> None:
    exact_keys(key, {"key_id", "generation", "public_key_ed25519_hex", "public_key_sha256", "valid_from_utc", "valid_until_utc"}, f"key {key_id}")
    require(key["key_id"] == key_id, f"key-id drift: {key_id}")
    require(key["generation"] == 2 and type(key["generation"]) is int, f"generation drift: {key_id}")
    require(key["valid_from_utc"] == "2026-08-30T00:00:00Z", f"valid-from drift: {key_id}")
    require(key["valid_until_utc"] == "2027-08-30T00:00:00Z", f"valid-until drift: {key_id}")
    public = key["public_key_ed25519_hex"]
    require(lower_sha256(public), f"public-key grammar drift: {key_id}")
    require(lower_sha256(key["public_key_sha256"]), f"public-key hash grammar drift: {key_id}")
    require(hashlib.sha256(bytes.fromhex(public)).hexdigest() == key["public_key_sha256"], f"public-key hash mismatch: {key_id}")


def check(root: Path) -> None:
    for relative, expected in OLD_FILES.items():
        require(sha256(root / relative) == expected, f"historical accepted artifact changed: {relative}")

    authority = load(root, AUTHORITY)
    supersession = load(root, SUPERSESSION)
    trust = load(root, TRUST)
    account = load(root, ACCOUNT)

    exact_keys(authority, {"schema_version", "stage", "record_id", "status", "accepted_predecessor", "incident", "candidate_generation_2", "generation_semantics", "custody", "verification", "activation", "authorization", "closed_surfaces"}, "authority")
    require(authority["schema_version"] == 1, "authority schema drift")
    require(authority["stage"] == "Stage 8B-P R2B Trust Rebind R0", "stage drift")
    require(authority["status"] == "GENERATION_2_MATERIALIZED_LOCALLY_BACKUP_REQUIRED_NOT_ACTIVE", "status drift")
    require(authority["accepted_predecessor"] == {
        "source_ref": "a2586c428cd97349956efb12409ff37aea1fbe78",
        "archive_name": "moex-trading-project-a2586c4.zip",
        "archive_sha256": "9cfe5f76a0b2ef45c8daae11704c909687808787e3b7e99c5c54d5aeb8254b7a",
        "verdict": "ACCEPTED",
    }, "accepted predecessor drift")
    incident = authority["incident"]
    require(incident["classification"] == "PRIVATE_MATERIAL_LOST" and incident["affected_generation"] == 1, "incident classification drift")
    require(incident["affected_trust_manifest_sha256"] == OLD["trust"], "lost trust lineage drift")
    require(incident["affected_public_key_set_sha256"] == OLD["key_set"], "lost key-set lineage drift")
    require(incident["affected_authorization_public_key_sha256"] == OLD["authorization"], "lost authorization lineage drift")
    require(incident["affected_account_key_manifest_sha256"] == OLD["account"], "lost account lineage drift")
    for key in ("private_material_recoverable_from_public_projection", "private_material_found_in_source_or_handoff", "authorization_issued_before_loss", "installation_performed_before_loss"):
        require(incident[key] is False, f"incident safety drift: {key}")
    require(incident["finam_requests_before_loss"] == 0 and type(incident["finam_requests_before_loss"]) is int, "pre-loss FINAM count drift")

    candidate = authority["candidate_generation_2"]
    require(candidate == {
        "generation": 2,
        "trust_manifest": TRUST.as_posix(),
        "trust_manifest_sha256": NEW["trust"],
        "authorization_public_key_sha256": NEW["authorization"],
        "public_key_set_sha256": NEW["key_set"],
        "account_key_manifest": ACCOUNT.as_posix(),
        "account_key_manifest_sha256": NEW["account"],
        "valid_from_utc": "2026-08-30T00:00:00Z",
        "valid_until_utc": "2027-08-30T00:00:00Z",
        "active": False,
    }, "generation-2 candidate drift")
    require(sha256(root / TRUST) == NEW["trust"], "generation-2 trust bytes drift")
    require(sha256(root / ACCOUNT) == NEW["account"], "generation-2 account bytes drift")
    require(all(OLD[key] != NEW[key] for key in OLD), "rebind reused old public fingerprint")

    semantics = authority["generation_semantics"]
    require(semantics == {
        "signature_domain_version": 1,
        "key_ids_remain_v1_domain_ids": True,
        "rotation_is_bound_by_generation_field": True,
        "authorization_generation": 2,
        "helper_acceptance_generation": 2,
        "source_key_generation": 2,
        "account_key_generation": "2",
        "lossy_or_same_generation_rebind_allowed": False,
    }, "generation semantics drift")
    custody = authority["custody"]
    require(custody["primary_copy_status"] == "PRESENT_AND_CRYPTOGRAPHICALLY_VERIFIED", "primary copy drift")
    require(custody["primary_location_class"] == "PERSISTENT_OPERATOR_OWNED_STORAGE_OUTSIDE_SOURCE_TREE", "custody class drift")
    for key in ("absolute_private_path_recorded_in_repository", "private_material_in_repository", "private_material_in_handoff", "backup_attestation_present", "activation_without_verified_backup_allowed"):
        require(custody[key] is False, f"custody boundary opened: {key}")
    require(custody["directory_mode"] == "0700" and custody["private_file_mode"] == "0600", "custody mode drift")
    require(custody["private_signing_seed_count"] == 13 and custody["private_account_key_count"] == 1, "private inventory drift")
    require(custody["encrypted_offline_backup_status"] == "REQUIRED_NOT_VERIFIED", "backup gate drift")
    verification = authority["verification"]
    require(verification == {
        "generator": "stage8b-r2b-trust-rebind-key-ceremony",
        "verifier": "stage8b-r2b-trust-rebind-key-ceremony-verify",
        "exact_directory_inventory_required": True,
        "private_to_public_bindings_verified": 13,
        "account_key_hash_binding_verified": True,
        "generator_and_verifier_public_fingerprints_identical": True,
        "private_values_exported": False,
    }, "verification evidence drift")
    activation = authority["activation"]
    require(activation["next_allowed_step"] == "INDEPENDENT_REVIEW_THEN_BACKUP_ATTESTATION_AND_GENERATION_2_COMPOSITION_REBUILD", "next-step drift")
    require(all(activation[key] is False for key in ("public_authority_selection_changed", "production_binaries_rebuilt", "helper_acceptance_reissued", "production_credentials_installed", "package_authorization_issued", "controlled_installation_allowed")), "activation opened")
    require(authority["authorization"] == "NOT_ISSUED", "authorization issued")
    require(set(authority["closed_surfaces"]) == {"production_account_host", "real_credentials", "finam_network", "finam_auth_service", "finam_broker_get", "http_post_delete", "broker_dispatch", "redis_live", "runtime_live", "real_orders"}, "closed-surface inventory drift")
    require(all(value is False for value in authority["closed_surfaces"].values()), "closed surface opened")

    exact_keys(trust, {"schema_version", "environment", "authorization_key", "helper_acceptance_key", "source_keys", "public_key_set_sha256", "rotation_requires_new_reviewed_package"}, "trust manifest")
    require(trust["schema_version"] == 1 and trust["environment"] == "production" and trust["rotation_requires_new_reviewed_package"] is True, "trust header drift")
    validate_public_key(trust["authorization_key"], "stage8b-r2a5-production-package-authorization-v1")
    validate_public_key(trust["helper_acceptance_key"], "stage8b-r2a5-production-helper-acceptance-v1")
    require(trust["authorization_key"]["public_key_sha256"] == NEW["authorization"], "authorization public key drift")
    require(set(trust["source_keys"]) == SOURCES, "source-key inventory drift")
    for source in sorted(SOURCES):
        validate_public_key(trust["source_keys"][source], f"{source}-ed25519-v1")
    old_trust = load(root, Path("docs/stage-8/stage8b-p-r2a5-production-trust-manifest.json"))
    require(trust["helper_acceptance_key"]["public_key_sha256"] != old_trust["helper_acceptance_key"]["public_key_sha256"], "helper key was reused")
    for source in SOURCES:
        require(trust["source_keys"][source]["public_key_sha256"] != old_trust["source_keys"][source]["public_key_sha256"], f"source key was reused: {source}")
    parts = [
        trust["helper_acceptance_key"]["key_id"],
        str(trust["helper_acceptance_key"]["generation"]),
        trust["helper_acceptance_key"]["public_key_sha256"],
        "2026-08-30T00:00:00.000Z",
        "2027-08-30T00:00:00.000Z",
    ]
    for source in sorted(SOURCES):
        key = trust["source_keys"][source]
        parts.extend([source, key["key_id"], str(key["generation"]), key["public_key_sha256"], "2026-08-30T00:00:00.000Z", "2027-08-30T00:00:00.000Z"])
    require(digest_parts("stage8b-p-r2a5-public-key-set-v1", parts) == trust["public_key_set_sha256"] == NEW["key_set"], "public-key-set digest drift")

    exact_keys(account, {"schema_version", "entries"}, "account manifest")
    require(account["schema_version"] == 1 and isinstance(account["entries"], list) and len(account["entries"]) == 1, "account manifest shape drift")
    entry = account["entries"][0]
    exact_keys(entry, {"generation_id", "key_sha256", "relative_key_path", "valid_from_utc", "valid_until_utc"}, "account entry")
    require(entry["generation_id"] == "2" and entry["relative_key_path"] == "generation-2.hex", "account generation drift")
    require(lower_sha256(entry["key_sha256"]), "account key hash grammar drift")
    require(entry["valid_from_utc"] == "2026-08-30T00:00:00Z" and entry["valid_until_utc"] == "2027-08-30T00:00:00Z", "account validity drift")
    old_account = load(root, Path("docs/stage-8/stage8b-p-r2a5-production-account-key-manifest.json"))
    require(entry["key_sha256"] != old_account["entries"][0]["key_sha256"], "account key was reused")

    require(supersession["schema_version"] == 1 and supersession["status"] == "CANDIDATE_REBIND_RECORDED_NOT_ACTIVATED", "supersession status drift")
    old = supersession["superseded_candidate"]
    require(old["generation"] == 1 and old["private_material_status"] == "LOST" and old["authorization"] == "NOT_ISSUED", "superseded generation drift")
    require(old["issued_packages"] == old["installations"] == old["finam_requests"] == 0, "superseded effects claimed")
    require(old["may_authorize_future_execution"] is False, "lost generation may authorize")
    replacement = supersession["replacement_candidate"]
    require(replacement["generation"] == 2 and replacement["backup_status"] == "REQUIRED_NOT_VERIFIED" and replacement["authorization"] == "NOT_ISSUED" and replacement["active"] is False, "replacement state drift")
    require(all(value is False for value in supersession["transition_state"].values()), "supersession transition opened")
    require(len(supersession["activation_preconditions"]) == 7 and len(set(supersession["activation_preconditions"])) == 7, "activation-precondition inventory drift")
    require(supersession["rollback"] == {"generation_1_private_material_recovery_assumed": False, "fallback_to_superseded_private_set_allowed": False, "new_rebind_requires_generation_greater_than_2": True}, "rollback policy drift")

    for path in root.rglob("*"):
        if path.is_file() and path.name in SECRET_NAMES:
            raise RuntimeError(f"private ceremony file entered source tree: {path.relative_to(root)}")

    rust = (root / RUST).read_text(encoding="utf-8")
    for marker in (
        "const R2B_TRUST_REBIND_PROFILE: KeyCeremonyProfile",
        "generation: 2,",
        'account_key_file: "account-binding-generation-2.hex"',
        'account_key_relative_path: "generation-2.hex"',
        'for root in [',
        '"/private/var/folders",',
        "if output.starts_with(root) {",
        "!output.starts_with(current)",
        ".custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);\n    let file = options.open(path)?;\n    let mut bytes = Vec::new();",
        "metadata.nlink() != 1",
        "fn verify_seed_binding(",
        "verify_trust_rebind_key_ceremony(",
        "ceremony_verifier_rejects_secret_mode_and_binding_drift",
    ):
        require(marker in rust, f"Rust ceremony enforcement missing: {marker}")
    require((root / GENERATOR).is_file() and (root / VERIFIER).is_file(), "ceremony CLI missing")

    require(sha256(root / MATRIX) == MATRIX_SHA256, "acceptance matrix content drift")
    with (root / MATRIX).open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require([row["id"] for row in rows] == [f"TRB-R0-{index:03d}" for index in range(1, 31)], "acceptance matrix identity drift")
    require(all(row["status"] == "PASS" and all(row.values()) for row in rows), "acceptance matrix incomplete")
    design = (root / DESIGN).read_text(encoding="utf-8")
    for marker in ("distinct generation-2 candidate", "REQUIRED_NOT_VERIFIED", "does not activate", "NOT_ISSUED"):
        require(marker in design, f"design marker missing: {marker}")
def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    arguments = parser.parse_args()
    try:
        check(arguments.root.resolve())
    except (KeyError, OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2b-trust-rebind-r0-check: FAIL {error}") from error
    print("stage8b-p-r2b-trust-rebind-r0-check: PASS generation=2 keys=13 account=2 backup=REQUIRED_NOT_VERIFIED active=false authorization=NOT_ISSUED finam=false")


if __name__ == "__main__":
    main()
