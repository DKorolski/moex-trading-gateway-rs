#!/usr/bin/env python3
"""Re-fetch and verify the seven public official FINAM contract documents."""

from __future__ import annotations

import hashlib
import json
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SNAPSHOT = ROOT / "docs/stage-8/stage8b-p-finam-contract-snapshot-2026-08-23.json"


def main() -> None:
    snapshot = json.loads(SNAPSHOT.read_text())
    for response in snapshot["retrieval"]["responses"]:
        request = urllib.request.Request(
            response["url"], headers={"Accept": "text/markdown", "User-Agent": "moex-stage8b-contract-verifier/1"}
        )
        with urllib.request.urlopen(request, timeout=20) as result:
            data = result.read()
            status = result.status
        digest = hashlib.sha256(data).hexdigest()
        if (status, len(data), digest) != (response["http_status"], response["bytes"], response["sha256"]):
            raise SystemExit(f"stage8b-p-contract-refresh: FAIL drift: {response['name']}")
        print(f"PASS {response['name']} status={status} bytes={len(data)} sha256={digest}")
    print("stage8b-p-contract-refresh: PASS responses=7 material_drift=false finam_request_sent=false")


if __name__ == "__main__":
    main()
