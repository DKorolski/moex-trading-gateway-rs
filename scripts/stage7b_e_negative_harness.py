#!/usr/bin/env python3
"""Aggregate Stage 7B-e mutation harness (19 new + 40 inherited cases)."""
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
NORMATIVE = Path("docs/stage-7/stage7b-fault-matrix-normative.json")
TZ = Path("docs/stage-7/TZ_STAGE7B_PRODUCTION_DURABILITY_COMPOSITION_2026-08-12.md")
PROOF_GENERATOR = Path("scripts/stage7b_proof_map.py")
LIVE = Path("crates/strategy-runtime-core/src/stage6d_live_core.rs")
SERVICE_ROOT = Path("crates/runtime-durable-service/src/lib.rs")
JOURNAL = Path("crates/strategy-runtime-core/src/stage6_journal_backend.rs")
RECOVERY = Path("crates/runtime-durable-service/src/recovery.rs")
SUBPROCESS = Path("crates/runtime-durable-service/tests/stage7b_redis_service_subprocess.rs")
HANDOFF = Path("scripts/make_stage7b_e_handoff_archive.py")
GATE = Path("scripts/stage7b_e_gate.sh")

COPY_PATHS = (
    DESCRIPTOR,
    FAULTS,
    NORMATIVE,
    TZ,
    PROOF_GENERATOR,
    Path("docs/stage-7/stage7b-acceptance-proof-map.json"),
    Path("docs/stage-7/stage7b-e-aggregate-closure.md"),
    Path("docs/stage-7/stage7b-d-c-r2-review-closure.md"),
    Path("docs/current-status.md"),
    Path("docs/roadmap.md"),
    CHECK,
    GATE,
    Path("scripts/stage7b_fault_matrix_check.py"),
    HANDOFF,
    SERVICE_ROOT,
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


def append(path: Path, suffix: str) -> None:
    path.write_text(path.read_text() + suffix)


def delete_fault(root: Path) -> None:
    path = root / NORMATIVE
    document = json.loads(path.read_text())
    document["faults"] = document["faults"][:-1]
    path.write_text(json.dumps(document, indent=2) + "\n")


def mutate_fault(root: Path, fault_id: str, key: str, value: object) -> None:
    path = root / NORMATIVE
    document = json.loads(path.read_text())
    row = next(row for row in document["faults"] if row["id"] == fault_id)
    row[key] = value
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
    ("remove-inherited-stage7a-gate", lambda root: replace(root / GATE, "inherited-stage7a-gate.txt", "omitted-stage7a-gate.txt")),
    ("weaken-x12-required-result", lambda root: mutate_fault(root, "X12", "required_result", "restart may settle an ACK")),
    ("map-x02-to-old-torn-frame-test", lambda root: mutate_fault(root, "X02", "witnesses", ["stage6b_torn_write_failpoints_leave_reopen_fail_closed"])),
    ("map-x12-to-redis-free-d-a-test", lambda root: mutate_fault(root, "X12", "witnesses", ["stage7b_d_a_b051_sigkill_after_seal_reconstructs_without_provider"])),
    ("remove-x19-restart-witness", lambda root: replace(root / JOURNAL, "stage7b_e_x19_sync_failure_reopen_validates_actual_disk_state_conservatively", "removed_x19_restart_witness")),
    ("mutate-normative-x02-boundary", lambda root: mutate_fault(root, "X02", "boundary", "Journal create: adjacent torn-frame boundary.")),
    ("inject-hidden-stage8-adapter", lambda root: replace(root / SERVICE_ROOT, "pub struct Stage7bWritableDurableAuthority {", "pub struct Stage8ProtectedExecutionAdapter;\n\npub struct Stage7bWritableDurableAuthority {")),
    ("inject-hidden-stage8-after-test-module", lambda root: append(root / SERVICE_ROOT, "\n\npub struct Stage8ProtectedExecutionAdapter;\n")),
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
    print(f"stage7b-e-negative: PASS cases={len(CASES)} inherited=40 aggregate=59")


if __name__ == "__main__":
    main()
