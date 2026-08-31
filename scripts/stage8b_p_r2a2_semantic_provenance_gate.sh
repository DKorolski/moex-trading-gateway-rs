#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

bash scripts/stage8b_p_r1b_identity_gate.sh
python3 scripts/stage8b_p_r2a2_semantic_provenance_check.py
python3 scripts/stage8b_p_r2a2_semantic_provenance_negative_harness.py
python3 -m json.tool docs/stage-8/stage8b-p-r2a2-semantic-provenance-authority.json >/dev/null
cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml -- --check
cargo test --manifest-path tools/stage8b-readonly-preflight/Cargo.toml
cargo clippy --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets -- -D warnings
cargo build --release --manifest-path tools/stage8b-readonly-preflight/Cargo.toml
actual_helper_sha256="$(shasum -a 256 tools/stage8b-readonly-preflight/target/release/stage8b-readonly-preflight | awk '{print $1}')"
[[ "$actual_helper_sha256" == "0c6dcde920de131863fe12632b0e3092f30fedc796e4627873cea89b6aace363" ]]
if tools/stage8b-readonly-preflight/target/release/stage8b-readonly-preflight >/dev/null 2>&1; then
  echo "stage8b-p-r2a2-gate: FAIL qualification binary unexpectedly opened" >&2
  exit 1
fi
git diff --check

echo "stage8b-p-r2a2-gate: PASS semantic=true provenance=hmac strict=true bounded=true tls=true binary=fail-closed real_http=false authorization=NOT_ISSUED"
