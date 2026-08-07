#!/usr/bin/env python3
"""Named mutation matrix for Stage 5G-e-d-c R2 source/final-authority closure."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/application.rs")
REDUCER = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/reducer.rs")
CLEAN = Path("crates/strategy-runtime-core/src/stage5g_clean_restart.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
PARENT = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs")
CONTRACT = Path("docs/stage-5/stage5g-e-d-c-r2-final-application-authority-contract.json")
DESIGN = Path("docs/stage-5/stage5g-e-d-c-r2-final-application-authority-contract.md")


def replace_once(source: str, old: str, new: str) -> str:
    if source.count(old) < 1:
        raise RuntimeError(f"mutation anchor missing: {old[:120]!r}")
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
        "BeforeReplayPolicyClassification",
        "AfterReplayPolicyClassifiedDisabledBeforeBlockedResult",
    ]
    for value in failures:
        values.append((f"remove-failure-boundary-{value}", APP, f"    {value},\n", ""))

    for index in range(1, 13):
        marker = f"fn stage5g_edc_grst{index:02}_"
        values.append((f"remove-grst{index:02}-full-chain-witness", REDUCER, marker,
                       f"fn removed_stage5g_edc_grst{index:02}_"))

    source_mutations = [
        "FreshPackageFingerprint", "PreRestartPackageFingerprint",
        "ReductionPreSemanticFingerprint", "OperationalIdentityCommitment",
        "FreshPackageId", "FreshSnapshotEpoch", "FreshCapturedAt",
        "SwapFreshIdAndEpoch", "HistoryCounts", "SourceProofCommitment",
    ]
    for value in source_mutations:
        values.append((f"remove-source-mutation-{value}", APP, f"    {value},\n", ""))

    source_mismatches = [
        "WrongScenario", "WrongDisposition", "WrongReason", "WrongOperationalIdentity",
        "WrongCommandRequest", "WrongParentSnapshot", "WrongFreshPackageId",
        "WrongFreshEpoch", "WrongFreshCapturedAt", "WrongFreshFingerprint",
        "WrongPreRestartFingerprint", "WrongReductionPreFingerprint",
        "WrongHistoryCounts", "WrongSourceProofCommitment",
    ]
    for value in source_mismatches:
        values.append((f"remove-source-mismatch-{value}", APP, f"    {value},\n", ""))

    trace_fields = [
        "serializer_called", "runtime_reconstruction_called",
        "replay_policy_classification_started",
        "replay_policy_classified_disabled", "blocked_result_constructed",
    ]
    for value in trace_fields:
        values.append((f"remove-real-trace-field-{value}", APP,
                       f"    pub(crate) {value}: usize,\n", ""))

    evidence_fields = [
        "parent_snapshot_id", "parent_snapshot_revision", "fresh_captured_at",
        "application_source_proof_sha256", "post_restart_package_fingerprint_sha256",
    ]
    for value in evidence_fields:
        values.append((f"remove-evidence-field-{value}", APP, f"    {value}:", f"    removed_{value}:"))

    values.extend([
        ("lower-application-evidence-schema", PARENT,
         "STAGE5G_FRESH_TRUTH_APPLICATION_EVIDENCE_SCHEMA_VERSION: u16 = 3",
         "STAGE5G_FRESH_TRUTH_APPLICATION_EVIDENCE_SCHEMA_VERSION: u16 = 2"),
        ("contract-base-ref-drift", CONTRACT,
         "67e13aeecd3bf0dc33e570770b0e4b90f5fec0cf",
         "18240b26a5bea77ea71c851f72a644706a7e0b57"),
        ("contract-schema-version-drift", CONTRACT,
         '"application_evidence_schema_version": 3',
         '"application_evidence_schema_version": 2'),
        ("contract-opens-redis-surface", CONTRACT,
         '"redis": false',
         '"redis": true'),
        ("contract-negative-floor-lowered", CONTRACT,
         '"aggregate_negative_minimum": 450',
         '"aggregate_negative_minimum": 1'),
        ("source-proof-field-escaped", APP,
         "pub(crate) struct Stage5gFreshTruthApplicationSourceProof {\n    scenario_id: String,\n",
         "pub(crate) struct Stage5gFreshTruthApplicationSourceProof {\n    pub(crate) scenario_id: String,\n"),
        ("source-proof-gains-derive", APP,
         "pub(crate) struct Stage5gFreshTruthApplicationSourceProof {",
         "#[derive(Clone)]\npub(crate) struct Stage5gFreshTruthApplicationSourceProof {"),
        ("source-proof-constructor-callsite-removed", APP,
         "Stage5gFreshTruthApplicationSourceProof::from_application_parts(&parts, &candidate)",
         "Stage5gFreshTruthApplicationSourceProof::forged_from_strings(&parts, &candidate)"),
        ("source-proof-constructor-renamed", APP,
         "fn from_application_parts(",
         "fn forged_from_application_parts("),
        ("source-validation-call-removed", APP,
         "validate_stage5g_application_evidence_against_source(&evidence, &source_proof)?;",
         "let _ = (&evidence, &source_proof);"),
        ("validated-token-field-escaped", APP,
         "    evidence: Stage5gFreshTruthApplicationEvidenceV1,\n    #[cfg(test)]",
         "    pub(crate) evidence: Stage5gFreshTruthApplicationEvidenceV1,\n    #[cfg(test)]"),
        ("validated-token-pre-final-authority-restored", APP,
         "    evidence: Stage5gFreshTruthApplicationEvidenceV1,\n    #[cfg(test)]",
         "    evidence: Stage5gFreshTruthApplicationEvidenceV1,\n    authority_commitment_sha256: String,\n    #[cfg(test)]"),
        ("validated-token-constructor-not-source-fallible", APP,
         "Result<Self, Stage5gFreshTruthApplicationSourceMismatch>",
         "Self"),
        ("second-validated-token-mint", APP,
         "Stage5gValidatedPostApplication::new(",
         "Stage5gValidatedPostApplication::new(Stage5gValidatedPostApplication::new("),
        ("early-application-authority-calculation", APP,
         "    let fresh_runtime = parts.restart.stage5g_fresh_reconstruction_candidate();",
         "    let authority_commitment_sha256 = stage5g_application_authority_sha256(&evidence);\n    let fresh_runtime = parts.restart.stage5g_fresh_reconstruction_candidate();"),
        ("finalized-token-field-escaped", APP,
         "    authority_commitment_sha256: String,\n    #[cfg(test)]",
         "    pub(crate) authority_commitment_sha256: String,\n    #[cfg(test)]"),
        ("finalized-token-lacks-authority", APP,
         "    authority_commitment_sha256: String,\n",
         ""),
        ("finalization-method-renamed", APP,
         "fn finalize_post_restart_package_fingerprint(",
         "fn finalize_without_post_restart_package_fingerprint("),
        ("final-authority-not-from-completed-evidence", APP,
         "stage5g_application_authority_sha256(&self.evidence)",
         "stage5g_application_authority_sha256(&Stage5gFreshTruthApplicationEvidenceV1::default())"),
        ("clean-exporter-does-not-finalize-post-fingerprint", CLEAN,
         "finalize_post_restart_package_fingerprint(post_package_fingerprint)",
         "finalize_post_restart_package_fingerprint(\"0\".repeat(64))"),
        ("restore-post-package-check-removed", CLEAN,
         "stage5g_application_post_package_fingerprint_matches_projection(",
         "removed_post_package_projection_check("),
        ("post-package-recompute-helper-removed", CLEAN,
         "stage5g_post_application_package_fingerprint_sha256_from_parent(",
         "removed_post_application_package_fingerprint_sha256_from_parent("),
        ("restore-parent-id-binding-removed", CLEAN,
         "evidence.parent_snapshot_id()",
         "evidence.fresh_package_id()"),
        ("restore-parent-revision-binding-removed", CLEAN,
         "evidence.parent_snapshot_revision()",
         "0"),
        ("restore-persisted-post-fingerprint-compare-removed", CLEAN,
         "evidence.post_restart_package_fingerprint_sha256()",
         "\"0\""),
        ("serializer-adapter-trait-removed", CLEAN,
         "trait Stage5gApplicationProjectionSerializer",
         "trait RemovedStage5gApplicationProjectionSerializer"),
        ("serializer-failing-adapter-removed", CLEAN,
         "Stage5gFailingApplicationProjectionSerializer",
         "RemovedApplicationProjectionSerializer"),
        ("restore-adapter-trait-removed", CLEAN,
         "trait Stage5gRuntimeReconstructionAdapter",
         "trait RemovedStage5gRuntimeReconstructionAdapter"),
        ("restore-failing-adapter-removed", CLEAN,
         "Stage5gFailingRuntimeReconstructionAdapter",
         "RemovedRuntimeReconstructionAdapter"),
        ("policy-b-old-before-name-restored", APP,
         "BeforeReplayPolicyClassification",
         "BeforeDisabledReplayProjection"),
        ("policy-b-old-after-name-restored", APP,
         "AfterReplayPolicyClassifiedDisabledBeforeBlockedResult",
         "AfterDisabledReplayProjectionBeforeAuthentication"),
        ("source-mapping-test-removed", REDUCER,
         "stage5g_edc_r2_application_source_mapping_mismatches_fail_before_post_token",
         "removed_r2_application_source_mapping_test"),
        ("post-package-tamper-witness-removed", REDUCER,
         '"post_package_fingerprint",',
         '"removed_post_package_fingerprint",'),
        ("source-proof-commitment-self-check-removed", APP,
         "== stage5g_application_source_proof_sha256_from_evidence(evidence)",
         "== evidence.application_source_proof_sha256"),
        ("actual-source-proof-type-facaded", LIB,
         "Option<crate::stage5g_fresh_broker_truth::Stage5gFreshTruthApplicationSourceProof>",
         "Option<()>"),
        ("actual-finalized-token-type-facaded", LIB,
         "Option<crate::stage5g_fresh_broker_truth::Stage5gFinalizedPostApplication>",
         "Option<()>"),
        ("design-durability-scope-removed", DESIGN,
         "external durable storage",
         "external-storage-scope-removed"),
        ("design-exact-replay-policy-removed", DESIGN,
         "ExactReplay remains disabled",
         "ExactReplay policy omitted"),
    ])
    return values


def main() -> None:
    matrix = cases()
    if len(matrix) < 70:
        raise SystemExit(f"stage5g-edc-r2-negative: FAIL: only {len(matrix)} cases")
    with tempfile.TemporaryDirectory(prefix="stage5g-edc-r2-negative-") as temp:
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
                ["python3", "scripts/stage5g_edc_r2_check.py", "--root", str(work), "--skip-git"],
                cwd=work,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.STDOUT,
                check=False,
            )
            target.write_text(originals[path])
            if result.returncode == 0:
                raise SystemExit(f"stage5g-edc-r2-negative: FAIL: survived {name}")
            print(f"PASS {name}")
    print(f"stage5g-edc-r2-negative: PASS {len(matrix)}/{len(matrix)}")


if __name__ == "__main__":
    main()
