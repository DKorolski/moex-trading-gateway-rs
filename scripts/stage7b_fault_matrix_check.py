#!/usr/bin/env python3
"""Validate the immutable Stage 7B X01-X20 matrix and exact witnesses."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINDING = ROOT / "docs/stage-7/stage7b-fault-matrix.json"
NORMATIVE = ROOT / "docs/stage-7/stage7b-fault-matrix-normative.json"
TZ = ROOT / "docs/stage-7/TZ_STAGE7B_PRODUCTION_DURABILITY_COMPOSITION_2026-08-12.md"
ACCEPTED_D_C = "2b6371adb905654e0ddd8b6714159bcef737b577"
NORMATIVE_SHA256 = "d4f5dc4ee8a65ee007a2fe01075927dd6136ec1df8557c8dc37e8105dd0936c9"
TZ_SHA256 = "200e42acef2bb30cf24e3d2a5bc38df99ed853d70d6310653f315e76d1f4c1e0"
SOURCE_FILES = (
    "crates/runtime-durable-service/src/lib.rs",
    "crates/runtime-durable-service/src/recovery.rs",
    "crates/runtime-durable-service/src/recovery/redis_settlement.rs",
    "crates/runtime-durable-service/tests/stage7b_writer_lock_subprocess.rs",
    "crates/runtime-durable-service/tests/stage7b_redis_service_subprocess.rs",
    "crates/strategy-runtime-core/src/stage6_journal_backend.rs",
    "scripts/stage7b_e_check.py",
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fail(message: str) -> None:
    raise SystemExit(f"stage7b-fault-matrix: FAIL: {message}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact-dir", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    binding = json.loads(BINDING.read_text())
    normative = json.loads(NORMATIVE.read_text())
    if sha256(NORMATIVE) != NORMATIVE_SHA256:
        fail("normative matrix content/hash drift")
    if sha256(TZ) != TZ_SHA256:
        fail("normative Stage 7B TZ content/hash drift")
    expected_binding = {
        "schema_version": 2,
        "stage": "7B-e",
        "candidate_revision": "r2",
        "accepted_stage7b_d_c_ref": ACCEPTED_D_C,
        "normative_matrix_path": str(NORMATIVE.relative_to(ROOT)),
        "normative_matrix_sha256": NORMATIVE_SHA256,
        "normative_tz_path": str(TZ.relative_to(ROOT)),
        "normative_tz_sha256": TZ_SHA256,
        "fault_count": 20,
        "cross_process_exactly_once_claimed": False,
    }
    if binding != expected_binding:
        fail("candidate binding differs from the exact R2 contract")
    if normative.get("stage") != "7B-e":
        fail("normative stage drift")
    if normative.get("source_tz_sha256") != TZ_SHA256:
        fail("normative matrix no longer binds the accepted TZ")

    faults = normative.get("faults", [])
    if normative.get("fault_count") != 20 or len(faults) != 20:
        fail("fault count must be exactly 20")
    expected_ids = [f"X{index:02d}" for index in range(1, 21)]
    if [row.get("id") for row in faults] != expected_ids:
        fail("fault IDs/order drift")

    source = "\n".join((ROOT / path).read_text() for path in SOURCE_FILES)
    all_witnesses: list[str] = []
    for row in faults:
        witnesses = row.get("witnesses")
        if not isinstance(witnesses, list) or not witnesses:
            fail(f"{row['id']} has no exact witness")
        if not isinstance(row.get("boundary"), str) or not row["boundary"]:
            fail(f"{row['id']} boundary absent")
        if not isinstance(row.get("required_result"), str) or not row["required_result"]:
            fail(f"{row['id']} required result absent")
        if not isinstance(row.get("proof_type"), str) or not row["proof_type"]:
            fail(f"{row['id']} proof type absent")
        is_static_exception = row["id"] in {"X03", "X11"}
        if bool(row.get("power_loss_static_exception")) != is_static_exception:
            fail(f"{row['id']} static power-loss exception drift")
        if is_static_exception and not row.get("exception_rationale"):
            fail(f"{row['id']} static exception rationale absent")
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
        "schema_version": 2,
        "stage": "7B-e",
        "candidate_revision": "r2",
        "accepted_stage7b_d_c_ref": ACCEPTED_D_C,
        "normative_matrix_sha256": NORMATIVE_SHA256,
        "normative_tz_sha256": TZ_SHA256,
        "fault_count": 20,
        "passed_count": 20,
        "all_faults_passed": True,
        "debug_release_evidence_bound": evidence_bound,
        "faults": [
            {
                "id": row["id"],
                "boundary": row["boundary"],
                "required_result": row["required_result"],
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
        "stage7b-fault-matrix: PASS faults=20/20 normative=true "
        f"debug_release_bound={str(evidence_bound).lower()}"
    )


if __name__ == "__main__":
    main()
