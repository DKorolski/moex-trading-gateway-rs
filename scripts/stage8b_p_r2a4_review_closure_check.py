#!/usr/bin/env python3
"""Static closure checker for Stage 8B-P R2A4."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def fail(message: str) -> None:
    raise SystemExit(f"stage8b-p-r2a4-check: FAIL {message}")


def require(value: bool, message: str) -> None:
    if not value:
        fail(message)


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root = args.root.resolve()
    docs = root / "docs/stage-8"
    tool = root / "tools/stage8b-readonly-preflight"
    r2a2 = (tool / "src/r2a2.rs").read_text()
    r2a3 = (tool / "src/r2a3.rs").read_text()
    r2a4 = (tool / "src/r2a4.rs").read_text()
    main = (tool / "src/main.rs").read_text()
    launcher = (tool / "src/bin/stage8b-r2a4-launcher.rs").read_text()
    rehearsal = (root / "scripts/stage8b_p_r2a4_linux_rehearsal.sh").read_text()
    production = json.loads((docs / "stage8b-p-r2a4-authority.json").read_text())
    controlled = json.loads((docs / "stage8b-p-r2a4-controlled-authority.json").read_text())
    status = json.loads((docs / "stage8b-p-r2a4-status.json").read_text())
    build = json.loads((docs / "stage8b-p-r2a4-build-evidence.json").read_text())

    require(production["stage"] == "8B-P" and production["revision"] == "R2A4", "identity drift")
    require(production["authorization_status"] == "NOT_ISSUED", "production authorization opened")
    require(status["promotion"]["r2b_real_credentials_allowed"] is False, "R2B opened")
    for name in (
        "authorization_public_key_sha256",
        "trust_manifest_sha256",
        "public_key_set_sha256",
        "account_key_manifest_sha256",
    ):
        require(len(production[name]) == 64 and production[name] != "0" * 64, f"production authority absent: {name}")
        require(len(controlled[name]) == 64 and controlled[name] != "0" * 64, f"controlled authority absent: {name}")
    production_trust_path = docs / "stage8b-p-r2a4-production-trust-manifest.json"
    production_account_path = docs / "stage8b-p-r2a4-production-account-key-manifest.json"
    production_trust = json.loads(production_trust_path.read_text())
    require(production["trust_manifest_sha256"] == sha(production_trust_path), "production trust manifest drift")
    require(production["account_key_manifest_sha256"] == sha(production_account_path), "production account manifest drift")
    require(production["authorization_public_key_sha256"] == production_trust["authorization_key"]["public_key_sha256"], "authorization key drift")
    require(production["public_key_set_sha256"] == production_trust["public_key_set_sha256"], "public key set drift")
    require(len(production_trust["source_keys"]) == 11, "production source key inventory drift")
    require(controlled["authorization_status"] == "NOT_ISSUED", "controlled authority status drift")
    qualification = status["qualification"]
    require(qualification["producer_count"] == 11 and qualification["issuer_count"] == 11, "identity count drift")
    require(qualification["source_specific_receipt_directories"] is True, "shared receipt topology")
    require(qualification["place_full_tls_sequence"] is True, "PLACE qualification absent")
    require(qualification["cancel_full_tls_sequence"] is True, "CANCEL qualification absent")
    require(qualification["real_finam_used"] is False, "real FINAM opened")

    helper_sha = (docs / "stage8b-p-r2a4-accepted-helper-sha256.txt").read_text().strip()
    require(len(helper_sha) == 64 and set(helper_sha) <= set("0123456789abcdef"), "helper SHA malformed")
    require(helper_sha != "0" * 64, "helper SHA not frozen")
    require(
        build["builder_image"]
        == "rust@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0",
        "builder image is not immutable",
    )
    require(build["canonical_cargo_metadata_sha256"] != "0" * 64, "Cargo graph absent")
    require(build["source_tree_unchanged"] is True, "build mutated source tree")
    require(build["source_tree_pre_build_sha256"] == build["source_tree_post_build_sha256"], "source tree drift")
    require(build["reproducible_build_count"] == 2, "second build absent")
    require(build["all_linux_binaries_identical"] is True, "Linux builds differ")
    require(build["linux_release_sha256"]["stage8b-readonly-preflight"] == helper_sha, "accepted helper/build mismatch")
    require(len(build["linux_release_sha256"]) == 10, "Linux binary inventory drift")
    require(build["production_path_controlled_place"] == "PASS", "PLACE rehearsal evidence absent")
    require(build["production_path_controlled_cancel"] == "PASS", "CANCEL rehearsal evidence absent")
    for closed in (
        "real_credential_used",
        "real_authservice_request_sent",
        "real_broker_get_sent",
        "real_order_post_delete_sent",
    ):
        require(build[closed] is False, f"build evidence opened surface: {closed}")

    required_r2a4 = (
        "pub struct R2a4RunPackage",
        "pub run_identity_sha256: String",
        "pub manifest_sha256: String",
        "pub keyed_account_binding_hmac_sha256: String",
        "pub account_key_generation_id: String",
        "pub account_key_manifest_sha256: String",
        "pub public_key_set_sha256: String",
        "pub source_generation_commitment_sha256: String",
        "pub operator_decision_sha256: String",
        "package_preimage(&package)",
        "package.manifest_sha256",
        "package.source_generation_commitment_sha256",
        "package.operator_decision_sha256",
        "rotation_requires_new_reviewed_package",
        "read_owned_fd(",
        "libc::O_CLOEXEC | libc::O_NOFOLLOW",
        ".create_new(true)",
        "strict_single_line(",
        "claim_nonce(",
        "source_directory.join(\"generations\")",
        "issuer_executable_sha256",
        "produce_for_effective_uid",
        "issue_for_effective_uid",
        "run_controlled_fixed_layout",
        "serve_controlled_tls_once",
    )
    for marker in required_r2a4:
        require(marker in r2a4, f"R2A4 marker absent: {marker}")
    require(r2a4.count("pub public_key_set_sha256: String") == 2, "trust/package key-set binding drift")
    require(r2a4.count("libc::O_CLOEXEC | libc::O_NOFOLLOW") == 6, "fd no-follow inventory drift")
    require(r2a4.count(".create_new(true)") == 4, "create-new inventory drift")
    require(r2a4.count("issuer_executable_sha256") == 3, "issuer identity inventory drift")
    require(r2a4.count('join("receipts").join(source).join("receipt.json")') >= 2, "per-source receipt paths absent")
    require(".trim()" not in r2a4, "general whitespace normalization present")
    require('mode == "--r2a4-qualify-fixed-layout"' in main, "fixed-layout entry absent")
    require('mode == "--r2b-one-shot"' in main, "production entry absent")
    require("--controlled-fixed-layout" in launcher and "verified_exec(" in launcher, "R2A4 fd launcher absent")
    require("let environment: Vec<CString> = Vec::new();" in launcher, "ambient environment forwarded")
    require("PLACE CANCEL" in rehearsal and rehearsal.count("setpriv --reuid") == 2, "Linux UID rehearsal incomplete")
    require(rehearsal.count('"$PACKAGE_ISSUER"') == 2 and '"$LAUNCHER" --controlled-fixed-layout' in rehearsal, "orchestration bypass")
    producer_unit = (root / "deploy/stage8b-r2a4/stage8b-r2a4-producer@.service").read_text()
    issuer_unit = (root / "deploy/stage8b-r2a4/stage8b-r2a4-issuer@.service").read_text()
    require("User=%i" in producer_unit and "User=%i" in issuer_unit, "numeric service identity drift")
    require("authority-producer %i" not in producer_unit and "authority-issuer %i" not in issuer_unit, "caller source argument restored")
    require("ReadWritePaths=/run/moex-trading/stage8b/r2a4/receipts" in issuer_unit, "issuer write root drift")
    sysusers = (root / "deploy/stage8b-r2a4/stage8b-r2a4.sysusers").read_text()
    require(sysusers.count("u m8p") == 11, "producer UID inventory drift")
    require(sysusers.count("u m8i") == 11, "issuer UID inventory drift")

    for marker in (
        "exact.exec_id == listed.exec_id",
        "&& exact.executed_quantity == listed.executed_quantity",
        "&& exact.remaining_quantity == listed.remaining_quantity",
        "&& exact.accept_at == listed.accept_at",
        "&& exact.transact_at == listed.transact_at",
        "&& exact.withdraw_at == listed.withdraw_at",
    ):
        require(marker in r2a2, f"exact/list equality missing: {marker}")
    require("issuer_executable_sha256: String" in r2a3, "issuer binary identity absent")
    require("Operation::Cancel => 6" in r2a3 and "Operation::Cancel => 6" in r2a4, "full CANCEL sequence absent")
    require(".delete(" not in r2a4 and "Method::DELETE" not in r2a4, "order DELETE introduced")
    require("api.finam.ru" not in rehearsal, "controlled rehearsal targets FINAM")
    require(sha(docs / "stage8b-p-r2a4-controlled-authority.json") != sha(docs / "stage8b-p-r2a4-authority.json"), "controlled and production authorities conflated")
    print("stage8b-p-r2a4-check: PASS sources=11 issuers=11 exact_package=true pinned_controlled_trust=true place_tls=true cancel_tls=true production_authorization=NOT_ISSUED real_finam=false")


if __name__ == "__main__":
    main()
