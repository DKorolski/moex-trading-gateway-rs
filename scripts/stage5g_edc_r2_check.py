#!/usr/bin/env python3
"""Stage 5G-e-d-c R2 final application authority checker."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

BASE = "67e13aeecd3bf0dc33e570770b0e4b90f5fec0cf"
BRANCH = "stage5g-lifecycle"
APP = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/application.rs")
REDUCER = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/reducer.rs")
CLEAN = Path("crates/strategy-runtime-core/src/stage5g_clean_restart.rs")
PARENT = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
CONTRACT = Path("docs/stage-5/stage5g-e-d-c-r2-final-application-authority-contract.json")
DESIGN = Path("docs/stage-5/stage5g-e-d-c-r2-final-application-authority-contract.md")

FAILURES = [
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
    "BeforeReplayPolicyClassification",
    "AfterReplayPolicyClassifiedDisabledBeforeBlockedResult",
]

SOURCE_MUTATIONS = [
    "FreshPackageFingerprint",
    "PreRestartPackageFingerprint",
    "ReductionPreSemanticFingerprint",
    "OperationalIdentityCommitment",
    "FreshPackageId",
    "FreshSnapshotEpoch",
    "FreshCapturedAt",
    "SwapFreshIdAndEpoch",
    "HistoryCounts",
    "SourceProofCommitment",
]

SOURCE_MISMATCHES = [
    "WrongScenario",
    "WrongDisposition",
    "WrongReason",
    "WrongOperationalIdentity",
    "WrongCommandRequest",
    "WrongParentSnapshot",
    "WrongFreshPackageId",
    "WrongFreshEpoch",
    "WrongFreshCapturedAt",
    "WrongFreshFingerprint",
    "WrongPreRestartFingerprint",
    "WrongReductionPreFingerprint",
    "WrongHistoryCounts",
    "WrongSourceProofCommitment",
]


def require(ok: bool, message: str) -> None:
    if not ok:
        raise SystemExit(f"stage5g-edc-r2-check: FAIL: {message}")


def text(root: Path, path: Path) -> str:
    target = root / path
    require(target.is_file() and not target.is_symlink(), f"missing {path}")
    return target.read_text()


def enum_variants(source: str, name: str) -> list[str]:
    found = re.search(rf"enum\s+{name}\s*\{{(.*?)\n\}}", source, re.S)
    require(found is not None, f"missing enum {name}")
    return re.findall(r"(?m)^\s{4}([A-Z][A-Za-z0-9_]*)\s*,\s*$", found.group(1))


def struct_body(source: str, name: str) -> str:
    found = re.search(rf"struct\s+{name}(?:<[^>]+>)?\s*\{{(.*?)\n\}}", source, re.S)
    require(found is not None, f"missing struct {name}")
    return found.group(1)


def struct_fields(source: str, name: str) -> list[str]:
    body = struct_body(source, name)
    return re.findall(r"(?m)^\s+(?:pub\(crate\)\s+)?([a-z][A-Za-z0-9_]*):", body)


def function_body(source: str, name: str) -> str:
    found = re.search(rf"fn\s+{name}\s*\((.*?)\n\}}", source, re.S)
    require(found is not None, f"missing function {name}")
    return found.group(0)


def check(root: Path, check_git: bool) -> None:
    app = text(root, APP)
    reducer = text(root, REDUCER)
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
        require(head_parent == BASE, "HEAD is not one direct successor to 67e13ae")
        require(branch == BRANCH, "wrong branch")

    require(contract["stage"] == "5G-e-d-c R2", "contract stage drift")
    require(contract["base_ref"] == BASE, "contract base drift")
    require(contract["application_evidence_schema_version"] == 3, "schema version drift")
    require(contract["failure_boundary_count"] == 14, "failure count drift")
    require(contract["grst_full_chain_witness_count"] == 12, "GRST count drift")
    require(contract["aggregate_negative_minimum"] >= 450, "mutation floor drift")
    require(all(v is False for v in contract["closed_surfaces"].values()), "surface opened")

    evidence_fields = struct_fields(app, "Stage5gFreshTruthApplicationEvidenceV1")
    authority_body = function_body(app, "stage5g_application_authority_sha256")
    authority_fields = struct_fields(authority_body, "Authority")
    material = set(evidence_fields)
    authority_material = set(authority_fields) - {"domain"}
    require(material == authority_material,
            f"authority/evidence field inventory mismatch: {sorted(material ^ authority_material)}")
    for field in [
        "parent_snapshot_id",
        "parent_snapshot_revision",
        "fresh_captured_at",
        "application_source_proof_sha256",
        "post_restart_package_fingerprint_sha256",
    ]:
        require(field in evidence_fields, f"R2 evidence field missing: {field}")

    source_proof = struct_body(app, "Stage5gFreshTruthApplicationSourceProof")
    require("pub(" not in source_proof and "pub " not in source_proof,
            "source proof field escaped")
    source_proof_prefix = app[: app.find("struct Stage5gFreshTruthApplicationSourceProof")].rsplit("\n\n", 1)[-1]
    require("#[derive" not in source_proof_prefix, "source proof gained derive capabilities")
    require(app.count("Stage5gFreshTruthApplicationSourceProof::from_application_parts(&parts, &candidate)") == 1,
            "source proof construction callsite drift")
    require(app.count("fn from_application_parts(") == 1, "source proof constructor drift")
    require(app.count("validate_stage5g_application_evidence_against_source(&evidence, &source_proof)?") == 1,
            "source/evidence validation callsite drift")
    require(
        enum_variants(app, "Stage5gFreshTruthApplicationSourceMismatch")
        == SOURCE_MISMATCHES,
        "typed source mismatch inventory drift",
    )

    token = struct_body(app, "Stage5gValidatedPostApplication")
    require("pub(" not in token and "pub " not in token, "validated token field escaped")
    require("authority_commitment_sha256" not in token,
            "validated token still carries pre-final authority")
    require(app.count("Stage5gValidatedPostApplication::new(") == 1,
            "validated token mint count drift")
    require("source_proof: Stage5gFreshTruthApplicationSourceProof" in app,
            "validated token does not consume source proof")
    require("Result<Self, Stage5gFreshTruthApplicationSourceMismatch>" in app,
            "validated token constructor is not fallible on source mismatch")
    require("let authority_commitment_sha256 = stage5g_application_authority_sha256(&evidence);" not in app,
            "application authority calculated before post fingerprint")

    finalized = struct_body(app, "Stage5gFinalizedPostApplication")
    require("pub(" not in finalized and "pub " not in finalized, "finalized token field escaped")
    require("authority_commitment_sha256" in finalized, "finalized token lacks authority")
    require(app.count("fn finalize_post_restart_package_fingerprint(") == 1,
            "finalization method definition drift in application module")
    require("stage5g_application_authority_sha256(&self.evidence)" in app,
            "final authority is not calculated from completed evidence")
    require(clean.count("finalize_post_restart_package_fingerprint(post_package_fingerprint)") == 1,
            "clean exporter does not finalize after post fingerprint")

    require(clean.count("stage5g_application_post_package_fingerprint_matches_projection(") == 2,
            "restore-side post-package fingerprint check/call drift")
    require(clean.count("stage5g_post_application_package_fingerprint_sha256_from_parent(") == 3,
            "post-package fingerprint recomputation helper/call drift")
    require("evidence.parent_snapshot_id()" in clean and "evidence.parent_snapshot_revision()" in clean,
            "restore recompute does not use parent binding")
    require("evidence.post_restart_package_fingerprint_sha256()" in clean,
            "restore recompute does not compare persisted post fingerprint")

    require(enum_variants(app, "Stage5gFreshTruthApplicationFailurePoint") == FAILURES,
            "fourteen failure boundaries drifted")
    for phase in [
        "serializer_called", "runtime_reconstruction_called",
        "replay_policy_classification_started",
        "replay_policy_classified_disabled", "blocked_result_constructed",
    ]:
        require(f"pub(crate) {phase}: usize" in app, f"R2 trace field missing: {phase}")
    require("trait Stage5gApplicationProjectionSerializer" in clean
            and clean.count("Stage5gFailingApplicationProjectionSerializer") == 3
            and "SerializerCalled" in clean,
            "serialization failure is not inside serializer adapter")
    require("trait Stage5gRuntimeReconstructionAdapter" in clean
            and clean.count("Stage5gFailingRuntimeReconstructionAdapter") == 3
            and "RuntimeReconstructionCalled" in clean,
            "restore failure is not inside reconstruction adapter")
    require("BeforeDisabledReplayProjection" not in all_rust
            and "AfterDisabledReplayProjectionBeforeAuthentication" not in all_rust,
            "nominal Policy-B failure names survived")
    require("ReplayPolicyClassificationStarted" in app
            and "ReplayPolicyClassifiedDisabled" in app,
            "Policy-B classification phases missing")

    require(enum_variants(app, "Stage5gFreshTruthApplicationSourceMutation") == SOURCE_MUTATIONS,
            "source mutation witness inventory drift")
    require("stage5g_edc_r2_application_source_mapping_mismatches_fail_before_post_token" in reducer,
            "R2 source mapping behavioral witness missing")
    require('"post_package_fingerprint",' in reducer
            and 'evidence["post_restart_package_fingerprint_sha256"] =' in reducer,
            "R2 post-package fingerprint tamper witness missing")
    require(app.count("stage5g_application_source_proof_sha256_from_evidence") == 2,
            "source proof commitment self-consistency missing")

    for actual in [
        "Stage5gFreshTruthReduction",
        "Stage5gOwnedReconciliationCandidate",
        "Stage5gValidatedPostApplication",
        "Stage5gFreshTruthApplicationSourceProof",
        "Stage5gFinalizedPostApplication",
    ]:
        require(actual in reducer and actual in lib, f"actual ownership type missing: {actual}")

    witnesses = re.findall(r"fn (stage5g_edc_grst\d{2}_[a-z0-9_]+)\(\)", reducer)
    require(len(witnesses) == 12, f"expected 12 named GRST witnesses, got {len(witnesses)}")
    require([int(re.search(r"grst(\d{2})", name).group(1)) for name in witnesses] == list(range(1, 13)),
            "GRST witnesses are not exactly 01..12 in order")

    require("STAGE5G_FRESH_TRUTH_APPLICATION_EVIDENCE_SCHEMA_VERSION: u16 = 3" in parent,
            "application evidence schema drift")
    forbidden = [
        "redis::", "reqwest", "Method::POST", "Method::DELETE", ".post(", ".delete(",
        "finam_client", "dispatch_order", "runtime_live", "on_broker_bar(", "on_timer(",
    ]
    for value in forbidden:
        require(value not in app, f"forbidden application surface: {value}")
    require("external durable storage" in design and "Stage 6" in design, "scope docs drift")
    require("ExactReplay remains disabled" in design, "Policy B docs drift")
    print("stage5g-edc-r2-check: PASS")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    check(args.root.resolve(), not args.skip_git)


if __name__ == "__main__":
    main()
