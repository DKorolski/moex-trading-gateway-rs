#!/usr/bin/env python3
"""Fail-closed Stage 8A-3 R2 endpoint-classifier scanner."""

from __future__ import annotations

import csv
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "16180ac4f8eab761b3b055c1f5515f62cd94bfb9"
BRANCH = "stage8a3-endpoint-classifier"
MODULE = Path("crates/finam-gateway/src/stage8a3_endpoint_classifier.rs")
TESTS = Path("crates/finam-gateway/src/stage8a3_endpoint_classifier/tests.rs")
LIB = Path("crates/finam-gateway/src/lib.rs")
STAGE8A1 = Path("crates/finam-gateway/src/stage8a1_execution_capability.rs")
STAGE8A2 = Path(
    "crates/finam-gateway/src/stage8a1_execution_capability/"
    "stage8a2_builder_composition.rs"
)
AUTHORITY = Path("docs/stage-8/stage8a3-contract-authority.json")
SNAPSHOT = Path("docs/stage-8/stage8a3-finam-contract-snapshot-2026-08-15.json")
ENTRY = Path("docs/stage-8/stage8a3-entry-contract.md")
DESCRIPTOR = Path("docs/stage-8/stage8a3-r1-implementation-descriptor.json")
MATRIX = Path("docs/stage-8/STAGE8A_3_R1_ACCEPTANCE_MATRIX_2026-08-15.csv")
INVENTORY = Path("docs/stage-8/STAGE8A_3_R1_NEGATIVE_INVENTORY_2026-08-15.md")
CURRENT_STATUS = Path("docs/current-status.md")
ROADMAP = Path("docs/roadmap.md")

ALLOWED_CHANGED_PATHS = {
    str(MODULE),
    str(TESTS),
    str(LIB),
    "docs/current-status.md",
    "docs/roadmap.md",
    str(AUTHORITY),
    str(SNAPSHOT),
    str(ENTRY),
    str(DESCRIPTOR),
    str(MATRIX),
    str(INVENTORY),
    "scripts/make_stage8a3_handoff_archive.py",
    "scripts/stage8a3_check.py",
    "scripts/stage8a3_gate.sh",
    "scripts/stage8a3_handoff_safety_check.py",
    "scripts/stage8a3_negative_harness.py",
    "scripts/stage8a3_proof_map.py",
}

# Filled for the final candidate after production and contract files stop moving.
PINNED_FINAL_SHA256: dict[Path, str] = {
    MODULE: "f34c9fef5e219dad15b0a00ce1eaf63311ec9f77d1997e422b977e5c8ffe47b3",
    TESTS: "dbd58f32d0f1f5e5c96806bd84a56d974842fdb6502ea66347d1d3a264806ae8",
    LIB: "24b8d8229608abb0667928cb3bad474b80543e02a22a3b3203a1590df4321f15",
    AUTHORITY: "b54aa10fa2a32dd252262d3f6e5549db2dfff9339d13cc81add06d9b44dc7cde",
    SNAPSHOT: "da1ca5547271764543108238198dbeba4e1161cc3eabc399c572f5ab2f0ed5e0",
    ENTRY: "0c43f97e295e6c0ea7278786935a96570c98235e43aba39d04753ea47a31b697",
    DESCRIPTOR: "c2a4f910d2c86c798feed15714e73d7a4aa416eb53e2a7e100295fe04534a129",
    MATRIX: "8c550cf968bff8664a99b61f0e7a2790e3665b11115222a08a9e89fadc2122fa",
    INVENTORY: "55cf9e4a3befcb61e0ff4a092eaf409774e86fdb8428537f7eea01f9d3aafdb7",
}

FORBIDDEN_TOKENS = (
    "classify_order_endpoint_local_http_response",
    "classify_order_endpoint_local_http_response_for_context",
    "FinamOrderEndpointLocalHttpResponse",
    "FinamOrderEndpointContext",
    "FinamOrderEndpointClassifiedResponse",
    "FinamOrderEndpointMappedResult",
    "BrokerRejected",
    "400..=499",
    "RetryAllowed",
    "RetryAfter",
    "Maintenance",
    "DefinitelyNotSent",
    "ProvenNoMatch",
    "FlatConfirmed",
    "NoFillConfirmed",
    "Stage8a4Reconciliation",
    "reqwest",
    "M3d2RealOrderEndpointTransport",
    "EndpointGateApproved",
    "m3j16_actual_one_shot",
    "redis::",
    "RedisCommandConsumer",
    "BrokerDispatch",
    "RuntimeLive",
    "RealStrategyOrder",
    "StopLoss",
    "Sltp",
    "BracketOrder",
    "ReplaceOrder",
    "MultiLeg",
    "same_request_retry",
    "raw_response_body",
    "raw_broker_order_id",
    "raw_client_order_id",
    "raw_account_id",
    "accept_missing_place_order_id",
    "CancelAcceptedCandidate 204",
    "CancelAcceptedCandidate empty",
)


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def code_without_comments(source: str) -> str:
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    return re.sub(r"//[^\n]*", "", source)


