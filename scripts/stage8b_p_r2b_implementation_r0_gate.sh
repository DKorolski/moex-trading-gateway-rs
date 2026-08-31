#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2b_issuance_systemd_check.py
python3 scripts/stage8b_p_r2b_implementation_r0_check.py
python3 scripts/stage8b_p_r2b_implementation_r0_negative_harness.py

python3 -m py_compile \
  scripts/stage8b_p_r2b_issuance_systemd_check.py \
  scripts/stage8b_p_r2b_implementation_r0_check.py \
  scripts/stage8b_p_r2b_implementation_r0_negative_harness.py \
  scripts/stage8b_p_r2b_implementation_r0_handoff_safety_check.py \
  scripts/make_stage8b_p_r2b_implementation_r0_handoff.py

python3 -m json.tool docs/stage-8/stage8b-p-r2b-implementation-r0-authority.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2b-implementation-r0-evidence.json >/dev/null

cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --check
cargo test --manifest-path tools/stage8b-readonly-preflight/Cargo.toml
cargo clippy --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets --all-features -- -D warnings

git diff --check
echo "stage8b-p-r2b-implementation-r0-gate: PASS predecessor=ebec9a100c92872134f3de91644cec50e2ed073a phases=6 services=31 units=18 rows=52 negative_mutations=70 installed=false enabled=false started=false authorization=NOT_ISSUED finam_requests=0"
