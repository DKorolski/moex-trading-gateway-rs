#!/usr/bin/env bash
set -euo pipefail

echo "source_ref=$(git rev-parse HEAD)"
echo "source_tree=$(git rev-parse 'HEAD^{tree}')"
rustc --version --verbose
cargo --version --verbose

run() {
  printf 'COMMAND:'
  printf ' %q' "$@"
  printf '\n'
  "$@"
}

run git diff --check
run python3 -c '
import json
from pathlib import Path

schema = json.loads(Path("docs/stage-8/stage8b-p1e-first-boot-source-bundle-schema-v2.json").read_text())
assert schema["properties"]["schema_version"]["const"] == 2
assert schema["properties"]["domain"]["const"] == "moex.stage8b.p1e.first-boot-source-bundle.v2"
assert "history_coverage" in schema["required"]
candidate = schema["$defs"]["candidate_bar"]["allOf"][1]
assert set(candidate["required"]) >= {
    "redis_id", "semantic_id_sha256", "payload_sha256",
    "open_ts_utc_ms", "close_ts_utc_ms", "source_m1",
}
assert candidate["properties"]["source_m1"]["minItems"] == 10
assert candidate["properties"]["source_m1"]["maxItems"] == 10
print("PASS first-boot-source-schema-v2")
'
run cargo test -p strategy-runtime-core --all-features
run env RUST_MIN_STACK=33554432 cargo test -p runtime-durable-service --all-features --lib
run env RUST_MIN_STACK=33554432 cargo test -p runtime-durable-service --all-features --test stage7b_redis_service_subprocess -- --test-threads=1
run cargo test -p strategy-runtime-core --all-features --doc
run cargo test -p runtime-durable-service --all-features --doc
run cargo fmt --all -- --check
run cargo clippy -p strategy-runtime-core -p runtime-durable-service --all-targets --all-features -- -D warnings

echo "PASS stage8b-p1e-i1-first-boot-correction-gate schema=2 candidate=canonical-p1-m10 history=exact-session-windows f00=filesystem-matrix operational_redis=false finam_write=false live=false"
