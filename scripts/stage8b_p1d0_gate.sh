#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage8b_p1d0_check.py
python3 scripts/stage8b_p1d0_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1d0_check.py \
  scripts/stage8b_p1d0_negative_harness.py \
  scripts/make_stage8b_p1d0_handoff.py \
  scripts/stage8b_p1d0_handoff_safety_check.py
cargo fmt --all --check
git diff --exit-code \
  3d08f84a4a01d08265120def697584c3e60bcd3c \
  -- Cargo.toml Cargo.lock crates .github

echo "PASS stage8b-p1d0-gate rows=44 negatives=21 design_only=true implementation=false db0=false finam=false dispatch=false live=false real_orders=false"
