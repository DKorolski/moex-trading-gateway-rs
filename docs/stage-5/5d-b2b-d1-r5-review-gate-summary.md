# Stage 5D-b2b-d1-r5 review gate summary

Status: implementation candidate, no-I/O.

Stage 5D-b2b-d1-r5 closes the two remaining r4 review gaps without changing
the accepted production restored-transition design.

Scope closed by this slice:

- the new broker-position Long/Short positives now traverse strict JSON
  serialization and `Stage5dPersistenceEnvelope::from_json_str_strict` before
  bind/apply/bootstrap/riskgate injection/restored;
- the non-empty known-order and pending-request index positives use the same
  strict round-trip path;
- `is_paper_only == false` is covered as a closed-boundary pre-callback blocker;
- a specific non-acknowledged recovery-decision case is covered as
  `RecoveryIncomplete`;
- every remaining mismatch has an explicit blocker ownership row.

Operational boundary remains closed:

- Redis: disabled;
- FINAM: disabled;
- broker transport: disabled;
- dispatch: disabled;
- runtime-live: disabled;
- broker execution/order submission: disabled.

## Stage 5D-b2b-d1-r5 blocker ownership table

| Case | Representable at b2b-d? | Owning validation stage/function | Focused test or earlier-gate proof | Expected reason | Callback/state/capability invariant |
| --- | --- | --- | --- | --- | --- |
| Incomplete recovery | yes | `validate_stage5d_runtime_state_restored_preflight` | `stage5d_b2bd_incomplete_recovery_blocks_before_callback` and common matrix | `RecoveryIncomplete` | callback 0, retained capability |
| Pending riskgate finalization | yes | `validate_stage5d_runtime_state_restored_preflight` | `pending_finalization` matrix row | `PendingRiskGateFinalizations` | callback 0, fingerprints retained |
| Non-acknowledged recovery decision | yes | `validate_stage5d_runtime_state_restored_preflight` | `non_acknowledged_recovery_decision` matrix row | `RecoveryIncomplete` | callback 0, fingerprints retained |
| Recovery-plan fingerprint mismatch | yes | `validate_stage5d_runtime_state_restored_preflight` | `recovery_plan_fingerprint` matrix row | `RecoveryPlanBindingMismatch` | callback 0, fingerprints retained |
| Known-order / pending-request index mismatch | yes | `validate_stage5d_runtime_state_restored_preflight` | `known_index` and `pending_index` matrix rows | `RecoveryIndexMismatch` | callback 0, fingerprints retained |
| Runtime-host attached | yes | `validate_stage5d_runtime_state_restored_preflight` | `closed_boundary` matrix row | `ClosedBoundaryOpened` | callback 0, retained capability |
| Intent-sink attached | yes | `validate_stage5d_runtime_state_restored_preflight` | `intent_sink_boundary` matrix row | `ClosedBoundaryOpened` | callback 0, retained capability |
| `is_paper_only == false` | yes | `validate_stage5d_runtime_state_restored_preflight` | `not_paper_only_boundary` matrix row | `ClosedBoundaryOpened` | callback 0, retained capability |
| Broker position quantity mismatch | yes | `validate_stage5d_runtime_restore_broker_truth` | `broker_position` matrix row | `BrokerTruthPositionMismatch` | callback 0, retained capability |
| Broker side mismatch while flat/open | yes | `validate_stage5d_runtime_restore_broker_truth` | flat-side and r4 open Long/Short side mismatch tests | `BrokerTruthSideMismatch` | callback 0, retained capability |
| Broker-owned protective IDs | yes | `validate_stage5d_runtime_restore_broker_truth` | TP/SL/exchange-stop matrix rows | `BrokerOwnedProtectiveId` | callback 0, retained capability |
| Strategy/account/instrument binding mismatch | no, owned before b2b-d | `validate_loaded_envelope_binding` and `validate_stage5d_broker_truth_bootstrap` | `stage5d_b2b_public_bind_rejects_account_mismatch_before_apply`, `stage5d_b2b_public_bind_rejects_instrument_mismatch_before_apply`, `stage5d_b2bb_retry_rejects_cross_account_fresh_admission_and_preserves_capability` | `BindingMismatch` / `RecoveryPlanBindingMismatch` if tampered after type-state | no restored callback capability is produced on the normal path |
| Config/profile mismatch | no, owned before b2b-d | `validate_loaded_envelope_binding` | `stage5d_b2b_public_bind_rejects_unsupported_source_build` and config/profile binding tests | `BindingMismatch` | no restored callback capability is produced on the normal path |
| Riskgate identity/generation/evidence mismatch | no, owned before b2b-d | `stage5d_validate_riskgate_ledger_evidence` and `stage5d_authoritative_riskgate_state_from_evidence` | `stage5d_b2bc1_riskgate_generation_must_match_envelope`, identity/generation/outbox mismatch tests | `LedgerGenerationMismatch` / riskgate injection block | no injected capability is produced |
| `BrokerQuantityNotRepresentable` | no practical Stage 5D fixture; decimal broker qty is finite before f64 conversion | `validate_stage5d_broker_truth_bootstrap` and `validate_stage5d_runtime_restore_broker_truth` defensive branch | documented defensive branch; exact lot/decimal normalization remains future pre-live hardening | `BrokerQuantityNotRepresentable` if broker Decimal cannot produce finite f64 | callback 0 by branch ordering |

## Required local gates

```text
python3 scripts/stage5c_api_freeze_check.py
python3 scripts/stage5d_additive_freeze_check.py
bash scripts/forbidden_surface_scan.sh
bash scripts/forbidden_surface_negative_harness.sh
python3 scripts/stage5d_additive_freeze_negative_harness.py
python3 scripts/handoff_provenance_negative_harness.py
bash scripts/test_m4_3x_evidence_no_redis.sh
cargo fmt --all --check
cargo test -p strategy-runtime-core b2bd --lib
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets -- -D warnings
```

