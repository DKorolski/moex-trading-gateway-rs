#!/usr/bin/env python3
"""Source-bound Stage 8B-P R2A2 semantic/provenance checker."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage8b-p-r2a2-check: FAIL {message}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path)
    args = parser.parse_args()
    root = (args.root or Path(__file__).resolve().parents[1]).resolve()
    module = (root / "tools/stage8b-readonly-preflight/src/r2a2.rs").read_text()
    main_source = (root / "tools/stage8b-readonly-preflight/src/main.rs").read_text()
    legacy = (root / "tools/stage8b-readonly-preflight/src/lib.rs").read_text()
    manifest = (root / "tools/stage8b-readonly-preflight/Cargo.toml").read_text()
    authority = json.loads(
        (root / "docs/stage-8/stage8b-p-r2a2-semantic-provenance-authority.json").read_text()
    )
    build = json.loads(
        (root / "docs/stage-8/stage8b-p-r2a2-build-evidence.json").read_text()
    )
    launcher = (root / "scripts/launch_stage8b_p_r2a2_qualified.sh").read_text()

    require(authority["revision"] == "R2A2", "authority revision drift")
    require(authority["pre_network_local_authorities"]["count"] == 11, "local source count drift")
    require(authority["pre_network_local_authorities"]["accepted_key_generation_id"] == "1", "local receipt key generation drift")
    require(authority["pre_network_local_authorities"]["broker_derived_sources_allowed"] is False, "broker truth moved before network")
    require(authority["manifest_validation"]["accepted_r1b_fixed_identities_exact"] is True, "fixed R1B identity validation disabled")
    require(authority["manifest_validation"]["dynamic_receipt_claims_exact"] is True, "dynamic authority binding disabled")
    require(authority["manifest_validation"]["account_binding"]["constant_time_verify"] is True, "constant-time account verification disabled")
    require(authority["manifest_validation"]["endpoint_identity_recomputed_from_accepted_r1b_formula"] is True, "endpoint recomputation disabled")
    require(authority["strict_broker_truth"]["unknown_or_missing_required_shape"] == "BLOCK", "strict broker schema weakened")
    require(authority["strict_broker_truth"]["position_must_equal_approved_baseline"] is True, "position baseline weakened")
    require(authority["response_caps_bytes"] == {
        "auth": 65536,
        "exact_order": 262144,
        "orders": 4194304,
        "trades": 16777216,
        "account": 4194304,
    }, "response cap drift")
    require(all(authority["tls_qualification"][key] is True for key in (
        "standalone_helper_build",
        "wrong_ca_rejected_before_http",
        "wrong_hostname_rejected_before_http",
    )), "TLS qualification weakened")
    require(authority["helper_launch"]["self_hash_is_authority"] is False, "self hash became authority")
    require(authority["helper_launch"]["r2a2_binary_network_entry"] == "FAIL_CLOSED", "R2A2 binary opened network")
    require(authority["closed_surfaces"]["authorization_status"] == "NOT_ISSUED", "authorization issued")
    require(all(value is True for key, value in authority["closed_surfaces"].items() if key != "authorization_status"), "closed surface opened")

    markers = (
        "pub const LOCAL_RECEIPT_DOMAIN: &str = \"stage8b-p-r2a2-local-authority-receipt-v1\";",
        "pub const LOCAL_RECEIPT_KEY_GENERATION_ID: &str = \"1\";",
        "pub const ACCOUNT_HMAC_DOMAIN: &[u8] = b\"moex-stage8b-account-binding-v1\";",
        "pub const ENDPOINT_IDENTITY_DOMAIN: &str = \"stage8b-i-r2-endpoint-identity-v1\";",
        "pub const MAX_RUN_AHEAD_MS: i64 = 60_000;",
        "pub const AUTH_BODY_CAP: usize = 64 * 1024;",
        "pub const EXACT_ORDER_BODY_CAP: usize = 256 * 1024;",
        "pub const ORDERS_BODY_CAP: usize = 4 * 1024 * 1024;",
        "pub const TRADES_BODY_CAP: usize = 16 * 1024 * 1024;",
        "pub const ACCOUNT_BODY_CAP: usize = 4 * 1024 * 1024;",
        "#[serde(deny_unknown_fields)]",
        "mac.verify_slice(&asserted)",
        "libc::O_CLOEXEC | libc::O_NOFOLLOW",
        "metadata.nlink() != 1",
        "metadata.uid() != effective_uid",
        "metadata.mode() & 0o077 != 0",
        "load_production_source_keys(",
        "load_production_account_key(",
        "endpoint_identity(",
        "validate_manifest_and_local_authorities(",
        "broker_derived_sources_accepted_pre_network: false",
        "if target_position != manifest.approved_pre_run_position",
        "if !working",
        "while let Some(chunk) = response.chunk().await",
        "raw_body_sha256_exported: false",
        "standalone_helper_tls_accepts_only_matching_ca_and_hostname",
        "controlled_pipeline_derives_broker_truth_only_after_fresh_reads",
    )
    for marker in markers:
        require(marker in module, f"implementation marker missing: {marker}")
    require(module.count("#[serde(deny_unknown_fields)]") == 12, "strict DTO/receipt schema count drift")
    require(module.count("mac.verify_slice(&asserted)") == 2, "constant-time verifier count drift")
    require(module.count("raw_body_sha256_exported: false") == 3, "raw-body privacy marker count drift")
    require(module.count('source_name: "account_orders"') == 0, "caller broker orders receipt restored")
    require("danger_accept_invalid_certs" not in module, "invalid certificate override enabled")
    require("danger_accept_invalid_hostnames" not in module, "invalid hostname override enabled")
    require("FINAM_SECRET_TOKEN" not in main_source and "FINAM_ACCOUNT_ID" not in main_source, "R2A2 binary reads real credential/account")
    require("qualification-only" in main_source and "network entry remain closed" in main_source, "fail-closed binary marker missing")
    require("#[cfg(test)]" in legacy and "pub(crate) async fn execute_production" in legacy, "R2A1 production call path not retired")
    require("[workspace]" in manifest, "helper left standalone workspace")
    for dependency in ("hmac = \"0.12\"", "rust_decimal", "rcgen", "tokio-rustls"):
        require(dependency in manifest, f"qualification dependency missing: {dependency}")
    helper_sha = "0c6dcde920de131863fe12632b0e3092f30fedc796e4627873cea89b6aace363"
    require(build["helper"]["executable_sha256"] == helper_sha, "candidate helper digest drift")
    require(build["helper"]["self_hash_is_authority"] is False, "build evidence trusts self hash")
    require(build["qualification"]["unit_and_controlled_tests_passed"] == build["qualification"]["unit_and_controlled_tests_total"] == 22, "controlled test evidence drift")
    require(build["qualification"]["new_negative_mutations_passed"] == build["qualification"]["new_negative_mutations_total"] == 26, "negative evidence drift")
    require(build["qualification"]["full_gate_exit_code"] == 0, "full gate evidence failed")
    require(f'accepted_sha256="{helper_sha}"' in launcher, "external launcher digest drift")
    require('helper="/opt/moex-trading/stage8b-r2a2/bin/stage8b-readonly-preflight"' in launcher, "launcher path became caller-selected")
    require('sha256sum "$helper"' in launcher and 'exec "$helper"' in launcher, "external pre-exec digest check missing")
    print("stage8b-p-r2a2-check: PASS local_hmac_receipts=11 broker_truth=post_read strict_dto=true bounded=true tls=true real_http=false authorization=NOT_ISSUED")


if __name__ == "__main__":
    main()
