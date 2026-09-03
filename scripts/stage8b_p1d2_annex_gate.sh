#!/usr/bin/env bash
set -euo pipefail

git diff --check
python3 scripts/stage8b_p1d2_annex_check.py
python3 scripts/stage8b_p1d2_annex_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1d2_annex_check.py \
  scripts/stage8b_p1d2_annex_negative_harness.py \
  scripts/make_stage8b_p1d2_annex_handoff.py \
  scripts/stage8b_p1d2_annex_handoff_safety_check.py
cargo fmt --all -- --check

echo "PASS stage8b-p1d2-annex-gate rows=60 negatives=30 design_only=true source=false db0=false ack=false xack=false finam=false"
