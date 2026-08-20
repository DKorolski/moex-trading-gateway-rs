#!/usr/bin/env python3
"""Fail-closed semantic checker for Stage 8A-4 durable composition I3 R4."""

from __future__ import annotations

import csv
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ACCEPTED_I2 = "90f46052cc31cea012437eddb59fb7c3ca5c2320"
ACCEPTED_I2_REVIEW_SHA256 = "196c2b69161081f9034eb9399f41245f11ccd7eca229fadc3f8ec842cd1231f0"
REJECTED_I3_R1 = "a490bbe700c51f0e9c6debd2a007cb9b5061c3d8"
REJECTED_I3_R1_REVIEW_SHA256 = "c0ecc723ab98ba67560cb857e2761d0913f47c8ff78355bc04e74c8e03b585fe"
REJECTED_I3_R2 = "62e5e0509adb9cceb1d9947b5b3f92120e2f19ea"
REJECTED_I3_R2_REVIEW_SHA256 = "606ce34c3369fe732dfced14c283fe2bf1020e5c64db638109daa6b26f55d1cc"
REJECTED_I3_R3 = "3aa267029d512ba21f91dd95eb118b8d51810b56"
REJECTED_I3_R3_REVIEW_SHA256 = "aeae8245d421510301672a3885eb2396efdee0071c1dbd1af8313a9aa3d29cb3"
R4_SPEC_SHA256 = "5f0bfb0fd65ce5723b883638735c610220c51d279b8b7e7085fad9e544ed79a5"
BRANCH = "stage8a4-durable-composition-i3-r4"
ROWS = 69
NEGATIVE_CASES = 80

AUTHORITY = Path("docs/stage-8/stage8a4-durable-composition-i3-authority.json")
CONTRACT = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I3_IMPLEMENTATION_2026-08-16.md")
MATRIX = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I3_ACCEPTANCE_MATRIX_2026-08-16.csv")
NEGATIVE = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I3_NEGATIVE_INVENTORY_2026-08-16.md")
CORE = Path("crates/strategy-runtime-core/src/stage6d_live_core.rs")
CORE_LIB = Path("crates/strategy-runtime-core/src/lib.rs")
IDENTITY = Path("crates/strategy-runtime-core/src/stage6_durable_identity.rs")
CLEAN_RESTART = Path("crates/strategy-runtime-core/src/stage5g_clean_restart.rs")
JOURNAL = Path("crates/strategy-runtime-core/src/stage6_journal_backend.rs")
REPLAY_V2 = Path("crates/strategy-runtime-core/src/stage6_reconciliation_v2.rs")
RUNTIME = Path("crates/runtime-durable-service/src/recovery.rs")
RUNTIME_LIB = Path("crates/runtime-durable-service/src/lib.rs")
RUNTIME_CARGO = Path("crates/runtime-durable-service/Cargo.toml")
STAGE8A1 = Path("crates/finam-gateway/src/stage8a1_execution_capability.rs")
STAGE8A1_TEST = Path("crates/finam-gateway/tests/stage8a1_r3_authority_boundary.rs")
FINAM_LIB = Path("crates/finam-gateway/src/lib.rs")
FINAM_CARGO = Path("crates/finam-gateway/Cargo.toml")
RECONCILIATION = Path("crates/finam-gateway/src/stage8a4_reconciliation.rs")
I2 = Path("crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i2.rs")
I2_TESTS = Path("crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i2/tests.rs")
I3 = Path("crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i2/durable_writer_i3.rs")
STATUS = Path("docs/current-status.md")
ROADMAP = Path("docs/roadmap.md")
README = Path("README.md")
COMPILE_FAIL = Path("scripts/stage8a4_durable_composition_i3_external_compile_fail.sh")
DEPENDENCY_GRAPH = Path("scripts/stage8a4_durable_composition_i3_dependency_graph_check.sh")

