#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARTIFACT_DIR="${STAGE8A0_ARTIFACT_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/stage8a0-artifact.XXXXXX")}"
mkdir -p "$ARTIFACT_DIR"

python3 scripts/stage8a0_check.py >"$ARTIFACT_DIR/contract-check.txt" 2>&1
python3 scripts/stage8a0_closed_surface_check.py >"$ARTIFACT_DIR/closed-surface.txt" 2>&1
python3 scripts/stage8a0_negative_harness.py >"$ARTIFACT_DIR/negative.txt" 2>&1
python3 scripts/stage8a0_proof_map.py >"$ARTIFACT_DIR/proof-map.json"
python3 -m py_compile \
  scripts/stage8a0_check.py \
  scripts/stage8a0_closed_surface_check.py \
  scripts/stage8a0_negative_harness.py \
  scripts/stage8a0_proof_map.py \
  scripts/stage8a0_handoff_safety_check.py \
  scripts/make_stage8a0_handoff_archive.py \
  >"$ARTIFACT_DIR/python-compile.txt" 2>&1
printf '%s\n' 'python-compile: PASS' >>"$ARTIFACT_DIR/python-compile.txt"

cargo fmt --all -- --check >"$ARTIFACT_DIR/fmt.txt" 2>&1
printf '%s\n' 'fmt: PASS' >>"$ARTIFACT_DIR/fmt.txt"
cargo test --workspace >"$ARTIFACT_DIR/test.txt" 2>&1
printf '%s\n' 'test: PASS' >>"$ARTIFACT_DIR/test.txt"
cargo test --workspace --doc >"$ARTIFACT_DIR/doctest.txt" 2>&1
printf '%s\n' 'doctest: PASS' >>"$ARTIFACT_DIR/doctest.txt"
cargo clippy --workspace --all-targets --all-features -- -D warnings >"$ARTIFACT_DIR/clippy.txt" 2>&1
printf '%s\n' 'clippy: PASS' >>"$ARTIFACT_DIR/clippy.txt"
git diff --check >"$ARTIFACT_DIR/diff-check.txt" 2>&1
printf '%s\n' 'diff-check: PASS' >>"$ARTIFACT_DIR/diff-check.txt"
rustc --version >"$ARTIFACT_DIR/toolchain.txt"
cargo --version >>"$ARTIFACT_DIR/toolchain.txt"
python3 --version >>"$ARTIFACT_DIR/toolchain.txt"

printf 'stage8a0-gate: PASS rows=36 negatives=36 parity=MATCH next=8A-1-pending production=closed artifact_dir=%s\n' "$ARTIFACT_DIR"
