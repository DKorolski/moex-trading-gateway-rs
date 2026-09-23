#!/usr/bin/env python3
"""Validate the immutable I1 process-supervision correction handoff."""

from __future__ import annotations

import json
import sys
import zipfile

import stage8b_p1e_i1_process_supervision_handoff_safety_check as base


PARENT = "95f733d866b5a81488bf1efaffc38eb2e2f0b2bc"
BRANCH = base.BRANCH
STAGE = base.STAGE
MARKER = base.MARKER
MANIFEST = base.MANIFEST
COMMIT_RAW = base.COMMIT_RAW
EVIDENCE = base.EVIDENCE
LOGS = base.LOGS


def check(path: str) -> dict[str, object]:
    original_parent = base.PARENT
    base.PARENT = PARENT
    try:
        return base.check(path)
    finally:
        base.PARENT = original_parent


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(
            "usage: stage8b_p1e_i1_process_supervision_correction_handoff_safety_check.py ARCHIVE"
        )
    try:
        result = check(sys.argv[1])
    except (
        OSError,
        UnicodeDecodeError,
        ValueError,
        KeyError,
        TypeError,
        zipfile.BadZipFile,
        json.JSONDecodeError,
    ) as error:
        print(f"stage8b-p1e-i1-process-supervision-correction-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print(
        "stage8b-p1e-i1-process-supervision-correction-handoff-safety: PASS "
        + json.dumps(result, sort_keys=True)
    )


if __name__ == "__main__":
    main()