SCRIPT_FILES = {
    "scripts/stage8a4_durable_composition_i3_check.py",
    "scripts/stage8a4_durable_composition_i3_negative_harness.py",
    "scripts/stage8a4_durable_composition_i3_proof_map.py",
    "scripts/stage8a4_durable_composition_i3_gate.sh",
    "scripts/stage8a4_durable_composition_i3_handoff_safety_check.py",
    "scripts/make_stage8a4_durable_composition_i3_handoff.py",
    "scripts/stage8a4_i3_stage8a1_successor_check.py",
    str(COMPILE_FAIL),
    str(DEPENDENCY_GRAPH),
}
REQUIRED = {str(path) for path in (
    AUTHORITY, CONTRACT, MATRIX, NEGATIVE, CORE, CORE_LIB, IDENTITY, CLEAN_RESTART,
    JOURNAL, REPLAY_V2, RUNTIME, RUNTIME_LIB, RUNTIME_CARGO, STAGE8A1,
    STAGE8A1_TEST, FINAM_LIB, FINAM_CARGO, RECONCILIATION, I2, I2_TESTS, I3,
    STATUS, ROADMAP, README,
)} | SCRIPT_FILES | {"Cargo.lock"}
ALLOWED_CHANGED = REQUIRED | {
    "Cargo.toml",
    "crates/strategy-runtime-core/Cargo.toml",
    "crates/runtime-durable-service/tests/stage7b_writer_lock_subprocess.rs",
    "crates/finam-gateway/src/stage8a1_execution_capability/stage8a2_builder_composition.rs",
}


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


def contains_all(document: str, markers: tuple[str, ...], label: str) -> None:
    for marker in markers:
        require(marker in document, f"{label} marker missing: {marker}")


