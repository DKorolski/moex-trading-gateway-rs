#!/usr/bin/env python3
"""Static closure checker for Stage 8B-P R2A5."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def fail(message: str) -> None:
    raise SystemExit(f"stage8b-p-r2a5-check: FAIL {message}")


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
    tool = root / "tools/stage8b-readonly-preflight/src"
    r2a3 = (tool / "r2a3.rs").read_text()
    r2a5 = (tool / "r2a5.rs").read_text()
    adapter = (root / "crates/finam-gateway/src/stage8a1_execution_capability.rs").read_text()
    launcher = (tool / "bin/stage8b-r2a5-launcher.rs").read_text()
    rehearsal = (root / "scripts/stage8b_p_r2a5_linux_rehearsal.sh").read_text()
    producer_unit = (root / "deploy/stage8b-r2a5/stage8b-r2a5-producer@.service").read_text()
    production_path = docs / "stage8b-p-r2a5-authority.json"
    trust_path = docs / "stage8b-p-r2a5-production-trust-manifest.json"
    account_path = docs / "stage8b-p-r2a5-production-account-key-manifest.json"
    source_path = docs / "stage8b-p-r2a5-source-adapter-authority.json"
    helper_path = docs / "stage8b-p-r2a5-accepted-helper-authority.json"
    production = json.loads(production_path.read_text())
    trust = json.loads(trust_path.read_text())
    source = json.loads(source_path.read_text())
    helper = json.loads(helper_path.read_text())
    status = json.loads((docs / "stage8b-p-r2a5-status.json").read_text())

    require(production["revision"] == "R2A5", "identity drift")
    require(production["authorization_status"] == "NOT_ISSUED", "R2B opened")
    require(status["authorization_status"] == "NOT_ISSUED", "status opened")
    require(all(value is False for value in status["closed_surfaces"].values()), "closed surface opened")
    require(status["promotion"]["typed_operator_decision_required_in_r2b"] is True, "operator decision carry-forward lost")
    require(production["trust_manifest_sha256"] == sha(trust_path), "trust digest drift")
    require(production["account_key_manifest_sha256"] == sha(account_path), "account manifest drift")
    require(production["source_adapter_authority_sha256"] == sha(source_path), "source adapter digest drift")
    require(source["manual_or_operator_publication_allowed"] is False, "manual publication opened")
    require(source["production_root"] == "/var/lib/moex-trading/operational-authorities", "upstream root drift")
    require(len(source["sources"]) == 10, "operational source inventory drift")
    require(all(item["max_future_skew_ms"] == 250 for item in source["sources"]), "future skew drift")

    helper_sha = (docs / "stage8b-p-r2a5-accepted-helper-sha256.txt").read_text().strip()
    require(len(helper_sha) == 64 and helper_sha != "0" * 64, "helper SHA not frozen")
    require(helper["status"] == "ACCEPTED" and helper["revision"] == "R2A5", "helper authority absent")
    require(helper["helper_executable_sha256"] == helper_sha, "helper authority digest drift")
    require(helper["acceptance_key_id"] == trust["helper_acceptance_key"]["key_id"], "helper key drift")
    require(len(helper["signature_ed25519_hex"]) == 128, "helper signature malformed")
    require(trust["helper_acceptance_key"]["key_id"] != trust["authorization_key"]["key_id"], "helper/package keys conflated")

    for marker in (
        "pub source_observed_at_utc: DateTime<Utc>",
        "pub produced_at_utc: DateTime<Utc>",
        "signed.source_observed_at_utc != signed.receipt.observed_at_utc",
        "signed.produced_at_utc < signed.source_observed_at_utc",
    ):
        require(marker in r2a3, f"timestamp binding absent: {marker}")
    require(r2a3.count("pub source_observed_at_utc: DateTime<Utc>") == 2, "source timestamp field inventory drift")
    require(r2a3.count("pub produced_at_utc: DateTime<Utc>") == 2, "producer timestamp field inventory drift")
    require("runtime.push(signed.receipt.observed_at_utc)" in r2a3, "runtime skew does not use source time")
    require("control.push(signed.receipt.observed_at_utc)" in r2a3, "control skew does not use source time")
    for marker in (
        "validate_source_freshness(source, source_observed_at_utc, produced_at)",
        "source_observed_at_utc: snapshot.source_observed_at_utc",
        "produced_at_utc: snapshot.produced_at_utc",
        "draft.helper_executable_sha256 != accepted_helper.helper_executable_sha256",
        "if executable_sha256 != accepted_helper.helper_executable_sha256",
        "helper-acceptance.ed25519",
        "source_timestamp_substitution_fails_even_with_valid_source_signature",
        "stale_source_cannot_be_laundered_by_fresh_producer_time",
        "future_source_beyond_budget_is_rejected",
    ):
        require(marker in r2a5, f"R2A5 control absent: {marker}")
    require("authoritative-stores/current.json" not in r2a5, "manual final intermediate restored")

    for marker in (
        "pub fn publish_stage8b_r2a5_operational_sources(",
        "Stage8a1DurableRequestAuthority",
        "Stage8a1TrustedCurrentSources",
        "publish_stage8b_r2a5_records(output_root, records)",
        ".create_new(true)\n            .mode(0o640)",
        ".custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)\n            .open(&temporary)",
        "r2a5_source_adapter_publishes_from_opaque_operational_authorities_only",
    ):
        require(marker in adapter, f"owner adapter control absent: {marker}")
    require("ACCEPTED_SHA256.trim()" in launcher and "verified_exec(" in launcher, "launcher exact helper check absent")
    require("let environment: Vec<CString> = Vec::new();" in launcher, "ambient environment forwarded")
    require("operational-authorities" in producer_unit and "authoritative-stores" not in producer_unit, "producer unit source drift")
    require("/proc/sys/kernel/random/boot_id" in producer_unit, "trusted clock path absent")
    require("PLACE CANCEL" in rehearsal and rehearsal.count("setpriv --reuid") == 2, "controlled rehearsal incomplete")
    require("authoritative-stores" in rehearsal and "test ! -e" in rehearsal, "manual intermediate negative absent")
    require("api.finam.ru" not in rehearsal, "controlled rehearsal targets FINAM")
    require(".delete(" not in r2a5 and "Method::DELETE" not in r2a5, "order DELETE introduced")
    print("stage8b-p-r2a5-check: PASS operational_sources=10 clock=kernel timestamps=source helper_layers=3 authorization=NOT_ISSUED real_finam=false")


if __name__ == "__main__":
    main()
