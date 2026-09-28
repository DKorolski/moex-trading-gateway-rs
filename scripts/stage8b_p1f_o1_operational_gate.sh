#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage8b_p1f_o1_operational_check.py
python3 scripts/stage8b_p1f_o1_operational_negative_harness.py
python3 scripts/stage8b_p1f_o1_systemd_behavioral_test.py
python3 -m py_compile \
  scripts/stage8b_p1f_o1_collect.py \
  scripts/stage8b_p1f_o1_operational_check.py \
  scripts/stage8b_p1f_o1_operational_negative_harness.py \
  scripts/stage8b_p1f_o1_systemd_behavioral_test.py \
  scripts/stage8b_p1f_o1_operational_handoff_safety_check.py \
  scripts/make_stage8b_p1f_o1_operational_handoff.py
bash -n scripts/stage8b_p1f_o1_readonly_probe.sh
echo "PASS stage8b-p1f-o1-operational-gate activation=false db15=empty"