def check(root: Path = ROOT, git_scope: bool = True) -> None:
    for item in REQUIRED:
        require((root / item).is_file(), f"missing required file: {item}")
    authority = json.loads(read(root, AUTHORITY))
    lineage = {
        "stage": "8A-4-durable-composition-I3-R4",
        "status": "implementation_candidate_independent_acceptance_pending",
        "branch": BRANCH,
        "accepted_i2_r3_ref": ACCEPTED_I2,
        "accepted_i2_r3_review_sha256": ACCEPTED_I2_REVIEW_SHA256,
        "rejected_i3_r1_ref": REJECTED_I3_R1,
        "rejected_i3_r1_review_sha256": REJECTED_I3_R1_REVIEW_SHA256,
        "rejected_i3_r2_ref": REJECTED_I3_R2,
        "rejected_i3_r2_review_sha256": REJECTED_I3_R2_REVIEW_SHA256,
        "rejected_i3_r3_ref": REJECTED_I3_R3,
        "rejected_i3_r3_review_sha256": REJECTED_I3_R3_REVIEW_SHA256,
        "i3_r4_correction_spec_sha256": R4_SPEC_SHA256,
        "sole_writer_owner": "Stage7bRecoveryReadyOwner",
    }
    for key, value in lineage.items():
        require(authority.get(key) == value, f"authority drift: {key}")
    for key in (
        "sealed_linear_writer_authority", "exact_request_truth_binding",
        "writer_entry_truth_freshness", "exact_control_operational_binding",
        "post_write_error_poison_sticky", "suffix_fault_matrix_covered",
        "covering_seal_required", "covering_seal_reread_validated", "v2_first",
        "each_append_fsync_backed", "stage8a1_r3_authority_restored",
        "broker_neutral_runtime_dependency", "broker_core_sqlite_baseline_unchanged",
        "production_normal_composition_path",
        "production_restart_without_i2_candidate", "external_raw_mutator_compile_fail",
        "sealed_authority_publicly_constructible", "incomplete_restart_remains_pending",
        "complete_uncovered_restart_remains_pending",
        "source_truth_control_bound_in_authority_hmac", "writer_entry_ed25519_attested",
        "writer_issuer_public_key_pinned_by_operational_identity",
        "writer_issuer_private_key_regular_and_owner_only",
        "production_normal_path_directly_tested",
        "production_v2_only_recovery_directly_tested",
        "production_partial_suffix_recovery_directly_tested",
        "production_complete_before_s1_recovery_directly_tested",
    ):
        expected = False if key == "sealed_authority_publicly_constructible" else True
        require(authority.get(key) is expected, f"authority drift: {key}")
    for key in (
        "raw_batch_writer_publicly_callable", "raw_core_append_normal_public_api",
        "durable_receipt_grants_ack_or_readiness", "ack_readiness_enabled",
        "redis_live_enabled", "finam_post_delete_enabled", "broker_dispatch_enabled",
        "runtime_live_enabled", "real_orders_enabled", "stage8a5_authorized",
    ):
        require(authority.get(key) is False, f"closed surface opened: {key}")

    core, core_lib, identity = read(root, CORE), read(root, CORE_LIB), read(root, IDENTITY)
    clean_restart = read(root, CLEAN_RESTART)
    journal, replay, runtime = read(root, JOURNAL), read(root, REPLAY_V2), read(root, RUNTIME)
    runtime_cargo, stage8a1 = read(root, RUNTIME_CARGO), read(root, STAGE8A1)
    stage8a1_test, finam_lib = read(root, STAGE8A1_TEST), read(root, FINAM_LIB)
    finam_cargo, reconciliation = read(root, FINAM_CARGO), read(root, RECONCILIATION)
    i2, i2_tests, i3 = read(root, I2), read(root, I2_TESTS), read(root, I3)
    compile_fail = read(root, COMPILE_FAIL)
    dependency_graph = read(root, DEPENDENCY_GRAPH)

    # P0: no external raw Stage8A4 storage mutation, under any historical name.
    require("pub(crate) fn stage8a4_internal_append_durable_batch" in core, "raw core append is not crate-private")
    production_core = re.sub(
        r'(?ms)#\[cfg\(feature = "stage5g-artifact-fixtures"\)\]\s*#\[doc\(hidden\)\]\s*pub fn stage8a4_test_append_durable_batch_with_suffix_limit\([^}]+\}\s*',
        "",
        core,
        count=1,
    )
    require(len(production_core) < len(core), "feature-gated crash fixture boundary missing")
    exported_raw = re.search(
        r"(?ms)(?:#\[doc\(hidden\)\]\s*)?pub\s+fn\s+(?!stage8a4_writer_entry_attestation_sha256)\w*stage8a4\w*(?:append|apply|persist|write|mutat)\w*\([^)]*Stage6Stage8a4DurableBatch",
        production_core,
    )
    require(exported_raw is None, "exported raw Stage8A4 mutator")
    require("stage8a4_internal_append_durable_batch" not in core_lib, "raw core append re-exported")
    contains_all(core, (
        "struct Stage6Stage8a4SealedWriteAuthority", "pub fn apply_stage8a4_validated_writer_entry",
        "entry: Stage6Stage8a4ValidatedWriteEntry", "fn verify(",
        "stage8a4_write_authority_hmac_sha256", "Stage8a4WriteAuthorityInvalid",
        "source_evidence_binding_sha256", "writer_truth_binding_sha256",
        "control_binding_sha256",
        "pub fn verify_issuer_attestation", "verify_stage8a4_writer_signature",
        "stage8a4_writer_issuer_public_key_hex != entry.issuer_public_key_hex()",
    ), "sealed core writer")
    require(
        "Stage6Stage8a4ValidatedWriteEntry::issue" not in core + runtime + i3,
        "caller-forgeable validated writer issuer restored",
    )
    require(
        "pub struct Stage6Stage8a4SealedWriteAuthority" not in core,
        "sealed Stage8A4 authority became public",
    )
    contains_all(compile_fail, (
        "temporary external crate", "stage8a4_internal_append_durable_batch",
        "append_stage8a4_durable_batch", "Stage6Stage8a4SealedWriteAuthority::seal",
        "Stage6Stage8a4ValidatedWriteEntry::issue",
    ), "external compile-fail")

    # P0: independently accepted Stage8A1 current-owner authority remains intact.
    contains_all(stage8a1, (
        "Stage7bCompositeReadinessSnapshot", "Stage7bPaperReadinessPhase",
        "pub fn from_stage7b_owner", "authorize_stage8a1_durable_request",
        "revalidate_place_capability", "revalidate_cancel_capability", "read_arm_registration",
    ), "Stage8A1 R3 authority")
    require("from_current_stage6_authority" not in stage8a1, "caller-provided Stage8A1 seal authority restored")
    require("Stage8a1CompositeReadinessSnapshot" not in stage8a1, "local readiness lookalike restored")
    contains_all(stage8a1_test, (
        "from_stage7b_owner", "owner_mediated_constructor_boundary",
        "trusted_issuer_is_the_public_no_send_authority_boundary",
    ), "Stage8A1 boundary regression")

    # P1: broker-neutral runtime dependency direction.
    require("finam-gateway" not in runtime_cargo, "runtime depends on FINAM gateway")
    for forbidden in ("broker-finam", "reqwest", "rusqlite"):
        require(forbidden not in runtime_cargo, f"runtime broker-neutral dependency opened: {forbidden}")
    require('runtime-durable-service = { path = "../runtime-durable-service" }' in finam_cargo, "FINAM composition dependency missing")
    contains_all(dependency_graph, (
        "cargo tree -p runtime-durable-service --edges normal",
        "broker-core's pre-existing broker-neutral order-path store owns rusqlite",
    ), "dependency graph proof")

    # Production normal and restart paths consume only authenticated sealed authority.
    contains_all(runtime, (
        "pub fn append_stage8a4_validated_entry_and_cover",
        "entry: Stage6Stage8a4ValidatedWriteEntry", "apply_stage8a4_validated_writer_entry",
        "revalidate_cached_committed_seal", "refresh_stage7b_durable_frontier",
        "advance_recovery_seal", "validate_recovered_binding",
        "stage8a4_i3_writer_commits_covering_s1_and_restarts_from_mixed_journal",
    ), "Stage7 sealed writer")
    contains_all(i3, (
        "pub(crate) fn reconcile_persist_and_cover_stage8a4_from_production_sources",
        "issue_durable_request_context_from_current_authority",
        "issue_stage8a4_policy_from_frozen_config",
        "issue_stage8a4_source_evidence_from_readonly_acquisition",
        "pub fn reconcile_persist_and_cover_stage8a4",
        "build_private_durable_candidate(Stage8a4I2CompositionInput",
        "issue_private_durable_write_authority(",
        "pub struct Stage8a4DurableWriteAuthority", "pub fn persist_and_cover",
        "fn issue_private_durable_write_authority", "candidate: Stage8a4I2DurableCandidate",
        "current_truth: Stage8a4FreshTruthAdmission", "controls: Stage8a4PostEffectControlEvidence",
        "sign_stage8a4_writer_attestation", "verify_issuer_attestation",
        "owner.append_stage8a4_validated_entry_and_cover",
        "stage8a4_i3_normal_production_path_persists_exact_batch_covers_s1_and_restarts_ready",
    ), "positive normal composition")
    contains_all(stage8a1, (
        "STAGE8A4_WRITER_SIGNING_KEY_FILE", "writer_signing_key_identity",
        "metadata.permissions().mode() & 0o077 != 0",
        "stage8a4_writer_issuer_public_key_hex", "sign_stage8a4_writer_attestation",
    ), "private writer issuer trust root")
    contains_all(runtime, (
        "stage8a4_i3_rejects_forged_or_wrong_trust_root_attestation_before_append",
        "malformed_signature", "Stage8a4WriteAuthorityInvalid",
    ), "writer issuer adversarial tests")
    contains_all(finam_lib, ("reconcile_persist_and_cover_stage8a4",), "normal composition export")
    require("pub fn issue_private_durable_write_authority" not in i3, "private issuer exported")
    require("Stage8a4DurableWriterParts" not in i3 + finam_lib + reconciliation, "R2 raw writer parts remain")
    contains_all(i3, (
        "pub fn recover_persisted_stage8a4_suffix_and_cover",
        "pending_recovery_material", "issue_persisted_recovery_write_authority",
        "authorize_pending_recovery_request",
        "Stage6Stage8a4DurableBatch::recover_from_persisted_transition",
        "stage8a4_i3_production_recovery_repairs_v2_only_crash_and_covers_s1",
        "stage8a4_i3_production_recovery_repairs_partial_exact_suffix_and_covers_s1",
        "stage8a4_i3_production_recovery_covers_complete_batch_without_s1",
    ), "production restart composition")
    require(
        "stage8a4_test_append_durable_batch_with_suffix_limit" not in i3,
        "production I3 integration tests use raw/test batch writer",
    )
    pending_material_body = re.search(
        r"(?ms)pub fn stage8a4_pending_recovery_material\(.*?\n    pub fn boot_mode",
        core,
    )
    require(pending_material_body is not None, "pending recovery material body missing")
    require(
        "Stage6ReconciliationBatchCompletionV2::Incomplete" not in pending_material_body.group(0),
        "complete uncovered batch cannot be recovered before S1",
    )
    contains_all(core, (
        "pub fn recover_from_persisted_transition", "reconstruct_stage8a4_suffix_from_v2",
    ), "persisted V2 reconstruction")
    contains_all(core, (
        "stage8a4_i3_appends_v2_then_exact_suffix_and_is_idempotent",
    ), "normal idempotent append")
    contains_all(replay, (
        "pub(crate) fn reconstruct_stage8a4_suffix_from_v2", "matches_record(record)",
    ), "exact manifest reconstruction")
    contains_all(runtime, (
        "Stage8a4I3RecoveryPendingOwner", "Stage7bRestartOutcome::Stage8a4I3Pending",
        "stage8a4_i3_restart_covers_v2_only_crash_then_repairs_exact_suffix",
        "stage8a4_i3_restart_covers_partial_suffix_then_appends_only_missing_record",
        "stage8a4_i3_restart_rejects_unrelated_record_after_uncovered_v2",
        "drop(transition);", "drop(suffix);", "stage8a4_pending_recovery_material",
    ), "restart without process-local I2")

    # Preserve R2 exact truth/control and sticky post-write fail-stop.
    contains_all(reconciliation, (
        "admitted_request_id", "admitted_account_id", "admitted_instrument",
        "admitted_durable_binding_sha256", "admitted_canonical_truth_sha256",
        "writer_entry_valid_until",
    ), "fresh truth admission")
    contains_all(i3, (
        "admission.admitted_request_id != identity.strategy_request_id()",
        "&admission.admitted_account_id != identity.account_id()",
        "&admission.admitted_instrument != identity.instrument()",
        "admission.admitted_durable_binding_sha256 != durable_binding.as_str()",
        "admission.writer_entry_valid_until <= now",
        "controls.operational_identity_sha256() != current_operational_identity_sha256",
        "controls.runtime_config_fingerprint_sha256()", "controls.authority_scope_sha256()",
        "controls.arm_registration_sha256()", "controls.current_control_binding_sha256()",
    ), "exact writer binding")
    contains_all(i2_tests, (
        "i3_writer_entry_truth_is_exact_request_account_instrument_and_time_bound",
        "i3_writer_entry_detects_changed_current_orphan_state",
    ), "truth regression tests")
    contains_all(journal, (
        "durability_uncertain = true", "TestIoFailpoint::BeforeFrameWrite",
        "TestIoFailpoint::AfterFrameHeaderWrite", "TestIoFailpoint::AfterPartialRecordWrite",
        "TestIoFailpoint::AfterFrameHashWrite", "TestIoFailpoint::BeforeSync",
        "TestIoFailpoint::SyncFailure", "stage8a4_i3_every_post_write_failpoint_is_sticky",
    ), "journal uncertainty")
    contains_all(core, ("JournalMutationMayHaveOccurred", "classify_stage8a4_append_error"), "core uncertainty")
    contains_all(runtime, (
        "journal_mutation_uncertain: bool", "self.journal_mutation_uncertain = true",
        "if self.seal_commit_uncertain || self.journal_mutation_uncertain",
        "stage8a4_i3_post_write_fault_matrix_poison_is_sticky_in_process",
        "stage8a4_i3_suffix_post_write_faults_are_sticky_in_process",
        "stage8a4_i3_pre_write_failure_does_not_poison_owner",
    ), "owner fail-stop")
    contains_all(clean_restart, (
        "stage8a4_write_authority_hmac_sha256", "stage8a4_verify_write_authority_hmac_sha256",
    ), "commitment-key authority")
    for forbidden in ("reqwest", "Method::POST", "Method::DELETE", ".post(", ".delete(", "redis::", "XACK"):
        require(forbidden not in i3, f"forbidden I3 execution surface: {forbidden}")

    contract, negative = read(root, CONTRACT), read(root, NEGATIVE)
    status, roadmap, readme = read(root, STATUS), read(root, ROADMAP), read(root, README)
    contains_all(contract, (
        "I3 R3", REJECTED_I3_R2, "broker-neutral", "external compile-fail", "lost I2",
        "sticky", "covering S1", "I4 remains separately review-gated", "FINAM POST/DELETE",
    ), "contract")
    require("58." in negative and "lost I2" in negative, "negative inventory incomplete")
    for label, document in (("status", status), ("roadmap", roadmap), ("README", readme)):
        contains_all(document, ("I3 R2", "I3 R3", "I4"), label)

    with (root / MATRIX).open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == ROWS, "acceptance row count drift")
    require([row["id"] for row in rows] == [f"I3-{index:03d}" for index in range(1, ROWS + 1)], "acceptance IDs drift")
    require(all(row["requirement"].strip() and row["evidence"].strip() for row in rows), "acceptance evidence empty")

    if git_scope and (root / ".git").exists():
        require(git_output(root, "merge-base", "--is-ancestor", REJECTED_I3_R2, "HEAD") == "", "I3 R2 not ancestor")
        require(git_output(root, "branch", "--show-current") == BRANCH, "wrong branch")
        changed = set(filter(None, git_output(root, "diff", "--name-only", REJECTED_I3_R2, "--").splitlines()))
        untracked = set(filter(None, git_output(root, "ls-files", "--others", "--exclude-standard").splitlines()))
        candidate = {path for path in changed | untracked if not path.startswith(("reports/", "tmp/", "target/"))}
        require(candidate <= ALLOWED_CHANGED, f"out-of-scope paths: {sorted(candidate - ALLOWED_CHANGED)}")
        production_diff = git_output(root, "diff", "--unified=0", REJECTED_I3_R2, "--", "crates")
        added = "\n".join(line[1:] for line in production_diff.splitlines() if line.startswith("+") and not line.startswith("+++"))
        for forbidden in ("Method::POST", "Method::DELETE", ".post(", ".delete(", "XACK"):
            require(forbidden not in added, f"new execution surface: {forbidden}")


def main() -> None:
    root, git_scope, args = ROOT, True, sys.argv[1:]
    if args and args[0] == "--root":
        root, args = Path(args[1]).resolve(), args[2:]
    if args == ["--no-git"]:
        git_scope = False
    elif args:
        raise SystemExit("usage: stage8a4_durable_composition_i3_check.py [--root PATH] [--no-git]")
    try:
        check(root, git_scope=git_scope)
    except (CheckFailure, KeyError, ValueError, json.JSONDecodeError, AttributeError) as error:
        print(f"stage8a4-durable-composition-i3-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print(f"stage8a4-durable-composition-i3-check: PASS rows={ROWS} negatives={NEGATIVE_CASES} sealed=true broker_neutral=true recovery=true ack=false execution=false")


if __name__ == "__main__":
    main()
