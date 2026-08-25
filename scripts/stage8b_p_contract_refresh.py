#!/usr/bin/env python3
"""Re-fetch and verify the seven public official FINAM contract documents."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SNAPSHOT = "docs/stage-8/stage8b-p-finam-contract-snapshot-2026-08-23.json"
R1_SNAPSHOT = "docs/stage-8/stage8b-p-finam-contract-snapshot-2026-08-24.json"


def fetch_document(url: str) -> tuple[int, bytes]:
    result = subprocess.run(
        [
            "curl",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--http1.1",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "--retry",
            "2",
            "--retry-delay",
            "1",
            "--retry-all-errors",
            "--header",
            "Accept: text/markdown",
            "--user-agent",
            "moex-stage8b-contract-verifier/1",
            "--write-out",
            "\n%{http_code}",
            url,
        ],
        check=False,
        capture_output=True,
    )
    if result.returncode != 0 or b"\n" not in result.stdout:
        raise RuntimeError(
            f"public documentation fetch failed: curl={result.returncode} "
            f"stderr={result.stderr.decode(errors='replace')[:200]}"
        )
    data, raw_status = result.stdout.rsplit(b"\n", 1)
    try:
        status = int(raw_status)
    except ValueError as error:
        raise RuntimeError("public documentation HTTP status is malformed") from error
    return status, data


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--snapshot",
        choices=(DEFAULT_SNAPSHOT, R1_SNAPSHOT),
        default=DEFAULT_SNAPSHOT,
    )
    args = parser.parse_args()
    snapshot = json.loads((ROOT / args.snapshot).read_text())
    for response in snapshot["retrieval"]["responses"]:
        status, data = fetch_document(response["url"])
        digest = hashlib.sha256(data).hexdigest()
        if (status, len(data), digest) != (response["http_status"], response["bytes"], response["sha256"]):
            raise SystemExit(f"stage8b-p-contract-refresh: FAIL drift: {response['name']}")
        print(f"PASS {response['name']} status={status} bytes={len(data)} sha256={digest}")
    print("stage8b-p-contract-refresh: PASS responses=7 material_drift=false finam_request_sent=false")


if __name__ == "__main__":
    main()
