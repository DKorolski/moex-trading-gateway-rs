#!/usr/bin/env python3
"""Immutable snapshot gate for independently reviewed R2-c-a bytes."""

from __future__ import annotations

import argparse
import hashlib
import sys
from pathlib import Path

EXPECTED = {
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": "2315b70ba14432da56b777057506e69e425295a9c1b221e08438cc9e16af3d77",
    "docs/adr/adr-stage5g-c-r2ca-market-terminal-no-callback-authority.md": "f3484e3de592dfbf0250ed8148d7efb507785e98ccd95a5a5beb7f1a1fde8d3b",
    "docs/stage-5/stage5g-c-r2ca-market-terminal-authority.json": "832f0f7ac20a92bb3aa892f8ef964da6affe6317dae071164cf9de072daf74b9",
    "scripts/stage5g_c_r2ca_authority_check.py": "7475fa9aaf240a86441118b0f3e304c63237be1b7cee2dcb4982ba4adc41206b",
}


def check(root: Path) -> None:
    for relative, expected in EXPECTED.items():
        path = root / relative
        if not path.is_file():
            raise ValueError(f"missing pinned artifact: {relative}")
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError(f"pinned artifact drift: {relative}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except ValueError as error:
        print(f"stage5g-c-r2ca-snapshot-gate: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-r2ca-snapshot-gate: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
