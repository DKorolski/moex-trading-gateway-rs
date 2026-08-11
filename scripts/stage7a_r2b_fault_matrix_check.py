#!/usr/bin/env python3
"""Validate the complete Stage 7A-R2b F1-F15 semantic fault evidence."""
from __future__ import annotations

import argparse
import json
from pathlib import Path

MATRIX = Path("docs/stage-7/stage7a-r2b-fault-matrix.json")
REQUIRED_FIELDS = {
    "id", "point", "artifact", "witness", "redis_pending", "stage6_reentry",
    "paper_effect_repeat", "ack_repeat", "xack", "readiness",
}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    source = json.loads(MATRIX.read_text())
    faults = source.get("faults", [])
    expected_ids = [f"F{index}" for index in range(1, 16)]
    if [fault.get("id") for fault in faults] != expected_ids:
        raise SystemExit("stage7a-r2b-fault-matrix: FAIL: F1-F15 identity drift")
    evaluated = []
    for fault in faults:
        missing = sorted(REQUIRED_FIELDS - set(fault))
        artifact = args.artifact_dir / fault["artifact"]
        text = artifact.read_text(errors="replace") if artifact.is_file() else ""
        matched = fault["witness"] in text
        supplemental = fault.get("supplemental_witnesses", [])
        supplemental_matched = [token for token in supplemental if token in text]
        passed = not missing and matched and all(
            isinstance(fault[field], str) and fault[field]
            for field in REQUIRED_FIELDS - {"id"}
        ) and len(supplemental_matched) == len(supplemental)
        evaluated.append({
            "id": fault["id"],
            "point": fault["point"],
            "status": "PASS" if passed else "FAIL",
            "artifact": fault["artifact"],
            "exact_witness": fault["witness"],
            "witness_matched": matched,
            "supplemental_witnesses": supplemental,
            "supplemental_witnesses_matched": supplemental_matched,
            "missing_fields": missing,
            "semantics": {key: fault[key] for key in (
                "redis_pending", "stage6_reentry", "paper_effect_repeat",
                "ack_repeat", "xack", "readiness",
            )},
        })
    report = {
        "schema_version": 1,
        "stage": "7A-R2b",
        "fault_count": len(evaluated),
        "pass_count": sum(row["status"] == "PASS" for row in evaluated),
        "all_faults_passed": all(row["status"] == "PASS" for row in evaluated),
        "faults": evaluated,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    if not report["all_faults_passed"]:
        failed = [row["id"] for row in evaluated if row["status"] != "PASS"]
        raise SystemExit(f"stage7a-r2b-fault-matrix: FAIL faults={failed}")
    print("stage7a-r2b-fault-matrix: PASS faults=15 F1-F15=complete")


if __name__ == "__main__":
    main()
