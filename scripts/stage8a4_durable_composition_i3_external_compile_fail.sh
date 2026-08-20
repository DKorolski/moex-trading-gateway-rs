#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/stage8a4-i3-external.XXXXXX")"
trap 'rm -rf "$work"' EXIT

# A temporary external crate must be unable to import raw mutation or the
# private sealed capability. These are the exact R4 bypass names, not only
# historical aliases.
mkdir -p "$work/src"
printf '%s\n' \
  '[package]' \
  'name = "stage8a4-i3-external-compile-fail"' \
  'version = "0.0.0"' \
  'edition = "2021"' \
  '' \
  '[dependencies]' \
  "strategy-runtime-core = { path = \"$repo_root/crates/strategy-runtime-core\" }" \
  >"$work/Cargo.toml"
printf '%s\n' \
  'use strategy_runtime_core::stage8a4_internal_append_durable_batch;' \
  'use strategy_runtime_core::append_stage8a4_durable_batch;' \
  'use strategy_runtime_core::Stage6Stage8a4SealedWriteAuthority;' \
  'use strategy_runtime_core::apply_stage8a4_sealed_durable_write;' \
  'fn main() {' \
  '    let _ = stage8a4_internal_append_durable_batch;' \
  '    let _ = append_stage8a4_durable_batch;' \
  '    let _ = Stage6Stage8a4SealedWriteAuthority::seal;' \
  '    let _ = apply_stage8a4_sealed_durable_write;' \
  '}' \
  >"$work/src/main.rs"

if cargo check --manifest-path "$work/Cargo.toml" >"$work/check.txt" 2>&1; then
  echo "stage8a4-i3-external-compile-fail: FAIL raw mutator compiled" >&2
  exit 1
fi

grep -Eq 'unresolved import|private (function|struct)|no `stage8a4_internal_append_durable_batch`' "$work/check.txt"

# The public broker-neutral entry cannot regain the former unauthenticated
# `issue` constructor. Signature verification is the only production mint.
printf '%s\n' \
  'use strategy_runtime_core::Stage6Stage8a4ValidatedWriteEntry;' \
  'fn main() {' \
  '    let _ = Stage6Stage8a4ValidatedWriteEntry::issue;' \
  '}' \
  >"$work/src/main.rs"
if cargo check --manifest-path "$work/Cargo.toml" >"$work/issue-check.txt" 2>&1; then
  echo "stage8a4-i3-external-compile-fail: FAIL unauthenticated issuer compiled" >&2
  exit 1
fi
grep -Eq 'no function or associated item named `issue`|no function or associated item' "$work/issue-check.txt"
echo "stage8a4-i3-external-compile-fail: PASS"
