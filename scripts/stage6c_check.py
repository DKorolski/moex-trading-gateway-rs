#!/usr/bin/env python3
"""Static, semantic, compatibility and golden checks for Stage 6C."""
from __future__ import annotations

import hashlib
import json
import subprocess
from pathlib import Path

BASE = "a4e55c42aac6d2470d6ab874c61c19be1b771b3f"
ACCEPTED_STAGE6B = "f0d5e3912243ba85c6f372722c97e815f254a962"
STAGE6A = "c399e2bc2c7e62cc2116a6eac970058bb47c4a49"
MAIN = "14359aadb3178c83692441b748b060d06ce12903"
BRANCH = "stage6-durable-chain"
IDENTITY = Path("crates/strategy-runtime-core/src/stage6_durable_identity.rs")
REPLAY = Path("crates/strategy-runtime-core/src/stage6_replay.rs")
BACKEND = Path("crates/strategy-runtime-core/src/stage6_journal_backend.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
DESCRIPTOR = Path("docs/stage-6/stage6c-replay-descriptor.json")
COMPATIBILITY = Path("docs/stage-6/stage6c-storage-compatibility-manifest.json")
GOLDEN = Path("docs/stage-6/stage6c-golden-manifest.json")

REQUIRED_IDENTITY = (
    "STAGE6_DURABLE_RECORD_SCHEMA_VERSION: u16 = 1",
    "pub enum Stage6ReconciliationDispositionV1",
    "pub enum Stage6CancelOutcomeV1",
    "pub enum Stage6RequestFinalDispositionV1",
    "pub enum Stage6ConflictKindV1",
    "pub(crate) enum Stage6JournalPayloadV1",
    "DispatchAttemptRecorded {",
    "accepted_request_payload_sha256: Stage6Sha256Digest",
    "CancelOutcomeObserved {",
    "ReconciliationObserved {",
    "RequestFinalized {",
    "ConflictObserved {",
    "pub fn dispatch_attempt_recorded(",
    "pub fn cancel_outcome_observed(",
    "pub fn reconciliation_observed(",
    "pub fn request_finalized(",
    "pub fn conflict_observed(",
    "pub(crate) fn durable_request_identity(",
    "pub(crate) fn event_kind(",
    "pub(crate) fn previous_record_id(",
    "pub(crate) fn causal_parent_id(",
    "pub(crate) fn payload(",
    "InvalidDispatchAttemptOrdinal",
    "InvalidCancelOutcomePayload",
    "InvalidActionEvent",
    "InvalidConflictPayload",
    "fn validate_action_event(",
)

REQUIRED_REPLAY = (
    "STAGE6_REPLAY_SCHEMA_VERSION: u16 = 1",
    'REPLAY_FINGERPRINT_DOMAIN: &[u8] = b"stage6-replay-snapshot-v1"',
    "pub enum Stage6DispatchSafetyStateV1",
    "ReadyForFirstDispatch",
    "ReconciliationRequired",
    "RetryEligibleSameIdentity",
    "DispatchForbidden",
    "pub enum Stage6ReplayError",
    "SequenceStartInvalid",
    "SequenceGap",
    "PreviousRecordMismatch",
    "IdentityDrift",
    "ConflictingReplay",
    "CausalParentMissing",
    "DispatchAttemptInvalid",
    "BlindRedispatchBlocked",
    "BrokerOrderConflict",
    "BrokerTradeConflict",
    "CancelTargetConflict",
    "CancelOutcomeConflict",
    "InvalidActionEvent",
    "EventAfterFinalization",
    "InvalidTransition",
    "pub struct Stage6RecoveredRequestV1",
    "pub struct Stage6ReplaySnapshotV1",
    "pub struct Stage6ReplayEngineV1",
    "BTreeMap::<String, Vec<u8>>::new()",
    "BTreeMap::<String, WorkingRequest>::new()",
    "previous == &canonical",
    "return Err(Stage6ReplayError::ConflictingReplay)",
    "return Err(Stage6ReplayError::CausalParentMissing)",
    "record.event_kind() != Stage6JournalEventKind::RequestAccepted",
    "record.lifecycle_sequence().get() != 1",
    "record.previous_record_id().is_some()",
    "record.lifecycle_sequence().get() != self.last_sequence + 1",
    "record.previous_record_id() != Some(&self.last_record_id)",
    "self.validate_identity(record.durable_request_identity())?",
    "validate_event_for_action(self.identity.action(), record.payload())?",
    "accepted_request_payload_sha256 != &self.accepted_payload_sha256",
    "attempt_ordinal != self.dispatch_attempt_count + 1",
    "return Err(Stage6ReplayError::BlindRedispatchBlocked)",
    "Stage6ReconciliationDispositionV1::NoBrokerOrderFound",
    "self.dispatch_safety_state = Stage6DispatchSafetyStateV1::RetryEligibleSameIdentity",
    "self.establish_broker_order(broker_order_id)?",
    "return Err(Stage6ReplayError::BrokerTradeConflict)",
    "return Err(Stage6ReplayError::EventAfterFinalization)",
    "if self.cancel_outcome.is_some()",
    "return Err(Stage6ReplayError::CancelOutcomeConflict)",
    "self.identity.action() != Stage6DurableActionKind::Place",
    "fn validate_event_for_action(",
    "hasher.update(REPLAY_FINGERPRINT_DOMAIN)",
    "stage6c_memory_backend_records_replay_identically",
    "stage6c_old_stage6a_place_golden_is_byte_identical_and_decodable",
    "stage6c_old_stage6a_cancel_golden_is_byte_identical_and_decodable",
    "stage6c_old_stage6b_one_frame_golden_is_scannable_and_replayable",
)

FORBIDDEN_REPLAY_PRODUCTION = (
    "std::fs", "OpenOptions", "File::", "Stage6FileJournalBackend",
    "redis::", "XREADGROUP", "XAUTOCLAIM", "reqwest", "broker_finam",
    "finam_gateway", "Method::POST", "Method::DELETE", ".post(", ".delete(",
    "std::thread::spawn", "tokio::spawn", "runtime_callback", "dispatch_command",
    "paper_fill", "slippage", "order_book", "NativeStopOrder", "ProtectiveOrderPayload",
)


class CheckFailure(ValueError):
    pass


def require(value: bool, message: str) -> None:
    if not value:
        raise CheckFailure(message)


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def extract_block(source: str, needle: str, start: int = 0) -> str:
    position = source.index(needle, start)
    opening = source.index("{", position)
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[opening:index + 1]
    raise CheckFailure(f"unterminated block: {needle}")


def validate_descriptor(value: dict) -> None:
    require(value.get("schema_version") == 1 and value.get("stage") == "6C", "descriptor header drift")
    require(value.get("status") == "implementation_candidate", "status drift")
    require(value.get("accepted_stage6b_ref") == ACCEPTED_STAGE6B, "Stage 6B ref drift")
    require(value.get("stage6c_r1_predecessor_ref") == BASE, "Stage 6C-R1 predecessor drift")
    require(value.get("required_branch") == BRANCH, "branch drift")
    require(value.get("durable_record_schema_version") == 1, "durable schema drift")
    require(value.get("replay_schema_version") == 1, "replay schema drift")
    require(value.get("replay_fingerprint_domain") == "stage6-replay-snapshot-v1", "fingerprint domain drift")
    require(value.get("positive_test_count") == 73, "positive test count drift")
    require(value.get("stage6c_r1_test_count") == 19, "R1 test count drift")
    require(value.get("crash_window_test_count") == 10, "crash-window count drift")
    require(value.get("negative_case_minimum") == 180, "negative minimum drift")
    require(value.get("duplicate_policy") == "exact_canonical_bytes_idempotent_conflict_fail_closed", "duplicate policy drift")
    require(value.get("sequence_policy") == "per_request_contiguous_with_exact_previous_record", "sequence policy drift")
    require(value.get("causal_parent_policy") == "must_appear_earlier_in_physical_history", "causal policy drift")
    require(value.get("blind_redispatch_blocked") is True, "blind redispatch opened")
    require(value.get("retry_authority") == "place_only_authoritative_no_broker_order_found_same_identity", "retry authority drift")
    require(value.get("cancel_outcome_policy") == "first_unique_outcome_authoritative_later_unique_outcome_fail_closed", "cancel outcome policy drift")
    require(value.get("cancel_retry_policy") == "generic_reconciliation_never_authorizes_cancel_retry", "cancel retry policy drift")
    require(value.get("action_event_policy") == "place_and_cancel_event_matrix_fail_closed", "action/event policy drift")
    require(value.get("broker_ids_are_opaque") is True, "broker ID policy drift")
    require(value.get("canonical_collections") == "btree_map_and_ordered_vectors", "canonical collection drift")
    require(value.get("stage6c_status") == "r1_open_pending_independent_acceptance", "Stage 6C status drift")
    require(value.get("stage6d_plus_open") is False, "Stage 6D+ opened")
    require(value.get("closed_surfaces") and not any(value["closed_surfaces"].values()), "closed surface opened")


def validate_identity(source: str) -> None:
    for token in REQUIRED_IDENTITY:
        require(token in source, f"required identity token absent: {token}")
    require(source.count("pub(crate) enum Stage6JournalPayloadV1") == 1, "payload visibility drift")
    require("pub enum Stage6JournalPayloadV1" not in source, "payload became public")
    dispatch = extract_block(source, "pub fn dispatch_attempt_recorded(")
    require("attempt_ordinal == 0" in dispatch and "InvalidDispatchAttemptOrdinal" in dispatch, "dispatch ordinal validation drift")
    cancel = extract_block(source, "pub fn cancel_outcome_observed(")
    require(
        "identity.action() != Stage6DurableActionKind::Cancel" in cancel
        and "identity.target_broker_order_id() != Some(&target_broker_order_id)" in cancel,
        "cancel target binding drift",
    )
    conflict = extract_block(source, "pub fn conflict_observed(")
    require("expected_digest.is_some() != observed_digest.is_some()" in conflict, "conflict digest pairing drift")
    require(
        "matches!(self.payload, Stage6JournalPayloadV1::Marker)" in source
        and "UnsupportedEventPayload" in source,
        "reserved marker rejection drift",
    )
    action_event = extract_block(source, "fn validate_action_event(")
    for token in (
        "Stage6DurableActionKind::Place",
        "Stage6DurableActionKind::Cancel",
        "Stage6JournalPayloadV1::BrokerOrderObserved",
        "Stage6JournalPayloadV1::BrokerTradeObserved",
        "Stage6ReconciliationDispositionV1::Inconclusive",
        "Stage6DurableIdentityError::InvalidActionEvent",
    ):
        require(token in action_event, f"identity action/event guard drift: {token}")
    broker_order = extract_block(source, "pub fn broker_order_observed(")
    broker_trade = extract_block(source, "pub fn broker_trade_observed(")
    reconciliation = extract_block(source, "pub fn reconciliation_observed(")
    require(
        "identity.action() != Stage6DurableActionKind::Place" in broker_order
        and "Stage6DurableIdentityError::InvalidActionEvent" in broker_order,
        "broker order action guard drift",
    )
    require(
        "identity.action() != Stage6DurableActionKind::Place" in broker_trade
        and "Stage6DurableIdentityError::InvalidActionEvent" in broker_trade,
        "broker trade action guard drift",
    )
    require(
        "validate_action_event" in reconciliation and "identity.action()" in reconciliation,
        "reconciliation constructor action guard drift",
    )


def validate_replay(source: str) -> None:
    for token in REQUIRED_REPLAY:
        require(token in source, f"required replay token absent: {token}")
    production = source.split("#[cfg(test)]\nmod tests", 1)[0]
    for token in FORBIDDEN_REPLAY_PRODUCTION:
        require(token not in production, f"forbidden replay production token: {token}")
    require(source.count("fn stage6c_") == 72, "Stage 6C replay test count drift")
    for window in range(1, 11):
        require(f"fn stage6c_cw{window}_" in source, f"crash window CW{window} absent")
    replay = extract_block(source, "pub fn replay(")
    require(replay.index("if let Some(previous) = seen_records.get") < replay.index("WorkingRequest::from_first"), "duplicate classification occurs after transition")
    require(replay.index("causal_parent_id()") < replay.index("WorkingRequest::from_first"), "causal parent checked too late")
    apply = extract_block(source, "fn apply(&mut self, record:")
    require(apply.index("validate_identity") < apply.index("last_sequence + 1") < apply.index("previous_record_id()"), "identity/sequence/previous order drift")
    attempt = extract_block(source, "fn apply_dispatch_attempt(")
    require(
        "Stage6DispatchSafetyStateV1::RetryEligibleSameIdentity" in attempt
        and "if self.identity.action() == Stage6DurableActionKind::Place" in attempt
        and "BlindRedispatchBlocked" in attempt,
        "dispatch safety transition drift",
    )
    reconciliation = extract_block(source, "fn apply_reconciliation(")
    require(
        "NoBrokerOrderFound" in reconciliation
        and "RetryEligibleSameIdentity" in reconciliation
        and reconciliation.count("self.identity.action() != Stage6DurableActionKind::Place") == 2,
        "Place-only authoritative retry transition drift",
    )
    action_event = extract_block(source, "fn validate_event_for_action(")
    require("Stage6ReconciliationDispositionV1::Inconclusive" in action_event, "replay action/event matrix drift")
    require("Stage6ReplayError::InvalidActionEvent" in action_event, "replay action/event rejection drift")
    apply = extract_block(source, "fn apply(&mut self, record:")
    require(
        apply.index("validate_event_for_action") < apply.index("match record.payload()")
        and "if self.cancel_outcome.is_some()" in apply
        and "CancelOutcomeConflict" in apply,
        "cancel monotonicity or pre-mutation action guard drift",
    )
    establish = extract_block(source, "fn establish_broker_order(")
    require("known| known != broker_order_id" in establish, "broker order conflict check drift")
    fingerprint = extract_block(source, "fn replay_fingerprint(")
    require("REPLAY_FINGERPRINT_DOMAIN" in fingerprint and "serde_json::to_vec" in fingerprint, "fingerprint authority drift")


def validate_compatibility(root: Path, value: dict) -> None:
    require(value.get("schema_version") == 1, "compatibility schema drift")
    require(value.get("accepted_stage6a_ref") == STAGE6A, "compatibility Stage 6A ref drift")
    require(value.get("accepted_stage6b_ref") == ACCEPTED_STAGE6B, "compatibility Stage 6B ref drift")
    require(value.get("accepted_stage6a_source_pre_extension_sha256") == "42374c406a9d20df2cc2266e752c0b5722ebe7c7fe2a459741359b0bd1f39fd4", "pre-extension authority drift")
    files = value.get("immutable_files", [])
    require(len(files) == 9, "compatibility file count drift")
    for item in files:
        require(sha(root / item["path"]) == item["sha256"], f"immutable compatibility drift: {item['path']}")


def validate_golden(root: Path, value: dict) -> None:
    require(value.get("schema_version") == 1, "golden schema drift")
    require(value.get("fingerprint_domain") == "stage6-replay-snapshot-v1", "golden domain drift")
    fixtures = value.get("fixtures", [])
    require(len(fixtures) == 1, "golden fixture count drift")
    item = fixtures[0]
    require(sha(root / item["path"]) == item["sha256"], "replay golden file SHA drift")
    require((root / item["path"]).read_text().strip() == item["semantic_fingerprint"], "replay semantic fingerprint drift")


def check(root: Path) -> None:
    branch = subprocess.check_output(["git", "branch", "--show-current"], cwd=root, text=True).strip()
    require(branch == BRANCH, "wrong branch")
    validate_descriptor(json.loads((root / DESCRIPTOR).read_text()))
    validate_identity((root / IDENTITY).read_text())
    validate_replay((root / REPLAY).read_text())
    validate_compatibility(root, json.loads((root / COMPATIBILITY).read_text()))
    validate_golden(root, json.loads((root / GOLDEN).read_text()))
    lib = (root / LIB).read_text()
    require("mod stage6_replay;" in lib and "pub use stage6_replay::{" in lib, "Stage 6C linkage absent")
    require((root / IDENTITY).read_text().count("fn stage6c_r1_") == 1, "identity R1 test count drift")
    print("stage6c-r1-check: PASS positive=73 r1=19 crash_windows=10 compatibility=9 golden=1")


def main() -> None:
    try:
        check(Path.cwd().resolve())
    except CheckFailure as error:
        raise SystemExit(f"stage6c-r1-check: FAIL: {error}") from error


if __name__ == "__main__":
    main()
