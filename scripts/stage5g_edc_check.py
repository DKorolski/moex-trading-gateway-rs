#!/usr/bin/env python3
"""Stage 5G-e-d-c current-tree semantic and closed-surface checker."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path


BASE_REF = "2b2bcc671c68722b3b84b914b785ffcb83f6802d"
CONTRACT = Path("docs/stage-5/stage5g-e-d-c-application-contract.json")
DESIGN = Path("docs/stage-5/stage5g-e-d-c-application-contract.md")
PARENT = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs")
REDUCER = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/reducer.rs")
APPLICATION = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/application.rs")
CLEAN_RESTART = Path("crates/strategy-runtime-core/src/stage5g_clean_restart.rs")
ORDER_POSITION = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
RUNTIME = Path("crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
STATUS = Path("docs/current-status.md")
ONBOARDING = Path("docs/reviewer-onboarding-and-roadmap.md")
GATE = Path("scripts/stage5g_edc_gate.sh")
HANDOFF = Path("scripts/make_stage5g_edc_handoff_archive.py")
FORBIDDEN_AUTHORITY_REF = "bd4742ef4b727ae8fa43d561c6674dea71b86b57"

RESULTS = [
    "Stage5gFreshTruthApplied",
    "Stage5gFreshTruthContinued",
    "Stage5gFreshTruthApplicationBlocked",
]
FAILURE_POINTS = [
    "BeforeCandidateExtraction",
    "AfterCandidateExtraction",
    "AfterPreflightBeforeTransition",
    "InsideCanonicalTransition",
    "AfterTransitionBeforeEquality",
    "AfterEqualityBeforeExport",
    "DuringSerialization",
    "AfterBytesBeforeSourceDrop",
    "AfterSourceDropBeforeDecode",
    "DuringAuthenticationVerification",
    "DuringRestore",
    "AfterRestoreBeforeEvidenceEquality",
    "BeforeDisabledReplayProjection",
    "AfterDisabledReplayProjectionBeforeAuthentication",
]
EVIDENCE_FIELDS = [
    "schema_version", "scenario_id", "disposition", "reason",
    "operational_identity_commitment_sha256", "fresh_package_id",
    "fresh_snapshot_epoch", "fresh_package_fingerprint_sha256",
    "pre_restart_package_fingerprint_sha256",
    "reduction_pre_semantic_fingerprint_sha256", "candidate_fingerprint_sha256",
    "applied_post_semantic_fingerprint_sha256",
    "post_restart_package_fingerprint_sha256", "ignored_terminal_order_count",
    "ignored_historical_trade_count", "runtime_transition_applied",
    "callback_invoked", "transport_opened", "exact_replay_enabled",
]
FOCUSED_TESTS = [
    "stage5g_edc_applies_owned_working_candidate_through_authenticated_roundtrip",
    "stage5g_edc_failure_matrix_preserves_exact_pre_application_authority",
    "stage5g_edc_timer_continuation_is_an_exact_noop",
    "stage5g_edc_independent_identical_runs_are_byte_deterministic",
    "stage5g_edc_post_package_rejects_wrong_key_missing_tag_and_semantic_tamper",
    "stage5g_edc_late_fill_trade_permutation_produces_identical_post_package",
    "stage5g_edc_all_source_terminal_states_continue_without_mutation",
    "stage5g_edc_terminal_late_fill_candidates_apply_for_canceled_and_expired",
    "stage5g_edc_blocked_grst01_and_grst08_retain_restart_authority",
]
COMPILE_FAIL_WITNESSES = [
    "stage5g_edc_compile_fail_reduction_clone",
    "stage5g_edc_compile_fail_candidate_extraction",
    "stage5g_edc_compile_fail_apply_twice",
    "stage5g_edc_compile_fail_reduction_reuse",
    "stage5g_edc_compile_fail_blocked_to_candidate",
    "stage5g_edc_compile_fail_continued_to_candidate",
    "stage5g_edc_compile_fail_applied_exposes_candidate",
    "stage5g_edc_compile_fail_diagnostic_reconstruction",
    "stage5g_edc_compile_fail_raw_rows_application",
    "stage5g_edc_compile_fail_reduction_serialization",
]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage5g-edc-check: FAIL: {message}")


def enum_variants(source: str, name: str) -> list[str]:
    match = re.search(rf"enum\s+{re.escape(name)}\s*\{{(.*?)\n\}}", source, re.S)
    require(match is not None, f"enum missing: {name}")
    return re.findall(r"(?m)^\s{4}([A-Z][A-Za-z0-9_]*)\s*,\s*$", match.group(1))


def struct_fields(source: str, name: str) -> list[str]:
    match = re.search(rf"struct\s+{re.escape(name)}\s*\{{(.*?)\n\}}", source, re.S)
    require(match is not None, f"struct missing: {name}")
    return re.findall(
        r"(?m)^\s{4}(?:pub\(crate\)\s+)?([a-z][a-z0-9_]*)\s*:", match.group(1)
    )


def check(root: Path, check_git: bool) -> None:
    paths = [
        CONTRACT, DESIGN, PARENT, REDUCER, APPLICATION, CLEAN_RESTART,
        ORDER_POSITION, RUNTIME, LIB, STATUS, ONBOARDING, GATE, HANDOFF,
    ]
    for path in paths:
        require((root / path).is_file() and not (root / path).is_symlink(), f"missing {path}")

    if check_git:
        parent = subprocess.check_output(["git", "rev-parse", "HEAD^"], cwd=root, text=True).strip()
        branch = subprocess.check_output(["git", "branch", "--show-current"], cwd=root, text=True).strip()
        require(parent == BASE_REF, "HEAD is not one direct successor to accepted R5")
        require(branch == "stage5g-lifecycle", "wrong branch")

    contract = json.loads((root / CONTRACT).read_text())
    require(contract["stage"] == "5G-e-d-c", "stage drift")
    require(contract["accepted_predecessor"] == BASE_REF, "predecessor drift")
    require(contract["branch"] == "stage5g-lifecycle", "contract branch drift")
    require(contract["owning_entry_point"] == "apply_stage5g_fresh_truth_reduction", "entry drift")
    require(contract["input"] == "Stage5gFreshTruthReduction", "input drift")
    require(contract["results"] == RESULTS, "result partition drift")
    require(contract["replay_policy"] == "B_DISABLED", "replay policy drift")
    require(contract["failure_injection_point_count"] == 14, "failure count drift")
    require(contract["minimum_negative_mutation_count"] == 320, "negative floor drift")
    require(contract["application_evidence_schema_version"] == 1, "evidence schema drift")
    for field in [
        "canonical_transition_reused", "authenticated_export_drop_restore",
        "source_post_state_consumed_before_success", "pre_authority_returned_on_failure",
    ]:
        require(contract[field] is True, f"contract flag drift: {field}")
    require(all(value is False for value in contract["closed_surfaces"].values()), "surface opened")

    application = (root / APPLICATION).read_text()
    reducer = (root / REDUCER).read_text()
    parent = (root / PARENT).read_text()
    clean = (root / CLEAN_RESTART).read_text()
    order_position = (root / ORDER_POSITION).read_text()
    runtime = (root / RUNTIME).read_text()
    lib = (root / LIB).read_text()
    gate = (root / GATE).read_text()
    handoff = (root / HANDOFF).read_text()

    require(parent.count("mod application;") == 1, "application module registration drift")
    require(
        application.count("pub(crate) fn apply_stage5g_fresh_truth_reduction(") == 1,
        "owning entry must exist exactly once",
    )
    signature = application.split("pub(crate) fn apply_stage5g_fresh_truth_reduction(", 1)[1].split(") ->", 1)[0]
    require("reduction: Stage5gFreshTruthReduction" in signature, "entry does not consume reduction")
    require("&mut" not in signature and "Broker" not in signature, "forbidden input admitted")
    for result in RESULTS:
        require(application.count(f"struct {result}") == 1, f"result missing: {result}")
    require(enum_variants(application, "Stage5gFreshTruthApplicationFailurePoint") == FAILURE_POINTS,
            "failure-point order/content drift")
    require(struct_fields(parent, "Stage5gFreshTruthApplicationEvidenceV1") == EVIDENCE_FIELDS,
            "application evidence drift")
    require("candidate_fingerprint_sha256\n            == evidence.applied_post_semantic_fingerprint_sha256" in parent,
            "candidate/applied equality validation missing")
    require("&& !evidence.callback_invoked" in parent, "callback evidence guard missing")
    require("&& !evidence.transport_opened" in parent, "transport evidence guard missing")
    require("&& !evidence.exact_replay_enabled" in parent, "exact replay evidence guard missing")

    require("into_application_parts(self)" in reducer, "consuming reduction access missing")
    require("#[derive(Clone, Copy, Default)]\nstruct Stage5gHistoryEvidence" in reducer,
            "history evidence type missing")
    require(reducer.count(".with_history_counts(") >= 30, "history propagation coverage regressed")
    candidate_prefix = reducer.split("pub(crate) struct Stage5gOwnedReconciliationCandidate", 1)[0]
    candidate_attributes = candidate_prefix.rsplit("\n\n", 1)[-1]
    require("#[derive" not in candidate_attributes, "candidate gained derive capabilities")

    require(application.count("apply_stage5g_restart_canonical_order_position_state") == 2,
            "canonical transition not reused")
    require(application.count("stage5g_export_post_application_order_position") == 1,
            "post-package export missing")
    require(application.count("restore_stage5g_clean_restart") == 2, "fresh restore missing")
    require("drop(parts.restart);" in application, "source restart not consumed on success")
    active_body = order_position.split(
        "pub(crate) fn apply_stage5g_canonical_order_position_evidence(", 1
    )[1].split("enum Stage5gCanonicalStateTransition", 1)[0]
    restart_body = order_position.split(
        "pub(crate) fn apply_stage5g_restart_canonical_order_position_state(", 1
    )[1].split("fn canonical_state_failure", 1)[0]
    require("Stage5gCanonicalApplicationMode::RestartFreshTruth" in restart_body,
            "restart-only canonical mode missing")
    require("Stage5gCanonicalApplicationMode::ActiveSession" in active_body,
            "active canonical mode missing")
    require("stage5g_clean_reconstruction_candidate" in runtime, "fresh runtime reconstruction missing")
    require(
        "fresh_truth_application_evidence"
        in struct_fields(clean, "Stage5gCleanRestartProjectionV1"),
        "application evidence not package-bound",
    )

    for test in FOCUSED_TESTS:
        require(reducer.count(f"fn {test}()") == 1, f"focused witness missing: {test}")
    require("pub mod stage5g_edc_compile_fail_facade" in lib, "compile-fail facade missing")
    for witness in COMPILE_FAIL_WITNESSES:
        require(lib.count(witness) == 1, f"compile-fail witness missing: {witness}")

    forbidden = [
        "redis::", "reqwest", "Method::POST", "Method::DELETE", ".post(", ".delete(",
        "finam_client", "dispatch_order", "runtime_live", "on_broker_bar(", "on_timer(",
    ]
    for token in forbidden:
        require(token not in application, f"forbidden application token: {token}")

    require(f'accepted_ref="{BASE_REF}"' in gate, "gate predecessor drift")
    require(
        f'forbidden_ref="{FORBIDDEN_AUTHORITY_REF}"' in gate,
        "forbidden authority ref drift",
    )
    require(gate.count("bash scripts/forbidden_surface_scan.sh") == 1,
            "forbidden scanner invocation drift")
    require(gate.count("bash scripts/forbidden_surface_negative_harness.sh") == 1,
            "forbidden negative invocation drift")
    require(
        gate.index('git worktree add --detach "$forbidden_root"')
        < gate.index("bash scripts/forbidden_surface_scan.sh"),
        "forbidden scanner must run in its detached authority tree",
    )
    require('cd "$forbidden_root"' in gate, "forbidden scanner worktree isolation missing")
    for artifact in [
        "stage5g-edc-full-gate.txt",
        "stage5g-edc-source-manifest.json",
        "stage5g-edc-toolchain.txt",
        "stage5g-edc-evidence-manifest.json",
    ]:
        require(artifact in handoff, f"handoff evidence missing: {artifact}")
    require('replace(str(ROOT), "<REPO>")' in handoff, "repository path redaction missing")
    require('replace(str(Path.home()), "<HOME>")' in handoff, "home path redaction missing")
    require('stdout=subprocess.PIPE' in handoff and 'stderr=subprocess.STDOUT' in handoff,
            "full gate capture missing")
    require('["git", "archive", "--format=tar", "HEAD"]' in handoff,
            "handoff is not sourced from exact HEAD")

    design = (root / DESIGN).read_text()
    require("Policy B" in design and "ExactReplay` remains disabled" in design,
            "Policy B documentation missing")
    require("Stage 6" in design and "external cas/fsync" in design.lower(), "durability boundary missing")
    for doc in [STATUS, ONBOARDING]:
        text = (root / doc).read_text()
        require("2b2bcc6" in text and "Stage 5G-e-d-c" in text, f"status drift: {doc}")
        require("ExactReplay" in text and "disabled" in text, f"replay policy drift: {doc}")

    print("stage5g-edc-check: PASS")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    check(args.root.resolve(), not args.skip_git)


if __name__ == "__main__":
    main()
