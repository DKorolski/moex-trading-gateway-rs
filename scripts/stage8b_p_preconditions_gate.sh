#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/stage8b_tls_qualification_check.py --no-git
python3 scripts/stage8b_p_contract_refresh.py
python3 scripts/stage8b_p_build_repro.py
python3 scripts/stage8b_p_preconditions_check.py
python3 scripts/stage8b_p_preconditions_negative_harness.py
python3 -m json.tool docs/stage-8/stage8b-p-finam-contract-snapshot-2026-08-23.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-build-identity-2026-08-23.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-governance-observation-2026-08-23.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-preconditions-authority.json >/dev/null
cargo fmt --all -- --check
git diff --check

echo "stage8b-p-preconditions-gate: PASS revision=R1 rows=36 negatives=24 contract=ready build=ready governance=pending stage8b_p=false finam=false broker_effect=false"
