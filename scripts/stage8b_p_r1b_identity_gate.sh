#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# Preserve and replay the complete R1/R1A contract and all inherited 98
# fail-closed mutations. No account or broker request is performed.
bash scripts/stage8b_p_r1a_authorization_gate.sh

python3 scripts/stage8b_p_r1b_identity_check.py
python3 scripts/stage8b_p_r1b_identity_negative_harness.py
python3 -m json.tool docs/stage-8/stage8b-p-r1b-authorization-authority.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r1b-network-endpoint-authority.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r1b-run-identity-authority.json >/dev/null
python3 -m py_compile \
  scripts/stage8b_p_r1b_identity_check.py \
  scripts/stage8b_p_r1b_identity_negative_harness.py
cargo fmt --all -- --check
git diff --check

echo "stage8b-p-r1b-identity-gate: PASS rows=40 endpoint_goldens=2 run_goldens=2 new_negatives=36 inherited=98 total=134 authorization=NOT_ISSUED broker_get=false arm=false transport=false finam=false"
