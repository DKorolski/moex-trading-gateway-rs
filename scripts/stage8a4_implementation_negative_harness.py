#!/usr/bin/env python3
"""Run 40 inherited plus 10 R2 fail-closed Stage 8A-4 mutations."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage8a4_implementation_check as checker


def replacement(path: Path, old: str, new: str):
    def mutate(root: Path) -> None:
        target = root / path
        text = target.read_text()
        if old not in text:
            raise RuntimeError(f"mutation source absent: {path}: {old}")
        target.write_text(text.replace(old, new, 1))
    return mutate


def json_value(path: Path, keys: tuple[str, ...], value: object):
    def mutate(root: Path) -> None:
        target = root / path
        data = json.loads(target.read_text())
        node = data
        for key in keys[:-1]:
            node = node[key]
        node[keys[-1]] = value
        target.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
    return mutate


def main() -> None:
    a, s, l = checker.AUTHORITY, checker.SOURCE, checker.LIB
    mutations = [
        ("accepted-design-ref", json_value(a, ("accepted_design_ref",), "0" * 40)),
        ("review-hash", json_value(a, ("accepted_design_review_sha256",), "0" * 64)),
        ("forged-accepted", json_value(a, ("status",), "accepted")),
        ("truth-type", json_value(a, ("canonical_truth_type",), "serde_json::Value")),
        ("implementation-kind", json_value(a, ("implementation_kind",), "transport")),
        ("public-constructors", json_value(a, ("public_input_constructors",), True)),
        ("typed-completeness", json_value(a, ("source_completeness_encoded_as_types",), False)),
        ("exact-replaces-safety", json_value(a, ("exact_order_lookup_replaces_account_safety",), True)),
        ("deterministic-split", json_value(a, ("deterministic_bounded_interval_split",), False)),
        ("trade-dedup", json_value(a, ("broker_trade_id_deduplication",), False)),
        ("orthogonal-state", json_value(a, ("orthogonal_lifecycle_and_fill",), False)),
        ("proven-no-match", json_value(a, ("proven_no_match_available",), True)),
        ("retry-authority", json_value(a, ("retry_authority_available",), True)),
        ("send-authority", json_value(a, ("send_authority_available",), True)),
        ("test-count", json_value(a, ("focused_test_count",), 27)),
        ("compile-fail-count", json_value(a, ("compile_fail_doctest_count",), 2)),
        ("durable-apply", json_value(a, ("closed", "durable_apply_or_journal_bridge"), False)),
        ("ack-publish", json_value(a, ("closed", "ack_or_readiness_publication"), False)),
        ("redis-live", json_value(a, ("closed", "redis_live_consumer"), False)),
        ("dispatch", json_value(a, ("closed", "broker_dispatch"), False)),
        ("finam-send", json_value(a, ("closed", "finam_post_delete"), False)),
        ("resend", json_value(a, ("closed", "same_request_retry_or_resend"), False)),
        ("runtime-live", json_value(a, ("closed", "runtime_live"), False)),
        ("real-orders", json_value(a, ("closed", "real_orders"), False)),
        ("stage8a5", json_value(a, ("closed", "stage8a5"), False)),
        ("stage8b", json_value(a, ("closed", "stage8b"), False)),
        ("module-public", replacement(l, "mod stage8a4_reconciliation;", "pub mod stage8a4_reconciliation;")),
        ("durable-unbound", replacement(s, "canonical_truth_sha256.as_bytes(),\n            context.durable_binding_sha256.as_bytes(),", "canonical_truth_sha256.as_bytes(),\n            canonical_truth_sha256.as_bytes(),")),
        ("policy-unbound", replacement(s, "context.durable_binding_sha256.as_bytes(),\n            policy.policy_binding_sha256.as_bytes(),", "context.durable_binding_sha256.as_bytes(),\n            context.durable_binding_sha256.as_bytes(),")),
        ("saturation-weakened", replacement(s, "returned_count >= interval.requested_limit", "returned_count > interval.requested_limit")),
        ("trade-conflict-weakened", replacement(s, "Stage8a4ReconciliationReason::TradeIdentityConflict", "Stage8a4ReconciliationReason::NoCandidate")),
        ("identity-conflict-weakened", replacement(s, "Stage8a4ReconciliationReason::ExactIdentityDisagreement", "Stage8a4ReconciliationReason::NoCandidate")),
        ("unknown-terminal", replacement(s, "Stage8a4ReconciliationReason::UnknownOrderStatus", "Stage8a4ReconciliationReason::NoCandidate")),
        ("retry-true", replacement(s, "retry_authorized: false", "retry_authorized: true")),
        ("send-true", replacement(s, "send_authorized: false", "send_authorized: true")),
        ("orders-wrapper-removed", replacement(s, "Stage8a4NonPaginatedOrdersSnapshotComplete", "Stage8a4GenericOrdersEvidence")),
        ("split-depth-removed", replacement(s, "interval.split_depth > policy.max_interval_split_depth", "false")),
        ("exact-pushed-to-safety", replacement(s, "values.push(exact);", "values.push(exact); truth.orders.push(exact.clone());")),
        ("network-token", replacement(s, "use std::collections::BTreeMap;", "use std::collections::BTreeMap;\nuse reqwest::Client;")),
        ("historical-authority", replacement(s, "use std::collections::BTreeMap;", "use std::collections::BTreeMap;\nuse crate::CancelBrokerTruthDecision;")),
        ("admitted-durable-binding-removed", replacement(s, "admitted_durable_binding_sha256: String,", "discarded_durable_binding_sha256: String,")),
        ("admitted-policy-binding-removed", replacement(s, "admitted_policy_binding_sha256: String,", "discarded_policy_binding_sha256: String,")),
        ("context-cross-pair-allowed", replacement(s, "admission.admitted_durable_binding_sha256 != context.durable_binding_sha256", "false")),
        ("policy-cross-pair-allowed", replacement(s, "admission.admitted_policy_binding_sha256 != policy.policy_binding_sha256", "false")),
        ("canonical-payload-equality-weakened", replacement(s, "evidence.canonical_truth_payload_sha256 != canonical_truth_sha256", "false")),
        ("exact-get-timing-wrapper-removed", replacement(s, "pub struct Stage8a4ExactOrderObservation", "struct RemovedExactOrderObservation")),
        ("selected-identity-validator-removed", replacement(s, "fn selected_order_identity(", "fn removed_selected_order_identity(")),
        ("trade-secondary-identity-weakened", replacement(s, "if broker_conflict\n        || client_conflict", "if false\n        || false")),
        ("raw-trade-representative-hashed", replacement(s, "b\"stage8a4-deduplicated-material-trades-v2\"", "b\"stage8a4-raw-representative-trades\"")),
        ("non-exact-source-binding-removed", replacement(s, "admission.truth_binding_sha256.as_bytes(),\n            admission.source_evidence_binding_sha256.as_bytes(),", "admission.truth_binding_sha256.as_bytes(),\n            admission.truth_binding_sha256.as_bytes(),")),
    ]
    if len(mutations) != 50:
        raise SystemExit("stage8a4-implementation-negative: FAIL inventory count")

    with tempfile.TemporaryDirectory(prefix="stage8a4-implementation-negative-") as raw:
        root = Path(raw) / "repo"
        shutil.copytree(
            checker.ROOT, root,
            ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports", "__pycache__"),
        )
        originals: dict[Path, str] = {}
        for name, mutate in mutations:
            for path in (a, s, l):
                target = root / path
                originals[path] = target.read_text()
            mutate(root)
            try:
                checker.check(root, git_scope=False)
            except checker.CheckFailure:
                print(f"PASS {name}")
            else:
                raise SystemExit(f"stage8a4-implementation-negative: FAIL survived={name}")
            finally:
                for path, text in originals.items():
                    (root / path).write_text(text)
    print("stage8a4-implementation-negative: PASS 50/50")


if __name__ == "__main__":
    main()
