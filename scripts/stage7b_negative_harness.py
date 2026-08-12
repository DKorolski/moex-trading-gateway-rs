#!/usr/bin/env python3
"""Pinned Stage 7B-a negative mutations for the owned journal boundary."""
from __future__ import annotations

import json
from pathlib import Path

import stage7b_check as checker

ROOT = Path(__file__).resolve().parents[1]
BACKEND = (ROOT / "crates/strategy-runtime-core/src/stage6_journal_backend.rs").read_text()
LIVE = (ROOT / "crates/strategy-runtime-core/src/stage6d_live_core.rs").read_text()
DESCRIPTOR = json.loads((ROOT / "docs/stage-7/stage7b-entry-descriptor.json").read_text())


def must_fail(name: str, backend: str = BACKEND, live: str = LIVE, descriptor: dict | None = None) -> None:
    try:
        if descriptor is None:
            checker.validate_backend(backend, live)
        else:
            checker.validate_descriptor(descriptor)
    except (checker.CheckFailure, ValueError):
        print(f"PASS {name}")
        return
    raise SystemExit(f"stage7b-negative: mutation survived: {name}")


def main() -> None:
    must_fail("owned-backend-removed", BACKEND.replace("pub enum Stage6OwnedJournalBackend", "enum RemovedOwnedBackend", 1))
    must_fail("owned-backend-clone", BACKEND.replace("#[derive(Debug)]\npub enum Stage6Owned", "#[derive(Debug, Clone)]\npub enum Stage6Owned", 1))
    must_fail("runtime-memory-owner", live=LIVE.replace("journal: Stage6OwnedJournalBackend,", "journal: Stage6MemoryJournalBackend,", 1))
    must_fail("create-new-not-exclusive", BACKEND.replace(".create_new(true)", ".create(true)", 1))
    must_fail("create-header-sync-removed", BACKEND.replace("file.sync_data()?;", "", 1))
    must_fail("create-parent-sync-removed", BACKEND.replace("sync_parent_directory(&path)?;", "", 1))
    must_fail("open-existing-creates", BACKEND.replace("let file = OpenOptions::new().read(true).write(true).open(&path)?;", "let file = OpenOptions::new().read(true).write(true).create(true).open(&path)?;", 1))
    must_fail("ambiguous-public-open", BACKEND + "\nimpl Stage6FileJournalBackend { pub fn open() {} }\n")
    changed = dict(DESCRIPTOR)
    changed["cross_process_exactly_once_claimed"] = True
    must_fail("external-exactly-once-claim", descriptor=changed)
    changed = dict(DESCRIPTOR)
    changed["negative_case_count"] = 9
    must_fail("negative-count-drift", descriptor=changed)
    print("stage7b-negative: PASS cases=10")


if __name__ == "__main__":
    main()
