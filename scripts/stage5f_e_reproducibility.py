#!/usr/bin/env python3
"""Run the accepted Stage 5F matrix three times and bind deterministic evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
GOLDEN = ROOT / "docs/stage-5/stage5f-d-golden-results.json"
INVENTORY = ROOT / "docs/stage-5/stage5f-d-scenario-inventory.json"
SCENARIOS = (
    ROOT
    / "tests/fixtures/stage5/stage5f/v2/scenarios/atomic-hybrid-scenarios.json"
)
COMMAND = [
    "cargo",
    "test",
    "-q",
    "-p",
    "strategy-runtime-core",
    "stage5f_d_full_matrix_matches_frozen_golden",
    "--",
    "--test-threads=1",
]
EXPECTED_ROWS = [f"F{ordinal:02}" for ordinal in range(1, 35)]


class ReproducibilityFailure(RuntimeError):
    pass


def fail(message: str) -> None:
    raise ReproducibilityFailure(message)


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            fail(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(), object_pairs_hook=strict_object)
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read {path.relative_to(ROOT)}: {exc}")
    if not isinstance(value, dict):
        fail(f"{path.relative_to(ROOT)} must contain an object")
    return value


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode()


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def semantic_projection() -> tuple[str, str, int]:
    golden = read_json(GOLDEN)
    inventory = read_json(INVENTORY)
    results = golden.get("results")
    rows = inventory.get("rows")
    if not isinstance(results, list) or not isinstance(rows, list):
        fail("accepted results/inventory rows are missing")
    if [row.get("row_id") for row in results] != EXPECTED_ROWS:
        fail("golden result row order drift")
    if [row.get("row_id") for row in rows] != EXPECTED_ROWS:
        fail("inventory row order drift")

    fingerprints = [
        {
            "row_id": row["row_id"],
            "pre_state_fingerprint": row["pre_state_fingerprint"],
            "accepted_post_state_fingerprint": row[
                "accepted_post_state_fingerprint"
            ],
            "ordered_intent_vector_sha256": row[
                "ordered_intent_vector_sha256"
            ],
            "settlement_identity_sha256": row["settlement_identity_sha256"],
        }
        for row in results
    ]
    fingerprint_sha256 = sha256_bytes(
        b"moex.stage5f.aggregate.fingerprint-vector.v1\0"
        + canonical_bytes(fingerprints)
    )
    evidence = {
        "results": results,
        "inventory_rows": rows,
        "scenario_catalog_sha256": sha256_file(SCENARIOS),
        "source_bindings": inventory.get("source_bindings"),
        "matrix_summary": inventory.get("summary"),
    }
    evidence_sha256 = sha256_bytes(
        b"moex.stage5f.aggregate.semantic-evidence.v1\0"
        + canonical_bytes(evidence)
    )
    return fingerprint_sha256, evidence_sha256, len(results)


def run_once(index: int) -> dict[str, Any]:
    started_at = utc_now()
    completed = subprocess.run(
        COMMAND,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    finished_at = utc_now()
    if completed.returncode != 0:
        sys.stdout.buffer.write(completed.stdout)
        sys.stderr.buffer.write(completed.stderr)
        fail(f"matrix execution {index} failed with {completed.returncode}")
    fingerprint_sha256, evidence_sha256, row_count = semantic_projection()
    return {
        "run_index": index,
        "command": COMMAND,
        "started_at_utc": started_at,
        "finished_at_utc": finished_at,
        "exit_code": completed.returncode,
        "row_count": row_count,
        "fingerprint_vector_sha256": fingerprint_sha256,
        "semantic_evidence_sha256": evidence_sha256,
        "stdout_sha256": sha256_bytes(completed.stdout),
        "stderr_sha256": sha256_bytes(completed.stderr),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--output",
        type=Path,
        default=ROOT / "reports/stage5f/stage5f-fingerprint-reproducibility.json",
    )
    parser.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()
    try:
        if args.runs < 3:
            fail("at least three executions are required")
        runs = [run_once(index) for index in range(1, args.runs + 1)]
        fingerprints = {run["fingerprint_vector_sha256"] for run in runs}
        evidence_hashes = {run["semantic_evidence_sha256"] for run in runs}
        if len(fingerprints) != 1:
            fail("fingerprint-vector SHA-256 drift between executions")
        if len(evidence_hashes) != 1:
            fail("semantic-evidence SHA-256 drift between executions")
        payload = {
            "schema_version": 1,
            "stage": "5F-e-aggregate-acceptance",
            "source_ref": subprocess.check_output(
                ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
            ).strip(),
            "run_count": len(runs),
            "all_runs_passed": True,
            "all_fingerprints_identical": True,
            "all_semantic_evidence_identical": True,
            "fingerprint_vector_sha256": next(iter(fingerprints)),
            "semantic_evidence_sha256": next(iter(evidence_hashes)),
            "golden_results_sha256": sha256_file(GOLDEN),
            "scenario_inventory_sha256": sha256_file(INVENTORY),
            "scenario_catalog_sha256": sha256_file(SCENARIOS),
            "runs": runs,
        }
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(
            json.dumps(payload, indent=2, ensure_ascii=False, sort_keys=True)
            + "\n"
        )
    except (ReproducibilityFailure, OSError, subprocess.SubprocessError) as exc:
        print(f"stage5f-e-reproducibility: FAIL: {exc}", file=sys.stderr)
        return 1
    print(
        "stage5f-e-reproducibility: ok "
        f"runs={len(runs)} "
        f"fingerprint_sha256={payload['fingerprint_vector_sha256']} "
        f"evidence_sha256={payload['semantic_evidence_sha256']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
