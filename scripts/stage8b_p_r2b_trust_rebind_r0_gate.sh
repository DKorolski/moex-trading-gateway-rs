#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2b_controlled_installation_impl_r0_preflight_check.py
python3 scripts/stage8b_p_r2b_trust_rebind_r0_check.py
python3 scripts/stage8b_p_r2b_trust_rebind_r0_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p_r2b_trust_rebind_r0_check.py \
  scripts/stage8b_p_r2b_trust_rebind_r0_negative_harness.py \
  scripts/stage8b_p_r2b_trust_rebind_r0_handoff_safety_check.py \
  scripts/make_stage8b_p_r2b_trust_rebind_r0_handoff.py
for document in \
  docs/stage-8/stage8b-p-r2b-trust-rebind-r0-authority.json \
  docs/stage-8/stage8b-p-r2b-trust-rebind-r0-supersession.json \
  docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json \
  docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-account-key-manifest.json; do
  python3 -m json.tool "$document" >/dev/null
done

cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml -- --check
cargo test --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets
cargo clippy --locked --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets -- -D warnings
git diff --check

echo "stage8b-p-r2b-trust-rebind-r0-gate: PASS generation=2 rust_tests=51 negative=36 historical_immutable=true backup=REQUIRED_NOT_VERIFIED active=false authorization=NOT_ISSUED finam=false"
