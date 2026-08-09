#!/usr/bin/env python3
"""Static and provenance checks for the bounded Stage 6A schema slice."""

from __future__ import annotations

import hashlib
import json
import subprocess
from pathlib import Path

BASE = "14359aadb3178c83692441b748b060d06ce12903"
BRANCH = "stage6-durable-chain"
MODULE = Path("crates/strategy-runtime-core/src/stage6_durable_identity.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
DESCRIPTOR = Path("docs/stage-6/stage6a-schema-descriptor.json")
INVENTORY = Path("docs/stage-6/stage6a-direct-authority-inventory.json")
GOLDEN = Path("docs/stage-6/stage6a-golden-manifest.json")
TRANSITION_INVENTORY = Path("docs/stage-6/transition-5-to-6-authority-inventory.json")

REQUIRED_SOURCE = (
    "STAGE6_DURABLE_RECORD_SCHEMA_VERSION: u16 = 1", "Stage6DurableRequestIdentityV1",
    "Stage6DurableCommandSnapshotV1", "Stage6JournalRecordV1", "Stage6JournalRecordId",
    "Stage6LifecycleSequence", "Stage6Sha256Digest", "Stage6JournalEventKind",
    "ClientOrderId::from_strategy_request", "stage6-journal-record-v1", "to_be_bytes()",
    "canonical_payload_sha256", "source_evidence_sha256", "target_broker_order_id",
    "target_order_client_order_id", "durable_cancel_client_order_id", "RequestAccepted",
    "DispatchAttemptRecorded", "BrokerOrderObserved", "BrokerTradeObserved",
    "CancelOutcomeObserved", "ReconciliationObserved", "RequestFinalized", "ConflictObserved",
    "fn validate_self", "fn validate_intrinsic", "fn validate_snapshot_identity",
    "fn event_matches_payload", "fn encode_canonical", "fn decode_canonical",
    "Stage6JournalRecordWireV1", "Stage6DurableRequestIdentityWireV1",
    "stage6a_place_and_cancel_records_match_exact_golden_bytes",
)
FORBIDDEN_SOURCE = (
    "std::fs", "File::", "OpenOptions", "rusqlite", "redis::", "reqwest", "broker_finam",
    "finam_gateway", "TcpStream", "TcpListener", "tokio::net", "XREADGROUP", "XAUTOCLAIM",
    "Method::POST", "Method::DELETE", ".post(", ".delete(", "std::thread::spawn",
    "tokio::spawn", "sleep(", "ReplaceOrder", "StopLoss", "TakeProfit", "SLTP", "Bracket",
    "dispatch_command", "runtime_callback", "FinamOrder", "HashMap", "SystemTime::now",
)

class CheckFailure(ValueError): pass

def require(value: bool, message: str) -> None:
    if not value: raise CheckFailure(message)

def sha(path: Path) -> str: return hashlib.sha256(path.read_bytes()).hexdigest()

def validate_descriptor(value: dict) -> None:
    require(value.get("schema_version") == 1, "descriptor schema drift")
    require(value.get("stage") == "6A", "stage drift")
    require(value.get("status") == "implementation_candidate", "status drift")
    require(value.get("accepted_predecessor") == BASE, "predecessor drift")
    require(value.get("required_branch") == BRANCH, "branch drift")
    require(value.get("durable_record_schema_version") == 1, "record schema drift")
    require(value.get("positive_test_count") == 24, "positive count drift")
    require(value.get("negative_case_minimum") == 80, "negative count drift")
    require(value.get("logical_record_id_includes_payload_digest") is False, "payload entered logical ID")
    require(value.get("cancel_request_identity_separate_from_target_client_identity") is True, "cancel identities collapsed")
    require(value.get("stage6a_status") == "open_pending_independent_acceptance", "Stage 6A status drift")
    require(value.get("stage6b_plus_open") is False, "Stage 6B+ opened")
    require(value.get("closed_surfaces") and not any(value["closed_surfaces"].values()), "closed surface opened")

def validate_source(source: str) -> None:
    for token in REQUIRED_SOURCE: require(token in source, f"required source token absent: {token}")
    for token in FORBIDDEN_SOURCE: require(token not in source, f"forbidden source token: {token}")
    require(source.count("fn stage6a_") == 24, "positive test count drift")
    require("pub struct Stage6DurableCommandSnapshotV1 {" in source, "snapshot is not opaque")
    require("pub enum Stage6DurableCommandSnapshotV1" not in source, "snapshot variants exposed")
    require("impl<'de> Deserialize<'de> for Stage6JournalRecordV1" in source, "validated record decode absent")

def validate_inventory(root: Path, value: dict) -> None:
    require(value.get("schema_version") == 1 and value.get("accepted_predecessor") == BASE, "inventory header drift")
    transition = value.get("accepted_transition_inventory", {})
    require(transition.get("path") == str(TRANSITION_INVENTORY), "transition inventory path drift")
    require(transition.get("sha256") == sha(root / TRANSITION_INVENTORY), "transition inventory SHA drift")
    accepted = json.loads((root / TRANSITION_INVENTORY).read_text())
    require(transition.get("authority_count") == len(accepted.get("authorities", [])) == 17, "authority count drift")
    for item in accepted["authorities"]:
        require(sha(root / item["path"]) == item["sha256"], f"accepted authority drift: {item['path']}")
    direct = value.get("direct_schema_authorities", [])
    require(len(direct) == 3, "direct authority count drift")
    for item in direct: require(sha(root / item["path"]) == item["sha256"], f"direct authority drift: {item['path']}")

def validate_golden(root: Path, value: dict) -> None:
    require(value.get("schema_version") == 1, "golden schema drift")
    fixtures = value.get("fixtures", [])
    require(len(fixtures) == 2, "golden fixture count drift")
    for item in fixtures:
        target = root / item["path"]
        require(sha(target) == item["sha256"], f"golden SHA drift: {target}")
        parsed = json.loads(target.read_text())
        require(parsed.get("schema_version") == 1, "golden record schema drift")
        require(parsed.get("event_kind") == "request_accepted", "golden event drift")

def check(root: Path) -> None:
    require(subprocess.check_output(["git", "branch", "--show-current"], cwd=root, text=True).strip() == BRANCH, "wrong branch")
    validate_descriptor(json.loads((root / DESCRIPTOR).read_text()))
    validate_source((root / MODULE).read_text())
    validate_inventory(root, json.loads((root / INVENTORY).read_text()))
    validate_golden(root, json.loads((root / GOLDEN).read_text()))
    lib = (root / LIB).read_text()
    require("mod stage6_durable_identity;" in lib and "pub use stage6_durable_identity::{" in lib, "minimal lib linkage absent")
    print("stage6a-check: PASS positive=24 authorities=20 golden=2")

def main() -> None:
    try: check(Path.cwd().resolve())
    except CheckFailure as error: raise SystemExit(f"stage6a-check: FAIL: {error}") from error

if __name__ == "__main__": main()
