#!/usr/bin/env python3
"""Fail-closed semantic checker for Stage 8A-4 durable composition I3 R2."""

from __future__ import annotations

import csv
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ACCEPTED_I2 = "90f46052cc31cea012437eddb59fb7c3ca5c2320"
ACCEPTED_I2_REVIEW_SHA256 = "196c2b69161081f9034eb9399f41245f11ccd7eca229fadc3f8ec842cd1231f0"
REJECTED_I3_R1 = "a490bbe700c51f0e9c6debd2a007cb9b5061c3d8"
REJECTED_I3_R1_REVIEW_SHA256 = "c0ecc723ab98ba67560cb857e2761d0913f47c8ff78355bc04e74c8e03b585fe"
BRANCH = "stage8a4-durable-composition-i3-r2"
ROWS = 60
NEGATIVE_CASES = 48

AUTHORITY = Path("docs/stage-8/stage8a4-durable-composition-i3-authority.json")
CONTRACT = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I3_IMPLEMENTATION_2026-08-16.md")
MATRIX = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I3_ACCEPTANCE_MATRIX_2026-08-16.csv")
NEGATIVE = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I3_NEGATIVE_INVENTORY_2026-08-16.md")
CORE = Path("crates/strategy-runtime-core/src/stage6d_live_core.rs")
CORE_LIB = Path("crates/strategy-runtime-core/src/lib.rs")
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

