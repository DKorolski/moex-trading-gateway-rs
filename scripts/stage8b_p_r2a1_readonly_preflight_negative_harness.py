#!/usr/bin/env python3
"""Reject source and authority weakening of Stage 8B-P R2A1."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage8b_p_r2a1_readonly_preflight_check.py"
NETWORK = "docs/stage-8/stage8b-p-r2a1-network-topology-authority.json"
QUERY = "docs/stage-8/stage8b-p-r2a1-query-policy-authority.json"
CURRENT = "docs/stage-8/stage8b-p-r2a1-current-source-authority.json"
SOURCE = "tools/stage8b-readonly-preflight/src/lib.rs"


def json_edit(path: str, edit: Callable[[dict], None]) -> Callable[[Path], None]:
    def mutate(root: Path) -> None:
        file = root / path
        value = json.loads(file.read_text())
        edit(value)
        file.write_text(json.dumps(value, indent=2) + "\n")
    return mutate


def source_replace(old: str, new: str) -> Callable[[Path], None]:
    def mutate(root: Path) -> None:
        file = root / SOURCE
        text = file.read_text()
        if old not in text:
            raise RuntimeError(f"mutation anchor missing: {old}")
        file.write_text(text.replace(old, new, 1))
    return mutate


def cases() -> list[tuple[str, Callable[[Path], None]]]:
    return [
        ("retry-omitted", source_replace(".retry(reqwest::retry::never())", "")),
        ("redirect-omitted", source_replace(".redirect(Policy::none())", "")),
        ("proxy-omitted", source_replace(".no_proxy()", "")),
        ("https-weakened", source_replace(".https_only(https_only)", ".https_only(false)")),
        ("auth-budget", source_replace("AUTH_REQUEST_BUDGET: usize = 2", "AUTH_REQUEST_BUDGET: usize = 3")),
        ("place-budget", source_replace("PLACE_GET_BUDGET: usize = 3", "PLACE_GET_BUDGET: usize = 4")),
        ("cancel-budget", source_replace("CANCEL_GET_BUDGET: usize = 4", "CANCEL_GET_BUDGET: usize = 5")),
        ("timeout", source_replace("REQUEST_TIMEOUT_MS: u64 = 10_000", "REQUEST_TIMEOUT_MS: u64 = 30_000")),
        ("interval", source_replace("MIN_REQUEST_INTERVAL_MS: u64 = 250", "MIN_REQUEST_INTERVAL_MS: u64 = 0")),
        ("trades-limit-source", source_replace("TRADES_LIMIT: usize = 1_000", "TRADES_LIMIT: usize = 2_000")),
        ("trades-window-source", source_replace("TRADES_WINDOW_MS: i64 = 24 * 60 * 60 * 1_000", "TRADES_WINDOW_MS: i64 = 48 * 60 * 60 * 1_000")),
        ("page-full-nonblocking", source_replace("if parsed.trades.len() >= TRADES_LIMIT", "if parsed.trades.len() > TRADES_LIMIT")),
        ("validation-gate-bypassed", source_replace("current_sources = validate_current_sources(", "current_sources = /* bypass */ validate_current_sources(")),
        ("synthetic-guard", source_replace("!value.starts_with(\"SYNTHETIC\")", "true")),
        ("network-base", json_edit(NETWORK, lambda x: x.__setitem__("production_base_url", "https://example.invalid"))),
        ("effect-sha", json_edit(NETWORK, lambda x: x.__setitem__("effect_executable_sha256_unchanged", "0" * 64))),
        ("auth-count", json_edit(NETWORK, lambda x: x["auth_service"].__setitem__("exact_request_count", 1))),
        ("token-details-get", json_edit(NETWORK, lambda x: x["auth_service"].__setitem__("token_details_is_get", True))),
        ("broker-post", json_edit(NETWORK, lambda x: x["broker_truth"].__setitem__("method_allowlist", ["GET", "POST"]))),
        ("place-total", json_edit(NETWORK, lambda x: x["source_level_total_request_count"].__setitem__("PLACE", 6))),
        ("cancel-total", json_edit(NETWORK, lambda x: x["source_level_total_request_count"].__setitem__("CANCEL", 7))),
        ("real-auth", json_edit(NETWORK, lambda x: x["r2a1_execution"].__setitem__("auth_request_sent", True))),
        ("authorization", json_edit(NETWORK, lambda x: x["r2a1_execution"].__setitem__("authorization_status", "ISSUED"))),
        ("query-filter", json_edit(QUERY, lambda x: x.__setitem__("orders_filter", "ServerDefault"))),
        ("query-limit", json_edit(QUERY, lambda x: x["trades"].__setitem__("limit", 500))),
        ("query-window", json_edit(QUERY, lambda x: x["trades"].__setitem__("window_ms", 3600000))),
        ("query-basis", json_edit(QUERY, lambda x: x["trades"].__setitem__("time_basis", "ResponseTime"))),
        ("query-pagination", json_edit(QUERY, lambda x: x["trades"].__setitem__("pagination", "CallerCursor"))),
        ("query-page-full", json_edit(QUERY, lambda x: x["trades"].__setitem__("page_full_is_blocking", False))),
        ("query-override", json_edit(QUERY, lambda x: x.__setitem__("caller_override_allowed", True))),
        ("unknown-shape", json_edit(QUERY, lambda x: x.__setitem__("unknown_response_shape_is_blocking", False))),
        ("source-removed", json_edit(CURRENT, lambda x: x["required_inputs"].pop())),
        ("source-duplicate", json_edit(CURRENT, lambda x: x["required_inputs"].append(x["required_inputs"][0].copy()))),
        ("issuer", json_edit(CURRENT, lambda x: x["required_inputs"][0].__setitem__("issuer", "Caller"))),
        ("schema", json_edit(CURRENT, lambda x: x["required_inputs"][0].__setitem__("evidence_schema", "unknown"))),
        ("digest-domain", json_edit(CURRENT, lambda x: x["required_inputs"][0].__setitem__("digest_domain", "weak"))),
        ("freshness-key", json_edit(CURRENT, lambda x: x["required_inputs"][0].__setitem__("freshness_budget_key", "unknown"))),
        ("identity-links", json_edit(CURRENT, lambda x: x["identity_links"].pop())),
        ("missing-allowed", json_edit(CURRENT, lambda x: x["failure_semantics"].__setitem__("missing", "ALLOW"))),
        ("k2-authority", json_edit(CURRENT, lambda x: x["authority"].__setitem__("can_issue_k2", True))),
    ]


def main() -> None:
    mutations = cases()
    if len(mutations) != 40:
        raise SystemExit(f"stage8b-p-r2a1-negative: FAIL inventory={len(mutations)}")
    with tempfile.TemporaryDirectory(prefix="stage8b-p-r2a1-negative-") as temp:
        root = Path(temp) / "root"
        shutil.copytree(ROOT, root, ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports"))
        originals = {path: (root / path).read_bytes() for path in (NETWORK, QUERY, CURRENT, SOURCE)}
        for index, (name, mutation) in enumerate(mutations, 1):
            for path, content in originals.items():
                (root / path).write_bytes(content)
            mutation(root)
            result = subprocess.run(
                ["python3", CHECKER, "--no-build"], cwd=root, text=True,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r2a1-negative: FAIL mutation passed: {name}")
            print(f"PASS {index:02d}/40 {name}")
    print("stage8b-p-r2a1-negative: PASS 40/40")


if __name__ == "__main__":
    main()
