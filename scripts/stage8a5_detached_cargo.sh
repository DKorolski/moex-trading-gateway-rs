#!/usr/bin/env bash
set -euo pipefail

: "${STAGE8A5_REAL_CARGO:?missing real cargo path}"
: "${STAGE8A5_DETACHED_TARGET_ROOT:?missing detached target root}"

workspace="$(git rev-parse --show-toplevel 2>/dev/null || pwd -P)"
workspace="$(cd "$workspace" && pwd -P)"
target_key="$(printf '%s' "$workspace" | shasum -a 256 | awk '{print $1}')"
export CARGO_TARGET_DIR="$STAGE8A5_DETACHED_TARGET_ROOT/$target_key"
mkdir -p "$CARGO_TARGET_DIR"
exec "$STAGE8A5_REAL_CARGO" "$@"
