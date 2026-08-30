#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2b_issuance_systemd_check.py
python3 scripts/stage8b_p_r2b_systemd_unit_check.py
python3 scripts/stage8b_p_r2b_implementation_r0_r1_check.py
python3 scripts/stage8b_p_r2b_implementation_r0_r1_negative_harness.py

python3 -m py_compile \
  scripts/stage8b_p_r2b_issuance_systemd_check.py \
  scripts/stage8b_p_r2b_systemd_unit_check.py \
  scripts/stage8b_p_r2b_implementation_r0_r1_check.py \
  scripts/stage8b_p_r2b_implementation_r0_r1_negative_harness.py \
  scripts/stage8b_p_r2b_implementation_r0_r1_handoff_safety_check.py \
  scripts/make_stage8b_p_r2b_implementation_r0_r1_handoff.py

python3 -m json.tool docs/stage-8/stage8b-p-r2b-implementation-r0-r1-authority.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2b-implementation-r0-r1-linux-build-evidence.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2b-implementation-r0-r1-linux-rehearsal-evidence.json >/dev/null

cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --check
cargo test --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --no-default-features
cargo test --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-features
cargo clippy --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets --all-features -- -D warnings

git diff --check
echo "stage8b-p-r2b-implementation-r0-r1-gate: PASS predecessor=da83f5922d9e2a9a5a1db3e581d2d9f55d810d81 credentials=isolated linux_elf=2x2 phases=6 services=31 dynamic_failures=5 negative_mutations=20 installed=false enabled=false started=false authorization=NOT_ISSUED finam_requests=0"
