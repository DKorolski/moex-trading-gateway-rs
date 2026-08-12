#!/usr/bin/env python3
"""Stage 7B-a lineage, governance and journal-foundation checker."""
from __future__ import annotations

import csv
import hashlib
import json
import subprocess
from pathlib import Path

BASE = "2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64"
BRANCH = "stage7b-production-durability"
TZ_SHA256 = "200e42acef2bb30cf24e3d2a5bc38df99ed853d70d6310653f315e76d1f4c1e0"
MATRIX_SHA256 = "083cc6e1e0925f11efa4bc093fd7c2d3d4cbeb05fd275f68ed71be3bdac1931d"


class CheckFailure(ValueError):
    pass


def require(value: bool, message: str) -> None:
    if not value:
        raise CheckFailure(message)


def block(source: str, needle: str) -> str:
    start = source.index(needle)
    opening = source.index("{", start)
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[opening : index + 1]
    raise CheckFailure(f"unterminated block: {needle}")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_backend(source: str, live_core: str) -> None:
    require(
        "#[derive(Debug)]\npub enum Stage6OwnedJournalBackend" in source,
        "owned backend must remain non-Clone/non-Serialize",
    )
    owner = block(source, "impl Stage6JournalBackend for Stage6OwnedJournalBackend")
    for method in ("fn append(", "fn records(", "fn frontier(", "fn framed_bytes(", "fn validate_checkpoint("):
        require(method in owner, f"owned backend delegation missing: {method}")
    require("Stage6FileJournalBackend::open(" not in source, "ambiguous public open-or-create restored")
    require("pub fn open(" not in source, "ambiguous public open-or-create API restored")

    create = block(source, "pub fn create_new(")
    ordered = (".create_new(true)", "file.write_all", "file.sync_data()", "sync_parent_directory")
    positions = [create.index(token) for token in ordered]
    require(positions == sorted(positions), "create-new durability ordering drift")
    existing = block(source, "pub fn open_existing(")
    for forbidden in (".create(", ".create_new(", "write_all", "sync_parent_directory"):
        require(forbidden not in existing, f"open-existing mutates/creates storage: {forbidden}")
    require("OpenOptions::new().read(true).write(true).open" in existing, "existing file is not explicitly opened")
    require("#[cfg(test)]\n    fn open_for_test(" in source, "legacy ambiguous helper escaped test-only boundary")
    require("File::open(parent)?.sync_all()?" in source, "parent directory fsync absent")

    verify = block(source, "fn verify_external_frontier(")
    for token in (
        "scan_reader(&mut self.file, actual)",
        "observed.records != self.scan.records",
        "observed.frontiers != self.scan.frontiers",
        "observed.frontier != self.scan.frontier",
        "observed.last_frame_digest != self.scan.last_frame_digest",
    ):
        require(token in verify, f"full pre-append external-state verification missing: {token}")
    file_backend = block(source, "impl Stage6JournalBackend for Stage6FileJournalBackend")
    append_order = (
        "self.verify_external_frontier()?",
        "validate_record_for_storage(record)?",
        "self.file.write_all(&frame.prefix)?",
        "self.file.sync_data()",
    )
    append_positions = [file_backend.index(token) for token in append_order]
    require(
        append_positions == sorted(append_positions),
        "full external-state verification must precede every append write",
    )

    require(
        "journal: Stage6OwnedJournalBackend," in live_core,
        "recovered runtime does not own the backend enum",
    )
    require(
        "journal: Stage6MemoryJournalBackend," not in live_core.split("#[cfg(test)]", 1)[0],
        "production recovered runtime still owns memory backend directly",
    )
    for token in (
        "first_boot_stage6d_paper_with_owned_journal",
        "restart_stage6d_paper_with_owned_journal",
        "pub fn journal_is_file_backed",
    ):
        require(token in live_core, f"owned runtime composition API absent: {token}")


def validate_descriptor(descriptor: dict) -> None:
    expected = {
        "stage": "7B",
        "slice": "7B-a-R1",
        "accepted_predecessor": BASE,
        "blocking_acceptance_rows": 80,
        "semantic_proof_map_count": 80,
        "cross_process_fault_count": 20,
        "negative_case_count": 11,
        "single_writer_required": True,
        "single_writer_implemented": False,
        "recovery_seal_required": True,
        "recovery_seal_implemented": False,
        "inherited_stage7a_gate_required": True,
        "cross_process_exactly_once_claimed": False,
        "finam_post_delete": False,
        "broker_network_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
        "normative_matrix_repository_eol": "LF",
        "normative_matrix_source_crlf_sha256": "a665d8638f4dfdfea6e13b680c8e5dce23f76811bf208c22f809668a8cd24b5c",
    }
    for key, value in expected.items():
        require(descriptor.get(key) == value, f"descriptor drift: {key}")
    require(descriptor.get("normative_tz_sha256") == TZ_SHA256, "descriptor TZ hash drift")
    require(descriptor.get("normative_matrix_sha256") == MATRIX_SHA256, "descriptor matrix hash drift")


def check_lineage(root: Path) -> None:
    merge_base = subprocess.check_output(
        ["git", "merge-base", "HEAD", BASE], cwd=root, text=True
    ).strip()
    require(merge_base == BASE, "candidate is not based on accepted Stage 7A closure")


def check_governance(root: Path) -> None:
    status = (root / "docs/current-status.md").read_text(encoding="utf-8")
    roadmap = (root / "docs/roadmap.md").read_text(encoding="utf-8")
    status_words = " ".join(status.split())
    roadmap_words = " ".join(roadmap.split())
    for token in ("Stage 7A is CLOSED", BASE, "Stage 7B-a-R1 is the only active implementation candidate"):
        require(token in status_words, f"current status token absent: {token}")
    require("Stage 7B-a-R1 — journal foundation repair" in roadmap, "roadmap active slice drift")
    require("Stage 8+ remain closed" in roadmap_words, "Stage 8 closed boundary absent")


def check(root: Path) -> None:
    check_lineage(root)

    tz = root / "docs/stage-7/TZ_STAGE7B_PRODUCTION_DURABILITY_COMPOSITION_2026-08-12.md"
    matrix = root / "docs/stage-7/STAGE7B_ACCEPTANCE_MATRIX_2026-08-12.csv"
    require(sha256(tz) == TZ_SHA256, "normative Stage 7B TZ drift")
    require(sha256(matrix) == MATRIX_SHA256, "normative Stage 7B matrix drift")
    with matrix.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 80, "Stage 7B matrix must contain exactly 80 rows")
    require([r["ID"] for r in rows] == [f"B-{i:03d}" for i in range(1, 81)], "matrix IDs drift")

    check_governance(root)

    validate_backend(
        (root / "crates/strategy-runtime-core/src/stage6_journal_backend.rs").read_text(),
        (root / "crates/strategy-runtime-core/src/stage6d_live_core.rs").read_text(),
    )
    descriptor = json.loads((root / "docs/stage-7/stage7b-entry-descriptor.json").read_text())
    validate_descriptor(descriptor)
    subprocess.run(["python3", "scripts/stage7b_proof_map.py"], cwd=root, check=True)
    print("stage7b-check: PASS rows=80 slice=7B-a-R1 stage7b_accepted=false")


if __name__ == "__main__":
    try:
        check(Path.cwd().resolve())
    except (CheckFailure, ValueError, KeyError) as error:
        raise SystemExit(f"stage7b-check: FAIL: {error}") from error
