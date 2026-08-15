#!/usr/bin/env python3
"""Fail-closed semantic checker for Stage 8A-4 durable composition I1."""

from __future__ import annotations

import csv
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "dd01253596527d6cff1db11cc32ae3c3348c96a0"
REVIEW_SHA256 = "acb8364ee2100bf64e50522823b1da21093f96c73f93b20b4cdf9e7ac09b58ec"
BRANCH = "stage8a4-durable-composition-i1"

AUTHORITY = Path("docs/stage-8/stage8a4-durable-composition-i1-authority.json")
CONTRACT = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I1_IMPLEMENTATION_2026-08-15.md")
MATRIX = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I1_ACCEPTANCE_MATRIX_2026-08-15.csv")
NEGATIVE = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I1_NEGATIVE_INVENTORY_2026-08-15.md")
SOURCE = Path("crates/strategy-runtime-core/src/stage6_reconciliation_v2.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
IDENTITY = Path("crates/strategy-runtime-core/src/stage6_durable_identity.rs")
REPLAY = Path("crates/strategy-runtime-core/src/stage6_replay.rs")
BACKEND = Path("crates/strategy-runtime-core/src/stage6_journal_backend.rs")
CARGO = Path("crates/strategy-runtime-core/Cargo.toml")
GOLDEN = Path("fixtures/stage8a4-i1/canonical-golden-sha256.json")
V1_GOLDEN = Path("fixtures/stage6a/place-request-accepted-v1.json")

SCRIPT_FILES = {
    "scripts/stage8a4_durable_composition_i1_check.py",
    "scripts/stage8a4_durable_composition_i1_negative_harness.py",
    "scripts/stage8a4_durable_composition_i1_proof_map.py",
    "scripts/stage8a4_durable_composition_i1_gate.sh",
    "scripts/stage8a4_durable_composition_i1_handoff_safety_check.py",
    "scripts/make_stage8a4_durable_composition_i1_handoff.py",
}

REQUIRED = {
    str(AUTHORITY), str(CONTRACT), str(MATRIX), str(NEGATIVE), str(SOURCE),
    str(LIB), str(IDENTITY), str(REPLAY), str(BACKEND), str(CARGO),
    str(GOLDEN), str(V1_GOLDEN), *SCRIPT_FILES,
}

ALLOWED_CHANGED = REQUIRED - {str(V1_GOLDEN), str(CARGO)}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def read(root: Path, path: Path) -> str:
    candidate = root / path
    require(candidate.is_file(), f"missing required file: {path}")
    return candidate.read_text(encoding="utf-8")


def git_output(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def check(root: Path = ROOT, git_scope: bool = True) -> None:
    for item in REQUIRED:
        require((root / item).is_file(), f"missing required file: {item}")

    authority = json.loads(read(root, AUTHORITY))
    require(authority["stage"] == "8A-4-durable-composition-I1", "stage drift")
    require(authority["status"] == "implementation_candidate_independent_acceptance_pending", "status drift")
    require(authority["branch"] == BRANCH, "branch authority drift")
    require(authority["accepted_implementation_spec_r2_ref"] == BASE, "accepted spec ref drift")
    require(authority["accepted_implementation_spec_r2_review_sha256"] == REVIEW_SHA256, "accepted review hash drift")
    require(authority["implementation_slice"] == "I1_additive_v2_schema_codec_and_mixed_replay_only", "I1 scope drift")
    for key in (
        "v2_writer_enabled", "durable_apply_enabled", "composition_builder_enabled",
        "covering_seal_writer_enabled", "ack_readiness_enabled", "redis_live_enabled",
        "finam_post_delete_enabled", "broker_dispatch_enabled", "runtime_live_enabled",
        "real_orders_enabled", "stage8a5_authorized",
    ):
        require(authority[key] is False, f"closed surface opened: {key}")
    require(authority["stage6_v1_bytes_immutable"] is True, "V1 byte immutability drift")
    require(authority["stage6_v1_record_ids_immutable"] is True, "V1 ID immutability drift")
    require(authority["supported_record_schema_versions"] == [1, 2], "record version set drift")
    require(authority["golden_case_count"] == 20, "golden count drift")
    require(authority["focused_test_count"] >= 12, "focused test count drift")
    require(authority["compile_fail_case_count"] >= 2, "compile-fail count drift")

    source = read(root, SOURCE)
    lib = read(root, LIB)
    backend = read(root, BACKEND)
    replay = read(root, REPLAY)
    cargo = read(root, CARGO)
    contract = read(root, CONTRACT)
    negative = read(root, NEGATIVE)

    required_types = (
        "Stage6ReconciliationEndpointKindV2", "Stage6ReconciliationTransitionKindV2",
        "Stage6ReconciliationLifecycleV2", "Stage6ReconciliationFillEffectV2",
        "Stage6ExactLookupEvidenceV2", "Stage6BrokerOrderFactV2",
        "Stage6MaterialTradeFactV2", "Stage6AccountSafetySummaryV2",
        "Stage6PreAppendPreconditionV2", "Stage6SuffixManifestV2",
        "Stage6SuffixManifestEntryV2", "Stage6JournalRecordV2",
        "Stage6JournalRecordVersioned", "Stage6PendingReconciliationBatchV2",
        "Stage6MixedReplayEngineV2", "Stage6VersionedJournalReader",
    )
    for name in required_types:
        require(name in source, f"missing V2 type: {name}")

    for field in (
        "schema_version", "journal_record_id", "lifecycle_sequence",
        "previous_record_id", "causal_parent_id", "durable_request_identity",
        "event_kind", "payload", "canonical_payload_sha256", "source_evidence_sha256",
        "stable_transition_key_sha256", "durable_request_binding_sha256",
        "private_authoritative_outcome_binding_sha256", "exact_lookup_evidence",
        "broker_order_fact", "material_trade_facts", "fill_effect",
        "account_safety_summary", "pre_append_precondition", "deterministic_suffix_manifest",
        "received_ts", "account_orphan_orders_count", "target_unknown_orders_count",
        "expected_stage6_checkpoint_or_frontier_fingerprint",
        "expected_recovery_seal_generation", "expected_recovery_seal_fingerprint",
        "expected_request_state_fingerprint", "canonical_record_sha256",
    ):
        require(field in source, f"missing frozen V2 field: {field}")

    trade_block = source.split("pub struct Stage6MaterialTradeFactV2 {", 1)[1].split("\n}", 1)[0]
    safety_block = source.split("pub struct Stage6AccountSafetySummaryV2 {", 1)[1].split("\n}", 1)[0]
    precondition_block = source.split("pub struct Stage6PreAppendPreconditionV2 {", 1)[1].split("\n}", 1)[0]
    record_block = source.split("pub struct Stage6JournalRecordV2 {", 1)[1].split("\n}", 1)[0]
    for field in ("broker_trade_id", "broker_order_id", "client_order_id", "source_ts", "received_ts"):
        require(field in trade_block, f"material trade DTO field missing: {field}")
    for field in ("account_active_orders_count", "account_unknown_orders_count", "account_orphan_orders_count", "target_active_orders_count", "target_unknown_orders_count"):
        require(field in safety_block, f"account safety DTO field missing: {field}")
    for field in ("expected_stage6_checkpoint_or_frontier_fingerprint", "expected_recovery_seal_generation", "expected_recovery_seal_fingerprint", "expected_request_state_fingerprint"):
        require(field in precondition_block, f"pre-append DTO field missing: {field}")
    for field in ("schema_version", "journal_record_id", "lifecycle_sequence", "previous_record_id", "causal_parent_id", "durable_request_identity", "event_kind", "payload", "canonical_payload_sha256", "source_evidence_sha256"):
        require(field in record_block, f"V2 envelope field missing: {field}")

    for marker in (
        "#[serde(deny_unknown_fields)]", "probe_schema_version",
        "UnsupportedSchema", "AmbiguousSchema", "NonCanonicalEncoding",
        "same_stable_transition_key_with_different_v2_payload_fails_closed",
        "exact_duplicate_v2_is_idempotent_but_suffix_source_or_causality_drift_fails",
        "canonical_golden_matrix_is_stable", "exact_lookup_durable_binding_mismatch_fails_closed",
        "```compile_fail,E0277", "```compile_fail,E0599",
    ):
        require(marker in source, f"missing enforcement marker: {marker}")

    require("impl<'de> Deserialize<'de> for Stage6JournalRecordV2" not in source, "generic V2 Deserialize bypass opened")
    for forbidden in (
        "pub fn append", "pub fn write_v2", "pub fn apply_v2", "pub fn build_v2",
        "reqwest", "redis::", "Method::POST", "Method::DELETE", ".post(", ".delete(",
    ):
        require(forbidden not in source, f"forbidden I1 surface in V2 module: {forbidden}")

    require("mod stage6_reconciliation_v2;" in lib, "V2 module is not private")
    require("pub mod stage6_reconciliation_v2" not in lib, "V2 module exposed wholesale")
    require("finam-gateway" not in cargo and "finam_gateway" not in cargo, "dependency inversion")
    require("scan_versioned_framed_bytes" in backend, "version-aware framed scan missing")
    require("FRAME_MAGIC" in backend and "FRAME_VERSION" in backend, "frame validation missing")
    require("advance_causal_only" in replay and "is_finalized" in replay, "mixed replay bridge missing")
    require("I2, I3 and I4 remain separately review-gated" in contract, "future-slice closure missing")
    require("source/causal drift" in negative, "negative inventory incomplete")

    golden = json.loads(read(root, GOLDEN))
    require(
        hashlib.sha256((root / GOLDEN).read_bytes()).hexdigest()
        == "3e9393f5202489d3c82dc3809fc87d1bbb0ac792ef0c796a76705495e31319fb",
        "canonical golden fixture hash drift",
    )
    cases = golden.get("canonical_cases", {})
    require(golden.get("schema_version") == 1 and golden.get("algorithm") == "sha256", "golden header drift")
    require(len(cases) == 20, "golden case count mismatch")
    required_cases = {
        "PlaceExactWorkingBrokerOrderIdPresent", "PlaceExactWorkingBrokerOrderIdAbsent",
        "PlaceExactTerminalRejected", "PlacePartialFillTradeBrokerOrderIdPresent",
        "PlacePartialFillClientLinkedTradeBrokerOrderIdAbsent", "CancelExactWorking",
        "CancelTerminalCancelled", "ConflictHold", "StillUnknownHold",
        "ExactLookupNotAttempted", "ExactLookupSucceededWithObservation",
        "ExactLookupDocumentedNotFound", "ExactLookupUnavailable",
        "ExactLookupDecodeFailure", "ExactLookupStale", "MixedV1V2",
        "MixedV1V2PartialV1Suffix", "MixedV1V2CompleteV1Suffix",
        "UnknownRecordSchemaVersionFailClosed", "V1GoldenBytesUnchanged",
    }
    require(set(cases) == required_cases, "golden case inventory drift")
    require(all(isinstance(value, str) and len(value) == 64 and value == value.lower() for value in cases.values()), "invalid golden digest")
    v1_bytes = (root / V1_GOLDEN).read_bytes().removesuffix(b"\n")
    require(hashlib.sha256(v1_bytes).hexdigest() == "46a647e84c7c8042e4cd9dc83aa46c9a0e1c5db704110a25ca63787aeddd69fa", "V1 golden bytes changed")
    require(cases["V1GoldenBytesUnchanged"] == hashlib.sha256(v1_bytes).hexdigest(), "V1 golden binding drift")

    with (root / MATRIX).open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 40, "acceptance matrix row count drift")
    require([row["id"] for row in rows] == [f"I1-{index:03d}" for index in range(1, 41)], "acceptance IDs drift")

    if git_scope and (root / ".git").exists():
        require(git_output(root, "merge-base", "--is-ancestor", BASE, "HEAD") == "", "accepted spec is not ancestor")
        branch = git_output(root, "branch", "--show-current")
        require(branch == BRANCH, f"wrong branch: {branch}")
        changed = set(filter(None, git_output(root, "diff", "--name-only", BASE, "--").splitlines()))
        untracked = set(filter(None, git_output(root, "ls-files", "--others", "--exclude-standard").splitlines()))
        candidate = {path for path in changed | untracked if not path.startswith(("reports/", "tmp/", "target/"))}
        require(candidate <= ALLOWED_CHANGED, f"out-of-scope changed paths: {sorted(candidate - ALLOWED_CHANGED)}")
        require(not any(path.startswith(".github/") or path in {"Cargo.toml", "Cargo.lock"} for path in candidate), "Cargo/workflow drift")


def main() -> None:
    root = ROOT
    git_scope = True
    args = sys.argv[1:]
    if args and args[0] == "--root":
        root = Path(args[1]).resolve()
        args = args[2:]
    if args == ["--no-git"]:
        git_scope = False
    elif args:
        raise SystemExit("usage: stage8a4_durable_composition_i1_check.py [--root PATH] [--no-git]")
    try:
        check(root, git_scope=git_scope)
    except (CheckFailure, KeyError, ValueError, json.JSONDecodeError) as error:
        print(f"stage8a4-durable-composition-i1-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a4-durable-composition-i1-check: PASS rows=40 goldens=20 focused=12 writer=false apply=false execution=false")


if __name__ == "__main__":
    main()
