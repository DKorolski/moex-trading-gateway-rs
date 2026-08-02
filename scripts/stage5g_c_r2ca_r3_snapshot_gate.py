#!/usr/bin/env python3
"""Detached immutable snapshot gate for Stage 5G-c R2-c-a R3."""

from __future__ import annotations

import argparse
import hashlib
import sys
from pathlib import Path

EXPECTED = {
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": "ca357ea9e2dd39910d119e1033e00eef7698cf459255a95825591cd1c86984e7",
    "docs/adr/adr-stage5g-c-r2ca-r3-exact-receipt-clock-bracket-authority.md": "e00285622c08ece4cd8b3db1a4e66ee5a8e57fd4d868d60de34c04fa74dcd699",
    "docs/current-status.md": "f2fea3af51b0bea643a9e0b62ac3c79d4483bcf5248216bb2e036fd7f6be8b6d",
    "docs/stage-5/stage5g-c-r2ca-r3-exact-receipt-clock-bracket-authority.json": "2cd3506f6c30811fee5ed7fe5831134534fac027a869388f39026ed2ccb64914",
    "scripts/make_stage5g_c_r2ca_r3_handoff_archive.py": "fcfe47b3597a647166b24f1aa35a5ce225d6bcb9e1d85433dc8ef309fb61cfdf",
    "scripts/stage5g_c_r2ca_r3_authority_check.py": "5f66d1adf44dd65b51bd14b57a640366ea31d901130b9a6e8ad7caafdcaa5f22",
    "scripts/stage5g_c_r2ca_r3_authority_gate.sh": "23bc14d80b1fe38aee870dcf16666c92cfedf4387da916310f9f39e1f49e5537",
    "scripts/stage5g_c_r2ca_r3_authority_negative_harness.py": "fb078818f1533ae226ae0c040589227ad2386a1d683c68af4beb9aab14be0ede",
    "scripts/stage5g_c_r2ca_r3_handoff_safety_check.py": "c2ea5667b8ff80399a1d353a34fadeae38deb059fc00ab23bcbd165911016733",
    "scripts/stage5g_c_r2ca_r3_predecessor_gate.py": "c82787ebed23112f194716edf71c60f190c21060257e2c8fcb9da71e59b91eed",
    "scripts/stage5g_c_r2ca_r3_semantic_negative_harness.py": "2fe853aa30106788467c9dfa4c95b51afa9421c19c467d430e6326c0f3b0d4cb",
}


def check(root: Path) -> None:
    for relative, expected in EXPECTED.items():
        path = root / relative
        if not path.is_file():
            raise ValueError(f"missing pinned R3 artifact: {relative}")
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError(f"pinned R3 artifact drift: {relative}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, OSError) as error:
        print(f"stage5g-c-r2ca-r3-snapshot-gate: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-r2ca-r3-snapshot-gate: PASS")
    print(f"pinned_artifacts: {len(EXPECTED)}/11")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
