#!/usr/bin/env python3
"""Fail-closed checker for Generation-2 Composition Rebuild R0-R1."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

import stage8b_p_r2b_generation2_composition_r0_check as r0


ROOT = Path(__file__).resolve().parents[1]
STAGE = (
    "Stage 8B-P R2B Generation-2 Composition Rebuild R0-R1 "
    "Exact Phase-6 Evidence Closure"
)
ACCEPTED_R0_REF = "1a1933f90075591a88d4631c7c72599a1262115d"
ACCEPTED_R0_ARCHIVE_SHA256 = (
    "df438c441e7646192c0dcc9160644e74a018d7095256302aec32748333e3cd04"
)
R0_AUTHORITY_SHA256 = "043e671995fa270395b38d0faf6f296616d5000d2a3f74871b1878419c795edd"
R0_REHEARSAL_SHA256 = "b6a5d6d2e3d7ea417b96a9223a0fdc678f86bece5905370d6ab250b58fce4e6d"
BUILD_EVIDENCE_SHA256 = "202a02e646f14f096741078250b6bed0836eb63161af19bdd640059f32747507"
BUILD_SOURCE_REF = "c7667658288577229b7cf00e9dcef519ba2fd1d7"
BUILD_SOURCE_TREE = "c3dff5f4338ea9bae82071eaacc48511ce3e1f7e"
HELPER_SHA256 = "90508e097c8668d6fe90a15ef6014e480a9042bb36f0613351c02465d10aaca1"
HELPER_AUTHORITY_SHA256 = "956463ccaac2396635cb772e7237b772a50f09c722ba531e78774efee6782a7f"
EFFECT_IDENTITY_SHA256 = "ca330934df540de69b52d0463a665a1ab0ff89fa13eeb663b162763cc6bc83a0"
ORACLE_ID = "EXACT_TYPED_ROOT_TERMINAL_EVIDENCE"
ALLOWED_ERRORS = ["NETWORK_CONNECT_FAILURE", "TIMEOUT"]
REHEARSAL = Path(
    "docs/stage-8/"
    "stage8b-p-r2b-generation2-composition-r0-r1-linux-rehearsal-evidence.json"
)
AUTHORITY = Path(
    "docs/stage-8/stage8b-p-r2b-generation2-composition-r0-r1-authority.json"
)
DESIGN = Path(
    "docs/stage-8/"
    "STAGE8B_P_R2B_GENERATION2_COMPOSITION_REBUILD_R0_R1_2026-09-01.md"
)
MATRIX = Path(
    "docs/stage-8/"
    "STAGE8B_P_R2B_GENERATION2_COMPOSITION_REBUILD_R0_R1_ACCEPTANCE_MATRIX_2026-09-01.csv"
)
ORACLE = Path(
    "scripts/stage8b_p_r2b_generation2_composition_r0_r1_terminal_oracle.py"
)
MATERIALIZER = Path(
    "scripts/stage8b_p_r2b_generation2_composition_r0_r1_materialize_phase6.py"
)
RUNNER = Path(
    "scripts/stage8b_p_r2b_generation2_composition_r0_r1_phase6_runner.sh"
)
HEX40 = re.compile(r"[0-9a-f]{40}")
HEX64 = re.compile(r"[0-9a-f]{64}")
EXPECTED_BINARY_HASHES = {
    "stage8b-r2a5-authority-issuer": "6dc5be078029a833b2e465525498c76e8d5966fa2c8d4733cfa3dce6b5af74e0",
    "stage8b-r2a5-authority-producer": "fa494d0150cb3ed0f5f05378a8e1636f3160499f9f5cc881cbbed862c96229fc",
    "stage8b-r2a5-helper-acceptance-issuer": "82617f8f97cbe2b729a83bb27aae8eccbb72ab94411224384307771c07f29ba5",
    "stage8b-r2a5-package-issuer": "5aff3f7d4747113546272cb40fc444b5bfa0013116b49d20669e8e757091625c",
    "stage8b-r2b-generation2-helper-acceptance-issuer": "23ab1f964d56b6739ac60a90baeffda4fd557bc132c9288d241008447f1e2cf6",
    "stage8b-r2b-launcher": "52dfbd0e6bb0d07a92a3104be50c33a60af08905b6cd075aa4bd4a4c373da17e",
    "stage8b-r2b-run-package-draft-builder": "f171fc282e56d509e30bb92ea40340e559b19dc12ac63f9513bed9a926b72207",
    "stage8b-readonly-preflight": HELPER_SHA256,
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load(root: Path, relative: Path) -> dict[str, Any]:
    value = json.loads((root / relative).read_text(encoding="utf-8"))
    require(isinstance(value, dict), f"JSON object required: {relative}")
    return value


def exact_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    require(set(value) == expected, f"{label} schema drift")


def required_files() -> set[Path]:
    return {
        r0.TRUST,
        r0.ACCOUNT,
        r0.SOURCE_ADAPTER,
        r0.PRODUCTION_AUTHORITY,
        r0.HELPER_AUTHORITY,
        r0.HELPER_PIN,
        r0.BUILD,
        r0.REHEARSAL,
        r0.AUTHORITY,
        r0.DESIGN,
        r0.MATRIX,
        r0.STATUS,
        r0.CORE,
        r0.LAUNCHER,
        r0.ISSUER,
        r0.ISSUE_SCRIPT,
        r0.BUILD_SCRIPT,
        r0.MATERIALIZER,
        r0.RUNNER,
        r0.BASE_PHASE6,
        *(Path(path) for path in r0.ACCEPTED_BACKUP_HASHES),
        REHEARSAL,
        AUTHORITY,
        DESIGN,
        MATRIX,
        ORACLE,
        MATERIALIZER,
        RUNNER,
    }


def check_immutable_r0(root: Path, artifact_root: Path | None) -> Path:
    require(sha256(root / r0.AUTHORITY) == R0_AUTHORITY_SHA256, "accepted R0 authority drift")
    require(sha256(root / r0.REHEARSAL) == R0_REHEARSAL_SHA256, "accepted R0 rehearsal drift")
    require(sha256(root / r0.BUILD) == BUILD_EVIDENCE_SHA256, "accepted build evidence drift")
    require(sha256(root / r0.HELPER_AUTHORITY) == HELPER_AUTHORITY_SHA256, "helper authority drift")
    build = load(root, r0.BUILD)
    require(build.get("source_ref") == BUILD_SOURCE_REF, "production build source drift")
    require(build.get("source_tree") == BUILD_SOURCE_TREE, "production build tree drift")
    require(build.get("helper_sha256") == HELPER_SHA256, "production helper drift")
    records = build.get("binaries")
    require(isinstance(records, dict), "build binary inventory missing")
    require(
        {name: record.get("build_a_sha256") for name, record in records.items()}
        == EXPECTED_BINARY_HASHES,
        "accepted binary hash inventory drift",
    )
    require(
        all(record.get("build_b_sha256") == EXPECTED_BINARY_HASHES[name] for name, record in records.items()),
        "accepted build-b binary inventory drift",
    )
    helper = load(root, r0.HELPER_AUTHORITY)
    require(
        helper.get("effect_build_identity_sha256") == EFFECT_IDENTITY_SHA256,
        "helper effect identity drift",
    )
    resolved = r0.resolve_artifact_root(root, artifact_root)
    r0.check(root, resolved)
    return resolved


def check_source_contract(root: Path) -> None:
    oracle = (root / ORACLE).read_text(encoding="utf-8")
    materializer = (root / MATERIALIZER).read_text(encoding="utf-8")
    runner = (root / RUNNER).read_text(encoding="utf-8")
    required_oracle = (
        'ORACLE_ID = "EXACT_TYPED_ROOT_TERMINAL_EVIDENCE"',
        'ALLOWED_REQUEST_ERRORS = ("NETWORK_CONNECT_FAILURE", "TIMEOUT")',
        'EXPECTED_METHOD = "POST"',
        'EXPECTED_ROUTE = "/v1/sessions"',
        "actual_read_attempts = bool(attempts)",
        '"root_lifecycle_timeout": False',
        'require(attempt["status"] is None',
        'require(attempt["response_body_length"] is None',
        'require(all(value is False for value in effect_flags.values())',
    )
    for marker in required_oracle:
        require(marker in oracle, f"typed oracle contract missing: {marker}")
    require("AUTH_SESSION_FAILURE" not in oracle, "category-only auth terminal allowed")
    require("actual_read_attempts = True" not in oracle, "request-attempt proof hardcoded")
    for marker in (
        str(ORACLE),
        'request_boundary_proof["actual_read_attempts"]',
        '"request_boundary_proof":request_boundary_proof',
    ):
        require(marker in materializer, f"materializer typed-proof binding missing: {marker}")
    require('--network none' in runner, "Phase-6 network closure missing")
    require(':/ceremony:ro' in runner, "ceremony read-only mount missing")
    require(BUILD_SOURCE_REF in runner and BUILD_SOURCE_TREE in runner, "immutable build binding missing")
    require(ACCEPTED_R0_REF in runner, "accepted R0 lineage missing")
    require("FINAM" not in runner and "finam" not in runner, "FINAM path introduced in runner")
    for relative in (ORACLE, MATERIALIZER, RUNNER):
        data = (root / relative).read_bytes()
        for pattern in r0.PRIVATE_PATTERNS:
            require(pattern.search(data) is None, f"private custody marker exported: {relative}")

    with tempfile.TemporaryDirectory(prefix="stage8b-g2-r0-r1-materialized-") as temporary:
        output = Path(temporary) / "phase6.sh"
        environment = os.environ.copy()
        environment["PYTHONPATH"] = str(root / "scripts")
        result = subprocess.run(
            [sys.executable, str(root / MATERIALIZER), str(output)],
            cwd=root,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        require(result.returncode == 0, f"Phase-6 materialization failed: {result.stderr.strip()}")
        text = output.read_text(encoding="utf-8")
    forbidden = (
        "grep -Eq 'NETWORK_CONNECT_FAILURE|AUTH_SESSION_FAILURE'",
        '"actual_read_attempts":True',
        "stage8b-p-r2a5-production-trust-manifest.json",
        "stage8b-p-r2a5-production-account-key-manifest.json",
        "generation-1.hex",
    )
    require(not any(marker in text for marker in forbidden), "materialized Phase-6 legacy proof residue")
    for marker in (
        str(ORACLE),
        'request_boundary_proof["actual_read_attempts"]',
        '"request_boundary_proof":request_boundary_proof',
    ):
        require(marker in text, f"materialized exact proof missing: {marker}")


def check_request_proof(proof: dict[str, Any]) -> str:
    expected_keys = {
        "oracle",
        "category_only_oracle",
        "root_admission_succeeded",
        "typed_terminal_protocol_valid",
        "root_terminal_evidence_durable",
        "helper_identity_validation_succeeded",
        "helper_receipt_validation_succeeded",
        "helper_authority_validation_succeeded",
        "projected_credentials_loaded",
        "failed_attempt_required",
        "actual_read_attempts",
        "attempt_count",
        "failed_attempt",
        "allowed_request_error_categories",
        "request_timeout_requires_failed_attempt",
        "root_lifecycle_timeout",
        "effect_flags",
        "broker_dispatch",
        "real_order_flags",
    }
    exact_keys(proof, expected_keys, "request-boundary proof")
    true_fields = (
        "root_admission_succeeded",
        "typed_terminal_protocol_valid",
        "root_terminal_evidence_durable",
        "helper_identity_validation_succeeded",
        "helper_receipt_validation_succeeded",
        "helper_authority_validation_succeeded",
        "projected_credentials_loaded",
        "failed_attempt_required",
        "request_timeout_requires_failed_attempt",
    )
    require(proof.get("oracle") == ORACLE_ID, "request oracle drift")
    require(proof.get("category_only_oracle") is False, "category-only oracle opened")
    require(all(proof.get(key) is True for key in true_fields), "typed validation proof incomplete")
    require(type(proof.get("attempt_count")) is int and proof["attempt_count"] == 1, "attempt count drift")
    require(
        proof.get("actual_read_attempts") is (proof["attempt_count"] > 0),
        "actual_read_attempts is not derived",
    )
    require(proof.get("allowed_request_error_categories") == ALLOWED_ERRORS, "request error policy drift")
    require(proof.get("root_lifecycle_timeout") is False, "root lifecycle timeout counted")
    require(proof.get("broker_dispatch") is False and proof.get("real_order_flags") is False, "effect boundary opened")
    attempt = proof.get("failed_attempt")
    require(isinstance(attempt, dict), "failed request attempt missing")
    exact_keys(
        attempt,
        {
            "ordinal",
            "network_class",
            "method",
            "route_template",
            "error_category",
            "status",
            "response_body_length",
            "timeout_stage",
        },
        "failed request attempt",
    )
    require(attempt.get("ordinal") == 1, "first attempt ordinal drift")
    require(attempt.get("network_class") == "AuthService", "first attempt network class drift")
    require(attempt.get("method") == "POST", "first attempt method drift")
    require(attempt.get("route_template") == "/v1/sessions", "first attempt route drift")
    error = attempt.get("error_category")
    require(error in ALLOWED_ERRORS, "request error not allowed")
    require(attempt.get("status") is None and attempt.get("response_body_length") is None, "HTTP response on network failure")
    if error == "TIMEOUT":
        require(isinstance(attempt.get("timeout_stage"), str) and bool(attempt["timeout_stage"]), "request timeout stage missing")
    else:
        require(attempt.get("timeout_stage") is None, "connect failure carries timeout stage")
    effects = proof.get("effect_flags")
    require(
        isinstance(effects, dict)
        and set(effects)
        == {
            "operator_arm_issued",
            "dispatch_attempt_recorded",
            "effect_transport_entered",
            "order_post_sent",
            "order_delete_sent",
            "raw_body_exported",
            "credential_exported",
            "account_id_exported",
        }
        and all(value is False for value in effects.values()),
        "effect flag opened",
    )
    return str(error)


def check_rehearsal(root: Path) -> tuple[dict[str, Any], str]:
    old = load(root, r0.REHEARSAL)
    evidence = load(root, REHEARSAL)
    expected_keys = set(old) | {
        "request_boundary_proof",
        "production_build_source_ref",
        "production_build_source_tree",
        "accepted_r0_review_ref",
    }
    exact_keys(evidence, expected_keys, "R0-R1 rehearsal evidence")
    dynamic = {
        "stage",
        "source_ref",
        "source_tree",
        "terminal_evidence_sha256",
        "request_boundary_proof",
        "production_build_source_ref",
        "production_build_source_tree",
        "accepted_r0_review_ref",
    }
    for key, value in old.items():
        if key not in dynamic:
            require(evidence.get(key) == value, f"R0 rehearsal semantic drift: {key}")
    require(evidence.get("stage") == STAGE, "R0-R1 evidence stage drift")
    require(HEX40.fullmatch(str(evidence.get("source_ref"))) is not None, "R0-R1 source ref drift")
    require(HEX40.fullmatch(str(evidence.get("source_tree"))) is not None, "R0-R1 source tree drift")
    require(evidence.get("production_build_source_ref") == BUILD_SOURCE_REF, "production build source rebound")
    require(evidence.get("production_build_source_tree") == BUILD_SOURCE_TREE, "production build tree rebound")
    require(evidence.get("accepted_r0_review_ref") == ACCEPTED_R0_REF, "accepted R0 evidence lineage drift")
    require(evidence.get("linux_build_evidence_sha256") == BUILD_EVIDENCE_SHA256, "build evidence binding drift")
    require(HEX64.fullmatch(str(evidence.get("terminal_evidence_sha256"))) is not None, "terminal evidence digest drift")
    require(evidence.get("actual_read_attempts") is True, "validated request attempt missing")
    require(evidence.get("container_network_mode") == "none", "container network opened")
    require(evidence.get("external_network_available") is False, "external network opened")
    require(evidence.get("finam_endpoint_called") is False, "FINAM endpoint called")
    require(evidence.get("production_credentials_installed") is False, "production credentials installed")
    require(evidence.get("services_installed_to_production") is False, "production service installed")
    require(evidence.get("production_authorization") == "NOT_ISSUED", "authorization opened")
    proof = evidence.get("request_boundary_proof")
    require(isinstance(proof, dict), "structured request proof missing")
    error = check_request_proof(proof)
    require(evidence["actual_read_attempts"] is proof["actual_read_attempts"], "outer request claim drift")
    return evidence, error


def check_authority(root: Path, evidence: dict[str, Any], error: str) -> None:
    authority = load(root, AUTHORITY)
    exact_keys(
        authority,
        {
            "schema_version",
            "stage",
            "status",
            "accepted_r0",
            "immutable_production",
            "exact_phase6_evidence",
            "activation",
            "closed_surfaces",
            "next_allowed_step",
        },
        "R0-R1 authority",
    )
    require(authority.get("schema_version") == 1 and authority.get("stage") == STAGE, "R0-R1 authority stage drift")
    require(authority.get("status") == "INDEPENDENT_REVIEW_REQUIRED", "R0-R1 authority status drift")
    require(
        authority.get("accepted_r0")
        == {
            "source_ref": ACCEPTED_R0_REF,
            "archive_sha256": ACCEPTED_R0_ARCHIVE_SHA256,
            "verdict": "SUBSTANTIVELY_ACCEPTED_FORMAL_ACCEPTANCE_PENDING_R0_R1",
        },
        "accepted R0 lineage drift",
    )
    immutable = authority.get("immutable_production")
    require(isinstance(immutable, dict), "immutable production authority missing")
    exact_keys(
        immutable,
        {
            "build_source_ref",
            "build_source_tree",
            "build_evidence_sha256",
            "helper_executable_sha256",
            "helper_acceptance_sha256",
            "effect_build_identity_sha256",
            "binary_hashes",
            "production_binaries_rebuilt_in_r0_r1",
        },
        "immutable production authority",
    )
    require(immutable.get("build_source_ref") == BUILD_SOURCE_REF, "authority build source drift")
    require(immutable.get("build_source_tree") == BUILD_SOURCE_TREE, "authority build tree drift")
    require(immutable.get("build_evidence_sha256") == BUILD_EVIDENCE_SHA256, "authority build evidence drift")
    require(immutable.get("helper_executable_sha256") == HELPER_SHA256, "authority helper drift")
    require(immutable.get("helper_acceptance_sha256") == HELPER_AUTHORITY_SHA256, "authority helper acceptance drift")
    require(immutable.get("effect_build_identity_sha256") == EFFECT_IDENTITY_SHA256, "authority effect identity drift")
    require(immutable.get("binary_hashes") == EXPECTED_BINARY_HASHES, "authority binary inventory drift")
    require(immutable.get("production_binaries_rebuilt_in_r0_r1") is False, "production binary rebuild claimed")
    phase6 = authority.get("exact_phase6_evidence")
    require(isinstance(phase6, dict), "exact Phase-6 authority missing")
    require(
        phase6
        == {
            "source_ref": evidence["source_ref"],
            "source_tree": evidence["source_tree"],
            "evidence_sha256": sha256(root / REHEARSAL),
            "oracle": ORACLE_ID,
            "category_only_oracle": False,
            "actual_read_attempts": True,
            "request_ordinal": 1,
            "request_method": "POST",
            "request_route_template": "/v1/sessions",
            "request_error_category": error,
            "allowed_request_error_categories": ALLOWED_ERRORS,
            "container_network_mode": "none",
            "finam_endpoint_called": False,
        },
        "exact Phase-6 authority drift",
    )
    require(
        authority.get("activation")
        == {
            "generation_2_public_authority_selected": True,
            "generation_2_active": False,
            "production_credentials_installed": False,
            "controlled_installation": False,
            "package_authorization": "NOT_ISSUED",
        },
        "activation boundary drift",
    )
    require(
        authority.get("closed_surfaces")
        == {
            "finam_network": False,
            "auth_service_external_network": False,
            "broker_get": False,
            "http_post_delete": False,
            "broker_dispatch": False,
            "redis_live": False,
            "runtime_live": False,
            "real_orders": False,
        },
        "closed surface drift",
    )
    require(
        authority.get("next_allowed_step")
        == "INDEPENDENT_R0_R1_REVIEW_BEFORE_31_SERVICE_INSTALLATION_PACKAGE",
        "next-step drift",
    )

    status = (root / r0.STATUS).read_text(encoding="utf-8")
    for marker in (
        STAGE,
        ACCEPTED_R0_REF,
        "production binaries rebuilt in R0-R1: false",
        "Generation 2 remains inactive",
        "NOT_ISSUED",
    ):
        require(marker in status, f"current status marker missing: {marker}")
    with (root / MATRIX).open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(
        len(rows) == 24 and [row["id"] for row in rows] == [f"G2CRR1-{index:03d}" for index in range(1, 25)],
        "R0-R1 acceptance matrix inventory drift",
    )
    require(all(row["expected"] == "PASS" for row in rows), "R0-R1 acceptance matrix result drift")


def check(root: Path, artifact_root: Path | None = None) -> None:
    for relative in required_files():
        require((root / relative).is_file(), f"missing artifact: {relative}")
    check_immutable_r0(root, artifact_root)
    check_source_contract(root)
    evidence, error = check_rehearsal(root)
    check_authority(root, evidence, error)
    print(
        "stage8b-generation2-composition-r0-r1-check: PASS "
        "request=POST:/v1/sessions:1 "
        f"outcome={error} category_only=false binaries_rebuilt=false "
        "active=false authorization=NOT_ISSUED finam=false"
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--artifact-root", type=Path)
    arguments = parser.parse_args()
    check(arguments.root.resolve(), arguments.artifact_root)


if __name__ == "__main__":
    main()
