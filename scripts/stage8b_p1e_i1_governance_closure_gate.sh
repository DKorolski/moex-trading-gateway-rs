#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 -m json.tool docs/stage-8/stage8b-p1e-i1-governance-closure.json >/dev/null
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_governance_closure_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_governance_closure_negative_harness.py

accepted_archive="reports/handoff/moex-trading-project-a9bcd94-stage8b-p1e-i1-aggregate-acceptance.zip"
test -f "$accepted_archive"
actual_sha="$(shasum -a 256 "$accepted_archive" | awk '{print $1}')"
test "$actual_sha" = "5dfb664d86f37c00441c40b0d622db1fc87f6559812ab726d2a076c1f6bac81d"
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_aggregate_acceptance_handoff_safety_check.py "$accepted_archive"

git diff --check a9bcd940635b62c2a13f8d378453e6ca21511e30 --
echo "PASS stage8b-p1e-i1-governance-closure-gate rows=14 negatives=14 production_change=false p1f_design=true activation=false"
