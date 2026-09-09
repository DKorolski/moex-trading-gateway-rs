#!/usr/bin/env bash
set -euo pipefail

git diff --check
PYTHONPATH=scripts python3 scripts/stage8b_p1e_r10_design_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_r10_design_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1e_i0_scope_check.py \
  scripts/stage8b_p1e_i0_retained_evidence.py \
  scripts/stage8b_p1e_i0_p1d4_regression_check.py \
  scripts/stage8b_p1e_r10_design_check.py \
  scripts/stage8b_p1e_r10_design_negative_harness.py \
  scripts/stage8b_p1e_r10_design_handoff_safety_check.py \
  scripts/make_stage8b_p1e_r10_design_handoff.py
bash -n scripts/stage8b_p1e_i0_regression_gate.sh scripts/stage8b_p1e_r10_design_gate.sh

smoke_parent="$(mktemp -d "${TMPDIR:-/tmp}/stage8b-p1e-r10-retained-smoke.XXXXXX")"
trap 'rm -rf "$smoke_parent"' EXIT
mkdir "$smoke_parent/.temporary"
printf '%s\n' 'design-smoke' > "$smoke_parent/.temporary/gate.log"
python3 scripts/stage8b_p1e_i0_retained_evidence.py finalize \
  --temporary "$smoke_parent/.temporary" \
  --output "$smoke_parent/retained" \
  --status FAIL \
  --exit-code 1 \
  --accepted-r10 0000000000000000000000000000000000000000 \
  --source-ref "$(git rev-parse HEAD)" \
  --source-tree "$(git rev-parse 'HEAD^{tree}')"
python3 scripts/stage8b_p1e_i0_retained_evidence.py check "$smoke_parent/retained"
echo "PASS stage8b-p1e-r10-retained-evidence-smoke status=FAIL atomic=true manifest=true"

echo "PASS stage8b-p1e-r10-design-gate negatives=53 integrity=8 semantic=45 active=317 semantic_keys=36 fixtures=46 corrected=10 source_paths=7 retained=true exact_tests=6 design_only=true i0_now=false supervisor=false activation=false db0=false finam=false live=false"
