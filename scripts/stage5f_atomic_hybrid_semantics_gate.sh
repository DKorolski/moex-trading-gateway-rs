#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
accepted_b3f_ref="e14654f7129aa61011931306140a3bfefe2fcfbc"
snapshot_root="$(mktemp -d "${TMPDIR:-/tmp}/stage5f-b3f-snapshot.XXXXXX")"
trap 'rm -rf "$snapshot_root"' EXIT

git -C "$repo_root" archive "$accepted_b3f_ref" | tar -x -C "$snapshot_root"

(
  cd "$snapshot_root"
  python3 scripts/stage5e_b3f_callback_settlement_escrow_design_check.py
  python3 scripts/stage5e_b3f_production_ui_harness.py
)

cd "$repo_root"
stage5f_descriptor_json="$(python3 scripts/stage5f_descriptor.py --root .)"
stage5f_active_checker="$(STAGE5F_DESCRIPTOR_JSON="$stage5f_descriptor_json" python3 -c 'import json,os; print(json.loads(os.environ["STAGE5F_DESCRIPTOR_JSON"])["checker"])')"
python3 "$stage5f_active_checker"
python3 scripts/stage5f_ci_snapshot_inheritance_check.py
python3 scripts/stage5f_base_authority_negative_harness.py
python3 scripts/stage5c_api_freeze_check.py
python3 scripts/stage5d_additive_freeze_check.py
bash scripts/forbidden_surface_scan.sh
python3 -m py_compile \
  scripts/stage5f_descriptor.py \
  scripts/stage5f_atomic_hybrid_semantics_entry_check.py \
  scripts/stage5f_atomic_hybrid_semantics_negative_harness.py \
  scripts/stage5f_ci_snapshot_inheritance_check.py \
  scripts/stage5f_ci_snapshot_inheritance_negative_harness.py \
  scripts/stage5f_base_authority_negative_harness.py

echo "stage5f-atomic-hybrid-semantics-gate: ok"
