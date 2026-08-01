#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
redis_server="$(command -v redis-server || true)"
redis_cli="$(command -v redis-cli || true)"
redis_pid=""
redis_dir=""

cleanup() {
  local status=$?
  if [[ -n "$redis_pid" ]]; then
    kill -TERM "$redis_pid" 2>/dev/null || true
    wait "$redis_pid" 2>/dev/null || true
  fi
  if [[ -n "$redis_dir" ]]; then
    rm -rf "$redis_dir"
  fi
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

if [[ -z "$redis_server" || -z "$redis_cli" ]]; then
  echo "stage5f-e-redis-regression-gate: FAIL: redis-server and redis-cli are required" >&2
  exit 1
fi

redis_dir="$(mktemp -d "${TMPDIR:-/tmp}/stage5f-e-redis.XXXXXX")"
redis_port="$(python3 - <<'PY'
import socket

with socket.socket() as handle:
    handle.bind(("127.0.0.1", 0))
    print(handle.getsockname()[1])
PY
)"

"$redis_server" \
  --bind 127.0.0.1 \
  --port "$redis_port" \
  --dir "$redis_dir" \
  --save "" \
  --appendonly no \
  >"$redis_dir/redis.stdout" 2>"$redis_dir/redis.stderr" &
redis_pid=$!

for _ in $(seq 1 100); do
  if "$redis_cli" -h 127.0.0.1 -p "$redis_port" ping 2>/dev/null | grep -qx PONG; then
    break
  fi
  sleep 0.05
done
if ! "$redis_cli" -h 127.0.0.1 -p "$redis_port" ping 2>/dev/null | grep -qx PONG; then
  cat "$redis_dir/redis.stderr" >&2
  echo "stage5f-e-redis-regression-gate: FAIL: disposable Redis did not start" >&2
  exit 1
fi

(
  cd "$repo_root"
  FINAM_GATEWAY_REDIS_URL="redis://127.0.0.1:${redis_port}/" \
    FINAM_GATEWAY_SMOKE_STREAM="stage5f:e:redis_smoke" \
    bash scripts/redis_shadow_smoke.sh
  FINAM_GATEWAY_REDIS_URL="redis://127.0.0.1:${redis_port}/" \
    RUNTIME_BRIDGE_SMOKE_PREFIX="stage5f.e.runtime_bridge_smoke" \
    bash scripts/runtime_bridge_dry_smoke.sh
)

echo "stage5f-e-redis-regression-gate: ok isolated=true"
