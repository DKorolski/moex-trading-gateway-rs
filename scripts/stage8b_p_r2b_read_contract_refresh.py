#!/usr/bin/env python3
"""Verify or freshly observe the exact six-document R2B read/auth contract."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SNAPSHOT = Path("docs/stage-8/stage8b-p-r2a3-finam-read-contract-snapshot.json")
EVIDENCE = Path("docs/stage-8/stage8b-p-r2b-r0-r1-read-contract-refresh-evidence.json")
EXPECTED_SNAPSHOT_SHA256 = "7c8e6bcd02f907af93ea1386499d03bff194da76a1eb2b19dd9c2ff1f97403c5"
DOCUMENTS = {
    "auth": (
        "https://api.finam.ru/docs/rest/authservice_auth.md",
        ("**Path:** /v1/sessions", '"token": "string"'),
    ),
    "token_details": (
        "https://api.finam.ru/docs/rest/authservice_tokendetails.md",
        ("**Path:** /v1/sessions/details", '"account_ids"', '"readonly"'),
    ),
    "get_account": (
        "https://api.finam.ru/docs/rest/accountsservice_getaccount.md",
        ("**Path:** /v1/accounts/{account_id}", '"maintenance_margin"', '"portfolio_forts"'),
    ),
    "trades": (
        "https://api.finam.ru/docs/rest/accountsservice_trades.md",
        ("**Path:** /v1/accounts/{account_id}/trades", '"comment"', '"currency"'),
    ),
    "get_orders": (
        "https://api.finam.ru/docs/rest/ordersservice_getorders.md",
        ("**Path:** /v1/accounts/{account_id}/orders", '"triggered_order_id"'),
    ),
    "get_order": (
        "https://api.finam.ru/docs/rest/ordersservice_getorder.md",
        ("**Path:** /v1/accounts/{account_id}/orders/{order_id}", '"triggered_order_id"'),
    ),
}
EMBEDDERS = (
    "tools/stage8b-readonly-preflight/src/r2a3.rs",
    "tools/stage8b-readonly-preflight/src/r2a4.rs",
    "tools/stage8b-readonly-preflight/src/r2a5.rs",
)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def embedded_inventory(root: Path) -> tuple[bytes, dict[str, dict[str, object]]]:
    snapshot_bytes = (root / SNAPSHOT).read_bytes()
    require(digest(snapshot_bytes) == EXPECTED_SNAPSHOT_SHA256, "embedded snapshot hash drift")
    snapshot = json.loads(snapshot_bytes)
    records = snapshot.get("documents")
    require(isinstance(records, list) and len(records) == 6, "embedded inventory count drift")
    inventory = {record["name"]: record for record in records}
    require(set(inventory) == set(DOCUMENTS), "embedded document names drift")
    for name, (url, markers) in DOCUMENTS.items():
        record = inventory[name]
        require(record["url"] == url, f"embedded URL drift: {name}")
        body = (root / record["path"]).read_bytes()
        text = body.decode("utf-8")
        require(len(body) == record["bytes"], f"embedded bytes drift: {name}")
        require(digest(body) == record["sha256"], f"embedded digest drift: {name}")
        require(all(marker in text for marker in markers), f"embedded marker drift: {name}")
    include = 'include_bytes!("../../../' + SNAPSHOT.as_posix() + '")'
    for relative in EMBEDDERS:
        require(include in (root / relative).read_text(encoding="utf-8"), f"helper binding drift: {relative}")
    return snapshot_bytes, inventory


def verify_evidence(root: Path) -> None:
    snapshot_bytes, inventory = embedded_inventory(root)
    evidence = json.loads((root / EVIDENCE).read_text(encoding="utf-8"))
    require(evidence["document_count"] == 6, "evidence document count drift")
    require(evidence["read_contract_snapshot_path"] == SNAPSHOT.as_posix(), "evidence path drift")
    require(evidence["read_contract_snapshot_sha256"] == digest(snapshot_bytes), "evidence snapshot digest drift")
    require(evidence["helper_embedded_snapshot_sha256"] == digest(snapshot_bytes), "helper digest binding drift")
    require(evidence["future_run_package_contract_snapshot_sha256"] == digest(snapshot_bytes), "run-package binding drift")
    require(evidence["all_http_200"] is True and evidence["all_match_embedded_snapshot"] is True, "refresh failed")
    require(evidence["credentials_used"] is False, "credentials used")
    require(evidence["authservice_called"] is False and evidence["broker_get_sent"] is False, "broker endpoint used")
    require(evidence["activation_refresh_required"] is True, "activation refresh requirement removed")
    require(evidence["activation_max_age_seconds"] == 1800, "activation max age drift")
    observed = evidence.get("documents")
    require(isinstance(observed, list) and len(observed) == 6, "observed inventory drift")
    for record in observed:
        name = record["name"]
        require(name in inventory, f"unknown observed document: {name}")
        require(record["url"] == DOCUMENTS[name][0], f"observed URL drift: {name}")
        require(record["http_status"] == 200, f"observed HTTP drift: {name}")
        require(record["observed_bytes"] == inventory[name]["bytes"], f"observed bytes drift: {name}")
        require(record["observed_sha256"] == inventory[name]["sha256"], f"observed digest drift: {name}")
        require(record["required_markers"] == list(DOCUMENTS[name][1]), f"marker inventory drift: {name}")
        require(record["all_required_markers_present"] is True, f"marker evidence failed: {name}")
        require(record["matches_embedded_document"] is True, f"document mismatch: {name}")


def fresh_observation(root: Path) -> dict[str, object]:
    snapshot_bytes, inventory = embedded_inventory(root)
    records = []
    for name, (url, markers) in DOCUMENTS.items():
        request = urllib.request.Request(
            url, headers={"User-Agent": "moex-trading-stage8b-r2b-read-contract-refresh/1"}
        )
        with urllib.request.urlopen(request, timeout=30) as response:
            body = response.read()
            status = response.status
            content_type = response.headers.get_content_type()
        text = body.decode("utf-8")
        local = (root / inventory[name]["path"]).read_bytes()
        records.append(
            {
                "name": name,
                "url": url,
                "http_status": status,
                "content_type": content_type,
                "observed_bytes": len(body),
                "observed_sha256": digest(body),
                "required_markers": list(markers),
                "all_required_markers_present": all(marker in text for marker in markers),
                "matches_embedded_document": body == local,
            }
        )
    return {
        "schema_version": 1,
        "stage": "Stage 8B-P R2B Issuance Package R0-R1",
        "purpose": "DESIGN_CLOSURE_ONLY_NOT_ACTIVATION_AUTHORITY",
        "retrieved_at_utc": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "read_contract_snapshot_path": SNAPSHOT.as_posix(),
        "read_contract_snapshot_sha256": digest(snapshot_bytes),
        "helper_embedded_snapshot_sha256": digest(snapshot_bytes),
        "future_run_package_contract_snapshot_sha256": digest(snapshot_bytes),
        "document_count": 6,
        "documents": records,
        "all_http_200": all(record["http_status"] == 200 for record in records),
        "all_match_embedded_snapshot": all(record["matches_embedded_document"] for record in records),
        "credentials_used": False,
        "authservice_called": False,
        "broker_get_sent": False,
        "activation_refresh_required": True,
        "activation_max_age_seconds": 1800,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--refresh", action="store_true", help="fetch official public docs and print evidence JSON")
    args = parser.parse_args()
    if args.refresh:
        document = fresh_observation(ROOT)
        require(document["all_http_200"] is True, "official refresh HTTP failure")
        require(document["all_match_embedded_snapshot"] is True, "official contract drift")
        require(all(item["all_required_markers_present"] for item in document["documents"]), "official marker drift")
        print(json.dumps(document, indent=2, sort_keys=True))
        return
    verify_evidence(ROOT)
    print(
        "stage8b-p-r2b-read-contract-refresh: PASS mode=offline-verify documents=6 "
        f"snapshot_sha256={EXPECTED_SNAPSHOT_SHA256} activation_max_age_seconds=1800"
    )


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError, KeyError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2b-read-contract-refresh: FAIL {error}") from error
