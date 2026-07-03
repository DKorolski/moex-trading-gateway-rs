#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
redis_url="${FINAM_GATEWAY_REDIS_URL:-redis://127.0.0.1:6379/}"
prefix="${M3E_COMMAND_CONSUMER_SMOKE_PREFIX:-broker.m3e.command_consumer_smoke}"

cd "$repo_root"

cargo run -p broker-cli -- \
  m3e-command-consumer-redis-smoke \
  --redis-url "$redis_url" \
  --prefix "$prefix"
