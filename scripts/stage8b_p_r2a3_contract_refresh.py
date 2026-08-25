#!/usr/bin/env python3
"""Refresh the exact official FINAM read/auth contract snapshot for R2A3."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "docs/stage-8/finam-r2a3-read-contracts"
SNAPSHOT = ROOT / "docs/stage-8/stage8b-p-r2a3-finam-read-contract-snapshot.json"
BASE = "https://api.finam.ru/docs/rest"
DOCUMENTS = {
    "auth": f"{BASE}/authservice_auth.md",
    "token_details": f"{BASE}/authservice_tokendetails.md",
    "get_account": f"{BASE}/accountsservice_getaccount.md",
    "trades": f"{BASE}/accountsservice_trades.md",
    "get_orders": f"{BASE}/ordersservice_getorders.md",
    "get_order": f"{BASE}/ordersservice_getorder.md",
}
REQUIRED_MARKERS = {
    "auth": ("**Path:** /v1/sessions", '"token": "string"'),
    "token_details": ("**Path:** /v1/sessions/details", '"account_ids"', '"readonly"'),
    "get_account": (
        "**Path:** /v1/accounts/{account_id}",
        '"maintenance_margin"',
        '"daily_pnl"',
        '"unrealized_pnl"',
        '"portfolio_mct"',
        '"portfolio_forts"',
        '"first_trade_date"',
    ),
    "trades": (
        "**Path:** /v1/accounts/{account_id}/trades",
        '"comment"',
        '"accrued_interest"',
        '"currency"',
    ),
    "get_orders": (
        "**Path:** /v1/accounts/{account_id}/orders",
        '"triggered_order_id"',
    ),
    "get_order": (
        "**Path:** /v1/accounts/{account_id}/orders/{order_id}",
        '"triggered_order_id"',
    ),
}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def fetch(url: str) -> bytes:
    request = urllib.request.Request(
        url,
        headers={"User-Agent": "moex-trading-stage8b-r2a3-contract-refresh/1"},
    )
    with urllib.request.urlopen(request, timeout=20) as response:
        if response.status != 200:
            raise RuntimeError(f"unexpected HTTP status {response.status}: {url}")
        content_type = response.headers.get_content_type()
        if content_type not in {"text/markdown", "text/plain"}:
            raise RuntimeError(f"unexpected content type {content_type}: {url}")
        return response.read()


def collect() -> tuple[dict[str, bytes], dict[str, object]]:
    documents: dict[str, bytes] = {}
    records: list[dict[str, object]] = []
    for name, url in DOCUMENTS.items():
        body = fetch(url)
        text = body.decode("utf-8")
        for marker in REQUIRED_MARKERS[name]:
            if marker not in text:
                raise RuntimeError(f"required marker missing for {name}: {marker}")
        documents[name] = body
        records.append(
            {
                "name": name,
                "url": url,
                "path": f"docs/stage-8/finam-r2a3-read-contracts/{name}.md",
                "bytes": len(body),
                "sha256": digest(body),
            }
        )
    snapshot = {
        "schema_version": 1,
        "stage": "8B-P",
        "revision": "R2A3",
        "retrieved_at_utc": dt.datetime.now(dt.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z"),
        "official_index": f"{BASE}/",
        "production_host": "api.finam.ru",
        "documents": records,
        "real_credential_used": False,
        "authservice_request_sent": False,
        "broker_get_sent": False,
    }
    return documents, snapshot


def verify(snapshot: dict[str, object], documents: dict[str, bytes]) -> None:
    records = snapshot.get("documents")
    if not isinstance(records, list) or len(records) != len(DOCUMENTS):
        raise RuntimeError("snapshot document inventory mismatch")
    for record in records:
        if not isinstance(record, dict):
            raise RuntimeError("malformed snapshot record")
        name = record.get("name")
        if not isinstance(name, str) or name not in documents:
            raise RuntimeError("unknown snapshot document")
        body = documents[name]
        if record.get("url") != DOCUMENTS[name]:
            raise RuntimeError(f"URL mismatch: {name}")
        if record.get("bytes") != len(body) or record.get("sha256") != digest(body):
            raise RuntimeError(f"content mismatch: {name}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    if args.verify:
        snapshot = json.loads(SNAPSHOT.read_text(encoding="utf-8"))
        documents = {
            name: (OUTPUT / f"{name}.md").read_bytes() for name in DOCUMENTS
        }
        verify(snapshot, documents)
        print("stage8b-p-r2a3-contract-refresh: PASS mode=offline-verify documents=6")
        return
    documents, snapshot = collect()
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for name, body in documents.items():
        (OUTPUT / f"{name}.md").write_bytes(body)
    SNAPSHOT.write_text(
        json.dumps(snapshot, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    verify(snapshot, documents)
    print("stage8b-p-r2a3-contract-refresh: PASS mode=official-refresh documents=6")


if __name__ == "__main__":
    main()
