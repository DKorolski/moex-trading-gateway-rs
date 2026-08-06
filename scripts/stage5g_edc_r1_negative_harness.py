#!/usr/bin/env python3
"""Named mutation matrix for the Stage 5G-e-d-c R1 controlling proof."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/application.rs")
REDUCER = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/reducer.rs")
CLEAN = Path("crates/strategy-runtime-core/src/stage5g_clean_restart.rs")
ORDER = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
STAGE5D = Path("crates/strategy-runtime-core/src/stage5d_persistence.rs")


def replace_once(source: str, old: str, new: str) -> str:
    if source.count(old) < 1:
        raise RuntimeError(f"mutation anchor missing: {old[:100]!r}")
    return source.replace(old, new, 1)


def cases() -> list[tuple[str, Path, str, str]]:
    values: list[tuple[str, Path, str, str]] = []
    failures = [
        "BeforeCandidateExtraction", "AfterCandidateExtraction",
        "AfterPreflightBeforeTransition", "InsideCanonicalTransition",
        "AfterTransitionBeforeEquality", "AfterEqualityBeforeExport",
        "DuringSerialization", "AfterBytesBeforeSourceDrop",
        "AfterSourceDropBeforeDecode", "DuringAuthenticationVerification",
        "DuringRestore", "AfterRestoreBeforeEvidenceEquality",
        "BeforeDisabledReplayProjection",
        "AfterDisabledReplayProjectionBeforeAuthentication",
    ]
    for value in failures:
        values.append((f"remove-failure-boundary-{value}", APP, f"    {value},\n", ""))
    for index in range(1, 13):
        marker = f"fn stage5g_edc_grst{index:02}_"
        values.append((f"remove-grst{index:02}-application-witness", REDUCER, marker, f"fn removed_grst{index:02}_"))
    mismatches = [
        "OrderStatus", "FilledQuantity", "RemainingQuantity", "TradePayload", "TradeSet",
        "Position", "TargetIdentity", "IntentClass", "Attribution", "UnrelatedSlot",
        "LastTotalSequence", "CurrentEvidenceIdentity", "Watermark",
    ]
    for value in mismatches:
        values.append((
            f"remove-independent-mismatch-{value}", REDUCER,
            f"Stage5gRestartApplicationMismatch::{value}",
            f"Stage5gRestartApplicationMismatch::Removed{value}",
        ))
    values.extend([
        ("forge-post-package-without-application", CLEAN,
         "validated: crate::stage5g_fresh_broker_truth::Stage5gValidatedPostApplication",
         "state: crate::stage5g_order_position::Stage5gOrderPositionState"),
        ("second-post-token-construction", APP,
         "Stage5gValidatedPostApplication::new(",
         "Stage5gValidatedPostApplication::new(Stage5gValidatedPostApplication::new("),
        ("post-token-gains-clone", APP,
         "pub(crate) struct Stage5gValidatedPostApplication {",
         "#[derive(Clone)]\npub(crate) struct Stage5gValidatedPostApplication {"),
        ("second-canonical-transition-callsite", APP,
         "let transition =\n        apply_stage5g_restart_canonical_order_position_state(pre_state, canonical_evidence);",
         "let _second = apply_stage5g_restart_canonical_order_position_state(pre_state.clone(), canonical_evidence);\n    let transition = apply_stage5g_restart_canonical_order_position_state(pre_state, canonical_evidence);"),
        ("post-state-matches-always-true", APP,
         "candidate_fingerprint != post_state_fingerprint",
         "false"),
        ("remove-independent-restored-hash", APP,
         "restored_state_semantic_fingerprint(",
         "removed_restored_hash("),
        ("remove-global-state-invariants", APP,
         "stage5g_restart_application_global_invariants(",
         "removed_global_proof("),
        ("remove-inner-application-hmac", CLEAN,
         "verify_application_authority_hmac(",
         "removed_inner_hmac_check("),
        ("accept-fully-resealed-semantic-tamper", REDUCER,
         "fn stage5g_edc_r1_fully_resealed_evidence_and_state_tamper_matrix_is_rejected()",
         "fn removed_fully_resealed_evidence_and_state_tamper_matrix()"),
        ("replace-actual-type-assertion-with-facade", REDUCER,
         "fn stage5g_edc_r1_actual_production_types_are_linear_and_non_serializable()",
         "fn removed_actual_type_assertion()"),
        ("synthetic-reduction-compile-fail-facade", LIB,
         "Option<crate::stage5g_fresh_broker_truth::Stage5gFreshTruthReduction>",
         "Option<()>"),
        ("remove-fully-resealed-helper", STAGE5D,
         "pub(crate) fn stage5g_test_fully_reseal_application_package(",
         "pub(crate) fn removed_fully_reseal_application_package("),
        ("move-canonical-failure-before-commit", ORDER,
         "flag.replace(false)", "flag.get()"),
        ("forge-post-package-from-new-sibling", LIB,
         "Option<crate::stage5g_fresh_broker_truth::Stage5gValidatedPostApplication>",
         "Option<crate::stage5g_fresh_broker_truth::Stage5gValidatedPostApplication>; fn forged(){ stage5g_export_post_application_order_position("),
        ("forge-post-package-through-alias-wrapper", LIB,
         "Option<crate::stage5g_fresh_broker_truth::Stage5gValidatedPostApplication>",
         "Option<crate::stage5g_fresh_broker_truth::Stage5gValidatedPostApplication>; macro_rules! forged {()=>{stage5g_export_post_application_order_position(}}"),
        ("preflight-always-true", APP,
         "if !candidate.application_preflight_matches(",
         "if false && !candidate.application_preflight_matches("),
        ("restored-matches-always-true", APP,
         "fingerprint == evidence.restored_post_semantic_fingerprint_sha256()",
         "true || fingerprint == evidence.restored_post_semantic_fingerprint_sha256()"),
        ("copy-candidate-hash-as-post-hash", APP,
         "applied_post_semantic_fingerprint_sha256: post_state_fingerprint.clone()",
         "applied_post_semantic_fingerprint_sha256: candidate_fingerprint.clone()"),
        ("all-failure-points-alias-first", APP,
         "AfterCandidateExtraction", "BeforeCandidateExtraction"),
        ("remove-independent-post-hash", APP,
         "candidate.post_state_semantic_fingerprint(",
         "candidate.removed_post_state_semantic_fingerprint("),
        ("remove-exact-disposition-witness", REDUCER,
         "fn stage5g_edc_r1_exact_disposition_terminal_inconsistency()",
         "fn removed_exact_disposition_terminal_inconsistency()"),
    ])
    return values


def main() -> None:
    matrix = cases()
    if len(matrix) < 50:
        raise SystemExit(f"stage5g-edc-r1-negative: FAIL: only {len(matrix)} cases")
    with tempfile.TemporaryDirectory(prefix="stage5g-edc-r1-negative-") as temp:
        work = Path(temp) / "repo"
        shutil.copytree(
            ROOT,
            work,
            ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports", "*.zip"),
        )
        originals = {path: (ROOT / path).read_text() for _, path, _, _ in matrix}
        for name, path, old, new in matrix:
            target = work / path
            target.write_text(replace_once(originals[path], old, new))
            result = subprocess.run(
                ["python3", "scripts/stage5g_edc_r1_check.py", "--root", str(work), "--skip-git"],
                cwd=work,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.STDOUT,
                check=False,
            )
            target.write_text(originals[path])
            if result.returncode == 0:
                raise SystemExit(f"stage5g-edc-r1-negative: FAIL: survived {name}")
            print(f"PASS {name}")
    print(f"stage5g-edc-r1-negative: PASS {len(matrix)}/{len(matrix)}")


if __name__ == "__main__":
    main()
