#!/usr/bin/env python3
"""Source/contract/custody checker for Stage 8B-P R2A3."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def fail(message: str) -> None:
    raise SystemExit(f"stage8b-p-r2a3-check: FAIL {message}")


def require(condition: bool, message: str) -> None:
    if not condition:
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
    authority = json.loads((docs / "stage8b-p-r2a3-authority.json").read_text())
    build = json.loads((docs / "stage8b-p-r2a3-build-evidence.json").read_text())
    snapshot_path = docs / "stage8b-p-r2a3-finam-read-contract-snapshot.json"
    snapshot = json.loads(snapshot_path.read_text())
    r2a3 = (tool / "src/r2a3.rs").read_text()
    r2a2 = (tool / "src/r2a2.rs").read_text()
    lib_source = (tool / "src/lib.rs").read_text()
    main_source = (tool / "src/main.rs").read_text()
    launcher = (tool / "src/bin/stage8b-r2a3-launcher.rs").read_text()
    issuer = (tool / "src/bin/stage8b-r2a3-authority-issuer.rs").read_text()
    cargo = (tool / "Cargo.toml").read_text()

    require(authority["stage"] == "8B-P" and authority["revision"] == "R2A3", "identity drift")
    require(authority["authorization_status"] == "NOT_ISSUED", "authorization opened")
    closed = authority["closed_surfaces"]
    require(all(value is False for value in closed.values()), "closed surface opened")
    require(authority["runnable_helper"]["issued_run_package_in_repository"] is False, "run package issued")
    require(authority["runnable_helper"]["fd_bound_linux_execveat"] is True, "fd launch authority drift")
    require(authority["runnable_helper"]["ambient_environment_forwarded"] is False, "ambient environment opened")
    accepted_helper = (docs / "stage8b-p-r2a3-accepted-helper-sha256.txt").read_text().strip()
    require(len(accepted_helper) == 64 and all(ch in "0123456789abcdef" for ch in accepted_helper), "helper digest malformed")
    require(authority["runnable_helper"]["accepted_helper_sha256_status"] == "FROZEN", "helper digest not frozen")
    require(authority["runnable_helper"]["accepted_helper_sha256"] == accepted_helper, "helper digest authority drift")
    require(authority["provenance"]["model"] == "SOURCE_SPECIFIC_ED25519_ISSUERS", "signature model drift")
    require(authority["provenance"]["source_count"] == 11, "source count drift")
    require(authority["provenance"]["producer_and_issuer_separated"] is True, "custody separation removed")
    require(authority["provenance"]["verifier_private_key_access"] is False, "verifier can mint")
    require(authority["provenance"]["closed_claim_inventory_per_source"] is True, "claim inventory opened")
    require(authority["provenance"]["durable_nonce_replay_registry"] is True, "anti replay removed")
    require(build["revision"] == "R2A3" and build["build_target"] == "x86_64-unknown-linux-gnu", "build evidence drift")
    source_paths = {
        "lib_rs": tool / "src/lib.rs",
        "r2a2_rs": tool / "src/r2a2.rs",
        "r2a3_rs": tool / "src/r2a3.rs",
        "main_rs": tool / "src/main.rs",
        "launcher_rs": tool / "src/bin/stage8b-r2a3-launcher.rs",
        "issuer_rs": tool / "src/bin/stage8b-r2a3-authority-issuer.rs",
        "cargo_lock": tool / "Cargo.lock",
    }
    for name, path in source_paths.items():
        require(build["source_sha256"][name] == sha(path), f"build source drift: {name}")
    require(build["linux_release_sha256"]["stage8b-readonly-preflight"] == accepted_helper, "Linux helper evidence drift")
    require(build["authorization_status"] == "NOT_ISSUED", "build evidence opened authorization")
    require(build["real_credential_used"] is False and build["real_authservice_request_sent"] is False and build["real_broker_get_sent"] is False and build["real_order_post_delete_sent"] is False, "build evidence opened real network")
    chronology = authority["chronology"]
    require(chronology["control_source_max_skew_ms"] == 1000, "control skew drift")
    require(chronology["runtime_source_max_skew_ms"] == 5000, "runtime skew drift")
    require(chronology["minimum_broker_get_interval_ms"] == 250, "pacing drift")
    require(all(chronology[key] is True for key in (
        "revalidate_before_each_network_class",
        "revalidate_before_each_get",
        "revalidate_before_final_evidence",
        "token_timestamps_parsed",
        "request_start_end_recorded",
    )), "freshness chronology weakened")
    require(authority["reducer"]["place_prior_matching_trade"] == "BLOCK", "PLACE prior trade opened")
    require(authority["reducer"]["cancel_trade_must_match_exact_order"] is True, "CANCEL linkage opened")
    require(authority["reducer"]["exact_and_list_full_immutable_equality"] is True, "order equality weakened")

    require(snapshot["revision"] == "R2A3", "contract snapshot revision drift")
    require(snapshot["real_credential_used"] is False, "contract refresh used credential")
    require(snapshot["authservice_request_sent"] is False and snapshot["broker_get_sent"] is False, "contract refresh sent API request")
    require(len(snapshot["documents"]) == 6, "official document count drift")
    require(authority["official_read_contract"]["snapshot_sha256"] == sha(snapshot_path), "contract snapshot hash drift")
    names = {record["name"] for record in snapshot["documents"]}
    require(names == {"auth", "token_details", "get_account", "trades", "get_orders", "get_order"}, "official inventory drift")
    for record in snapshot["documents"]:
        path = root / record["path"]
        require(path.is_file(), f"official document absent: {record['name']}")
        require(path.stat().st_size == record["bytes"] and sha(path) == record["sha256"], f"official document drift: {record['name']}")

    fixture_dir = tool / "fixtures/r2a3"
    fixtures = {path.name: json.loads(path.read_text()) for path in fixture_dir.glob("*.json")}
    require(set(fixtures) == {"auth.json", "token-details.json", "account.json", "trades.json", "orders.json", "order.json"}, "golden fixture inventory drift")
    require("triggered_order_id" in fixtures["order.json"], "exact order trigger field absent")
    require("triggered_order_id" in fixtures["orders.json"]["orders"][0], "orders trigger field absent")
    require({"maintenance_margin", "daily_pnl", "unrealized_pnl"} <= set(fixtures["account.json"]["positions"][0]), "position fields incomplete")
    require({"portfolio_mct", "portfolio_forts", "first_trade_date"} <= set(fixtures["account.json"]), "account fields incomplete")
    require({"comment", "accrued_interest", "currency", "order_id"} <= set(fixtures["trades.json"]["trades"][0]), "trade fields incomplete")

    required_r2a3 = [
        "pub const SIGNATURE_DOMAIN: &[u8] = b\"stage8b-p-r2a3-source-receipt-ed25519-v1\";",
        "pub const CONTROL_SOURCE_MAX_SKEW_MS: i64 = 1_000;",
        "pub const RUNTIME_SOURCE_MAX_SKEW_MS: i64 = 5_000;",
        "pub const MIN_BROKER_GET_INTERVAL_MS: u64 = 250;",
        "producer_uid: u32",
        "source_generation: u64",
        "pub source_generation: u64,\n    pub run_nonce_sha256: String",
        "fn expected_claim_names(",
        "actual_claims != expected_claims",
        "require_owned_file(&source_path, source_producer_uid(source_name)?, false)?;",
        "require_owned_file(&path, source_issuer_uid(source)?, false)?;",
        "key.verify(&receipt_signing_preimage(&signed)?",
        "claim_run_nonce_once(\n        Path::new(PRODUCTION_NONCE_REGISTRY)",
        ".write(true)\n        .create_new(true)\n        .mode(0o600)",
        "libc::execveat(",
        "libc::AT_EMPTY_PATH",
        "descriptor_flags & !libc::FD_CLOEXEC",
        "current_linux_executable_sha256()?",
        "package.authorization_status != \"ISSUED\"",
        "manifest_operation != package_operation",
        "authorization_status: \"ISSUED\"",
        "authorization_status: \"NOT_ISSUED\"",
        "tokio::time::sleep(minimum - elapsed).await",
        "let (final_manifest, _) = revalidate(&input)?;",
        "crate::hardened_client_builder(true, Duration::from_secs(2))",
    ]
    for marker in required_r2a3:
        require(marker in r2a3, f"R2A3 source marker absent: {marker}")
    require("ed25519-dalek = \"2\"" in cargo, "Ed25519 dependency absent")
    require("pub(crate) fn hardened_client_builder(" in lib_source, "shared hardened client builder absent")
    require("--r2b-one-shot" in main_source and "--qualify-controlled" in main_source, "runnable modes absent")
    require(main_source.count('mode == "--r2b-one-shot"') == 1, "one-shot dispatch drift")
    require(main_source.count('mode == "--qualify-controlled"') == 1, "controlled dispatch drift")
    require("std::env::args().collect" in main_source and "tokio::time::interval" not in main_source, "entry widened")
    require("const HELPER: &str = \"/opt/moex-trading/stage8b-r2a3/bin/stage8b-readonly-preflight\";" in launcher, "helper path drift")
    require("let environment: Vec<CString> = Vec::new();" in launcher and "vars_os" not in launcher, "ambient environment forwarded")
    require("issue_from_fixed_source(&source)?" in issuer, "issuer entry absent")

    required_r2a2 = [
        "#[serde(deny_unknown_fields)]",
        "mac.verify_slice(&asserted)",
        "while let Some(chunk) = response.chunk().await",
        "raw_body_sha256_exported: false",
        "if target_position != manifest.approved_pre_run_position",
        "|| target_trade_count != 0",
        "trade.order_id.as_deref() != manifest.broker_order_id.as_deref()",
        "exact.order_id == listed.order_id",
        "&& exact.order == listed.order",
        "triggered_order_id: Option<String>",
        "maintenance_margin: Option<StrictDecimal>",
        "accrued_interest: Option<StrictDecimal>",
    ]
    for marker in required_r2a2:
        require(marker in r2a2, f"R2A2 semantic marker absent: {marker}")
    require(r2a2.count("#[serde(deny_unknown_fields)]") == 18, "strict DTO inventory drift")
    require(r2a2.count("mac.verify_slice(&asserted)") == 2, "constant-time verification inventory drift")
    require(r2a2.count("raw_body_sha256_exported: false") == 3, "redacted receipt inventory drift")
    require(r2a2.count("maintenance_margin: Option<StrictDecimal>") == 2, "account margin schema drift")
    require(r2a3.count(".post(") == 2, "AuthService POST topology drift")
    require(".delete(" not in r2a3 and "Method::DELETE" not in r2a3, "order DELETE introduced")
    require("FINAM_SECRET_TOKEN" not in main_source and "FINAM_ACCOUNT_ID" not in main_source, "embedded credential introduced")
    print("stage8b-p-r2a3-check: PASS contracts=6 fixtures=6 sources=11 ed25519=true runnable=true fd_bound=true authorization=NOT_ISSUED real_http=false")


if __name__ == "__main__":
    main()
