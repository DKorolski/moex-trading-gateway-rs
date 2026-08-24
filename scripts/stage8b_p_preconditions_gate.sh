#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/stage8b_tls_qualification_check.py --no-git
python3 scripts/stage8b_p_contract_refresh.py
python3 scripts/stage8b_p_build_repro.py
python3 scripts/stage8b_p_governance_refresh.py
python3 scripts/stage8b_p_preconditions_check.py
python3 scripts/stage8b_p_preconditions_negative_harness.py
python3 -m json.tool docs/stage-8/stage8b-p-finam-contract-snapshot-2026-08-23.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-build-identity-2026-08-23.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-governance-observation-2026-08-23.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-preconditions-authority.json >/dev/null

# Bind the complete canonical regression to the exact corrective commit.  The
# inherited current-tree replay remains separate from the current workspace
# checks so an accepted historical tree cannot substitute for this candidate.
bash scripts/current_tree_ci_gate.sh
bash scripts/test_m4_3x_evidence_no_redis.sh
cargo fmt --all -- --check
cargo test --workspace --all-targets -- --test-threads=1
cargo test --workspace --release --all-targets -- --test-threads=1
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings
scripts/redis_shadow_smoke.sh
scripts/runtime_bridge_dry_smoke.sh
git diff --check

echo "stage8b-p-full-regression: PASS current-tree=true debug=true release=true doc=true clippy=true redis-shadow=true runtime-bridge=true"
echo "stage8b-p-preconditions-gate: PASS revision=R4 rows=48 negatives=64 contract=accepted build=accepted governance=solo-accepted stage8b_p=false finam=false broker_effect=false"
