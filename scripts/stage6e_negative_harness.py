#!/usr/bin/env python3
"""Named live-critical negative assertions for Stage 6E."""
from __future__ import annotations

import subprocess
from pathlib import Path

import stage6e_check as checker


def main() -> None:
    root = Path.cwd().resolve()
    source = (root / checker.CORE).read_text()
    production = source.split("#[cfg(test)]", 1)[0]
    cases: list[tuple[str, bool]] = []

    required = {
        "cross-binding-private-proof": "struct Stage6eSemanticCrossBinding",
        "cross-binding-restart-function": "fn stage6e_semantic_cross_bind_restart(",
        "cross-binding-request": "command_request_id",
        "cross-binding-client": "command_client_order_id",
        "cross-binding-account": "projection.account_id",
        "cross-binding-instrument": "projection.instrument_id",
        "cross-binding-strategy": "projection.strategy_id",
        "cross-binding-attribution": "expected_attribution_fingerprint_sha256",
        "cross-binding-action": "expected_action",
        "cross-binding-cancel-target": "expected_cancel_target",
        "cross-binding-cancel-client": "target_order_client_order_id",
        "unmatched-effect-authority-rejected": "request.final_disposition().is_none()",
        "semantic-mismatch-error": "RestartSemanticCrossBindingMismatch",
        "accepted-truth-private-type": "pub struct Stage6eAcceptedFreshBrokerTruth",
        "accepted-truth-binding-error": "AcceptedFreshTruthBindingMismatch",
        "paper-issuer": "pub fn issue_stage6e_paper_fresh_broker_truth(",
        "typed-application": "pub fn apply_stage6e_accepted_fresh_truth(",
        "replay-binding": "stage6_replay_fingerprint_sha256",
        "frontier-binding": "journal_frontier_sha256",
        "checkpoint-binding": "authenticated_checkpoint_sha256",
        "semantic-fingerprint-binding": "semantic_cross_binding_fingerprint_sha256",
        "broker-order-correlation": "known_broker_order_id()",
        "broker-trade-correlation": "observed_broker_trade_ids()",
        "accepted-stage5-validator": "validate_stage5g_fresh_broker_truth_package",
        "accepted-stage5-binding": "bind_stage5g_fresh_truth_to_clean_restart",
        "accepted-stage5-reducer": "reduce_stage5g_fresh_broker_truth",
        "accepted-stage5-application": "apply_stage5g_fresh_truth_reduction",
        "provider-seam": "pub trait Stage6eFreshBrokerTruthProviderBoundary",
        "fingerprint-v2": "moex.stage6e.durable-runtime-recovered.v2",
        "cross-binding-domain": "moex.stage6e.stage5-stage6-semantic-cross-binding.v1",
        "source-attribution-hash": "stage5g_attribution_fingerprint_sha256",
        "raw-application-compile-fail": "fn raw_cannot_apply(",
        "linear-capability-compile-fail": "fn cannot_clone_or_serialize(",
    }
    cases.extend((name, token in source) for name, token in required.items())

    forbidden = {
        "no-old-raw-application": "pub fn apply_stage6d_restart_fresh_truth(",
        "no-public-accepted-fields": "pub validated:",
        "no-accepted-clone": "impl Clone for Stage6eAcceptedFreshBrokerTruth",
        "no-accepted-debug": "impl Debug for Stage6eAcceptedFreshBrokerTruth",
        "no-accepted-serialize": "impl Serialize for Stage6eAcceptedFreshBrokerTruth",
        "no-accepted-deserialize": "impl<'de> Deserialize<'de> for Stage6eAcceptedFreshBrokerTruth",
        "no-accepted-constructor": "pub fn new_stage6e_accepted",
        "no-provider-authorize-method": "fn authorize_provider(",
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
        "no-finam-dto": "FinamOrder",
        "no-broker-status-string": "raw_status",
    }
    cases.extend((name, token not in production) for name, token in forbidden.items())

    recover = checker.extract_block(source, "fn recover_stage6d_restart_from_authorities(")
    issuer = checker.extract_block(source, "pub fn issue_stage6e_paper_fresh_broker_truth(")
    application = checker.extract_block(source, "pub fn apply_stage6e_accepted_fresh_truth(")
    ordering = {
        "checkpoint-before-replay": recover.index("validate_checkpoint") < recover.index("Stage6ReplayEngineV1::replay"),
        "replay-before-cross-binding": recover.index("Stage6ReplayEngineV1::replay") < recover.index("stage6e_semantic_cross_bind_restart"),
        "cross-binding-before-fingerprint": recover.index("stage6e_semantic_cross_bind_restart") < recover.index("integration_fingerprint"),
        "fingerprint-before-recovered-capability": recover.index("integration_fingerprint") < recover.index("Ok(Stage6dDurableRuntimeRecovered"),
        "broker-correlation-before-stage5-validation": issuer.index("stage6d_validate_replayed_facts_against_truth") < issuer.index("validate_stage5g_fresh_broker_truth_package"),
        "stage5-validation-before-accepted-capability": issuer.index("validate_stage5g_fresh_broker_truth_package") < issuer.index("Ok(Stage6eAcceptedFreshBrokerTruth"),
        "accepted-binding-before-stage5-bind": application.index("AcceptedFreshTruthBindingMismatch") < application.index("bind_stage5g_fresh_truth_to_clean_restart"),
        "raw-type-absent-from-application": "Stage6ePaperFreshBrokerTruthInput" not in application,
    }
    cases.extend(ordering.items())

    witnesses = (
        "stage6e_matching_stage5_stage6_pair_is_cross_bound_before_capability",
        "stage6e_account_mismatch_is_rejected_during_restart",
        "stage6e_instrument_mismatch_is_rejected_during_restart",
        "stage6e_attribution_mismatch_is_rejected_during_restart",
        "stage6e_place_cancel_action_mismatch_is_rejected_during_restart",
        "stage6e_exact_cancel_target_is_cross_bound",
        "stage6e_cancel_target_mismatch_is_rejected_during_restart",
        "stage6e_extra_finalized_stage6_history_does_not_need_current_stage5_slot",
        "stage6e_extra_unresolved_stage6_authority_is_rejected",
        "stage6e_integration_and_cross_binding_fingerprints_are_deterministic",
        "stage6e_restart_rejects_stage6_request_identity_drift_before_capability",
        "stage6e_paper_issuer_rejects_known_broker_order_absent_from_fresh_truth",
        "stage6e_paper_issuer_rejects_known_broker_trade_absent_from_fresh_truth",
        "stage6e_accepted_truth_is_bound_to_exact_replay_and_frontier",
    )
    cases.extend((f"rust-witness-{name.removeprefix('stage6e_')}", f"fn {name}(" in source) for name in witnesses)

    for path in checker.UNCHANGED_FROM_BASE:
        current = (root / path).read_bytes()
        accepted = subprocess.check_output(["git", "show", f"{checker.BASE}:{path}"])
        cases.append((f"accepted-bytes-{Path(path).name}", current == accepted))

    names = [name for name, _ in cases]
    if len(names) != len(set(names)):
        raise SystemExit("stage6e-negative: FAIL duplicate case name")
    failed = [name for name, passed in cases if not passed]
    for name, passed in cases:
        print(f"{'PASS' if passed else 'FAIL'} {name}")
    if len(cases) < 48 or failed:
        raise SystemExit(
            f"stage6e-negative: FAIL passed={len(cases)-len(failed)} total={len(cases)} failed={','.join(failed)}"
        )
    print(f"stage6e-negative: PASS {len(cases)}/{len(cases)}")


if __name__ == "__main__":
    main()
