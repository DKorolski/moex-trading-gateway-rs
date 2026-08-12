#!/usr/bin/env python3
"""Descriptor-pinned Stage 7B-a-R1 negative mutation inventory."""
from __future__ import annotations

import json
from pathlib import Path

import stage7b_check as checker

ROOT = Path(__file__).resolve().parents[1]
BACKEND = (ROOT / "crates/strategy-runtime-core/src/stage6_journal_backend.rs").read_text()
LIVE = (ROOT / "crates/strategy-runtime-core/src/stage6d_live_core.rs").read_text()
DESCRIPTOR = json.loads(
    (ROOT / "docs/stage-7/stage7b-entry-descriptor.json").read_text()
)


def replace_block(source: str, needle: str, replacement: str) -> str:
    start = source.index(needle)
    opening = source.index("{", start)
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[:start] + replacement + source[index + 1 :]
    raise ValueError(f"unterminated source block: {needle}")


WEAK_FRONTIER_CHECK = """fn verify_external_frontier(&mut self) -> Result<(), Stage6JournalStorageError> {
        let actual = self.file.metadata()?.len();
        if actual != self.scan.frontier.journal_byte_length {
            return Err(Stage6JournalStorageError::ExternalMutationDetected);
        }
        let mut digest = [0_u8; FRAME_HASH_BYTES];
        self.file.seek(SeekFrom::Start(actual - FRAME_HASH_BYTES as u64))?;
        self.file.read_exact(&mut digest)?;
        if digest != self.scan.last_frame_digest {
            return Err(Stage6JournalStorageError::ExternalMutationDetected);
        }
        self.file.seek(SeekFrom::End(0))?;
        Ok(())
    }"""


def changed_descriptor(key: str, value: object) -> dict:
    changed = dict(DESCRIPTOR)
    changed[key] = value
    return changed


CASES = [
    {
        "name": "owned-backend-removed",
        "backend": BACKEND.replace(
            "pub enum Stage6OwnedJournalBackend", "enum RemovedOwnedBackend", 1
        ),
    },
    {
        "name": "owned-backend-clone",
        "backend": BACKEND.replace(
            "#[derive(Debug)]\npub enum Stage6Owned",
            "#[derive(Debug, Clone)]\npub enum Stage6Owned",
            1,
        ),
    },
    {
        "name": "runtime-memory-owner",
        "live": LIVE.replace(
            "journal: Stage6OwnedJournalBackend,",
            "journal: Stage6MemoryJournalBackend,",
            1,
        ),
    },
    {
        "name": "create-new-not-exclusive",
        "backend": BACKEND.replace(".create_new(true)", ".create(true)", 1),
    },
    {
        "name": "create-header-sync-removed",
        "backend": BACKEND.replace("file.sync_data()?;", "", 1),
    },
    {
        "name": "create-parent-sync-removed",
        "backend": BACKEND.replace("sync_parent_directory(&path)?;", "", 1),
    },
    {
        "name": "open-existing-creates",
        "backend": BACKEND.replace(
            "let file = OpenOptions::new().read(true).write(true).open(&path)?;",
            "let file = OpenOptions::new().read(true).write(true).create(true).open(&path)?;",
            1,
        ),
    },
    {
        "name": "ambiguous-public-open",
        "backend": BACKEND
        + "\nimpl Stage6FileJournalBackend { pub fn open() {} }\n",
    },
    {
        "name": "weak-length-and-tail-only-frontier-check",
        "backend": replace_block(
            BACKEND, "fn verify_external_frontier(", WEAK_FRONTIER_CHECK
        ),
    },
    {
        "name": "external-exactly-once-claim",
        "descriptor": changed_descriptor("cross_process_exactly_once_claimed", True),
    },
    {
        "name": "negative-count-drift",
        "descriptor": changed_descriptor("negative_case_count", 10),
    },
]


def must_fail(case: dict) -> None:
    try:
        descriptor = case.get("descriptor")
        if descriptor is None:
            checker.validate_backend(
                case.get("backend", BACKEND), case.get("live", LIVE)
            )
        else:
            checker.validate_descriptor(descriptor)
    except (checker.CheckFailure, ValueError):
        print(f"PASS {case['name']}")
        return
    raise SystemExit(f"stage7b-negative: mutation survived: {case['name']}")


def main() -> None:
    expected = DESCRIPTOR["negative_case_count"]
    actual = len(CASES)
    if actual != expected:
        raise SystemExit(
            f"stage7b-negative: inventory count mismatch expected={expected} actual={actual}"
        )
    for case in CASES:
        must_fail(case)
    print(f"stage7b-negative: PASS cases={actual}")


if __name__ == "__main__":
    main()
