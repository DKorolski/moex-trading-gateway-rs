#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
artifact_dir="${STAGE8A4_I3_ARTIFACT_DIR:-$repo_root/tmp/stage8a4-i3-gate}"
rm -rf "$artifact_dir"
mkdir -p "$artifact_dir"

run_gate() {
  local name="$1"
  shift
  "$@" >"$artifact_dir/$name.txt" 2>&1
}

run_gate inherited-i2 python3 scripts/stage8a4_durable_composition_i2_check.py --no-git
run_gate inherited-stage8a1 cargo test -p finam-gateway --test stage8a1_r3_authority_boundary -- --test-threads=1
run_gate inherited-stage8a1-negative python3 scripts/stage8a1_negative_harness.py
run_gate inherited-stage8a1-successor python3 scripts/stage8a4_i3_stage8a1_successor_check.py
run_gate semantic-check python3 scripts/stage8a4_durable_composition_i3_check.py
run_gate semantic-negative python3 scripts/stage8a4_durable_composition_i3_negative_harness.py
run_gate external-compile-fail bash scripts/stage8a4_durable_composition_i3_external_compile_fail.sh
run_gate dependency-graph bash scripts/stage8a4_durable_composition_i3_dependency_graph_check.sh
run_gate proof-map python3 scripts/stage8a4_durable_composition_i3_proof_map.py
run_gate fmt cargo fmt --all -- --check
run_gate core-focused cargo test -p strategy-runtime-core stage8a4_i3_ -- --test-threads=1
run_gate runtime-focused cargo test -p runtime-durable-service stage8a4_i3_ -- --test-threads=1
run_gate gateway-focused cargo test -p finam-gateway stage8a4 -- --test-threads=1
run_gate stage8a1-focused cargo test -p finam-gateway stage8a1_execution_capability -- --test-threads=1
run_gate core-tests cargo test -p strategy-runtime-core --no-fail-fast
run_gate runtime-tests cargo test -p runtime-durable-service --no-fail-fast -- --test-threads=1
run_gate gateway-tests cargo test -p finam-gateway --no-fail-fast
run_gate clippy cargo clippy -p strategy-runtime-core -p runtime-durable-service -p finam-gateway --all-targets --all-features -- -D warnings

python3 - "$artifact_dir/stage8a4-durable-composition-i3-gate-summary.json" <<'PY'
import json
import sys
from pathlib import Path
Path(sys.argv[1]).write_text(json.dumps({
    "schema_version": 1,
    "stage": "8A-4-durable-composition-I3-R6",
    "result": "PASS",
    "acceptance_rows": 84,
    "negative_cases": 95,
    "sealed_linear_writer_authority": True,
    "exact_request_truth_control_binding": True,
    "post_write_sticky_fail_stop": True,
    "stage8a1_r3_authority_restored": True,
    "broker_neutral_runtime_dependency": True,
    "broker_core_sqlite_baseline_unchanged": True,
    "production_normal_composition_path": True,
    "production_restart_without_i2_candidate": True,
    "writer_entry_ed25519_attested": True,
    "production_normal_and_three_recovery_paths_directly_tested": True,
    "fresh_process_sigkill_recovery_directly_tested": True,
    "arm_registration_ed25519_attested": True,
    "arm_registration_issuer_key_pinned": True,
    "arm_registration_exact_binding_verified": True,
    "normal_execution_issuer_requires_readable_control": True,
    "recovery_only_issuer_structurally_separate": True,
    "recovery_unreadable_control_maps_stale_or_unreadable": True,
    "recovery_missing_control_maps_stale_or_unreadable": True,
    "recovery_stale_control_maps_stale_or_unreadable": True,
    "recovery_stop_requested_permits_post_effect_persistence": True,
    "recovery_readable_identity_mismatch_fails_closed": True,
    "fresh_process_corrupt_control_recovery_directly_tested": True,
    "recovery_requires_precrash_objects": False,
    "recovery_recreates_operator_arm": False,
    "recovery_reads_existing_arm_registration": True,
    "external_raw_mutator_compile_fail": True,
    "v2_durable_append_enabled": True,
    "four_field_cas_enabled": True,
    "covering_seal_writer_enabled": True,
    "ack_readiness_enabled": False,
    "redis_live_enabled": False,
    "finam_post_delete_enabled": False,
    "runtime_live_enabled": False,
    "real_orders_enabled": False
}, indent=2) + "\n", encoding="utf-8")
PY

echo "stage8a4-durable-composition-i3-gate: PASS rows=84 negatives=95 sealed=private pending=true fresh_process=true unreadable_control=true rearm=false broker_neutral=true recovery=true ack=false execution=false"
