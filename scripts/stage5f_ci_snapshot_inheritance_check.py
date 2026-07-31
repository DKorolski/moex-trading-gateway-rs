#!/usr/bin/env python3
"""Fail-closed CI contract for Stage 5F's immutable B3F inheritance."""

from __future__ import annotations

import hashlib
import argparse
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CI = ROOT / ".github/workflows/ci.yml"
WRAPPER = ROOT / "scripts/stage5f_b3f_snapshot_provenance_gate.sh"
ACCEPTED_B3F_REF = "e14654f7129aa61011931306140a3bfefe2fcfbc"
EXPECTED_PASS_CASES = 580
# The canonical CI workflow pins this verifier before it starts this process.
# This verifier in turn pins and immediately executes the shared wrapper.
EXPECTED_WRAPPER_SHA256 = "f922a4f777fbb37e049ccb640f713b7ff7557cf4f86e8855823d7db328731e29"

REQUIRED_CI_FRAGMENTS = (
    "uses: actions/checkout@v4\n        with:\n"
    "          # Stage 5F runs the accepted B3F provenance harness from the exact\n"
    "          # immutable predecessor, not from the newer Stage 5F checkout.\n"
    "          fetch-depth: 0",
    "- name: Stage 5F execution authority and accepted B3F snapshot provenance\n"
    "        run: |",
    "python3 scripts/stage5f_ci_snapshot_inheritance_check.py --execute-verified-provenance",
    "- name: Prime immutable B3F dependency cache\n"
    "        run: |\n"
    "          set -euo pipefail\n"
    f'          accepted_b3f_ref="{ACCEPTED_B3F_REF}"\n'
    "          snapshot_root=\"$(mktemp -d \"${TMPDIR:-/tmp}/stage5f-b3f-dependency-prefetch.XXXXXX\")\"\n"
    "          trap 'rm -rf \"$snapshot_root\"' EXIT\n"
    "          git cat-file -e \"${accepted_b3f_ref}^{commit}\"\n"
    "          git archive \"$accepted_b3f_ref\" | tar -x -C \"$snapshot_root\"\n"
    "          (\n"
    "            cd \"$snapshot_root\"\n"
    "            CARGO_NET_OFFLINE=false cargo fetch --locked\n"
    "          )\n"
    "        timeout-minutes: 10",
    "- name: Stage 5F atomic Hybrid semantics gate\n"
    "        run: bash scripts/stage5f_atomic_hybrid_semantics_gate.sh",
    "- name: Stage 5F atomic Hybrid negative harness\n"
    "        run: python3 scripts/stage5f_atomic_hybrid_semantics_negative_harness.py",
    "- name: Stage 5F CI snapshot-inheritance negative harness\n"
    "        run: python3 scripts/stage5f_ci_snapshot_inheritance_negative_harness.py",
)

FORBIDDEN_CI_FRAGMENTS = (
    "- name: Stage 5E lifecycle event-time gate\n"
    "        run: bash scripts/stage5e_lifecycle_event_time_gate.sh",
    "- name: Handoff provenance negative harness\n"
    "        run: python3 scripts/handoff_provenance_negative_harness.py",
)

REQUIRED_WRAPPER_FRAGMENTS = (
    f'accepted_b3f_ref="{ACCEPTED_B3F_REF}"',
    f"expected_pass_cases={EXPECTED_PASS_CASES}",
    'git -C "$repo_root" cat-file -e "${accepted_b3f_ref}^{commit}"',
    "accepted B3F snapshot commit unavailable",
    'git -C "$snapshot_root" checkout --quiet --detach "$accepted_b3f_ref"',
    'git -C "$snapshot_root" rev-parse HEAD',
    "accepted B3F snapshot checkout drift",
    "python3 scripts/handoff_provenance_negative_harness.py",
    "stage5f-b3f-snapshot-provenance-gate: ok tested_source_ref=${accepted_b3f_ref} cases=${pass_cases}",
)


def fail(message: str) -> None:
    raise RuntimeError(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate() -> None:
    ci = CI.read_text()
    wrapper = WRAPPER.read_text()
    if sha256(WRAPPER) != EXPECTED_WRAPPER_SHA256:
        fail("Stage 5F wrapper authority digest drift")
    for fragment in FORBIDDEN_CI_FRAGMENTS:
        if fragment in ci:
            fail("legacy Stage 5E gate runs on Stage5F head")
    for fragment in REQUIRED_CI_FRAGMENTS:
        if fragment not in ci:
            fail("Stage 5F CI snapshot inheritance contract drift")
    for fragment in REQUIRED_WRAPPER_FRAGMENTS:
        if fragment not in wrapper:
            if "accepted_b3f_ref=" in fragment:
                fail("accepted B3F snapshot pin drift")
            if "checkout --quiet --detach" in fragment:
                fail("accepted B3F snapshot checkout drift")
            fail("Stage 5F snapshot provenance wrapper drift")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--execute-verified-provenance", action="store_true")
    args = parser.parse_args()
    try:
        validate()
        if args.execute_verified_provenance:
            subprocess.run(["bash", str(WRAPPER)], cwd=ROOT, check=True)
    except (OSError, RuntimeError, subprocess.CalledProcessError) as exc:
        print(f"stage5f-ci-snapshot-inheritance-check: FAIL: {exc}", file=sys.stderr)
        return 1
    if args.execute_verified_provenance:
        print("stage5f-ci-snapshot-inheritance-check: ok executed_verified_provenance=true")
    else:
        print("stage5f-ci-snapshot-inheritance-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
