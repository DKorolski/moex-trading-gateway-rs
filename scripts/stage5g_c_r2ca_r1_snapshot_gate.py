#!/usr/bin/env python3
"""Detached immutable snapshot gate for Stage 5G-c R2-c-a R1."""

from __future__ import annotations

import argparse
import hashlib
import sys
from pathlib import Path

EXPECTED = {
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": "4670090bb6046d9c70310ef07dfee2eafaa87f7873627db9de240ee5ab568d40",
    "docs/adr/adr-stage5g-c-r2ca-market-terminal-no-callback-authority.md": "a29d07d1959ae1e60f7fd67ca1accce8e44ea4b811bdd72208284b7c9ee27d5a",
    "docs/adr/adr-stage5g-c-r2ca-r1-market-terminal-state-coherence.md": "a9d2be37d4e3f781b758733f5fec0d29298f8202f14e7b1463683e8eab614a8b",
    "docs/stage-5/stage5g-c-r2ca-r1-market-terminal-state-coherence.json": "2f12415fec3829c4e6fcb3fd2cd1bd472014ea2d42c5a9bf26a52b454cb8063d",
    "docs/stage-5/stage5g-c-r2ca-market-terminal-authority.json": "2c3543a64bcba016d84a3f0dffbc7c90c81b1555fab4585463ec6b03c228d243",
    "docs/current-status.md": "b1642d082501410814a108e40e9ecbbd112ba23194c10f7f9fe4d2c756b75134",
    "scripts/stage5g_c_r2ca_r1_authority_check.py": "8ae7c43fb4cec9e073bd11cf62753e2673a0b45afc197ed6492d4acdfe336471",
    "scripts/stage5g_c_r2ca_r1_authority_negative_harness.py": "edc46560ddeb58909977c299ef743f4261aa749c29adb69a2e1aab2bd0fb51e3",
    "scripts/stage5g_c_r2ca_r1_authority_gate.sh": "2fb71c20ea661ca07feedcbe9d9196365b1ca12d17445c6d88b89cfa618a5c09",
    "scripts/stage5g_c_r2ca_r1_semantic_negative_harness.py": "b0afaac5d23f4d6f62e167c8a3c59ffc4de1e443996c15fc130bf5a4c2464800",
    "scripts/stage5g_c_r2ca_r1_handoff_safety_check.py": "9865301b02c5f4ed60d5353482bb7e932e35e0173ec1ce8aecae39963a3e938c",
    "scripts/make_stage5g_c_r2ca_r1_handoff_archive.py": "63b844e67c9243e856d675613f05004c33e635bf892623d5b36412f7e46d99f5",
}


def check(root: Path) -> None:
    for relative, expected in EXPECTED.items():
        path = root / relative
        if not path.is_file():
            raise ValueError(f"missing pinned R1 artifact: {relative}")
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError(f"pinned R1 artifact drift: {relative}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except ValueError as error:
        print(f"stage5g-c-r2ca-r1-snapshot-gate: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-r2ca-r1-snapshot-gate: PASS")
    print(f"pinned_artifacts: {len(EXPECTED)}/12")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
