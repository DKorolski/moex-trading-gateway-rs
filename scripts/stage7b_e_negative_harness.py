#!/usr/bin/env python3
"""Aggregate Stage 7B-e mutation harness (11 new + 40 inherited cases)."""
from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECK = Path("scripts/stage7b_e_check.py")
DESCRIPTOR = Path("docs/stage-7/stage7b-entry-descriptor.json")
FAULTS = Path("docs/stage-7/stage7b-fault-matrix.json")
PROOF_GENERATOR = Path("scripts/stage7b_proof_map.py")
LIVE = Path("crates/strategy-runtime-core/src/stage6d_live_core.rs")
JOURNAL = Path("crates/strategy-runtime-core/src/stage6_journal_backend.rs")
RECOVERY = Path("crates/runtime-durable-service/src/recovery.rs")
SUBPROCESS = Path("crates/runtime-durable-service/tests/stage7b_redis_service_subprocess.rs")
HANDOFF = Path("scripts/make_stage7b_e_handoff_archive.py")

COPY_PATHS = (
    DESCRIPTOR,
    FAULTS,
    PROOF_GENERATOR,
    Path("docs/stage-7/stage7b-acceptance-proof-map.json"),
    Path("docs/stage-7/stage7b-e-aggregate-closure.md"),
    Path("docs/stage-7/stage7b-d-c-r2-review-closure.md"),
    Path("docs/current-status.md"),
    Path("docs/roadmap.md"),
    CHECK,
    Path("scripts/stage7b_fault_matrix_check.py"),
    HANDOFF,
    LIVE,
    JOURNAL,
    RECOVERY,
    Path("crates/runtime-durable-service/src/recovery/redis_settlement.rs"),
    Path("crates/runtime-durable-service/src/recovery/redis_service.rs"),
    Path("crates/runtime-durable-service/Cargo.toml"),
    Path("crates/runtime-durable-service/tests/stage7b_writer_lock_subprocess.rs"),
    SUBPROCESS,
)


def mutate_json(path: Path, key: str, value: object) -> None:
    document = json.loads(path.read_text())
    document[key] = value
    path.write_text(json.dumps(document, indent=2) + "\n")


def replace(path: Path, old: str, new: str) -> None:
    source = path.read_text()
    if old not in source:
        raise SystemExit(f"stage7b-e-negative: fixture token absent: {path}: {old}")
    path.write_text(source.replace(old, new, 1))


def delete_fault(root: Path) -> None:
    path = root / FAULTS
    document = json.loads(path.read_text())
    document["faults"] = document["faults"][:-1]
    path.write_text(json.dumps(document, indent=2) + "\n")


CASES = (
    ("delete-b005-proof", lambda root: replace(root / PROOF_GENERATOR, '"B-005":', '"B-805":')),
    ("delete-x20-fault-row", delete_fault),
    ("fault-count-drift", lambda root: mutate_json(root / FAULTS, "fault_count", 19)),
    ("aggregate-negative-count-unpinned", lambda root: mutate_json(root / DESCRIPTOR, "negative_case_count", 49)),
    ("replace-production-file-authority-with-memory", lambda root: replace(root / LIVE, "journal: Stage6OwnedJournalBackend", "journal: Stage6MemoryJournalBackend")),
    ("introduce-second-journal-owner", lambda root: replace(root / RECOVERY, "recovered: Stage6dDurableRuntimeRecovered", "recovered: Stage6dDurableRuntimeRecovered,\n    mirrored_journal: Stage6OwnedJournalBackend")),
    ("remove-journal-parent-directory-fsync", lambda root: replace(root / JOURNAL, "sync_parent_directory(&path)?;", "/* removed directory durability barrier */")),
    ("remove-seal-parent-directory-fsync", lambda root: replace(root / RECOVERY, ".root_directory\n                .sync_all()", ".root_directory\n                .metadata()")),
    ("remove-x16-real-subprocess-witness", lambda root: replace(root / SUBPROCESS, "stage7b_e_x16_sigkill_during_claim_is_reclaimable_by_next_boot", "removed_x16_witness")),
    ("remove-source-manifest-from-preseal", lambda root: replace(root / HANDOFF, '"source-tree-manifest.json"', '"omitted-source-tree-manifest.json"')),
    ("self-accept-stage7b-before-review", lambda root: mutate_json(root / DESCRIPTOR, "stage7b_accepted", True)),
)


def main() -> None:
    descriptor = json.loads((ROOT / DESCRIPTOR).read_text())
    expected = descriptor.get("e_negative_case_count")
    if expected != len(CASES):
        raise SystemExit(
            f"stage7b-e-negative: FAIL descriptor/case-count drift descriptor={expected} actual={len(CASES)}"
        )
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage7b-e-negative-{name}-") as tmp:
            clone = Path(tmp) / "repo"
            subprocess.run(["git", "clone", "--quiet", "--no-hardlinks", str(ROOT), str(clone)], check=True)
            for relative in COPY_PATHS:
                source = ROOT / relative
                target = clone / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
            mutation(clone)
            result = subprocess.run(
                ["python3", str(clone / CHECK)],
                cwd=clone,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage7b-e-negative: FAIL mutation survived: {name}")
            print(f"PASS {name}")
    print(f"stage7b-e-negative: PASS cases={len(CASES)} inherited=40 aggregate=51")


if __name__ == "__main__":
    main()
