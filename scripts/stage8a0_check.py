#!/usr/bin/env python3
"""Validate the Stage 8A-0 FINAM contract refresh/freeze package."""

from __future__ import annotations

import csv
import hashlib
import json
import re
from pathlib import Path

import stage8a0_closed_surface_check as closed

ROOT = Path(__file__).resolve().parents[1]
BASE = closed.BASE
BRANCH = "stage8a0-contract-freeze"
DESCRIPTOR = Path("docs/stage-8/stage8a0-descriptor.json")
SNAPSHOT = Path("docs/stage-8/stage8a0-finam-contract-snapshot-2026-08-14.json")
PARITY = Path("docs/stage-8/stage8a0-contract-parity-evidence-2026-08-14.json")
MATRIX = Path("docs/stage-8/STAGE8A_0_R1_ACCEPTANCE_MATRIX_2026-08-14.csv")
INVENTORY = Path("docs/stage-8/STAGE8A_0_R1_NEGATIVE_INVENTORY_2026-08-14.md")
POLICY = Path("docs/stage-8/stage8a0-contract-freeze.md")
TIMING_EVIDENCE = Path("docs/stage-8/stage8a0-r1-timing-flake-evidence.json")
GATE_SCRIPT = Path("scripts/stage8a0_gate.sh")
HANDOFF_SCRIPT = Path("scripts/make_stage8a0_handoff_archive.py")
SAFETY_SCRIPT = Path("scripts/stage8a0_handoff_safety_check.py")
ORDER_REQUEST = Path("crates/broker-finam/src/order_request.rs")
IDS = Path("crates/broker-core/src/ids.rs")
MAPPER = Path("crates/broker-finam/src/mapper.rs")
DTO = Path("crates/broker-finam/src/dto.rs")
REGISTRY = Path("crates/broker-finam/src/instrument_registry.rs")
INSTRUMENT = Path("crates/broker-core/src/instrument.rs")
ENUM_FIXTURE = Path("crates/broker-finam/tests/fixtures/finam_spec/order_contract_enums_v2026_07_03.json")
ORDER_PATH = Path("crates/broker-core/src/order_path.rs")
STAGE7B = Path("docs/stage-7/stage7b-closure-descriptor.json")

MATRIX_SHA = "2f3692a26df9dfa4d8d5bb14ef36f5ca0a86ada7c85179ebf9d9263fb8be41b6"
NORMATIVE_PACKAGE_MATRIX_SHA = "b1fc4f2cbd6b861b1d956b4327e7b2502eb4bb61870a50adf2fcb256f785cecf"
INVENTORY_SHA = "ec67a816a8a1dfa061d09178342d0864369dd55b4ea5e517cf2823392ef926b0"
TIMING_EVIDENCE_SHA = "fa461397721c0cddc19b2ff0eba9930958a35f6b5383821491e6174d5e7552eb"
SNAPSHOT_SHA = "11062063c5f1f4f83f645af6b3a2e2716af363dca0bafdbdf3ee2b00da5d572e"
PARITY_SHA = "d7247d3a8802cc2600bdf3a9eda20fd5075cadf313ff81ad44217b826b431d6f"
SOURCE_HASHES = {
    ORDER_REQUEST: "e57789a15d4a33fad08b93580d50c5efa8aba92ea4f547a45a898c6e300b80e6",
    IDS: "39f2b004d812f62f3636292c6b98e86c8a8242ece0bbd1da46986c838b0cf0ed",
    MAPPER: "e1e91a075a8b73c99a6c2a76a3ec045e630de4da0943ed9d50d4756648b09b97",
    DTO: "01816b3e62aa72623238efe934470f663fd4a081710044d8ac15fd321a9b4f08",
    REGISTRY: "3423fe0381c15017de65398525658276a4079c443b940ab1df3ce01ee8499593",
    INSTRUMENT: "ab4d0b80296a1e99f2c63349efbb8435fb2e0b919de7cc95badb4b5d4aec062e",
    ENUM_FIXTURE: "212ab404fbccc0a7bcb77a43a2ec73f460c7d90629b14daf00020eb9f6041dcf",
    ORDER_PATH: "67dd132d40b6d1de773013a1c03e892f1aef03e3d40ac56ee43105aeb5cdce69",
    STAGE7B: "ee3e3555def13b5da6f699c4be62c2fff2c9bca1b82487036f7d9816b0a2c003",
}


class GateFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise GateFailure(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_json(root: Path, path: Path) -> dict:
    return json.loads((root / path).read_text())


def check(root: Path = ROOT, *, check_git_scope: bool = True) -> None:
    descriptor = load_json(root, DESCRIPTOR)
    snapshot = load_json(root, SNAPSHOT)
    parity = load_json(root, PARITY)
    fixture = load_json(root, ENUM_FIXTURE)

    require(descriptor["status"] == "r1_candidate_independent_acceptance_pending", "self-acceptance/status drift")
    require(descriptor["r0_base_candidate"] == "104cdf8f1ff0a645a5681eae653962ba59016123", "R0 base drift")
    require(descriptor["r0_independent_review_sha256"] == "212c2cb795353e9b0488c2a77c013e80734c6d94b84e2eee8f01852d221a6807", "R0 review hash drift")
    require(descriptor["r1_tz_sha256"] == "d3914cdb7d12e0c7b0b41316006038b544b5fe1fcf3c06892e8d00740fc4289c", "R1 TZ hash drift")
    require(descriptor["accepted_gate7_to_8_ref"] == BASE, "accepted Gate R3 ref drift")
    require(descriptor["accepted_gate7_to_8_review_sha256"] == "3df1d4fda8f1b6d68d7960398971646968214c88e13836d00559ad0ae09e9230", "accepted review hash drift")
    require(descriptor["accepted_stage7b_ref"] == "a1044e0dbe324c722b637498ca80ffafd9f0cbee", "Stage7B ref drift")
    require(descriptor["scope"] == "docs_evidence_checkers_only", "scope drift")
    require(descriptor["acceptance_row_count"] == 41, "matrix count drift")
    require(descriptor["negative_case_count"] == 41, "negative count drift")
    require(descriptor["acceptance_matrix_sha256"] == MATRIX_SHA == sha256(root / MATRIX), "matrix hash drift")
    require(descriptor["normative_package_matrix_sha256"] == NORMATIVE_PACKAGE_MATRIX_SHA, "normative package matrix hash drift")
    require(descriptor["negative_inventory_sha256"] == INVENTORY_SHA == sha256(root / INVENTORY), "inventory hash drift")
    require(descriptor["contract_snapshot_sha256"] == SNAPSHOT_SHA == sha256(root / SNAPSHOT), "snapshot hash drift")
    require(descriptor["contract_parity_sha256"] == PARITY_SHA == sha256(root / PARITY), "parity hash drift")
    require(descriptor["timing_flake_evidence_sha256"] == TIMING_EVIDENCE_SHA == sha256(root / TIMING_EVIDENCE), "timing evidence hash drift")
    require(descriptor["workspace_test_all_targets_required"] is True, "all-target coverage disabled")
    require(descriptor["workspace_and_doc_test_threads"] == 1, "serialized test policy drift")
    require(descriptor["all_gate_logs_packaged"] is True, "gate log packaging disabled")
    require(descriptor["all_gate_logs_sha256_bound"] is True, "gate log hash binding disabled")

    false_fields = (
        "production_rust_changes_allowed", "cargo_changes_allowed", "github_workflow_changes_allowed",
        "finam_post_delete_allowed", "broker_dispatch_allowed", "runtime_live_allowed", "real_orders_allowed",
        "stage8a_1_open", "stage8a_2_through_8a_5_open", "stage8b_open",
        "broker_generated_client_order_id_fallback_allowed", "proven_no_match_allowed",
        "same_durable_request_reexecution_after_dispatch_allowed", "definitely_not_sent_same_request_retry_allowed",
        "stage8_journal_reducer_allocator_added", "second_finam_serializer_allowed",
        "historical_stage5_scanner_sole_authority", "production_fix_in_stage8a0", "self_acceptance_allowed",
    )
    for field in false_fields:
        require(descriptor[field] is False, f"closed field opened: {field}")
    for field in (
        "client_order_id_explicit_required", "client_order_id_exact_durable_required",
        "stage7b_sole_lifecycle_authority", "existing_finam_request_builder_required",
        "stage8_specific_scanner_required", "workspace_regression_required",
    ):
        require(descriptor[field] is True, f"required invariant disabled: {field}")
    require(descriptor["initial_order_types"] == ["MARKET", "LIMIT"], "initial order type drift")
    require(descriptor["initial_time_in_force"] == ["DAY"], "initial TIF drift")
    require(descriptor["client_order_id_max_chars"] == 20, "client id max drift")
    require(descriptor["outgoing_order_comment_policy"] == "Disabled_None", "comment policy drift")
    require(descriptor["parity_verdict"] == "MATCH", "parity descriptor drift")
    require(descriptor["next_authorized_after_independent_acceptance"] == "Stage 8A-1 only", "next authority drift")

    require(snapshot["retrieved_at_utc"] == "2026-08-14T16:40:58Z", "retrieval timestamp drift")
    retrieval = snapshot["retrieval"]
    require(retrieval["official_rest_index"] == "https://api.finam.ru/docs/rest/", "official REST URL drift")
    require(retrieval["official_grpc_index"] == "https://api.finam.ru/docs/grpc/", "official gRPC URL drift")
    require("Accept: text/markdown" in retrieval["method"] and "without credentials" in retrieval["method"], "non-reproducible extraction method")
    require(len(retrieval["responses"]) == 7, "official response inventory drift")
    for response in retrieval["responses"]:
        require(response["url"].startswith("https://api.finam.ru/docs/"), "non-official source")
        require(re.fullmatch(r"[0-9a-f]{64}", response["sha256"]) is not None, "source hash missing")
        require(type(response["bytes"]) is int and response["bytes"] > 0, "source size invalid")

    place = snapshot["place_order"]
    cancel = snapshot["cancel_order"]
    require((place["method"], place["path"]) == ("POST", "/v1/accounts/{account_id}/orders"), "PLACE endpoint drift")
    require((cancel["method"], cancel["path"]) == ("DELETE", "/v1/accounts/{account_id}/orders/{order_id}"), "CANCEL endpoint drift")
    require(place["request_fields"] == ["symbol","quantity","side","type","time_in_force","limit_price","stop_price","stop_condition","legs","client_order_id","valid_before","comment"], "PLACE fields drift")
    statuses = ["200","400","401","404","429","500","503","504","default"]
    require(place["response_statuses"] == statuses, "PLACE statuses drift")
    require(cancel["response_statuses"] == statuses, "CANCEL statuses drift")
    require(cancel["status_meanings"]["400"] == "order_already_executed_cannot_cancel", "CANCEL 400 drift")

    enums = snapshot["enums"]
    require(enums["order_type"] == ["ORDER_TYPE_UNSPECIFIED","ORDER_TYPE_MARKET","ORDER_TYPE_LIMIT","ORDER_TYPE_STOP","ORDER_TYPE_STOP_LIMIT","ORDER_TYPE_MULTI_LEG"], "OrderType drift")
    require(enums["time_in_force"] == fixture["time_in_force"], "TIF fixture drift")
    require(enums["valid_before"] == fixture["valid_before"], "ValidBefore fixture drift")
    require(enums["order_status"] == fixture["order_status"], "OrderStatus fixture drift")
    require(snapshot["client_order_id_broker_contract"] == {"documented_optional": True, "broker_auto_generates_when_omitted": True, "maximum_characters": 20}, "broker ClientOrderId contract drift")
    require(snapshot["stage8_initial_policy"]["allowed_time_in_force"] == ["TIME_IN_FORCE_DAY"], "Stage8 TIF policy drift")
    require(snapshot["stage8_initial_policy"]["outgoing_comment"] == "Disabled/None", "Stage8 comment drift")
    require(snapshot["stage8_initial_policy"]["cancel_400"] == "ReconciliationRequired", "CANCEL 400 policy drift")
    require("auth/readiness blocked" in snapshot["stage8_initial_policy"]["cancel_401"] and "no same-request retry" in snapshot["stage8_initial_policy"]["cancel_401"], "CANCEL 401 policy drift")
    require(snapshot["stage8_initial_policy"]["cancel_404"] == "ReconciliationRequired", "CANCEL 404 policy drift")
    require("ReconciliationRequired" in snapshot["stage8_initial_policy"]["cancel_409_410"], "CANCEL 409/410 policy drift")
    require("DefinitelyNotSent does not permit same-request resend" in snapshot["durable_retry_invariant"], "durable retry invariant drift")
    prerequisites = snapshot["instrument_prerequisites"]
    require(prerequisites["asset_path"] == "/v1/assets/{symbol}", "asset source drift")
    require(prerequisites["asset_params_path"] == "/v1/assets/{symbol}/params", "asset params source drift")
    require(prerequisites["schedule_path"] == "/v1/assets/{symbol}/schedule", "schedule source drift")
    require("broker-neutral futures policy" in prerequisites["qty_step_source"], "qty step provenance drift")

    require(parity["parity_verdict"] == "MATCH" and parity["comparisons"]["material_contract_drift"] is False, "material drift silently accepted")
    require(parity["production_fix_in_stage8a0"] is False and parity["stage8a1_open"] is False, "production/next slice opened")
    parity_hashes = {Path(item["path"]): item["sha256"] for item in parity["project_sources"]}
    require(parity_hashes == SOURCE_HASHES, "project source inventory drift")
    for path, digest in SOURCE_HASHES.items():
        require(sha256(root / path) == digest, f"project source drift: {path}")

    order_source = (root / ORDER_REQUEST).read_text()
    require(len(re.findall(r"^pub fn build_place_order_request\(", order_source, re.M)) == 1, "PLACE serializer count drift")
    require(len(re.findall(r"^pub fn build_cancel_order_request\(", order_source, re.M)) == 1, "CANCEL serializer count drift")
    require("client_order_id: Some(order.client_order_id.as_str().to_string())" in order_source, "exact client id serialization absent")
    ids_source = (root / IDS).read_text()
    require("CLIENT_ORDER_ID_MAX_LEN: usize = 20" in ids_source and "ClientOrderIdError::Empty" in ids_source, "ClientOrderId validation drift")
    mapper_source = (root / MAPPER).read_text()
    require("_ => FinamOrderStatusClass::BlockingUnknown" in mapper_source, "unknown status not fail closed")
    for status in enums["order_status"]:
        token = status.removeprefix("ORDER_STATUS_")
        require(f'"{token}"' in mapper_source or token == "UNSPECIFIED", f"status classifier provenance missing: {status}")

    gate_text = (root / GATE_SCRIPT).read_text()
    require("cargo test --workspace --all-targets -- --test-threads=1" in gate_text, "all-target serialized test command absent")
    require("cargo test --workspace --doc -- --test-threads=1" in gate_text, "serialized doctest command absent")
    require("cargo clippy --workspace --all-targets --all-features -- -D warnings" in gate_text, "clippy coverage drift")
    handoff_text = (root / HANDOFF_SCRIPT).read_text()
    safety_text = (root / SAFETY_SCRIPT).read_text()
    required_artifacts = {
        "contract-check.txt", "closed-surface.txt", "negative.txt", "proof-map.json",
        "python-compile.txt", "fmt.txt", "test.txt", "doctest.txt", "clippy.txt",
        "diff-check.txt", "toolchain.txt", "timing-flake-evidence.json",
    }
    for name in required_artifacts:
        require(f'"{name}"' in handoff_text, f"handoff artifact omitted: {name}")
        require(f'"{name}"' in safety_text, f"safety artifact omitted: {name}")
    for token in ("artifact_hashes = {", '"gate_artifact_sha256": artifact_hashes,', "hashlib.sha256(payload).hexdigest()"):
        require(token in handoff_text, f"artifact hash binding absent: {token}")
    require("artifact_hashes = evidence.get(\"gate_artifact_sha256\")" in safety_text, "safety hash map validation absent")
    timing = load_json(root, TIMING_EVIDENCE)
    require(timing["exact_reconstructed_witness"]["test_name"] == "stage7b_e_x16_sigkill_during_claim_is_reclaimable_by_next_boot", "timing witness test drift")
    require(timing["exact_reconstructed_witness"]["failure_signature"] == "X16 child did not reach claim barrier", "timing signature drift")
    require(timing["isolated_rerun"]["result"].startswith("PASS"), "isolated rerun not proven")
    require(timing["r1_final_policy"]["workspace_command"] == "cargo test --workspace --all-targets -- --test-threads=1", "timing final command drift")
    require(timing["production_scope"]["production_rust_changed_from_r0_candidate"] is False, "production scope changed")

    with (root / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 41 and all(row["mandatory"] == "YES" for row in rows), "acceptance matrix rows drift")
    inventory_cases = re.findall(r"^\d+\. `.+`$", (root / INVENTORY).read_text(), re.M)
    require(len(inventory_cases) == 41, "negative inventory rows drift")
    policy = (root / POLICY).read_text()
    for token in ("independent acceptance pending", "MATERIAL_DRIFT_BLOCKED", "DefinitelyNotSent", "Stage 8A-1 only", "Disabled/None"):
        require(token in policy, f"policy token absent: {token}")

    if check_git_scope:
        require(root == ROOT, "git scope may only be checked at repository root")
        closed.check_git_scope()
        require(closed.git("branch", "--show-current") == BRANCH, "branch drift")


def main() -> None:
    try:
        check()
    except (GateFailure, closed.ClosedSurfaceFailure) as error:
        raise SystemExit(f"stage8a0-check: FAIL {error}") from error
    print("stage8a0-check: PASS r1 rows=41 parity=MATCH serialized-regression=true logs=hash-bound next=8A-1-pending production=closed")


if __name__ == "__main__":
    main()