SCRIPT_FILES = {
    "scripts/stage8a4_durable_composition_i3_check.py",
    "scripts/stage8a4_durable_composition_i3_negative_harness.py",
    "scripts/stage8a4_durable_composition_i3_proof_map.py",
    "scripts/stage8a4_durable_composition_i3_gate.sh",
    "scripts/stage8a4_durable_composition_i3_handoff_safety_check.py",
    "scripts/make_stage8a4_durable_composition_i3_handoff.py",
}
REQUIRED = {
    str(path) for path in (
        AUTHORITY, CONTRACT, MATRIX, NEGATIVE, CORE, CORE_LIB, JOURNAL, REPLAY_V2,
        RUNTIME, RUNTIME_LIB, RUNTIME_CARGO, STAGE8A1, STAGE8A1_TEST, FINAM_LIB,
        FINAM_CARGO, RECONCILIATION, I2, I2_TESTS, I3, STATUS, ROADMAP,
    )
} | SCRIPT_FILES | {"Cargo.lock"}
ALLOWED_CHANGED = REQUIRED | {
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
    require(authority["stage"] == "8A-4-durable-composition-I3-R2", "stage drift")
    require(authority["status"] == "implementation_candidate_independent_acceptance_pending", "status drift")
    require(authority["branch"] == BRANCH, "branch drift")
    require(authority["accepted_i2_r3_ref"] == ACCEPTED_I2, "I2 predecessor drift")
    require(authority["accepted_i2_r3_review_sha256"] == ACCEPTED_I2_REVIEW_SHA256, "I2 review hash drift")
    require(authority["rejected_i3_r1_ref"] == REJECTED_I3_R1, "I3 R1 ref drift")
    require(authority["rejected_i3_r1_review_sha256"] == REJECTED_I3_R1_REVIEW_SHA256, "I3 R1 review hash drift")
    require(authority["sole_writer_owner"] == "Stage7bRecoveryReadyOwner", "writer owner drift")
    for key in (
        "sealed_linear_writer_authority", "raw_batch_writer_publicly_callable", "raw_core_append_normal_public_api",
        "exact_request_truth_binding", "writer_entry_truth_freshness", "exact_control_operational_binding",
        "post_write_error_poison_sticky", "suffix_fault_matrix_covered", "covering_seal_required",
        "covering_seal_reread_validated", "v2_first", "each_append_fsync_backed",
    ):
        expected = key not in {"raw_batch_writer_publicly_callable", "raw_core_append_normal_public_api"}
        require(authority[key] is expected, f"authority drift: {key}")
    for key in (
        "durable_receipt_grants_ack_or_readiness", "ack_readiness_enabled", "redis_live_enabled",
        "finam_post_delete_enabled", "broker_dispatch_enabled", "runtime_live_enabled",
        "real_orders_enabled", "stage8a5_authorized",
    ):
        require(authority[key] is False, f"closed surface opened: {key}")

    core = read(root, CORE)
    core_lib = read(root, CORE_LIB)
    journal = read(root, JOURNAL)
    replay = read(root, REPLAY_V2)
    runtime = read(root, RUNTIME)
    runtime_lib = read(root, RUNTIME_LIB)
    runtime_cargo = read(root, RUNTIME_CARGO)
    stage8a1 = read(root, STAGE8A1)
    stage8a1_test = read(root, STAGE8A1_TEST)
    finam_lib = read(root, FINAM_LIB)
    finam_cargo = read(root, FINAM_CARGO)
    reconciliation = read(root, RECONCILIATION)
    i2 = read(root, I2)
    i2_tests = read(root, I2_TESTS)
    i3 = read(root, I3)

    # P0: no normal raw writer chain; Stage7 consumes only the opaque FINAM capability.
    require("pub fn append_stage8a4_durable_batch_and_cover" not in runtime, "raw Stage7 batch writer remains public")
    require("pub fn append_stage8a4_durable_batch(" not in core, "raw core append remains normal public API")
    require("pub use stage6d_live_core::append_stage8a4_durable_batch" not in core_lib, "raw core append re-exported")
    contains_all(runtime, (
        "pub fn append_stage8a4_durable_authority_and_cover",
        "authority: Stage8a4DurableWriteAuthority",
        "append_stage8a4_validated_parts_and_cover",
        "expected_runtime_config_fingerprint_sha256",
    ), "sealed Stage7 writer")
    contains_all(i3, (
        "pub struct Stage8a4DurableWriteAuthority",
        "pub struct Stage8a4DurableWriterParts",
        "fn issue_private_durable_write_authority",
        "candidate: Stage8a4I2DurableCandidate",
        "current_truth: Stage8a4FreshTruthAdmission",
        "controls: Stage8a4PostEffectControlEvidence",
        "current_stage6: Stage6DurableRequestAuthorityV1",
    ), "sealed I3 authority")
    require("pub fn issue_private_durable_write_authority" not in i3, "I3 issuer exported")
    require("compile_fail" in runtime_lib and "append_stage8a4_durable_batch_and_cover" in runtime_lib, "external raw-writer compile-fail proof missing")
    require("compile_fail" in reconciliation and "Stage8a4DurableWriteAuthority" in reconciliation, "opaque authority compile-fail proof missing")
    require("runtime-durable-service" not in finam_cargo, "old FINAM-to-runtime dependency remains")
    require("finam-gateway = { path = \"../finam-gateway\" }" in runtime_cargo, "sealed authority dependency inversion missing")

    # Exact current truth and exact current owner/control binding.
    contains_all(reconciliation, (
        "admitted_request_id", "admitted_account_id", "admitted_instrument",
        "admitted_durable_binding_sha256", "admitted_canonical_truth_sha256",
        "writer_entry_valid_until", "stage8a4_writer_entry_valid_until",
    ), "fresh truth admission")
    contains_all(i3, (
        "admission.admitted_request_id != identity.strategy_request_id()",
        "&admission.admitted_account_id != identity.account_id()",
        "&admission.admitted_instrument != identity.instrument()",
        "admission.admitted_durable_binding_sha256 != durable_binding.as_str()",
        "admission.writer_entry_valid_until <= now",
        "controls.operational_identity_sha256() != current_operational_identity_sha256",
        "controls.runtime_config_fingerprint_sha256()",
        "controls.authority_scope_sha256()",
        "controls.arm_registration_sha256()",
        "controls.current_control_binding_sha256()",
        "runtime_config_fingerprint_sha256",
    ), "exact writer binding")
    contains_all(stage8a1, (
        "pub(crate) fn operational_identity_sha256",
        "pub(crate) fn runtime_config_fingerprint_sha256",
        "pub(crate) fn authority_scope_sha256",
        "pub(crate) fn arm_registration_sha256",
        "pub(crate) fn accepted_command_payload_sha256",
        "read_arm_registration",
        "stage8a4_i3_same_command_cross_root_operational_identity_is_rejected",
    ), "control evidence")
    contains_all(i2_tests, (
        "i3_writer_entry_truth_is_exact_request_account_instrument_and_time_bound",
        "another_request", "another_account", "writer_entry_valid_until",
        "i3_writer_entry_detects_changed_current_orphan_state",
    ), "truth regression tests")

    # Post-write mutation uncertainty is sticky in backend, core and process owner.
    contains_all(journal, (
        "durability_uncertain = true", "TestIoFailpoint::BeforeFrameWrite",
        "TestIoFailpoint::AfterFrameHeaderWrite", "TestIoFailpoint::AfterPartialRecordWrite",
        "TestIoFailpoint::AfterFrameHashWrite", "TestIoFailpoint::BeforeSync",
        "TestIoFailpoint::SyncFailure", "stage8a4_i3_every_post_write_failpoint_is_sticky",
        "stage8a4_i3_pre_write_rejection_does_not_poison_backend",
    ), "journal uncertainty")
    contains_all(core, (
        "JournalMutationMayHaveOccurred", "classify_stage8a4_append_error",
        "stage8a4_internal_append_durable_batch", "refresh_after_append",
    ), "core uncertainty")
    contains_all(runtime, (
        "journal_mutation_uncertain: bool", "self.journal_mutation_uncertain = true",
        "if self.seal_commit_uncertain || self.journal_mutation_uncertain",
        "stage8a4_i3_post_write_fault_matrix_poison_is_sticky_in_process",
        "stage8a4_i3_suffix_post_write_faults_are_sticky_in_process",
        "stage8a4_i3_pre_write_failure_does_not_poison_owner",
        "owner.advance_recovery_seal(&setup.key).is_err()",
    ), "owner fail-stop")

    # Preserve the accepted I3 protocol and narrow restart exception.
    contains_all(core, (
        "durable_request_binding_sha256", "authorize_stage8a4_durable_batch_source",
        "validate_cancel_original_target_shape", "stable_transition_key_sha256",
        "canonical_v2_record_sha256", "verified_suffix_prefix_length",
        "Stage6ReconciliationBatchCompletionV2::Complete",
    ), "core I3 protocol")
    contains_all(journal, ("self.file.sync_data()", "Stage6JournalRecordVersioned::decode_canonical"), "journal durability")
    contains_all(replay, ("canonical_record_sha256", "matches_record"), "suffix manifest")
    contains_all(runtime, (
        "revalidate_cached_committed_seal", "refresh_stage7b_durable_frontier",
        "advance_recovery_seal", "validate_recovered_binding", "fn stage8a4_i3_uncovered_checkpoint",
        "stage8a4_i3_restart_covers_v2_only_crash_then_repairs_exact_suffix",
        "stage8a4_i3_restart_covers_partial_suffix_then_appends_only_missing_record",
        "stage8a4_i3_restart_rejects_unrelated_record_after_uncovered_v2",
    ), "restart and seal")

    contains_all(stage8a1, ("from_current_stage6_authority",), "Stage8A1 durable authority")
    contains_all(stage8a1_test, ("from_durable_authority",), "Stage8A1 boundary test")
    contains_all(finam_lib, ("Stage8a4DurableWriteAuthority", "Stage8a4DurableWriterParts"), "FINAM authority export")
    contains_all(i2, ("mod durable_writer_i3;", "struct Stage8a4I2DurableCandidate"), "I2 privacy")

    for forbidden in ("reqwest", "Method::POST", "Method::DELETE", ".post(", ".delete(", "redis::", "XACK"):
        require(forbidden not in i3, f"forbidden I3 execution surface: {forbidden}")

    contract = read(root, CONTRACT)
    negative = read(root, NEGATIVE)
    status = read(root, STATUS)
    roadmap = read(root, ROADMAP)
    contains_all(contract, (
        "I3 R2", "opaque", "exact request", "sticky", "suffix", "covering S1",
        "I4 remains separately review-gated", "FINAM POST/DELETE",
    ), "contract")
    require("48." in negative and "raw Stage7" in negative, "negative inventory incomplete")
    for label, document in (("status", status), ("roadmap", roadmap)):
        contains_all(document, ("a490bbe", "I3 R2", "I4"), label)

    with (root / MATRIX).open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == ROWS, "acceptance row count drift")
    require([row["id"] for row in rows] == [f"I3-{index:03d}" for index in range(1, ROWS + 1)], "acceptance IDs drift")
    require(all(row["requirement"].strip() and row["evidence"].strip() for row in rows), "acceptance evidence empty")

    if git_scope and (root / ".git").exists():
        require(git_output(root, "merge-base", "--is-ancestor", REJECTED_I3_R1, "HEAD") == "", "I3 R1 not ancestor")
        require(git_output(root, "branch", "--show-current") == BRANCH, "wrong branch")
        changed = set(filter(None, git_output(root, "diff", "--name-only", REJECTED_I3_R1, "--").splitlines()))
        untracked = set(filter(None, git_output(root, "ls-files", "--others", "--exclude-standard").splitlines()))
        candidate = {path for path in changed | untracked if not path.startswith(("reports/", "tmp/", "target/"))}
        require(candidate <= ALLOWED_CHANGED, f"out-of-scope paths: {sorted(candidate - ALLOWED_CHANGED)}")
        production_diff = git_output(root, "diff", "--unified=0", REJECTED_I3_R1, "--", "crates")
        added = "\n".join(line[1:] for line in production_diff.splitlines() if line.startswith("+") and not line.startswith("+++"))
        for forbidden in ("Method::POST", "Method::DELETE", ".post(", ".delete(", "XACK"):
            require(forbidden not in added, f"new execution surface: {forbidden}")


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
        raise SystemExit("usage: stage8a4_durable_composition_i3_check.py [--root PATH] [--no-git]")
    try:
        check(root, git_scope=git_scope)
    except (CheckFailure, KeyError, ValueError, json.JSONDecodeError) as error:
        print(f"stage8a4-durable-composition-i3-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print(f"stage8a4-durable-composition-i3-check: PASS rows={ROWS} opaque=true sticky=true ack=false execution=false")


if __name__ == "__main__":
    main()
