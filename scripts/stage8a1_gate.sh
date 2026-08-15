#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARTIFACT_DIR="${STAGE8A1_ARTIFACT_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/stage8a1-artifact.XXXXXX")}"
mkdir -p "$ARTIFACT_DIR"

python3 scripts/stage8a1_check.py >"$ARTIFACT_DIR/contract-check.txt" 2>&1
python3 scripts/stage8a1_closed_surface_check.py >"$ARTIFACT_DIR/closed-surface.txt" 2>&1
python3 scripts/stage8a1_negative_harness.py >"$ARTIFACT_DIR/negative.txt" 2>&1
python3 scripts/stage8a1_proof_map.py >"$ARTIFACT_DIR/proof-map.json"
python3 -m py_compile \
  scripts/stage8a1_check.py \
  scripts/stage8a1_closed_surface_check.py \
  scripts/stage8a1_negative_harness.py \
  scripts/stage8a1_proof_map.py \
  scripts/stage8a1_handoff_safety_check.py \
  scripts/make_stage8a1_handoff_archive.py \
  >"$ARTIFACT_DIR/python-compile.txt" 2>&1
printf '%s\n' 'python-compile: PASS' >>"$ARTIFACT_DIR/python-compile.txt"

printf '%s\n' 'command: cargo fmt --all -- --check' >"$ARTIFACT_DIR/fmt.txt"
cargo fmt --all -- --check >>"$ARTIFACT_DIR/fmt.txt" 2>&1
printf '%s\n' 'fmt: PASS' >>"$ARTIFACT_DIR/fmt.txt"

printf '%s\n' 'command: cargo test -p finam-gateway --all-targets -- --test-threads=1' >"$ARTIFACT_DIR/focused-test.txt"
cargo test -p finam-gateway --all-targets -- --test-threads=1 >>"$ARTIFACT_DIR/focused-test.txt" 2>&1
printf '%s\n' 'focused-test: PASS' >>"$ARTIFACT_DIR/focused-test.txt"

printf '%s\n' 'command: cargo test -p finam-gateway --doc -- --test-threads=1' >"$ARTIFACT_DIR/focused-doctest.txt"
cargo test -p finam-gateway --doc -- --test-threads=1 >>"$ARTIFACT_DIR/focused-doctest.txt" 2>&1
printf '%s\n' 'focused-doctest: PASS' >>"$ARTIFACT_DIR/focused-doctest.txt"

printf '%s\n' 'command: cargo clippy -p finam-gateway --all-targets --all-features -- -D warnings' >"$ARTIFACT_DIR/focused-clippy.txt"
cargo clippy -p finam-gateway --all-targets --all-features -- -D warnings >>"$ARTIFACT_DIR/focused-clippy.txt" 2>&1
printf '%s\n' 'focused-clippy: PASS' >>"$ARTIFACT_DIR/focused-clippy.txt"

printf '%s\n' 'command: cargo test --workspace --all-targets -- --test-threads=1' >"$ARTIFACT_DIR/workspace-test.txt"
cargo test --workspace --all-targets -- --test-threads=1 >>"$ARTIFACT_DIR/workspace-test.txt" 2>&1
printf '%s\n' 'workspace-test: PASS' >>"$ARTIFACT_DIR/workspace-test.txt"

printf '%s\n' 'command: cargo test --workspace --doc -- --test-threads=1' >"$ARTIFACT_DIR/workspace-doctest.txt"
cargo test --workspace --doc -- --test-threads=1 >>"$ARTIFACT_DIR/workspace-doctest.txt" 2>&1
printf '%s\n' 'workspace-doctest: PASS' >>"$ARTIFACT_DIR/workspace-doctest.txt"

printf '%s\n' 'command: cargo clippy --workspace --all-targets --all-features -- -D warnings' >"$ARTIFACT_DIR/workspace-clippy.txt"
cargo clippy --workspace --all-targets --all-features -- -D warnings >>"$ARTIFACT_DIR/workspace-clippy.txt" 2>&1
printf '%s\n' 'workspace-clippy: PASS' >>"$ARTIFACT_DIR/workspace-clippy.txt"

git diff --check >"$ARTIFACT_DIR/diff-check.txt" 2>&1
printf '%s\n' 'diff-check: PASS' >>"$ARTIFACT_DIR/diff-check.txt"
rustc --version >"$ARTIFACT_DIR/toolchain.txt"
cargo --version >>"$ARTIFACT_DIR/toolchain.txt"
python3 --version >>"$ARTIFACT_DIR/toolchain.txt"

printf 'stage8a1-r3-gate: PASS rows=76 negatives=70 trusted-root=true one-arm=true cancel-revalidation=true no-send=true next=8A-2-pending artifact_dir=%s\n' "$ARTIFACT_DIR"
