#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 1 ]]; then
  echo "usage: stage8b_p1e_i1a_source_gate.sh NEW_ABSOLUTE_RETAINED_OUTPUT" >&2
  exit 64
fi

repo_root="$(git rev-parse --show-toplevel)"
retained_output="$1"
if [[ "$retained_output" != /* ]]; then
  echo "stage8b-p1e-i1a-source-gate: FAIL retained output must be absolute" >&2
  exit 64
fi
if [[ "$retained_output" == "$repo_root" || "$retained_output" == "$repo_root/"* ]]; then
  echo "stage8b-p1e-i1a-source-gate: FAIL retained output must be outside repository" >&2
  exit 64
fi
if [[ -e "$retained_output" ]]; then
  echo "stage8b-p1e-i1a-source-gate: FAIL retained output must be fresh" >&2
  exit 64
fi
mkdir "$retained_output"
exec > >(tee "$retained_output/stage8b-p1e-i1a-source-gate.txt") 2>&1

source_ref="$(git rev-parse HEAD)"
source_tree="$(git rev-parse 'HEAD^{tree}')"
accepted_design="aa24e840ed8b7d18c80be6f1fdd8f50facf5b6d4"
design_snapshot="$(mktemp -d "${TMPDIR:-/tmp}/stage8b-p1e-i1a-r2-design.XXXXXX")"
cleanup() {
  rm -rf "$design_snapshot"
}
trap cleanup EXIT

git archive "$accepted_design" | tar -x -C "$design_snapshot"
(
  cd "$design_snapshot"
  bash scripts/stage8b_p1e_i1a_r2_design_gate.sh
)
echo "PASS stage8b-p1e-i1a-r2-design-baseline ref=$accepted_design"
python3 scripts/stage8b_p1e_i1a_source_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1a_source_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1d4_source_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1e_i1a_source_check.py \
  scripts/stage8b_p1e_i1a_source_negative_harness.py \
  scripts/stage8b_p1e_i1a_source_handoff_safety_check.py \
  scripts/make_stage8b_p1e_i1a_source_handoff.py

cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib --all-features
RUST_MIN_STACK=33554432 cargo test -p runtime-durable-service --lib --all-features
cargo test -p finam-gateway --lib
cargo test -p finam-gateway --lib --all-features stage8b_p1e_schedule_publisher

mkdir "$retained_output/p1d4-crash-evidence"
STAGE8B_P1D4_EVIDENCE_OUTPUT="$retained_output/p1d4-crash-evidence" \
STAGE8B_P1D4_EVIDENCE_SOURCE_REF="$source_ref" \
STAGE8B_P1D4_EVIDENCE_SOURCE_TREE="$source_tree" \
RUST_MIN_STACK=33554432 \
  cargo test -p runtime-durable-service --lib --all-features \
    stage8b_p1_semantic::redis::tests::p1d4_exhaustive_crash_replay_evidence_two_clean_runs \
    -- --ignored --exact
python3 scripts/stage8b_p1d4_crash_evidence_check.py "$retained_output/p1d4-crash-evidence"
python3 scripts/stage8b_p1d4_crash_evidence_negative_harness.py "$retained_output/p1d4-crash-evidence"

cargo test -p strategy-runtime-core --doc --all-features
cargo test -p runtime-durable-service --doc --all-features
cargo test -p finam-gateway --doc --all-features
cargo clippy -p strategy-runtime-core -p runtime-durable-service -p finam-gateway \
  --all-targets --all-features -- -D warnings

if [[ "$(git rev-parse HEAD)" != "$source_ref" || "$(git rev-parse 'HEAD^{tree}')" != "$source_tree" ]]; then
  echo "stage8b-p1e-i1a-source-gate: FAIL source ref/tree changed during run"
  exit 1
fi
if [[ -n "$(git status --porcelain --untracked-files=all)" ]]; then
  echo "stage8b-p1e-i1a-source-gate: FAIL source became dirty during run"
  exit 1
fi

echo "PASS stage8b-p1e-i1a-source-gate source_ref=$source_ref source_tree=$source_tree acceptance=81 r2=8 source_negatives=100 p1d4_sigkill=105x2 full_tests=true doctests=true clippy=true db0=false db15=false finam_write=false live=false"
