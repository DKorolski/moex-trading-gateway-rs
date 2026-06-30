#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
redis_url="${FINAM_GATEWAY_REDIS_URL:-redis://127.0.0.1:6379/}"
prefix="${RUNTIME_BRIDGE_SMOKE_PREFIX:-broker.m2i.runtime_bridge_smoke}"

cd "$repo_root"

cargo run -p broker-cli -- \
  runtime-bridge-redis-smoke \
  --redis-url "$redis_url" \
  --prefix "$prefix"
