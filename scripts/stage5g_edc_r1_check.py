#!/usr/bin/env python3
"""Stage 5G-e-d-c R1 structural, semantic-witness and closure checker."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

BASE = "18240b26a5bea77ea71c851f72a644706a7e0b57"
BRANCH = "stage5g-lifecycle"
APP = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/application.rs")
REDUCER = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/reducer.rs")
ORDER = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
CLEAN = Path("crates/strategy-runtime-core/src/stage5g_clean_restart.rs")
PARENT = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
CONTRACT = Path("docs/stage-5/stage5g-e-d-c-r1-application-authority-contract.json")
DESIGN = Path("docs/stage-5/stage5g-e-d-c-r1-application-authority-contract.md")

FAILURES = [
    "BeforeCandidateExtraction", "AfterCandidateExtraction",
    "AfterPreflightBeforeTransition", "InsideCanonicalTransition",
    "AfterTransitionBeforeEquality", "AfterEqualityBeforeExport",
    "DuringSerialization", "AfterBytesBeforeSourceDrop",
    "AfterSourceDropBeforeDecode", "DuringAuthenticationVerification",
    "DuringRestore", "AfterRestoreBeforeEvidenceEquality",
    "BeforeDisabledReplayProjection",
    "AfterDisabledReplayProjectionBeforeAuthentication",
]


def require(ok: bool, message: str) -> None:
    if not ok:
        raise SystemExit(f"stage5g-edc-r1-check: FAIL: {message}")


def text(root: Path, path: Path) -> str:
    target = root / path
    require(target.is_file() and not target.is_symlink(), f"missing {path}")
    return target.read_text()


def enum_variants(source: str, name: str) -> list[str]:
    found = re.search(rf"enum\s+{name}\s*\{{(.*?)\n\}}", source, re.S)
    require(found is not None, f"missing enum {name}")
    return re.findall(r"(?m)^\s{4}([A-Z][A-Za-z0-9_]*)\s*,\s*$", found.group(1))


def check(root: Path, check_git: bool) -> None:
    app = text(root, APP)
    reducer = text(root, REDUCER)
    order = text(root, ORDER)
    clean = text(root, CLEAN)
    parent = text(root, PARENT)
    lib = text(root, LIB)
    design = text(root, DESIGN)
    contract = json.loads(text(root, CONTRACT))
    source_root = root / "crates/strategy-runtime-core/src"
    all_rust = "\n".join(path.read_text() for path in sorted(source_root.rglob("*.rs")))

    if check_git:
        head_parent = subprocess.check_output(
            ["git", "rev-parse", "HEAD^"], cwd=root, text=True
        ).strip()
        branch = subprocess.check_output(
            ["git", "branch", "--show-current"], cwd=root, text=True
        ).strip()
        require(head_parent == BASE, "HEAD is not one direct successor to 18240b2")
        require(branch == BRANCH, "wrong branch")

    require(contract["stage"] == "5G-e-d-c R1", "contract stage drift")
    require(contract["base_ref"] == BASE, "contract base drift")
    require(contract["failure_boundary_count"] == 14, "failure count drift")
    require(contract["grst_full_chain_witness_count"] == 12, "GRST count drift")
    require(contract["aggregate_negative_minimum"] >= 400, "mutation floor drift")
    require(all(v is False for v in contract["closed_surfaces"].values()), "surface opened")

    token = re.search(
        r"pub\(crate\) struct Stage5gValidatedPostApplication\s*\{(.*?)\n\}", app, re.S
    )
    require(token is not None, "opaque post token missing")
    require("pub(" not in token.group(1) and "pub " not in token.group(1), "token field escaped")
    token_prefix = app[: token.start()].rsplit("\n\n", 1)[-1]
    require("#[derive" not in token_prefix, "post token gained derive capabilities")
    require(app.count("fn new(") == 1, "private token constructor drift")
    require(app.count("Stage5gValidatedPostApplication::new(") == 1, "token mint count drift")
    require(app.count("stage5g_export_post_application_order_position(validated_post_application") == 1,
            "post token consumption drift")
    export = clean.split("fn stage5g_export_post_application_order_position(", 1)[1].split(") ->", 1)[0]
    require("validated: crate::stage5g_fresh_broker_truth::Stage5gValidatedPostApplication" in export,
            "exporter does not consume opaque token")
    for forbidden in ["state:", "fresh_package_id:", "application_evidence:"]:
        require(forbidden not in export, f"raw exporter authority returned: {forbidden}")

    require(app.count("apply_stage5g_restart_canonical_order_position_state(pre_state, canonical_evidence)") == 1,
            "restart canonical transition must have one application callsite")
    require(app.count("apply_stage5g_restart_canonical_order_position_state(") == 1,
            "second restart canonical transition callsite detected")
    require(all_rust.count("apply_stage5g_restart_canonical_order_position_state(") == 2,
            "canonical transition escaped its definition/single-call source set")
    require(all_rust.count("stage5g_export_post_application_order_position(") == 2,
            "post-package exporter escaped its definition/single-call source set")
    require(order.count("stage5g_test_fail_restart_canonical_before_commit") == 1,
            "inside-transition seam drift")
    require("flag.replace(false)" in order, "inside-transition seam is not consumed at commit")

    require(enum_variants(app, "Stage5gFreshTruthApplicationFailurePoint") == FAILURES,
            "fourteen failure boundaries drifted or aliased")
    expected_failure_mentions = {
        "BeforeCandidateExtraction": 2, "AfterCandidateExtraction": 2,
        "AfterPreflightBeforeTransition": 2, "InsideCanonicalTransition": 3,
        "AfterTransitionBeforeEquality": 2, "AfterEqualityBeforeExport": 2,
        "DuringSerialization": 3, "AfterBytesBeforeSourceDrop": 3,
        "AfterSourceDropBeforeDecode": 3, "DuringAuthenticationVerification": 3,
        "DuringRestore": 3, "AfterRestoreBeforeEvidenceEquality": 2,
        "BeforeDisabledReplayProjection": 2,
        "AfterDisabledReplayProjectionBeforeAuthentication": 2,
    }
    failure_sources = app + clean + order
    for name, count in expected_failure_mentions.items():
        require(failure_sources.count(name) == count, f"failure seam alias/drift: {name}")
    for phase in [
        "candidate_extracted", "preflight_completed", "canonical_transition_started",
        "canonical_transition_completed", "post_equality_completed", "serialization_started",
        "bytes_produced", "post_state_consumed", "decode_started",
        "authentication_started", "authentication_completed", "restore_started",
        "restore_completed", "final_equality_completed",
    ]:
        require(phase in app, f"trace phase missing: {phase}")
    require("fail_during_serialization" in clean and "SerializationStarted" in clean,
            "serialization seam is not inside exporter")
    require("injected_authentication_key" in clean and "AuthenticationStarted" in clean,
            "authentication seam is not inside verifier")
    require("DuringRestore" in clean and "RestoreStarted" in clean,
            "restore seam is not inside restore operation")
    require("Grst09ExactReplayIsIdempotent" in app, "Policy-B replay branch missing")

    require("Stage5gApplicationSemanticProjection" in reducer, "canonical semantic projection missing")
    require("application_semantic_fingerprint" in reducer, "candidate hash missing")
    require("post_state_semantic_fingerprint" in reducer, "post-state hash missing")
    require(app.count("candidate.post_state_semantic_fingerprint(") == 1,
            "independent post-state hash call missing or duplicated")
    require("restored_state_semantic_fingerprint" in reducer, "restored-state hash missing")
    require(app.count("stage5g_restart_application_global_invariants(") == 1,
            "global invariant proof must have exactly one application call")
    require("candidate_fingerprint != post_state_fingerprint" in app,
            "candidate/post independent equality missing")
    require("if !candidate.application_preflight_matches(" in app, "candidate preflight bypassed")
    require("applied_post_semantic_fingerprint_sha256: post_state_fingerprint.clone()" in app,
            "post fingerprint is not independently derived")
    require("fingerprint == evidence.restored_post_semantic_fingerprint_sha256()" in app,
            "restored semantic equality bypassed")
    require("true || fingerprint == evidence.restored_post_semantic_fingerprint_sha256()" not in app,
            "restored semantic equality forced true")
    require("restored_state_semantic_fingerprint" in app, "restored independent equality missing")

    require("fresh_truth_application_authority_hmac_sha256" in clean,
            "inner application authority HMAC missing")
    require(clean.count("verify_application_authority_hmac(") == 2,
            "inner application HMAC must have one verifier and one validation call")
    require("stage5g_application_evidence_matches_state" in clean,
            "evidence/state cross-binding missing")
    require("stage5g_test_fully_reseal_application_package" in text(root, Path("crates/strategy-runtime-core/src/stage5d_persistence.rs")),
            "fully resealed adversarial helper missing")
    require("stage5g_edc_r1_fully_resealed_evidence_and_state_tamper_matrix_is_rejected" in reducer,
            "fully resealed tamper matrix missing")

    witnesses = re.findall(r"fn (stage5g_edc_grst\d{2}_[a-z0-9_]+)\(\)", reducer)
    require(len(witnesses) == 12, f"expected 12 named GRST witnesses, got {len(witnesses)}")
    require([int(re.search(r"grst(\d{2})", name).group(1)) for name in witnesses] == list(range(1, 13)),
            "GRST witnesses are not exactly 01..12 in order")
    require("stage5g_edc_r1_independent_post_state_mismatches_fail_before_package_commit" in reducer,
            "independent mismatch matrix missing")
    for mismatch in [
        "OrderStatus", "FilledQuantity", "RemainingQuantity", "TradePayload", "TradeSet",
        "Position", "TargetIdentity", "IntentClass", "Attribution", "UnrelatedSlot",
        "LastTotalSequence", "CurrentEvidenceIdentity", "Watermark",
    ]:
        require(reducer.count(f"Stage5gRestartApplicationMismatch::{mismatch}") == 1,
                f"mismatch witness drift: {mismatch}")

    require("stage5g_edc_r1_actual_production_types_are_linear_and_non_serializable" in reducer,
            "actual-type assertions missing")
    for disposition in [
        "apply_owned_candidate", "continue_from_committed_checkpoint", "await_fresh_broker_truth",
        "reconciliation_required", "manual_intervention_required", "terminal_inconsistency",
        "exact_replay_disabled",
    ]:
        require(f"stage5g_edc_r1_exact_disposition_{disposition}" in reducer,
                f"exact disposition witness missing: {disposition}")
    for actual in ["Stage5gFreshTruthReduction", "Stage5gOwnedReconciliationCandidate",
                   "Stage5gValidatedPostApplication"]:
        require(actual in reducer, f"actual ownership type missing: {actual}")
    require("Option<crate::stage5g_fresh_broker_truth::Stage5gFreshTruthReduction>" in lib,
            "compile-fail facade does not contain actual reduction")
    require("Option<crate::stage5g_fresh_broker_truth::Stage5gValidatedPostApplication>" in lib,
            "compile-fail facade does not contain actual post token")

    application_evidence = re.search(
        r"struct Stage5gFreshTruthApplicationEvidenceV1\s*\{(.*?)\n\}", app, re.S
    )
    require(application_evidence is not None, "application evidence missing")
    require("pub(" not in application_evidence.group(1), "application evidence fields escaped")

    forbidden = [
        "redis::", "reqwest", "Method::POST", "Method::DELETE", ".post(", ".delete(",
        "finam_client", "dispatch_order", "runtime_live", "on_broker_bar(", "on_timer(",
    ]
    for value in forbidden:
        require(value not in app, f"forbidden application surface: {value}")
    require("Policy B" in design and "ExactReplay is disabled" in design, "Policy B docs drift")
    require("external durable journal" in design and "Stage 6" in design, "scope docs drift")
    require("STAGE5G_FRESH_TRUTH_APPLICATION_EVIDENCE_SCHEMA_VERSION: u16 = 2" in parent,
            "application evidence schema drift")
    print("stage5g-edc-r1-check: PASS")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    check(args.root.resolve(), not args.skip_git)


if __name__ == "__main__":
    main()
