#!/usr/bin/env bash
set -euo pipefail

accepted_design=1a1ea05775f1d15b86fcc3495ad6863b851e9212
design_artifacts=(
  docs/stage-8/fixtures/stage8b-p1d4-command-publication-binding-v1.json
  docs/stage-8/stage8b-p1d4-crash-replay-design-r7.md
  docs/stage-8/stage8b-p1d4-crash-replay-evidence-r7.json
  docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v3.csv
  docs/stage-8/stage8b-p1d4-r7-acceptance-amendment.csv
  docs/stage-8/stage8b-p1d4-source-discovery-r7.md
  docs/stage-8/stage8b-p1d4-source-shape-r7.json
  scripts/make_stage8b_p1d4_r7_design_handoff.py
  scripts/stage8b_p1d4_r7_design_check.py
  scripts/stage8b_p1d4_r7_design_gate.sh
  scripts/stage8b_p1d4_r7_design_handoff_safety_check.py
  scripts/stage8b_p1d4_r7_design_negative_harness.py
)
git diff --quiet "$accepted_design" -- "${design_artifacts[@]}"
echo "PASS stage8b-p1d4-r7-design-baseline-integrity ref=$accepted_design artifacts=${#design_artifacts[@]}"
python3 scripts/stage8b_p1d4_source_check.py
python3 scripts/stage8b_p1d4_source_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1d4_crash_evidence_check.py \
  scripts/stage8b_p1d4_crash_evidence_negative_harness.py \
  scripts/stage8b_p1d4_source_check.py \
  scripts/stage8b_p1d4_source_negative_harness.py \
  scripts/make_stage8b_p1d4_source_handoff.py \
  scripts/stage8b_p1d4_source_handoff_safety_check.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib --all-features
RUST_MIN_STACK=33554432 cargo test -p runtime-durable-service --lib --all-features
evidence_output="$(pwd)/reports/stage8b-p1d4-r5-crash-evidence"
mkdir -p "$evidence_output"
STAGE8B_P1D4_EVIDENCE_OUTPUT="$evidence_output" \
STAGE8B_P1D4_EVIDENCE_SOURCE_REF="$(git rev-parse HEAD)" \
STAGE8B_P1D4_EVIDENCE_SOURCE_TREE="$(git rev-parse 'HEAD^{tree}')" \
RUST_MIN_STACK=33554432 \
  cargo test -p runtime-durable-service --lib --all-features \
    stage8b_p1_semantic::redis::tests::p1d4_exhaustive_crash_replay_evidence_two_clean_runs \
    -- --ignored --exact
python3 scripts/stage8b_p1d4_crash_evidence_check.py "$evidence_output"
python3 scripts/stage8b_p1d4_crash_evidence_negative_harness.py "$evidence_output"
cargo test -p strategy-runtime-core --doc --all-features
cargo test -p runtime-durable-service --doc --all-features
cargo clippy -p strategy-runtime-core -p runtime-durable-service \
  --all-targets --all-features -- -D warnings

echo "PASS stage8b-p1d4-source-gate rows=20 positive_sigkill=105 duplicate=105 conflict=105 two_clean_runs=true source_negatives=60 evidence_negatives=41 db0=false finam=false live=false p1e=false"
