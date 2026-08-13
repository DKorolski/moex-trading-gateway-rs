#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARTIFACT_DIR="${STAGE7B_D_DESIGN_ARTIFACT_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/stage7b-d-design-artifact.XXXXXX")}"
mkdir -p "$ARTIFACT_DIR"

cargo fmt --all -- --check >"$ARTIFACT_DIR/fmt.txt" 2>&1
printf '%s\n' 'fmt: PASS' >>"$ARTIFACT_DIR/fmt.txt"
python3 scripts/stage7b_d_design_check.py >"$ARTIFACT_DIR/design-check.txt" 2>&1
python3 scripts/stage7b_d_design_negative_harness.py >"$ARTIFACT_DIR/negative.txt" 2>&1
python3 -m py_compile \
  scripts/stage7b_d_design_check.py \
  scripts/stage7b_d_design_negative_harness.py \
  scripts/make_stage7b_d_design_handoff_archive.py \
  scripts/stage7b_proof_map.py >"$ARTIFACT_DIR/python-compile.txt" 2>&1
printf '%s\n' 'python-compile: PASS' >>"$ARTIFACT_DIR/python-compile.txt"
git diff --quiet c57ae8d5f98bbb11df0a81f78262d3916b276d81 -- \
  Cargo.toml Cargo.lock crates .cargo .github/workflows
printf '%s\n' 'accepted-stage7b-c-production-tree-unchanged: PASS' \
  >"$ARTIFACT_DIR/production-diff.txt"
git diff --check >"$ARTIFACT_DIR/diff-check.txt" 2>&1
printf '%s\n' 'diff-check: PASS' >>"$ARTIFACT_DIR/diff-check.txt"
rustc --version >"$ARTIFACT_DIR/toolchain.txt"
cargo --version >>"$ARTIFACT_DIR/toolchain.txt"

printf 'stage7b-d-design-gate: PASS artifact_dir=%s\n' "$ARTIFACT_DIR"
