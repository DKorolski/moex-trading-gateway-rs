#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

bash -n scripts/stage8b_p1f_o0_readonly_probe.sh scripts/stage8b_p1f_o0_systemd_query_behavioral_test.sh
python3 -m py_compile \
  scripts/stage8b_p1f_o0_collect.py \
  scripts/stage8b_p1f_o0_check.py \
  scripts/stage8b_p1f_o0_negative_harness.py \
  scripts/make_stage8b_p1f_o0_handoff.py \
  scripts/stage8b_p1f_o0_handoff_safety_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1f_o0_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1f_o0_negative_harness.py
bash scripts/stage8b_p1f_o0_systemd_query_behavioral_test.sh
git diff --check 609f999ae1184c53de6320125a52b93bfdad9ace --

echo "PASS stage8b-p1f-o0-gate rows=22 negative_cases=27 behavioral_controls=2 redis_server=exact p1_inventory=complete systemd_query=fail_closed remote_mutation=false o1_authorized=false rust_changes=0 cargo_changes=0"
