#!/usr/bin/env python3
"""Production-path and governance mutation harness for Stage 6E-R1."""
from __future__ import annotations

import copy
import json
from pathlib import Path

import stage6e_r1_check as checker


def rejected(action) -> bool:
    try:
        action()
    except (checker.CheckFailure, ValueError, KeyError):
        return True
    return False


def main() -> None:
    root = Path.cwd().resolve()
    source = (root / checker.CORE).read_text()
    descriptor = json.loads((root / checker.DESCRIPTOR).read_text())
    current = (root / checker.CURRENT).read_text()
    roadmap = (root / checker.ROADMAP).read_text()
    onboarding = (root / checker.ONBOARDING).read_text()
    cases: list[tuple[str, bool]] = []

    production_mutations = {
        "fingerprint-schema": "STAGE6D_INTEGRATION_FINGERPRINT_SCHEMA_VERSION: u16 = 3",
        "accepted-schema": "STAGE6E_ACCEPTED_FRESH_TRUTH_SCHEMA_VERSION: u16 = 2",
        "fingerprint-domain": "moex.stage6e-r1.durable-runtime-recovered.v3",
        "restore-domain": "moex.stage6e-r1.current-process-restore-epoch.v1",
        "restore-type": "struct Stage6RestoreEpoch",
        "host-epoch-constructor": "fn from_current_host_process()",
        "generation-id": "process_generation_id: Stage6Sha256Digest",
        "restore-time": "restore_completed_at: DateTime<Utc>",
        "recovered-epoch-owner": "restore_epoch: Option<Stage6RestoreEpoch>",
        "fingerprint-epoch-field": "current_process_restore_epoch_sha256",
        "request-scoped-issuer": "pub fn issue_stage6e_paper_fresh_broker_truth_for_request(",
        "request-id-parameter": "request_id: StrategyRequestId",
        "selected-request-validator": "fn stage6d_validate_selected_restart_request(",
        "temporal-validator": "fn validate_stage6e_temporal_authority(",
        "collection-start": "collection_started_at: DateTime<Utc>",
        "trusted-validation": "validation_observed_at: DateTime<Utc>",
        "request-error": "FreshTruthRequestNotCrossBound",
        "temporal-error": "FreshTruthTemporalAuthorityMismatch",
        "accepted-epoch-binding": "restore_epoch_fingerprint_sha256",
        "opaque-capability": "Stage6eAcceptedFreshBrokerTruth",
        "provider-seam": "Stage6eFreshBrokerTruthProviderBoundary",
        "semantic-cross-binding": "stage6e_semantic_cross_bind_restart",
        "source-attribution": "stage5g_attribution_fingerprint_sha256",
        "host-validation-call": "Utc::now()",
        "active-membership": "active_cross_bound_request_ids",
        "stage5-validator": "validate_stage5g_fresh_broker_truth_package",
        "restore-floor": "clean_restore_completed_at: restore_epoch.restore_completed_at",
        "collection-after-restore": "collection_started_at <= restore_completed_at",
        "capture-after-start": "captured_at < input.collection_started_at",
        "capture-before-validation": "captured_at > validation_observed_at",
        "orders-local-time": "orders_observed_at",
        "trades-local-time": "trades_observed_at",
        "positions-local-time": "positions_observed_at",
        "row-after-restore": "row.received_ts <= restore_completed_at",
        "row-before-validation": "row.received_ts > validation_observed_at",
    }
    for name, token in production_mutations.items():
        mutated = source.replace(token, f"MUTATED_{name.replace('-', '_')}")
        cases.append((f"production-{name}", rejected(lambda value=mutated: checker.validate_core(value))))

    for token in checker.FORBIDDEN_PRODUCTION:
        mutated = source.split("#[cfg(test)]", 1)[0] + f"\n// {token}\n#[cfg(test)]" + source.split("#[cfg(test)]", 1)[1]
        cases.append((f"closed-surface-{token}", rejected(lambda value=mutated: checker.validate_core(value))))

    descriptor_mutations = {
        "stage": "6E",
        "status": "accepted",
        "superseded_stage6e_ref": "0" * 40,
        "required_branch": "main",
        "source_ref_bound_by_handoff": False,
        "multi_current_request_issuance": "single",
        "selected_request_must_be_active_cross_bound_member": False,
        "current_process_restore_epoch": False,
        "restore_epoch_loaded_from_restart_package": True,
        "restore_epoch_loaded_from_broker_input": True,
        "trusted_validation_time_source": "broker_input",
        "collection_interval_explicit": False,
        "section_local_observation_times_explicit": False,
        "integration_fingerprint_schema_version": 2,
        "accepted_fresh_truth_schema_version": 1,
        "focused_test_count": 17,
        "negative_case_minimum": 47,
        "stage6a_b_c_compatibility_unchanged": False,
        "stage6_closed_after_independent_acceptance": False,
        "stage7_open_after_independent_acceptance": False,
    }
    for name, replacement in descriptor_mutations.items():
        mutated = copy.deepcopy(descriptor)
        mutated[name] = replacement
        cases.append((f"descriptor-{name}", rejected(lambda value=mutated: checker.validate_descriptor(value))))
    for name in descriptor["closed_surfaces"]:
        mutated = copy.deepcopy(descriptor)
        mutated["closed_surfaces"][name] = True
        cases.append((f"descriptor-open-{name}", rejected(lambda value=mutated: checker.validate_descriptor(value))))

    governance_tokens = (
        "Stage 6A, 6B, 6C and 6C-R1 are independently accepted",
        "Stage 6D is",
        "Stage 6E-R1 review candidate",
        "Stage 7 is CLOSED",
        "Stage 6E-R1 — final durable-chain closure repair",
        "Stage 7 remains closed",
        "active review target is **Stage 6E-R1**",
    )
    for token in governance_tokens:
        mutated_current = current.replace(token, "STALE_STATUS", 1)
        mutated_roadmap = roadmap.replace(token, "STALE_STATUS", 1)
        mutated_onboarding = onboarding.replace(token, "STALE_STATUS", 1)
        cases.append((
            f"governance-{token[:24]}",
            rejected(lambda c=mutated_current, r=mutated_roadmap, o=mutated_onboarding: checker.validate_governance(c, r, o)),
        ))

    names = [name for name, _ in cases]
    if len(names) != len(set(names)):
        raise SystemExit("stage6e-r1-negative: FAIL duplicate case name")
    failed = [name for name, passed in cases if not passed]
    for name, passed in cases:
        print(f"{'PASS' if passed else 'FAIL'} {name}")
    if len(cases) < 48 or failed:
        raise SystemExit(
            f"stage6e-r1-negative: FAIL passed={len(cases)-len(failed)} total={len(cases)} failed={','.join(failed)}"
        )
    print(f"stage6e-r1-negative: PASS {len(cases)}/{len(cases)}")


if __name__ == "__main__":
    main()
