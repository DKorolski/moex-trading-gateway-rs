#!/usr/bin/env python3
"""Source-bound Stage 8B-P R2A1 corrective contract checker."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TOOLS = ROOT / "tools/stage8b-readonly-preflight"
SOURCE = TOOLS / "src/lib.rs"
MAIN = TOOLS / "src/main.rs"
MANIFEST = TOOLS / "Cargo.toml"
LOCK = TOOLS / "Cargo.lock"
NETWORK = ROOT / "docs/stage-8/stage8b-p-r2a1-network-topology-authority.json"
QUERY = ROOT / "docs/stage-8/stage8b-p-r2a1-query-policy-authority.json"
CURRENT = ROOT / "docs/stage-8/stage8b-p-r2a1-current-source-authority.json"
R2A = ROOT / "docs/stage-8/stage8b-p-r2a-readonly-preflight-authority.json"
OLD_EFFECT_SHA = "677f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06"
CURRENT_AUTHORITY_SHA = "bff33a9ff8c816daae63d1e758baa985c8bf769885c07a3f9992147a1a900867"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage8b-p-r2a1-check: FAIL {message}")


def load(path: Path) -> dict:
    return json.loads(path.read_text())


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--no-build", action="store_true")
    parser.parse_args()

    network = load(NETWORK)
    query = load(QUERY)
    current = load(CURRENT)
    old = load(R2A)
    source = SOURCE.read_text()
    production_source = source.split("#[cfg(test)]", 1)[0]
    main_source = MAIN.read_text()
    manifest = MANIFEST.read_text()
    lock = LOCK.read_text()
    require(hashlib.sha256(CURRENT.read_bytes()).hexdigest() == CURRENT_AUTHORITY_SHA, "current-source authority content drift")

    require(network["revision"] == "R2A1", "network revision drift")
    require(network["architecture"] == "separate_non_authority_readonly_preflight_helper", "architecture drift")
    require(network["production_base_url"] == "https://api.finam.ru", "base URL drift")
    require(network["effect_executable_sha256_unchanged"] == OLD_EFFECT_SHA, "effect build drift")
    require(network["helper_workspace"] == "tools/stage8b-readonly-preflight", "helper path drift")
    require(network["helper_is_effect_workspace_member"] is False, "helper entered effect workspace")
    require(network["auth_service"] == {
        "exact_request_count": 2,
        "exact_order": ["POST /v1/sessions", "POST /v1/sessions/details"],
        "token_details_is_get": False,
    }, "auth topology drift")
    broker = network["broker_truth"]
    require(broker["method_allowlist"] == ["GET"], "broker method drift")
    require(broker["PLACE"]["exact_request_count"] == 3, "PLACE GET budget drift")
    require(broker["PLACE"]["broker_order_id_allowed"] is False, "PLACE hidden target enabled")
    require(broker["CANCEL"]["exact_request_count"] == 4, "CANCEL GET budget drift")
    require(broker["CANCEL"]["synthetic_or_default_order_id_allowed"] is False, "synthetic CANCEL enabled")
    require(network["source_level_total_request_count"] == {"PLACE": 5, "CANCEL": 6}, "total budget drift")
    require(network["client_policy"] == {
        "https_only": True,
        "timeout_ms": 10000,
        "retry": "reqwest::retry::never()",
        "redirect": "reqwest::redirect::Policy::none()",
        "system_proxy": "disabled_by_no_proxy",
        "minimum_broker_get_interval_ms": 250,
    }, "client policy drift")
    execution = network["r2a1_execution"]
    require(execution["authorization_status"] == "NOT_ISSUED", "authorization issued")
    require(all(value is False for key, value in execution.items() if key != "authorization_status"), "R2A1 execution surface opened")

    require(query["policy_id"] == "stage8b-p-r2a1-query-policy-v1", "query policy id drift")
    require(query["orders_filter"] == "ClientSideAccountInstrumentAndOrderIdentity", "orders filter drift")
    require(query["trades"] == {
        "limit": 1000,
        "window_ms": 86400000,
        "time_basis": "RequestRequestedAt",
        "pagination": "SinglePageNoCursor",
        "page_full_is_blocking": True,
    }, "trades query drift")
    require(query["caller_override_allowed"] is False, "caller query override enabled")
    require(query["unknown_response_shape_is_blocking"] is True and query["non_200_is_blocking"] is True, "completeness weakened")

    old_names = old["required_current_inputs"]
    rules = current["required_inputs"]
    names = [rule["source_name"] for rule in rules]
    require(len(rules) == 17 and len(set(names)) == 17 and names == old_names, "17-source inventory drift")
    for rule in rules:
        require(all(rule.get(key) for key in ("issuer", "evidence_schema", "digest_domain", "freshness_budget_key", "skew_group")), f"incomplete source rule: {rule.get('source_name')}")
        require(rule["freshness_budget_key"] in current["freshness_budgets_ms"], f"unknown freshness key: {rule['source_name']}")
    require(current["identity_links"] == ["run_identity_sha256", "selected_account_binding_sha256", "execution_build_identity_sha256"], "identity links drift")
    require(set(current["failure_semantics"].values()) == {"BLOCK"}, "source failure semantics weakened")
    require(current["export_policy"] == "redacted_digests_and_timestamps_only", "raw source export enabled")
    require(all(value is False for value in current["authority"].values()), "current source authority widened")

    for exact in (
        ".retry(reqwest::retry::never())",
        ".redirect(Policy::none())",
        ".no_proxy()",
        ".https_only(https_only)",
        "pub const AUTH_REQUEST_BUDGET: usize = 2;",
        "pub const PLACE_GET_BUDGET: usize = 3;",
        "pub const CANCEL_GET_BUDGET: usize = 4;",
        "pub const REQUEST_TIMEOUT_MS: u64 = 10_000;",
        "pub const MIN_REQUEST_INTERVAL_MS: u64 = 250;",
        "pub const TRADES_LIMIT: usize = 1_000;",
        "pub const TRADES_WINDOW_MS: i64 = 24 * 60 * 60 * 1_000;",
        "if parsed.trades.len() >= TRADES_LIMIT",
        "current_sources = validate_current_sources(",
        "let (auth, broker) = production_clients()?;",
    ):
        require(exact in source, f"implementation marker missing: {exact}")
    require(source.index("current_sources = validate_current_sources(") < source.index("let (auth, broker) = production_clients()?;"), "network client created before current-source validation")
    require(production_source.count(".post(") == 2, "AuthService POST callsite count drift")
    require(production_source.count(".get(url)") == 1, "broker GET callsite count drift")
    require(".delete(" not in production_source and "Method::DELETE" not in production_source, "DELETE surface added")
    require("SYNTHETIC" in source and "starts_with(\"SYNTHETIC\")" in source, "synthetic CANCEL guard missing")
    require("current_executable_sha256()" in main_source, "helper build identity not self-derived")
    require("create_new(true)" in main_source and "symlink_metadata" in main_source, "file boundary weakened")
    require("FINAM_SECRET_TOKEN" in main_source and "FINAM_ACCOUNT_ID" in main_source, "credential input drift")
    require("[workspace]" in manifest, "helper is not standalone workspace")
    require('reqwest = { version = "=0.12.24"' in manifest, "reqwest version drift")
    require('name = "reqwest"\nversion = "0.12.24"' in lock, "locked reqwest drift")
    require("tools/stage8b-readonly-preflight" not in (ROOT / "Cargo.toml").read_text(), "helper entered root effect workspace")
    print("stage8b-p-r2a1-check: PASS sources=17 PLACE=2POST+3GET CANCEL=2POST+4GET retry=never redirect=none proxy=none real_http=false authorization=NOT_ISSUED")


if __name__ == "__main__":
    main()
