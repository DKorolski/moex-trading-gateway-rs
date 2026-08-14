#!/usr/bin/env python3
"""Validate the frozen X01-X20 map and optionally bind it to gate logs."""
from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs/stage-7/stage7b-fault-matrix.json"
ACCEPTED_D_C = "2b6371adb905654e0ddd8b6714159bcef737b577"
SOURCE_FILES = (
    "crates/runtime-durable-service/src/lib.rs",
    "crates/runtime-durable-service/src/recovery.rs",
    "crates/runtime-durable-service/src/recovery/redis_settlement.rs",
    "crates/runtime-durable-service/tests/stage7b_writer_lock_subprocess.rs",
    "crates/runtime-durable-service/tests/stage7b_redis_service_subprocess.rs",
    "crates/strategy-runtime-core/src/stage6_journal_backend.rs",
    "scripts/stage7b_e_check.py",
)


def fail(message: str) -> None:
    raise SystemExit(f"stage7b-fault-matrix: FAIL: {message}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact-dir", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    document = json.loads(MATRIX.read_text())
    faults = document.get("faults", [])
    if document.get("stage") != "7B-e":
        fail("stage drift")
    if document.get("accepted_stage7b_d_c_ref") != ACCEPTED_D_C:
        fail("accepted d-c ref drift")
    if document.get("fault_count") != 20 or len(faults) != 20:
        fail("fault count must be exactly 20")
    if document.get("cross_process_exactly_once_claimed") is not False:
        fail("external exactly-once overclaim")
    expected = [f"X{index:02d}" for index in range(1, 21)]
    if [row.get("id") for row in faults] != expected:
        fail("fault IDs/order drift")
    source = "\n".join((ROOT / path).read_text() for path in SOURCE_FILES)
    all_witnesses: list[str] = []
    for row in faults:
        witnesses = row.get("witnesses")
        if not isinstance(witnesses, list) or not witnesses:
            fail(f"{row['id']} has no exact witness")
        if not row.get("proof_type") or not row.get("required_result"):
            fail(f"{row['id']} incomplete semantics")
        for witness in witnesses:
            token = witness.split("::")[-1]
            if token not in source:
                fail(f"{row['id']} witness absent from source: {witness}")
            all_witnesses.append(witness)
    if len(set(all_witnesses)) != len(all_witnesses):
        fail("one witness is reused as a substitute for multiple fault rows")

    evidence_bound = args.artifact_dir is not None
    if args.artifact_dir is not None:
        debug = (
            (args.artifact_dir / "runtime-debug.txt").read_text(errors="replace")
            + (args.artifact_dir / "core-debug.txt").read_text(errors="replace")
        )
        release = (
            (args.artifact_dir / "runtime-release.txt").read_text(errors="replace")
            + (args.artifact_dir / "core-release.txt").read_text(errors="replace")
        )
        for row in faults:
            for witness in row["witnesses"]:
                if "::" in witness:
                    continue
                marker = f"{witness} ... ok"
                if marker not in debug or marker not in release:
                    fail(f"{row['id']} debug/release evidence absent: {witness}")

    report = {
        "schema_version": 1,
        "stage": "7B-e",
        "accepted_stage7b_d_c_ref": ACCEPTED_D_C,
        "fault_count": 20,
        "passed_count": 20,
        "all_faults_passed": True,
        "debug_release_evidence_bound": evidence_bound,
        "faults": [
            {
                "id": row["id"],
                "proof_type": row["proof_type"],
                "witnesses": row["witnesses"],
                "status": "PASS",
            }
            for row in faults
        ],
    }
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(
        "stage7b-fault-matrix: PASS faults=20/20 "
        f"debug_release_bound={str(evidence_bound).lower()}"
    )


if __name__ == "__main__":
    main()
