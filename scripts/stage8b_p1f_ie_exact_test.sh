#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 2 ]]; then
  echo "stage8b-p1f-ie-exact-test: FAIL usage: PACKAGE QUALIFIED_SELECTOR" >&2
  exit 64
fi

package="$1"
selector="$2"
case "$package" in
  runtime-durable-service|finam-gateway) ;;
  *)
    echo "stage8b-p1f-ie-exact-test: FAIL unsupported package=$package" >&2
    exit 64
    ;;
esac
if [[ -z "$selector" || "$selector" != *"::tests::"* ]]; then
  echo "stage8b-p1f-ie-exact-test: FAIL invalid qualified selector=$selector" >&2
  exit 64
fi

output="$(mktemp "${TMPDIR:-/tmp}/stage8b-p1f-ie-exact.XXXXXX")"
trap 'rm -f "$output"' EXIT

if ! cargo test -p "$package" --all-features --lib "$selector" -- \
  --exact --test-threads=1 --nocapture 2>&1 | tee "$output"; then
  echo "stage8b-p1f-ie-exact-test: FAIL cargo package=$package selector=$selector" >&2
  exit 1
fi

selected="$({ grep -Foc "$selector" "$output" || true; } | tr -d '[:space:]')"
summary="$({ grep -Fc "test result: ok. 1 passed; 0 failed; 0 ignored;" "$output" || true; } | tr -d '[:space:]')"
if [[ "$selected" != "1" || "$summary" -lt 1 ]]; then
  echo "stage8b-p1f-ie-exact-test: FAIL selection package=$package selector=$selector selected=$selected summaries=$summary" >&2
  exit 1
fi

echo "PASS stage8b-p1f-ie-exact-test package=$package selector=$selector selected=1 passed=1 failed=0 ignored=0"
