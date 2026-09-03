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
        ("same-bar-check-removed", replace_all("provider", "Stage8bP1d1ExecutionEligibilityBlockReason::SameBar", "Stage8bP1d1ExecutionEligibilityBlockReason::InvalidChronology")),
        ("gap-check-removed", replace_all("provider", "Stage8bP1d1ExecutionEligibilityBlockReason::ExecutionBarGap", "Stage8bP1d1ExecutionEligibilityBlockReason::ScheduleRejected")),
        ("ttl-opened", replace_all("provider", "Stage8bP1d1ProviderError::TtlForbidden", "Stage8bP1d1ProviderError::UnsupportedCommand")),
        ("fill-price-drift", replace("provider", "fill_price: eligibility.execution_bar.open", "fill_price: Decimal::ZERO")),
        ("fill-time-drift", replace("provider", "fill_source_ts_utc_ms: eligibility.execution_bar.open_ts_utc_ms", "fill_source_ts_utc_ms: eligibility.execution_bar.close_ts_utc_ms")),
        ("order-domain-drift", replace("provider", "moex.stage8b.p1d.order-id.v1", "moex.stage8b.p1d.order-id.v2")),
        ("trade-domain-drift", replace("provider", "moex.stage8b.p1d.trade-id.v1", "moex.stage8b.p1d.trade-id.v2")),
        ("dispatch-binding-removed", replace("dispatch", "pub(crate) fn stage8b_p1d1_identity", "pub(crate) fn removed_stage8b_p1d1_identity")),
        ("second-schedule-bridge", lambda content: content.__setitem__("schedule", content["schedule"] + "\npub(crate) fn classify_stage8b_p1d1_execution_bar() {}\n")),
        ("canonical-adapter-removed", replace_all("canonical", "to_stage8b_p1d1_execution_bar", "removed_p1d1_execution_bar")),
        ("p1c-observation-removed", replace_all("redis", "p1d1_execution_bar_observation", "removed_execution_bar_observation")),
        ("matrix-row-removed", lambda content: content.__setitem__("matrix", "\n".join(content["matrix"].splitlines()[:-1]) + "\n")),
        ("matrix-weakened", replace("matrix", ",REQUIRED", ",OPTIONAL")),
        ("lineage-drift", replace("evidence", checker.BASE, "f" * 40)),
        ("db0-opened", replace("evidence", '"operational_redis_db0": false', '"operational_redis_db0": true')),
        ("ack-opened", replace("evidence", '"ack_settlement": false', '"ack_settlement": true')),
        ("next-stage-opened", replace("evidence", '"next_stage_authorized": false', '"next_stage_authorized": true')),
    ]
    for name, mutate in cases:
        rejected(name, mutate)
    print(f"PASS stage8b-p1d1-negative-harness {len(cases)}/{len(cases)}")


if __name__ == "__main__":
    main()
