#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export RUST_MIN_STACK=33554432
evidence_dir="${STAGE8B_I1_FIXED_INSTALL_EVIDENCE_DIR:-$repo_root/reports/stage8b-p1e-i1-fixed-install}"

python3 -m json.tool docs/stage-8/stage8b-p1e-i1-aggregate-acceptance.json >/dev/null
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_aggregate_acceptance_check.py --evidence-dir "$evidence_dir"
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_aggregate_acceptance_negative_harness.py

PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1a_source_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_transaction_v5_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_committed_restart_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_committed_restart_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1d4_source_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_process_supervision_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_process_supervision_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_telemetry_composition_check.py --skip-lineage
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_telemetry_composition_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_fixed_install_check.py --evidence-dir "$evidence_dir"
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_fixed_install_negative_harness.py

cargo fmt --all -- --check
bash scripts/stage8b_p1b_semantic_negative_harness.sh
cargo test -p strategy-runtime-core --lib --all-features -- --test-threads=1
cargo test -p runtime-durable-service --lib --all-features -- --test-threads=1
cargo test -p strategy-runtime-core --doc --all-features
cargo test -p runtime-durable-service --doc --all-features
cargo clippy -p strategy-runtime-core -p runtime-durable-service --all-targets --all-features -- -D warnings
git diff --check 7f2e876c4cad7a3a4a0fa10a1eb5202e58202d2f --

echo "PASS stage8b-p1e-i1-aggregate-acceptance-gate aggregate_negative=18/18 isolated_redis=true target_evidence=21 operational_activation=false"
