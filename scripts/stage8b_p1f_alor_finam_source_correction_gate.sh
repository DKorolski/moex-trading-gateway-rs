#!/bin/sh
set -eu

export RUST_MIN_STACK=33554432

python3 scripts/stage8b_p1f_alor_finam_source_correction_check.py
python3 scripts/stage8b_p1f_alor_finam_compare_rounds_test.py
cargo test -p strategy-runtime-core frozen_baseline07_replay_matches_all_38_alor_rounds
cargo test -p strategy-runtime-core disabled_live_mr_never_claims_owner_and_later_breakout_remains_eligible
cargo test -p strategy-runtime-core same_day_eod_blocks_new_entries_but_preserves_exit
cargo test -p runtime-durable-service canonical_m10_keeps_close_bound_identity_but_uses_start_model_label
cargo test -p runtime-durable-service exact_runtime_profile_builds_real_fingerprint
cargo test -p runtime-durable-service first_boot_start_labels_survive_restart_and_admit_adjacent_canonical_m10
cargo test -p runtime-durable-service stage8b_p1f_o2_systemd::tests:: -- --test-threads=1
cargo test -p runtime-durable-service stage8b_p1f_guardian::tests::custody_policy_has_no_service_uid_write_bit -- --exact --test-threads=1
echo "PASS stage8b-p1f-alor-finam-source-correction-gate"
