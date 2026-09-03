#!/usr/bin/env python3
"""Targeted in-memory negative mutations for the P1-d1 source contract."""

from __future__ import annotations

import copy
import json

import stage8b_p1d1_check as checker


def rejected(name: str, mutate) -> None:
    content = copy.deepcopy(checker.load_content())
    mutate(content)
    try:
        checker.validate_content(content)
    except (checker.CheckFailure, json.JSONDecodeError):
        print(f"PASS {name}")
        return
    raise SystemExit(f"FAIL mutation accepted: {name}")


def replace(key: str, old: str, new: str):
    def mutate(content: dict[str, str]) -> None:
        if old not in content[key]:
            raise SystemExit(f"harness source token missing: {key}:{old}")
        content[key] = content[key].replace(old, new, 1)

    return mutate


def replace_all(key: str, old: str, new: str):
    def mutate(content: dict[str, str]) -> None:
        if old not in content[key]:
            raise SystemExit(f"harness source token missing: {key}:{old}")
        content[key] = content[key].replace(old, new)

    return mutate


def main() -> None:
    cases = [
        ("public-entry-seam", replace("provider", "pub(crate) fn begin_stage8b", "pub fn begin_stage8b")),
        ("public-eligibility-mint", replace("provider", "pub(crate) fn observe_candidates", "pub fn observe_candidates")),
        ("decision-source-issuer-removed", replace("provider", "pub(crate) fn stage8b_p1d1_command_decision_binding_from_source", "fn removed_decision_source")),
        ("predecessor-redis-close-derivation-removed", replace("provider", "exact_redis_close_ms(&predecessor_redis_id)", "Some(1_788_422_400_000)")),
        ("same-bar-check-removed", replace_all("provider", "Stage8bP1d1ExecutionEligibilityBlockReason::SameBar", "Stage8bP1d1ExecutionEligibilityBlockReason::InvalidChronology")),
        ("gap-check-removed", replace_all("provider", "Stage8bP1d1ExecutionEligibilityBlockReason::ExecutionBarGap", "Stage8bP1d1ExecutionEligibilityBlockReason::ScheduleRejected")),
        ("ttl-opened", replace_all("provider", "Stage8bP1d1ProviderError::TtlForbidden", "Stage8bP1d1ProviderError::UnsupportedCommand")),
        ("fill-price-drift", replace("provider", "fill_price: eligibility.execution_bar.open", "fill_price: Decimal::ZERO")),
        ("fill-time-drift", replace("provider", "fill_source_ts_utc_ms: eligibility.execution_bar.open_ts_utc_ms", "fill_source_ts_utc_ms: eligibility.execution_bar.close_ts_utc_ms")),
        ("order-domain-drift", replace("provider", "moex.stage8b.p1d.order-id.v1", "moex.stage8b.p1d.order-id.v2")),
        ("trade-domain-drift", replace("provider", "moex.stage8b.p1d.trade-id.v1", "moex.stage8b.p1d.trade-id.v2")),
        ("receipt-snapshot-removed", replace("dispatch", "pub struct Stage6dPaperDispatchReceipt {\n    identity: Stage6DurableRequestIdentityV1,\n    command_snapshot: Stage6DurableCommandSnapshotV1,", "pub struct Stage6dPaperDispatchReceipt {\n    identity: Stage6DurableRequestIdentityV1,\n    removed_snapshot: Stage6DurableCommandSnapshotV1,")),
        ("receipt-accepted-digest-removed", replace("dispatch", "pub struct Stage6dPaperDispatchReceipt {\n    identity: Stage6DurableRequestIdentityV1,\n    command_snapshot: Stage6DurableCommandSnapshotV1,\n    accepted_command_payload_sha256: Stage6Sha256Digest,", "pub struct Stage6dPaperDispatchReceipt {\n    identity: Stage6DurableRequestIdentityV1,\n    command_snapshot: Stage6DurableCommandSnapshotV1,\n    removed_digest: Stage6Sha256Digest,")),
        ("combined-dispatch-seam-removed", replace("dispatch", "pub fn admit_stage7a_p1d1_market_dispatch", "fn removed_p1d1_dispatch")),
        ("combined-snapshot-check-removed", replace("dispatch", "accepted_snapshot != eligibility.durable_command_snapshot()", "false")),
        ("combined-digest-check-removed", replace("dispatch", "accepted.canonical_payload_sha256() != eligibility.accepted_command_payload_sha256()", "false")),
        ("stage7-owner-seam-removed", replace("service", "pub fn admit_p1d1_eligible_market_dispatch", "fn removed_p1d1_owner_dispatch")),
        ("p1c-decision-seam-removed", replace_all("redis", "p1d1_command_decision_binding", "removed_p1d1_decision_binding")),
        ("constructible-observation-returned", lambda content: content.__setitem__("provider", content["provider"] + "\npub struct Stage8bP1d1ExecutionBarObservation { pub open: i64 }\n")),
        ("internal-binder-exported", replace("lib", "admit_stage7a_p1d1_market_dispatch", "bind_stage8b_p1d1_market_dispatch")),
        ("second-schedule-bridge", lambda content: content.__setitem__("schedule", content["schedule"] + "\npub(crate) fn classify_stage8b_p1d1_execution_bar() {}\n")),
        ("matrix-row-removed", lambda content: content.__setitem__("matrix", "\n".join(content["matrix"].splitlines()[:-1]) + "\n")),
        ("policy-lineage-drift", replace("evidence", checker.POLICY_BASE, "e" * 40)),
        ("review-lineage-drift", replace("evidence", checker.REVIEW_BASE, "f" * 40)),
        ("db0-opened", replace("evidence", '"operational_redis_db0": false', '"operational_redis_db0": true')),
        ("next-stage-opened", replace("evidence", '"next_stage_authorized": false', '"next_stage_authorized": true')),
    ]
    for name, mutate in cases:
        rejected(name, mutate)
    print(f"PASS stage8b-p1d1-negative-harness {len(cases)}/{len(cases)}")


if __name__ == "__main__":
    main()
