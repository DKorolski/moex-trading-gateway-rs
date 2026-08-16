#!/usr/bin/env python3
"""Fail-closed semantic checker for Stage 8A-4 durable composition I2."""

from __future__ import annotations

import csv
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "113d2827ef255e8d2c2597a3acb38fe52dd7e52d"
REVIEW_SHA256 = "5ef7d0fcc645874a8d9bce7e2d2bb3004f06b038c81b0bf5496582464cb1b9e7"
BRANCH = "stage8a4-durable-composition-i2"

AUTHORITY = Path("docs/stage-8/stage8a4-durable-composition-i2-authority.json")
CONTRACT = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I2_IMPLEMENTATION_2026-08-16.md")
MATRIX = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I2_ACCEPTANCE_MATRIX_2026-08-16.csv")
NEGATIVE = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I2_NEGATIVE_INVENTORY_2026-08-16.md")
REDUCER = Path("crates/finam-gateway/src/stage8a4_reconciliation.rs")
SOURCE = Path("crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i2.rs")
TESTS = Path("crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i2/tests.rs")
LIB = Path("crates/finam-gateway/src/lib.rs")
CURRENT_STATUS = Path("docs/current-status.md")
ROADMAP = Path("docs/roadmap.md")
README = Path("README.md")

SCRIPT_FILES = {
    "scripts/stage8a4_durable_composition_i2_check.py",
    "scripts/stage8a4_durable_composition_i2_negative_harness.py",
    "scripts/stage8a4_durable_composition_i2_proof_map.py",
    "scripts/stage8a4_durable_composition_i2_gate.sh",
    "scripts/stage8a4_durable_composition_i2_handoff_safety_check.py",
    "scripts/make_stage8a4_durable_composition_i2_handoff.py",
}
REQUIRED = {
    str(AUTHORITY), str(CONTRACT), str(MATRIX), str(NEGATIVE), str(REDUCER),
    str(SOURCE), str(TESTS), str(LIB), str(CURRENT_STATUS), str(ROADMAP),
    str(README), *SCRIPT_FILES,
}
ALLOWED_CHANGED = REQUIRED - {str(LIB)}


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
    require(authority["stage"] == "8A-4-durable-composition-I2", "stage drift")
    require(authority["status"] == "implementation_candidate_independent_acceptance_pending", "status drift")
    require(authority["branch"] == BRANCH, "branch drift")
    require(authority["accepted_i1_r2_ref"] == BASE, "I1 predecessor drift")
    require(authority["accepted_i1_r2_review_sha256"] == REVIEW_SHA256, "I1 review hash drift")
    require(authority["implementation_slice"] == "I2_private_linear_composition_and_transition_builder_no_append", "slice drift")
    for key in (
        "private_outcome_is_public", "public_diagnostic_is_authority", "candidate_is_public",
        "candidate_is_clone", "candidate_is_serialize", "v2_writer_enabled",
        "durable_append_enabled", "cas_enabled", "covering_seal_writer_enabled",
        "ack_readiness_enabled", "redis_live_enabled", "finam_post_delete_enabled",
        "broker_dispatch_enabled", "runtime_live_enabled", "real_orders_enabled",
        "stage8a5_authorized",
    ):
        require(authority[key] is False, f"closed surface opened: {key}")
    require(authority["acceptance_row_count"] == 42, "acceptance count drift")
    require(authority["focused_test_count"] >= 10, "focused test count drift")
    require(authority["negative_case_count"] >= 20, "negative count drift")
    require(authority["compile_fail_case_count"] >= 2, "compile-fail count drift")

    reducer = read(root, REDUCER)
    source = read(root, SOURCE)
    tests = read(root, TESTS)
    lib = read(root, LIB)
    contract = read(root, CONTRACT)
    negative = read(root, NEGATIVE)
    status = read(root, CURRENT_STATUS)
    roadmap = read(root, ROADMAP)
    readme = read(root, README)

    require("mod durable_composition_i2;" in reducer, "private I2 module missing")
    require("pub mod durable_composition_i2" not in reducer, "I2 module public")
    require("struct Stage8a4AuthoritativeReconciliationOutcome" in reducer, "private outcome missing")
    require("pub struct Stage8a4AuthoritativeReconciliationOutcome" not in reducer, "private outcome exported")
    outcome_head = reducer.split("struct Stage8a4AuthoritativeReconciliationOutcome", 1)[0][-220:]
    require("derive(" not in outcome_head, "private outcome gained derives")
    require(".into_diagnostic()" in reducer, "diagnostic projection missing")
    require("diagnostic: Stage8a4ReconciliationDiagnostic" not in reducer, "diagnostic stored as authority")
    require("Stage8a4PrivateExactLookup" in reducer and "Stage8a4PrivateAccountSafety" in reducer, "private evidence missing")

    for marker in (
        "struct Stage8a4I2CompositionInput", "struct Stage8a4I2DurableCandidate",
        "fn build_private_durable_candidate", "PrivateJournalRecordV2Wire",
        "Stage6JournalRecordV2::decode_canonical", "build_v1_suffix",
        "build_suffix_manifest", "canonical_record_sha256", "STABLE_KEY_DOMAIN",
        'b"stage8a4-stable-transition-key-v1"', "PrivatePreAppendEvidence",
        "expected_stage6_checkpoint_or_frontier_fingerprint",
        "expected_recovery_seal_generation", "expected_recovery_seal_fingerprint",
        "expected_request_state_fingerprint", "Stage8a4PrivateExactLookup::NotAttempted",
        "Stage8a4PrivateExactLookup::Succeeded", "Stage8a4PrivateExactLookup::DocumentedNotFound",
        "Stage8a4PrivateExactLookup::Unavailable", "Stage8a4PrivateExactLookup::DecodeFailure",
        "Stage8a4PrivateExactLookup::Stale", "Stage6CancelOutcomeV1::ExecutionObserved",
        "Stage6CancelOutcomeV1::Rejected", "Stage6CancelOutcomeV1::Canceled",
        "Stage6CancelOutcomeV1::AlreadyTerminalNonExecution", "```compile_fail",
    ):
        require(marker in source, f"I2 enforcement marker missing: {marker}")
    require("pub struct Stage8a4I2" not in source, "I2 type exported")
    require("pub fn build_private_durable_candidate" not in source, "I2 builder exported")
    require("Stage8a4I2DurableCandidate" not in lib, "candidate reexported")
    candidate_head = source.split("struct Stage8a4I2DurableCandidate", 1)[0][-180:]
    require("derive(" not in candidate_head, "candidate gained derives")

    stable = source.split("let stable_key =", 1)[1].split("let v2_record_id", 1)[0]
    for marker in ("STABLE_KEY_DOMAIN", "durable_binding.as_str()", "outcome_binding.as_str()", "transition_bytes"):
        require(marker in stable, f"stable-key input missing: {marker}")
    for forbidden in ("expected_recovery_seal_generation", "expected_recovery_seal_fingerprint", "expected_request_state_fingerprint", "Uuid", "random"):
        require(forbidden not in stable, f"mutable/random stable-key input: {forbidden}")
    preappend = source.split("struct PrivatePreAppendEvidence {", 1)[1].split("\n}", 1)[0]
    for field in (
        "expected_stage6_checkpoint_or_frontier_fingerprint",
        "expected_recovery_seal_generation",
        "expected_recovery_seal_fingerprint",
        "expected_request_state_fingerprint",
    ):
        require(field in preappend, f"preappend field missing: {field}")
    manifest_entry = source.split("struct PrivateSuffixManifestEntry {", 1)[1].split("\n}", 1)[0]
    for field in (
        "ordinal", "event_kind", "journal_record_id", "lifecycle_sequence",
        "canonical_payload_sha256", "canonical_record_sha256",
    ):
        require(field in manifest_entry, f"suffix manifest field missing: {field}")
    disposition = source.split("fn effective_transition_kind", 1)[1].split("fn map_exact_lookup", 1)[0]
    documented = disposition.split("Stage8a4PrivateExactLookup::DocumentedNotFound", 1)[1].split(
        "Stage8a4PrivateExactLookup::Unavailable", 1
    )[0]
    require("outcome.selected_order.is_some()" in documented, "not-found contradiction test missing")
    require("ReconciliationConflictHold" in documented, "not-found contradiction no longer conflicts")
    require("ReconciliationStillUnknownHold" in documented, "not-found absence no longer holds")
    unavailable = disposition.split("Stage8a4PrivateExactLookup::Unavailable", 1)[1].split(
        "Stage8a4PrivateExactLookup::NotAttempted", 1
    )[0]
    require("ReconciliationStillUnknownHold" in unavailable, "failed exact lookup no longer unknown")

    for test in (
        "place_exact_filled_builds_v2_then_lossless_v1_suffix",
        "stable_key_ignores_mutable_preappend_generation",
        "place_without_broker_id_never_fabricates_order_or_trade_suffix",
        "cancel_working_remains_unresolved_without_suffix",
        "cancel_terminal_cancelled_projects_outcome_and_finalization_only",
        "succeeded_exact_lookup_preserves_typed_observation",
        "documented_not_found_without_exact_contradiction_is_still_unknown",
        "non_success_lookup_states_never_become_exact",
        "cancel_target_cross_binding_is_mandatory",
        "candidate_is_pure_and_deterministic_for_identical_inputs",
    ):
        require(test in tests, f"focused test missing: {test}")

    for forbidden in (
        "Stage6JournalBackend", "Stage6FileJournalBackend", "fn append(", ".append(",
        "compare_and_append", "redis::", "reqwest", "Method::POST", "Method::DELETE",
        ".post(", ".delete(", "CommandAck", "Readiness", "BrokerDispatch",
    ):
        require(forbidden not in source, f"forbidden I2 surface: {forbidden}")
    for marker in ("no journal backend", "I3 writer/CAS/append/seal", "I4 ACK/readiness", "FINAM POST/DELETE"):
        require(marker in contract, f"contract boundary missing: {marker}")
    require("20." in negative and "runtime-live" in negative, "negative inventory incomplete")

    with (root / MATRIX).open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 42, "acceptance matrix row count drift")
    require([row["id"] for row in rows] == [f"I2-{index:03d}" for index in range(1, 43)], "acceptance IDs drift")
    require(all(row["requirement"].strip() and row["evidence"].strip() for row in rows), "acceptance evidence empty")

    for document_name, document in (("status", status), ("roadmap", roadmap), ("README", readme)):
        for marker in ("113d282", "I2", "I3", "I4"):
            require(marker in document, f"{document_name} marker missing: {marker}")

    if git_scope and (root / ".git").exists():
        require(git_output(root, "merge-base", "--is-ancestor", BASE, "HEAD") == "", "I1 predecessor is not ancestor")
        require(git_output(root, "branch", "--show-current") == BRANCH, "wrong branch")
        changed = set(filter(None, git_output(root, "diff", "--name-only", BASE, "--").splitlines()))
        untracked = set(filter(None, git_output(root, "ls-files", "--others", "--exclude-standard").splitlines()))
        candidate = {path for path in changed | untracked if not path.startswith(("reports/", "tmp/", "target/"))}
        require(candidate <= ALLOWED_CHANGED, f"out-of-scope paths: {sorted(candidate - ALLOWED_CHANGED)}")
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
        raise SystemExit("usage: stage8a4_durable_composition_i2_check.py [--root PATH] [--no-git]")
    try:
        check(root, git_scope=git_scope)
    except (CheckFailure, KeyError, ValueError, json.JSONDecodeError) as error:
        print(f"stage8a4-durable-composition-i2-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a4-durable-composition-i2-check: PASS rows=42 focused=10 append=false execution=false")


if __name__ == "__main__":
    main()
