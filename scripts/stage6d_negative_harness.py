#!/usr/bin/env python3
"""Named live-critical negative assertions for Stage 6D."""
from __future__ import annotations

import subprocess
from pathlib import Path

import stage6d_check as checker


def main() -> None:
    root = Path.cwd().resolve()
    source = (root / checker.CORE).read_text()
    production = source.split("#[cfg(test)]", 1)[0]
    cases: list[tuple[str, bool]] = []

    required = {
        "missing-journal-failclosed": "RestartJournalMissing",
        "first-boot-linear-authority": "Stage6dFirstBootAuthorization",
        "first-boot-config-binding": "FirstBootRuntimeConfigMismatch",
        "empty-first-boot-journal": "FirstBootJournalNotEmpty",
        "restart-package-canonical": "RestartPackageNonCanonical",
        "stage5-package-digest": "Stage5gPackageDigestMismatch",
        "checkpoint-digest": "CheckpointDigestMismatch",
        "operational-identity-digest": "operational_identity_sha256",
        "restart-commitment": "RestartCommitmentMismatch",
        "restart-hmac": "RestartAuthenticationFailed",
        "checkpoint-prefix-validation": "validate_checkpoint(&authenticated_checkpoint)",
        "full-replay-after-open": "Stage6ReplayEngineV1::replay(journal.records())",
        "single-recovered-authority": "Stage6dDurableRuntimeRecovered",
        "accepted-record-required": "AcceptedRecordRequired",
        "dispatch-record-required": "DispatchAttemptRecordRequired",
        "durable-ordering-violation": "DurableOrderingViolation",
        "typed-paper-truth": "Stage6dAcceptedBrokerTruth",
        "no-caller-evidence-digest": "accepted_paper_evidence(&receipt.identity, &outcome)",
        "place-order-found": "PlaceBrokerOrderFound",
        "place-no-order": "PlaceNoBrokerOrderFound",
        "place-inconclusive": "Inconclusive",
        "cancel-canceled": "CancelCanceled",
        "cancel-execution-observed": "CancelExecutionObserved",
        "cancel-rejected": "CancelRejected",
        "cancel-already-terminal": "CancelAlreadyTerminalNonExecution",
        "cancel-target-required": "target_broker_order_id()",
        "stage5-reviewed-identity": "stage5g_review_operational_identity_for_stage6d",
        "stage5-operational-authority": "authorize_stage5g_fresh_truth_operational_identity",
        "stage5-package-validation": "validate_stage5g_fresh_broker_truth_package",
        "stage5-restart-binding": "bind_stage5g_fresh_truth_to_clean_restart",
        "stage5-owning-reducer": "reduce_stage5g_fresh_broker_truth",
        "stage5-authenticated-application": "apply_stage5g_fresh_truth_reduction",
        "stage5-applied-classification": "Stage5gFreshTruthApplicationResult::Applied",
        "stage5-noop-classification": "Stage5gFreshTruthApplicationResult::Continued",
        "stage5-blocked-classification": "Stage5gFreshTruthApplicationResult::Blocked",
        "request-identity-cross-binding": "stage6d_match_restart_request",
        "broker-order-cross-binding": "stage6d_validate_replayed_facts_against_truth",
        "stage5-runtime-pre-fingerprint": "runtime_pre_fingerprint_sha256",
        "stage5-runtime-post-fingerprint": "runtime_post_fingerprint_sha256",
        "stage6-replay-fingerprint": "stage6_replay_fingerprint_sha256",
        "journal-frontier-fingerprint": "journal_frontier_sha256",
        "restart-recovery-marker": "restart_recovery_marker",
        "ndjson-evidence": "to_ndjson_line",
    }
    for name, token in required.items():
        cases.append((name, token in source))

    forbidden = {
        "no-redis-crate": "redis::",
        "no-redis-readgroup": "XREADGROUP",
        "no-redis-autoclaim": "XAUTOCLAIM",
        "no-reqwest": "reqwest",
        "no-finam-client": "broker_finam",
        "no-finam-gateway": "finam_gateway",
        "no-http-post": "Method::POST",
        "no-http-delete": "Method::DELETE",
        "no-post-builder": ".post(",
        "no-delete-builder": ".delete(",
        "no-file-journal": "Stage6FileJournalBackend",
        "no-filesystem-open": "OpenOptions",
        "no-tcp": "TcpStream",
        "no-tokio-spawn": "tokio::spawn",
        "no-thread-spawn": "std::thread::spawn",
        "no-native-stop": "NativeStopOrder",
        "no-protective-payload": "ProtectiveOrderPayload",
        "no-raw-status-field": "raw_status",
        "no-public-evidence-digest": "pub source_evidence",
    }
    for name, token in forbidden.items():
        cases.append((name, token not in production))

    ordering = {
        "missing-before-decode": source.index("RestartJournalMissing")
        < source.index("decode_and_authenticate_restart_package(authenticated_restart_package"),
        "stage5-digest-before-hmac": source.index("Stage5gPackageDigestMismatch")
        < source.index("stage6d_verify_hmac_sha256"),
        "checkpoint-digest-before-hmac": source.index("CheckpointDigestMismatch")
        < source.index("stage6d_verify_hmac_sha256"),
        "operational-digest-before-hmac": source.index("OperationalIdentityInvalid")
        < source.rindex("stage6d_verify_hmac_sha256"),
        "accepted-before-dispatch-append": source.index("append(&accepted)")
        < source.index("append(&dispatch_attempt)"),
        "dispatch-before-paper-capability": source.index("append(&dispatch_attempt)")
        < source.index("Ok(Stage6dPaperDispatchReceipt"),
        "typed-truth-before-outcome-records": source.index("let accepted_truth = Stage6dAcceptedBrokerTruth")
        < source.index("for record in records"),
        "stage5-validation-before-reducer": source.index("validate_stage5g_fresh_broker_truth_package")
        < source.index("reduce_stage5g_fresh_broker_truth(*restart, bound)"),
    }
    cases.extend(ordering.items())

    witnesses = (
        "stage6d_first_boot_requires_explicit_create_authority",
        "stage6d_first_boot_rejects_runtime_config_drift",
        "stage6d_restart_missing_journal_fails_before_package_decode",
        "stage6d_restart_wrapper_wrong_key_fails_closed",
        "stage6d_restart_wrapper_stage5_bytes_tamper_fails_closed",
        "stage6d_restart_wrapper_checkpoint_tamper_fails_closed",
        "stage6d_restart_wrapper_operational_identity_tamper_fails_closed",
        "stage6d_dispatch_ordering_rejects_non_dispatch_second_record",
        "stage6d_cancel_rejects_generic_place_outcome",
        "stage6d_d1_restart_after_accepted_is_ready_for_first_dispatch",
        "stage6d_d2_restart_after_dispatch_requires_reconciliation",
        "stage6d_d3_lost_place_response_recovers_broker_order_and_forbids_dispatch",
        "stage6d_place_no_order_enables_same_identity_retry",
        "stage6d_d5_trade_in_valid_suffix_is_preserved_once",
        "stage6d_d6_cancel_response_lost_restarts_unresolved_without_redispatch",
        "stage6d_d7_cancel_execution_observed_survives_restart",
        "stage6d_d8_checkpoint_ahead_of_journal_fails_closed",
        "stage6d_same_length_checkpoint_hash_mismatch_fails_closed",
        "stage6d_d9_longer_valid_suffix_is_accepted_deterministically",
        "stage6d_restart_truth_rejects_stage6_request_identity_drift",
        "stage6d_restart_truth_uses_accepted_stage5g_application_boundary_once",
        "stage6d_already_applied_terminal_truth_is_noop_through_stage5g",
    )
    for witness in witnesses:
        cases.append((f"rust-witness-{witness.removeprefix('stage6d_')}", f"fn {witness}(" in source))

    for path in checker.UNCHANGED_FROM_BASE:
        current = (root / path).read_bytes()
        accepted = subprocess.check_output(["git", "show", f"{checker.BASE}:{path}"])
        cases.append((f"accepted-bytes-{Path(path).name}", current == accepted))

    names = [name for name, _ in cases]
    if len(names) != len(set(names)):
        raise SystemExit("stage6d-negative: FAIL duplicate case name")
    failed = [name for name, passed in cases if not passed]
    for name, passed in cases:
        print(f"{'PASS' if passed else 'FAIL'} {name}")
    if len(cases) < 72 or failed:
        raise SystemExit(
            f"stage6d-negative: FAIL passed={len(cases)-len(failed)} total={len(cases)} failed={','.join(failed)}"
        )
    print(f"stage6d-negative: PASS {len(cases)}/{len(cases)}")


if __name__ == "__main__":
    main()
