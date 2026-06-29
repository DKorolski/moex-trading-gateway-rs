#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
redis_url="${FINAM_GATEWAY_REDIS_URL:-redis://127.0.0.1:6379/}"
stream="${FINAM_GATEWAY_SMOKE_STREAM:-finam:smoke}"

cd "$repo_root"

cargo run -p broker-cli -- \
  finam-gateway-redis-smoke \
  --redis-url "$redis_url" \
  --stream "$stream"
