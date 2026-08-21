#!/usr/bin/env python3
"""Exact 20-case mutation harness for the Stage 8A-5 aggregate contract."""

from __future__ import annotations

import csv
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
AUTHORITY = Path("docs/stage-8/stage8a5-aggregate-acceptance-authority.json")
MATRIX = Path("docs/stage-8/STAGE8A5_AGGREGATE_ACCEPTANCE_MATRIX_2026-08-21.csv")
GATE = Path("scripts/stage8a5_gate.sh")


def mutate_json(path: Path, key: str, value: object) -> None:
    data = json.loads(path.read_text(encoding="utf-8"))
    data[key] = value
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def main() -> None:
    mutations = (
        ("predecessor-drift", "json", "accepted_predecessor", "0" * 40),
        ("stage7b-drift", "json", "accepted_stage7b", "0" * 40),
        ("stage8a0-drift", "json", "accepted_stage8a0", "0" * 40),
        ("stage8a1-drift", "json", "accepted_stage8a1", "0" * 40),
        ("stage8a2-drift", "json", "accepted_stage8a2", "0" * 40),
        ("stage8a3-drift", "json", "accepted_stage8a3", "0" * 40),
        ("stage8a4-drift", "json", "accepted_stage8a4_reducer", "0" * 40),
        ("durable-design-drift", "json", "accepted_stage8a4_durable_design", "0" * 40),
        ("implementation-spec-drift", "json", "accepted_stage8a4_implementation_spec", "0" * 40),
        ("implementation-lineage-drift", "json", "accepted_stage8a4_i3", "0" * 40),
        ("matrix-row-reduction", "matrix", "", ""),
        ("production-rust-authorized", "json", "production_rust_changed", True),
        ("cargo-authorized", "json", "cargo_or_lock_changed", True),
        ("workflow-authorized", "json", "workflow_changed", True),
        ("stage7b-gate-omitted", "gate", "stage7b_e_gate.sh", "stage7b_e_gate_REMOVED.sh"),
        ("stage8-inherited-omitted", "gate", "stage8a5_inherited_stage8_check.py", "inherited_REMOVED.py"),
        ("scanner-omitted", "gate", "stage8a5_forbidden_surface_check.py", "scanner_REMOVED.py"),
        ("release-test-omitted", "gate", "cargo test --workspace --release --all-targets", "cargo test RELEASE_REMOVED"),
        ("external-compile-omitted", "gate", "stage8a4_durable_composition_i4_external_compile_fail.sh", "external_REMOVED.sh"),
        ("stage8b-opened", "json", "stage8b_authorized", True),
    )
    with tempfile.TemporaryDirectory(prefix="stage8a5-negative-") as temp:
        work = Path(temp) / "repo"
        shutil.copytree(
            ROOT,
            work,
            ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports", "*.log", "__pycache__"),
        )
        originals = {
            path: (work / path).read_text(encoding="utf-8") for path in (AUTHORITY, MATRIX, GATE)
        }
        passed = 0
        for name, kind, key, value in mutations:
            for path, text in originals.items():
                (work / path).write_text(text, encoding="utf-8")
            if kind == "json":
                mutate_json(work / AUTHORITY, key, value)
            elif kind == "gate":
                path = work / GATE
                path.write_text(path.read_text(encoding="utf-8").replace(key, str(value)), encoding="utf-8")
            else:
                path = work / MATRIX
                rows = list(csv.reader(path.read_text(encoding="utf-8").splitlines()))
                path.write_text("\n".join(",".join(row) for row in rows[:-1]) + "\n", encoding="utf-8")
            result = subprocess.run(
                ["python3", "scripts/stage8a5_check.py", "--root", str(work), "--no-git"],
                cwd=work,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8a5-negative: FAIL survived={name}")
            passed += 1
            print(f"PASS {passed:02d} {name}")
    print(f"stage8a5-negative: PASS cases={passed}/20")


if __name__ == "__main__":
    main()
