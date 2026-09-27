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
git diff --check e6b2f2dd2a35145dda4db0b2ae09e0581f56d989 --

echo "PASS stage8b-p1f-o0-gate rows=22 negative_cases=27 redis_server=exact p1_inventory=complete remote_mutation=false o1_authorized=false rust_changes=0 cargo_changes=0"
