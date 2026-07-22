#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5e_lifecycle_event_time_freeze_check.py
python3 scripts/stage5c_api_freeze_check.py
python3 scripts/stage5d_additive_freeze_check.py
bash scripts/forbidden_surface_scan.sh
bash scripts/test_m4_3x_evidence_no_redis.sh
python3 -m py_compile scripts/stage5e_lifecycle_event_time_freeze_check.py

echo "stage5e-lifecycle-event-time-gate: ok"
