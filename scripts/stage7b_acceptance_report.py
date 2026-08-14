#!/usr/bin/env python3
"""Produce the machine-readable Stage 7B aggregate acceptance candidate."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ACCEPTED_D_C = "2b6371adb905654e0ddd8b6714159bcef737b577"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fail(message: str) -> None:
    raise SystemExit(f"stage7b-acceptance-report: FAIL: {message}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact-dir", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    proof_path = ROOT / "docs/stage-7/stage7b-acceptance-proof-map.json"
    matrix_path = ROOT / "docs/stage-7/stage7b-fault-matrix.json"
    descriptor_path = ROOT / "docs/stage-7/stage7b-entry-descriptor.json"
    proof = json.loads(proof_path.read_text())
    matrix = json.loads(matrix_path.read_text())
    descriptor = json.loads(descriptor_path.read_text())
    if proof.get("implemented_count") != 80 or proof.get("pending_count") != 0:
        fail("proof map is not 80/80")
    if matrix.get("fault_count") != 20 or len(matrix.get("faults", [])) != 20:
        fail("fault matrix is not 20/20")
    if descriptor.get("accepted_stage7b_d_c_ref") != ACCEPTED_D_C:
        fail("accepted d-c ref drift")
    if descriptor.get("stage7b_accepted") is not False:
        fail("candidate cannot self-accept Stage 7B")

    artifacts: dict[str, str] = {}
    if args.artifact_dir:
        required = {
            "fmt.txt": "fmt: PASS",
            "stage7b-e-check.txt": "stage7b-e-check: PASS rows=80/80 faults=20/20 accepted=false",
            "stage7b-e-negative.txt": "stage7b-e-negative: PASS cases=11 inherited=40 aggregate=51",
            "inherited-d-c-gate.txt": "stage7b-d-c-gate: PASS",
            "fault-matrix.txt": "stage7b-fault-matrix: PASS faults=20/20 debug_release_bound=true",
            "runtime-debug.txt": "test result: ok",
            "runtime-release.txt": "test result: ok",
            "core-debug.txt": "test result: ok",
            "core-release.txt": "test result: ok",
            "workspace-tests.txt": "test result: ok",
            "workspace-docs.txt": "test result: ok",
            "clippy.txt": "Finished `dev` profile",
        }
        for name, marker in required.items():
            path = args.artifact_dir / name
            if not path.is_file() or marker not in path.read_text(errors="replace"):
                fail(f"artifact missing marker: {name}: {marker}")
            artifacts[name] = sha256(path)

    report = {
        "schema_version": 1,
        "stage": "7B-e",
        "accepted_stage7b_d_c_ref": ACCEPTED_D_C,
        "proof_rows": 80,
        "proof_rows_implemented": 80,
        "proof_rows_pending": 0,
        "fault_rows": 20,
        "fault_rows_passed": 20,
        "aggregate_negative_cases": 51,
        "all_required_candidate_gates_passed": bool(args.artifact_dir),
        "stage7b_accepted": False,
        "verdict": "INDEPENDENT_ACCEPTANCE_PENDING",
        "next_authorized_after_acceptance": "Gate 7→8 specification only",
        "finam_post_delete_enabled": False,
        "broker_network_dispatch_enabled": False,
        "runtime_live_enabled": False,
        "real_orders_enabled": False,
        "proof_map_sha256": sha256(proof_path),
        "fault_matrix_sha256": sha256(matrix_path),
        "descriptor_sha256": sha256(descriptor_path),
        "artifact_sha256": artifacts,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print("stage7b-acceptance-report: PASS rows=80/80 faults=20/20 accepted=false")


if __name__ == "__main__":
    main()
