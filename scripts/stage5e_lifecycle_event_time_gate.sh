#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5e_lifecycle_event_time_freeze_check.py
python3 scripts/stage5e_b_no_io_lifecycle_check.py
stage5e_descriptor_json="$(python3 scripts/stage5e_descriptor.py --root .)"
stage5e_active_checker="$(STAGE5E_DESCRIPTOR_JSON="$stage5e_descriptor_json" python3 -c 'import json,os; print(json.loads(os.environ["STAGE5E_DESCRIPTOR_JSON"])["checker"])')"
python3 "$stage5e_active_checker"
python3 scripts/stage5c_api_freeze_check.py
python3 scripts/stage5d_additive_freeze_check.py
bash scripts/forbidden_surface_scan.sh
bash scripts/test_m4_3x_evidence_no_redis.sh
python3 -m py_compile scripts/stage5e_lifecycle_event_time_freeze_check.py scripts/stage5e_b_no_io_lifecycle_check.py scripts/stage5e_b3_schedule_window_evidence_check.py scripts/stage5e_b3c_private_eligibility_seam_check.py scripts/stage5e_b3c_source_authority_freeze_extension_check.py scripts/stage5e_b3d_callback_authority_design_check.py scripts/stage5e_b3e_callback_invocation_design_check.py scripts/stage5e_b3f_callback_settlement_escrow_design_check.py scripts/stage5e_descriptor.py

echo "stage5e-lifecycle-event-time-gate: ok"