def markdown_section(source: str, heading: str) -> str:
    marker = f"## {heading}\n"
    require(marker in source, f"missing markdown section: {heading}")
    return source.split(marker, 1)[1].split("\n## ", 1)[0]


def base_file(path: Path) -> bytes:
    return subprocess.check_output(["git", "show", f"{BASE}:{path.as_posix()}"], cwd=ROOT)


def changed_paths() -> set[str]:
    tracked = subprocess.check_output(
        ["git", "diff", "--name-only", BASE, "--"], cwd=ROOT, text=True
    ).splitlines()
    untracked = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT, text=True
    ).splitlines()
    return {value for value in tracked + untracked if value}


def check(
    root: Path = ROOT,
    *,
    git_scope: bool = True,
    pin_hashes: bool = True,
    exact_successor: bool = True,
) -> None:
    descriptor = json.loads((root / DESCRIPTOR).read_text())
    require(descriptor["stage"] == "8A-3-R2", "stage drift")
    require(
        descriptor["status"] == "endpoint_classifier_no_send_candidate",
        "candidate status drift",
    )
    require(descriptor["accepted_predecessor"] == BASE, "predecessor drift")
    require(descriptor["acceptance_rows"] == 67, "acceptance count drift")
    require(descriptor["negative_cases"] == 44, "negative count drift")
    require(descriptor["official_contract_match"] is True, "contract match drift")
    require(all(descriptor["required"].values()), "required proof disabled")
    require(all(descriptor["closed"].values()), "closed surface opened")
    require(descriptor["next_after_acceptance"] == "Stage 8A-4 only", "next drift")

    authority = json.loads((root / AUTHORITY).read_text())
    require(authority["stage"] == "8A-3-R2", "authority stage drift")
    require(authority["accepted_predecessor"] == BASE, "authority predecessor drift")
    require(
        authority["accepted_predecessor_archive_sha256"]
        == "a5c494183f6afc37d013d4377422680820c204dd7f1feacd56e819218312dbd3",
        "predecessor archive drift",
    )
    require(
        authority["predecessor_review_sha256"]
        == "dfe8f45ae452ca3c1339b58eb28146b52f06c7a880e39f358a147f9a2eb43527",
        "predecessor review drift",
    )
    require(
        authority["r1_tz_sha256"]
        == "c77b5d37d46e1ed0aec08812022abe3ffbeadabac034d2e7a64d5ea10dc326e4",
        "R1 TZ drift",
    )
    require(authority["material_contract_drift"] is False, "material drift opened")
    require(authority["place_400_safe_decoder_available"] is False, "unsafe 400 decoder")
    require(authority["historical_classifier_authoritative"] is False, "historical authority")
    require(
        authority["r1_review_sha256"]
        == "bfe639c0151702cf1c8588a914a5382867c2cad8f86510703ab002fbd0b857f8",
        "R1 review authority drift",
    )
    require(authority["network_send_authorized"] is False, "send authorized")
    require(authority["retry_authority_available"] is False, "retry authorized")
    require(authority["proven_no_match_available"] is False, "no-match opened")

    snapshot = json.loads((root / SNAPSHOT).read_text())
    responses = {item["name"]: item for item in snapshot["retrieval"]["responses"]}
    require(
        responses["rest_place_order"]["sha256"]
        == "0fc4494e2f06a9bc8aebb10eb0a7de0500b661c9988a9fdfda526364348ff589",
        "official PLACE document drift",
    )
    require(responses["rest_place_order"]["bytes"] == 23736, "PLACE bytes drift")
    require(
        responses["rest_cancel_order"]["sha256"]
        == "595f123796fca321e9027c81ea1dc54d61b85862b9a1031fea73eaa2ef92b63e",
        "official CANCEL document drift",
    )
    require(responses["rest_cancel_order"]["bytes"] == 6727, "CANCEL bytes drift")
    require(snapshot["parity"]["material_contract_drift"] is False, "snapshot drift")
    require(
        snapshot["place_order"]["documented_statuses"]
        == [200, 400, 401, 404, 429, 500, 503, 504],
        "PLACE status contract drift",
    )
    require(
        snapshot["cancel_order"]["documented_statuses"]
        == [200, 400, 401, 404, 429, 500, 503, 504],
        "CANCEL status contract drift",
    )

    with (root / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 67, "acceptance matrix must contain 67 rows")
    require(
        [row["id"] for row in rows]
        == [f"S8A3R1-{index:03d}" for index in range(1, 65)]
        + [f"S8A3R2-{index:03d}" for index in range(65, 68)],
        "acceptance IDs drift",
    )
    require(all(row["mandatory"] == "YES" for row in rows), "optional row introduced")
    inventory = (root / INVENTORY).read_text()
    require(len(re.findall(r"^\d+\. ", inventory, re.M)) == 44, "negative inventory drift")

    source = (root / MODULE).read_text()
    code = code_without_comments(source)
    production = code.split("#[cfg(test)]\nmod tests;", 1)[0]
    require("pub fn for_place(" in production, "explicit PLACE context missing")
    require("pub fn for_cancel(" in production, "explicit CANCEL context missing")
    require(production.count("pub fn classify(") == 1, "classifier API count drift")
    require("default_to_place" not in production, "contextless default introduced")
    require("Stage8a3ExpectedContext::Place" in production, "PLACE context not matched")
    require("Stage8a3ExpectedContext::Cancel" in production, "CANCEL context not matched")
    require(
        ".ok_or(Stage8a3ContextError::EmptyInstrumentIdentity)?;" in production,
        "strict venue-symbol requirement missing",
    )
    require("unwrap_or(instrument.symbol)" not in production, "venue-symbol fallback opened")
    require("instrument.symbol" not in production, "broker-neutral symbol used as FINAM identity")
    require(production.count("201..=299 => {") == 2, "unknown 2xx arm drift")
    require(
        production.count("Stage8a3ReconciliationReason::UndocumentedSuccessStatus") >= 2,
        "unknown 2xx opened",
    )
    require(
        production.count("Stage8a3ReconciliationReason::UnsafeOrUnknownClientError") >= 1,
        "unsafe PLACE 400 policy drift",
    )
    require("decoded.order_id.trim().is_empty()" not in production, "pre-Option id drift")
    require("MissingBrokerOrderId" in production, "missing broker id not distinguished")
    require(
        "_ => return reconciliation_decision(Stage8a3ReconciliationReason::MissingBrokerOrderId),"
        in production,
        "empty broker id acceptance drift",
    )
    require("CorrelationMismatch" in production, "correlation mismatch not distinguished")
    require(
        production.count("return reconciliation_mismatch();") == 2,
        "PLACE/CANCEL mismatch policy drift",
    )
    require("CancelAlreadyExecuted" in production, "CANCEL 400 semantics missing")
    require("CancelTargetNotFound" in production, "CANCEL 404 semantics missing")
    require("Stage8a3LocalObservationKind::Timeout" in production, "timeout input missing")
    require("Stage8a3LocalObservationKind::Disconnected" in production, "disconnect input missing")
    require("Stage8a3LocalObservationKind::ResponseLost" in production, "loss input missing")
    require("Stage8a3LocalObservationKind::BodyReadFailed" in production, "body failure missing")
    require("pub struct Stage8a3ClassifiedObservation" in production, "opaque output missing")
    prefix = production.split("pub struct Stage8a3ClassifiedObservation", 1)[0]
    require(
        "#[derive(" not in prefix.rsplit("\n\n", 1)[-1],
        "classified observation gained a derive",
    )
    require("pub fn raw_" not in production, "raw getter introduced")
    require(
        re.search(r"pub\s+broker_order_id\s*:", production) is None,
        "raw broker id exported",
    )
    require(re.search(r"pub\s+account_id\s*:", production) is None, "raw account id exported")
    require(
        re.search(r"pub\s+client_order_id\s*:", production) is None,
        "raw client id exported",
    )
    for token in FORBIDDEN_TOKENS:
        require(token not in production, f"forbidden Stage 8A-3 token: {token}")

    status_section = markdown_section((root / CURRENT_STATUS).read_text(), "Current accepted boundary")
    require(
        "Stage 8A-2 R1 is independently accepted and closed at" in status_section
        and BASE in status_section,
        "current-status accepted predecessor authority drift",
    )
    require(
        "Stage 8A-3 R2 is the only active candidate" in status_section
        and "independent acceptance is pending" in status_section,
        "current-status active candidate authority drift",
    )
    require(
        status_section.count("only active candidate") == 1
        and "Stage 8A-2 is the only open implementation slice" not in status_section
        and "Stage 8A-3 classifier" not in status_section,
        "current-status contradictory stage authority",
    )
    require(
        "Stage 8A-4+, FINAM POST/DELETE, Redis live consumption, broker dispatch,"
        in status_section
        and "runtime-live and real orders remain closed" in status_section,
        "current-status closed-surface authority drift",
    )

    roadmap_section = markdown_section((root / ROADMAP).read_text(), "Current active stage")
    require(
        "Stage 8A-2 R1 is independently accepted and closed at" in roadmap_section
        and BASE in roadmap_section,
        "roadmap accepted predecessor authority drift",
    )
    require(
        "Stage 8A-3 R2 is the only active candidate" in roadmap_section
        and "independent acceptance is pending" in roadmap_section,
        "roadmap active candidate authority drift",
    )
    require(
        roadmap_section.count("only active candidate") == 1
        and "Stage 8A-2 —" not in roadmap_section
        and "Stage 8A-3 through 8A-5" not in roadmap_section,
        "roadmap contradictory stage authority",
    )
    require(
        "Stage 8A-4+, FINAM POST/DELETE, Redis live consumption, broker dispatch,"
        in roadmap_section
        and "runtime-live and real orders remain closed" in roadmap_section,
        "roadmap closed-surface authority drift",
    )

    tests = (root / TESTS).read_text()
    required_tests = (
        "place_exact_200_is_candidate_and_preserves_opaque_string_order_id",
        "place_missing_or_empty_order_id_requires_reconciliation",
        "place_every_correlation_mismatch_requires_reconciliation",
        "place_malformed_truncated_empty_non_object_and_oversized_200_reconcile",
        "place_undocumented_2xx_never_accepts",
        "place_400_is_never_status_or_text_promoted_to_rejection",
        "place_auth_and_configuration_statuses_are_endpoint_specific_blocks",
        "place_transient_and_undocumented_statuses_reconcile",
        "cancel_exact_200_is_candidate_without_flatness_semantics",
        "cancel_empty_malformed_or_contradictory_200_reconciles",
        "cancel_204_and_other_undocumented_2xx_reconcile",
        "cancel_status_table_is_fail_closed_and_endpoint_specific",
        "local_failure_observations_always_require_reconciliation",
        "same_status_has_endpoint_specific_semantics",
        "public_diagnostic_is_redacted_and_deterministic",
        "invalid_context_is_rejected_before_classification",
    )
    for name in required_tests:
        require(f"fn {name}()" in tests, f"mandatory test missing: {name}")

    if exact_successor:
        require((root / STAGE8A1).read_bytes() == base_file(STAGE8A1), "Stage 8A-1 drift")
        require((root / STAGE8A2).read_bytes() == base_file(STAGE8A2), "Stage 8A-2 drift")
        accepted_lib = base_file(LIB).decode()
        expected_lib = accepted_lib.replace(
            "pub mod stage8a1_execution_capability;\n",
            "pub mod stage8a1_execution_capability;\nmod stage8a3_endpoint_classifier;\n",
            1,
        ).replace(
            "};\n\nuse std::collections::{HashMap, HashSet};",
            "};\npub use stage8a3_endpoint_classifier::{\n"
            "    Stage8a3BodyCategory, Stage8a3ClassificationDiagnostic, "
            "Stage8a3ClassifiedObservation,\n"
            "    Stage8a3ContextError, Stage8a3CorrelationState, Stage8a3EndpointContext, "
            "Stage8a3EndpointKind,\n"
            "    Stage8a3LocalHttpObservation, Stage8a3ReconciliationReason, "
            "Stage8a3SemanticCategory,\n"
            "};\n\nuse std::collections::{HashMap, HashSet};",
            1,
        )
        require((root / LIB).read_text() == expected_lib, "crate export delta drift")

    if git_scope:
        branch = subprocess.check_output(
            ["git", "branch", "--show-current"], cwd=ROOT, text=True
        ).strip()
        require(branch == BRANCH, f"branch drift: {branch}")
        require(changed_paths() == ALLOWED_CHANGED_PATHS, "changed-path allowlist drift")

    if pin_hashes:
        require(PINNED_FINAL_SHA256, "final SHA-256 pins are not populated")
        for path, expected in PINNED_FINAL_SHA256.items():
            require(sha256(root / path) == expected, f"final SHA-256 drift: {path}")


def main() -> int:
    try:
        check()
    except (CheckFailure, KeyError, OSError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"stage8a3-r2-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage8a3-r2-check: PASS rows=67 endpoint-specific=true no-send=true next=8A-4-only")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
