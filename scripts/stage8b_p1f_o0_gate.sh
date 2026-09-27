#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

bash -n scripts/stage8b_p1f_o0_readonly_probe.sh
python3 -m py_compile \
  scripts/stage8b_p1f_o0_collect.py \
  scripts/stage8b_p1f_o0_check.py \
  scripts/stage8b_p1f_o0_negative_harness.py \
  scripts/make_stage8b_p1f_o0_handoff.py \
  scripts/stage8b_p1f_o0_handoff_safety_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1f_o0_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1f_o0_negative_harness.py
git diff --check 3a46a460ea4bd5c85c5befd036510c580941a265 --

echo "PASS stage8b-p1f-o0-gate rows=20 negative_cases=20 remote_mutation=false o1_authorized=false rust_changes=0 cargo_changes=0"
