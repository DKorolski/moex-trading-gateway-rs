#!/usr/bin/env python3
"""Validate the immutable committed owner-loop correction handoff."""

from __future__ import annotations

import json
import sys
import zipfile

import stage8b_p1e_i1_owner_loop_handoff_safety_check as base


# The correction source commit is the immutable parent of this packaging-only
# handoff commit.  Keeping this override in a separate checker preserves the
# original a1ec9eb handoff verifier unchanged.
PARENT = "5eeeb647cc98b5c14c6fecb1e0c0f20ed3de5487"
BRANCH = base.BRANCH
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
            "usage: stage8b_p1e_i1_owner_loop_correction_handoff_safety_check.py ARCHIVE"
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
        print(f"stage8b-p1e-i1-owner-loop-correction-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print(
        "stage8b-p1e-i1-owner-loop-correction-handoff-safety: PASS "
        + json.dumps(result, sort_keys=True)
    )


if __name__ == "__main__":
    main()
