#!/usr/bin/env python3
"""Detached immutable snapshot gate for Stage 5G-c R2-c-a R2."""

from __future__ import annotations

import argparse
import hashlib
import sys
from pathlib import Path

EXPECTED = {
    "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs": "fda7593117c41797d2a98e534937b53ead18451e6a3c89c5196eace0207959f3",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": "541b3dfffc838bd939790210c0a63e988a1c1d4a66f69bba52914a494b4cc3ea",
    "docs/adr/adr-stage5g-c-r2ca-r2-deterministic-terminal-fill-boundary.md": "785806c7f6a191ca31a0c43a2b0e115aceca1fcd1cebf6d316af6c8fe7a02ad0",
    "docs/current-status.md": "5e282d2fa47bfa0f353689b6c7c9fed751f874830664255a684e369d654c4280",
    "docs/stage-5/stage5g-c-r2ca-r2-deterministic-terminal-fill-boundary.json": "4b0c9481a3f668725af36b82af4f1cc06299bee5abdb0612cbfd8ac8e483db3a",
    "scripts/make_stage5g_c_r2ca_r2_handoff_archive.py": "0583364e36c8d50486a935dd95fc0b3b15ac39719ee2f2ff797d3501cccd53af",
    "scripts/stage5g_c_r2ca_r2_authority_check.py": "fa4eede37e5c4de0a0817c4f4a6c2f03010e8cbeb0950683391ff103781fe0c0",
    "scripts/stage5g_c_r2ca_r2_authority_gate.sh": "b85f57bbc16e9f39d4e74bfabc0086253fe7dd4ba855032026c3e8ff21d2b3e7",
    "scripts/stage5g_c_r2ca_r2_authority_negative_harness.py": "baabb55b73c5719993925dda2c937686040b6e58d81f99a653f3784cadc3caca",
    "scripts/stage5g_c_r2ca_r2_handoff_safety_check.py": "9f673e94cf166d5e513e9d358fd0e234d52f111fb06f90fbe9d997d38738ba16",
    "scripts/stage5g_c_r2ca_r2_predecessor_gate.py": "788b8dffc3ed167aaf72134c6cae11d3ba78ada1658716266e1711a68fa3061c",
    "scripts/stage5g_c_r2ca_r2_semantic_negative_harness.py": "aa1d099b8e0ef7c44a09aa24dd9d42eef74a9768798f163a825288c00ae1ad07",
}


def check(root: Path) -> None:
    for relative, expected in EXPECTED.items():
        path = root / relative
        if not path.is_file():
            raise ValueError(f"missing pinned R2 artifact: {relative}")
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError(f"pinned R2 artifact drift: {relative}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, OSError) as error:
        print(f"stage5g-c-r2ca-r2-snapshot-gate: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-r2ca-r2-snapshot-gate: PASS")
    print(f"pinned_artifacts: {len(EXPECTED)}/12")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
