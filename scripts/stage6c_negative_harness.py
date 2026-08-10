#!/usr/bin/env python3
"""Named adversarial mutation matrix for Stage 6C."""
from __future__ import annotations

import copy
import json
from pathlib import Path

import stage6c_check as checker

ROOT = Path(__file__).resolve().parents[1]


def rejected(name, action):
    try:
        action()
    except (checker.CheckFailure, KeyError, ValueError, json.JSONDecodeError):
        print(f"PASS {name}")
    else:
        raise SystemExit(f"stage6c-negative: FAIL accepted mutation: {name}")


def main():
    count = 0
    descriptor = json.loads((ROOT / checker.DESCRIPTOR).read_text())
    mutations = [
        ("schema_version", 2), ("stage", "6D"), ("status", "accepted"),
        ("accepted_stage6b_ref", "0" * 40), ("required_branch", "main"),
        ("durable_record_schema_version", 2), ("replay_schema_version", 2),
        ("replay_fingerprint_domain", "changed"), ("positive_test_count", 53),
        ("crash_window_test_count", 9), ("negative_case_minimum", 95),
        ("duplicate_policy", "last_write_wins"), ("sequence_policy", "timestamp_order"),
        ("causal_parent_policy", "allow_future"), ("blind_redispatch_blocked", False),
        ("retry_authority", "timeout"), ("broker_ids_are_opaque", False),
        ("canonical_collections", "hash_map"), ("stage6c_status", "closed"),
        ("stage6d_plus_open", True),
    ]
    for field, value in mutations:
        candidate = copy.deepcopy(descriptor)
        candidate[field] = value
        rejected(f"descriptor-{field}", lambda c=candidate: checker.validate_descriptor(c))
        count += 1
    for surface in descriptor["closed_surfaces"]:
        candidate = copy.deepcopy(descriptor)
        candidate["closed_surfaces"][surface] = True
        rejected(f"open-{surface}", lambda c=candidate: checker.validate_descriptor(c))
        count += 1

    identity = (ROOT / checker.IDENTITY).read_text()
    for index, token in enumerate(checker.REQUIRED_IDENTITY):
        candidate = identity.replace(token, f"REMOVED_STAGE6C_IDENTITY_{index}")
        rejected(f"identity-required-{index:02d}", lambda c=candidate: checker.validate_identity(c))
        count += 1
    identity_semantic = [
        ("dispatch-ordinal-zero-admitted", identity.replace("attempt_ordinal == 0", "false", 1)),
        ("cancel-action-unbound", identity.replace("identity.action() != Stage6DurableActionKind::Cancel", "false", 1)),
        ("cancel-target-unbound", identity.replace("identity.target_broker_order_id() != Some(&target_broker_order_id)", "false", 1)),
        ("conflict-digest-pair-unbound", identity.replace("expected_digest.is_some() != observed_digest.is_some()", "false", 1)),
        ("payload-made-public", identity.replace("pub(crate) enum Stage6JournalPayloadV1", "pub enum Stage6JournalPayloadV1", 1)),
    ]
    for name, candidate in identity_semantic:
        rejected(name, lambda c=candidate: checker.validate_identity(c))
        count += 1

    replay = (ROOT / checker.REPLAY).read_text()
    for index, token in enumerate(checker.REQUIRED_REPLAY):
        candidate = replay.replace(token, f"REMOVED_STAGE6C_REPLAY_{index}")
        rejected(f"replay-required-{index:02d}", lambda c=candidate: checker.validate_replay(c))
        count += 1
    marker = "#[cfg(test)]\nmod tests"
    for index, token in enumerate(checker.FORBIDDEN_REPLAY_PRODUCTION):
        candidate = replay.replace(marker, token + "\n" + marker, 1)
        rejected(f"forbidden-production-{index:02d}", lambda c=candidate: checker.validate_replay(c))
        count += 1
    replay_semantic = [
        ("exact-duplicate-not-idempotent", replay.replace("previous == &canonical", "false", 1)),
        ("conflicting-duplicate-last-write-wins", replay.replace("return Err(Stage6ReplayError::ConflictingReplay);", "continue;", 1)),
        ("causal-parent-future-admitted", replay.replace("return Err(Stage6ReplayError::CausalParentMissing);", "continue;", 1)),
        ("first-event-not-accepted", replay.replace("record.event_kind() != Stage6JournalEventKind::RequestAccepted", "false", 1)),
        ("first-sequence-not-one", replay.replace("record.lifecycle_sequence().get() != 1", "false", 1)),
        ("first-previous-admitted", replay.replace("record.previous_record_id().is_some()", "false", 1)),
        ("sequence-gap-admitted", replay.replace("record.lifecycle_sequence().get() != self.last_sequence + 1", "false", 1)),
        ("wrong-previous-admitted", replay.replace("record.previous_record_id() != Some(&self.last_record_id)", "false", 1)),
        ("identity-drift-admitted", replay.replace("self.validate_identity(record.durable_request_identity())?;", "", 1)),
        ("accepted-payload-digest-unbound", replay.replace("accepted_request_payload_sha256 != &self.accepted_payload_sha256", "false", 1)),
        ("attempt-ordinal-gap-admitted", replay.replace("attempt_ordinal != self.dispatch_attempt_count + 1", "false", 1)),
        ("blind-redispatch-admitted", replay.replace("return Err(Stage6ReplayError::BlindRedispatchBlocked);", "return Ok(());", 1)),
        ("retry-before-no-order", replay.replace("Stage6DispatchSafetyStateV1::RetryEligibleSameIdentity => {}", "Stage6DispatchSafetyStateV1::ReconciliationRequired => {}", 1)),
        ("no-order-does-not-authorize-retry", replay.replace("self.dispatch_safety_state = Stage6DispatchSafetyStateV1::RetryEligibleSameIdentity;", "self.dispatch_safety_state = Stage6DispatchSafetyStateV1::ReconciliationRequired;", 1)),
        ("broker-order-conflict-admitted", replay.replace("known| known != broker_order_id", "known| known == broker_order_id", 1)),
        ("trade-conflict-admitted", replay.replace("return Err(Stage6ReplayError::BrokerTradeConflict);", "return Ok(());", 1)),
        ("event-after-finalization-admitted", replay.replace("return Err(Stage6ReplayError::EventAfterFinalization);", "return Ok(());", 1)),
        ("fingerprint-domain-removed", replay.replace("hasher.update(REPLAY_FINGERPRINT_DOMAIN);", "", 1)),
    ]
    for name, candidate in replay_semantic:
        rejected(name, lambda c=candidate: checker.validate_replay(c))
        count += 1

    compatibility = json.loads((ROOT / checker.COMPATIBILITY).read_text())
    for field in ("accepted_stage6a_ref", "accepted_stage6b_ref", "accepted_stage6a_source_pre_extension_sha256"):
        candidate = copy.deepcopy(compatibility)
        candidate[field] = "0" * (40 if field.endswith("ref") else 64)
        rejected(f"compatibility-{field}", lambda c=candidate: checker.validate_compatibility(ROOT, c))
        count += 1
    for index in range(len(compatibility["immutable_files"])):
        candidate = copy.deepcopy(compatibility)
        candidate["immutable_files"][index]["sha256"] = "0" * 64
        rejected(f"compatibility-sha-{index:02d}", lambda c=candidate: checker.validate_compatibility(ROOT, c))
        count += 1

    golden = json.loads((ROOT / checker.GOLDEN).read_text())
    for field in ("fingerprint_domain",):
        candidate = copy.deepcopy(golden)
        candidate[field] = "changed"
        rejected(f"golden-{field}", lambda c=candidate: checker.validate_golden(ROOT, c))
        count += 1
    candidate = copy.deepcopy(golden)
    candidate["fixtures"][0]["sha256"] = "0" * 64
    rejected("golden-fixture-sha", lambda c=candidate: checker.validate_golden(ROOT, c))
    count += 1
    candidate = copy.deepcopy(golden)
    candidate["fixtures"][0]["semantic_fingerprint"] = "0" * 64
    rejected("golden-semantic-fingerprint", lambda c=candidate: checker.validate_golden(ROOT, c))
    count += 1

    if count < 96:
        raise SystemExit(f"stage6c-negative: FAIL only {count} cases")
    print(f"stage6c-negative: PASS {count}/{count}")


if __name__ == "__main__":
    main()
