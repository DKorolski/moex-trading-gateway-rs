#!/usr/bin/env python3
"""Exact 36 reviewed negative mutations for Stage 8A-0."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path
from typing import Callable

import stage8a0_check as checker

ROOT = Path(__file__).resolve().parents[1]
FILES = {
    checker.DESCRIPTOR, checker.SNAPSHOT, checker.PARITY, checker.MATRIX,
    checker.INVENTORY, checker.POLICY, checker.ORDER_REQUEST, checker.IDS,
    checker.MAPPER, checker.DTO, checker.REGISTRY, checker.INSTRUMENT,
    checker.ENUM_FIXTURE, checker.ORDER_PATH, checker.STAGE7B,
}


def copy_contract(destination: Path) -> None:
    for relative in FILES:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)


def edit_json(root: Path, path: Path, mutate: Callable[[dict], None]) -> None:
    target = root / path
    value = json.loads(target.read_text())
    mutate(value)
    target.write_text(json.dumps(value, indent=2) + "\n")


def descriptor(root: Path, mutate: Callable[[dict], None]) -> None:
    edit_json(root, checker.DESCRIPTOR, mutate)


def snapshot(root: Path, mutate: Callable[[dict], None]) -> None:
    edit_json(root, checker.SNAPSHOT, mutate)


def parity(root: Path, mutate: Callable[[dict], None]) -> None:
    edit_json(root, checker.PARITY, mutate)


def append(root: Path, path: Path, text: str) -> None:
    target = root / path
    target.write_text(target.read_text() + text)


CASES: list[tuple[str, Callable[[Path], None]]] = [
    ("forge-accepted-gate-r3", lambda root: descriptor(root, lambda value: value.__setitem__("accepted_gate7_to_8_ref", "0" * 40))),
    ("add-production-rust-change", lambda root: append(root, checker.ORDER_REQUEST, "\n// unauthorized Stage8 production delta\n")),
    ("allow-cargo-change", lambda root: descriptor(root, lambda value: value.__setitem__("cargo_changes_allowed", True))),
    ("allow-github-workflow-change", lambda root: descriptor(root, lambda value: value.__setitem__("github_workflow_changes_allowed", True))),
    ("open-stage8a1-directly", lambda root: descriptor(root, lambda value: value.__setitem__("stage8a_1_open", True))),
    ("enable-finam-post-delete", lambda root: descriptor(root, lambda value: value.__setitem__("finam_post_delete_allowed", True))),
    ("omit-official-rest-url", lambda root: snapshot(root, lambda value: value["retrieval"].__setitem__("official_rest_index", ""))),
    ("omit-official-grpc-source", lambda root: snapshot(root, lambda value: value["retrieval"].__setitem__("official_grpc_index", ""))),
    ("change-snapshot-without-sha", lambda root: snapshot(root, lambda value: value.__setitem__("retrieved_at_utc", "2026-08-15T00:00:00Z"))),
    ("non-reproducible-hand-edit", lambda root: snapshot(root, lambda value: value["retrieval"].__setitem__("method", "hand edited"))),
    ("change-place-path", lambda root: snapshot(root, lambda value: value["place_order"].__setitem__("path", "/v2/orders"))),
    ("change-cancel-path", lambda root: snapshot(root, lambda value: value["cancel_order"].__setitem__("path", "/v2/orders/{order_id}"))),
    ("omit-place-field", lambda root: snapshot(root, lambda value: value["place_order"]["request_fields"].remove("time_in_force"))),
    ("drop-order-type-value", lambda root: snapshot(root, lambda value: value["enums"]["order_type"].remove("ORDER_TYPE_MULTI_LEG"))),
    ("drop-tif-value", lambda root: snapshot(root, lambda value: value["enums"]["time_in_force"].remove("TIME_IN_FORCE_EXT"))),
    ("allow-non-day-stage8-tif", lambda root: descriptor(root, lambda value: value["initial_time_in_force"].append("IOC"))),
    ("omit-client-id-max20", lambda root: snapshot(root, lambda value: value["client_order_id_broker_contract"].__setitem__("maximum_characters", 21))),
    ("allow-broker-generated-client-id", lambda root: descriptor(root, lambda value: value.__setitem__("broker_generated_client_order_id_fallback_allowed", True))),
    ("enable-arbitrary-comment", lambda root: descriptor(root, lambda value: value.__setitem__("outgoing_order_comment_policy", "arbitrary"))),
    ("drop-place-default-status", lambda root: snapshot(root, lambda value: value["place_order"]["response_statuses"].remove("default"))),
    ("drop-cancel-401", lambda root: snapshot(root, lambda value: value["cancel_order"]["response_statuses"].remove("401"))),
    ("cancel400-broker-rejected", lambda root: snapshot(root, lambda value: value["stage8_initial_policy"].__setitem__("cancel_400", "BrokerRejected"))),
    ("cancel401-ordinary-retry", lambda root: snapshot(root, lambda value: value["stage8_initial_policy"].__setitem__("cancel_401", "ordinary reject and retry"))),
    ("cancel404-success", lambda root: snapshot(root, lambda value: value["stage8_initial_policy"].__setitem__("cancel_404", "Success"))),
    ("cancel409-success", lambda root: snapshot(root, lambda value: value["stage8_initial_policy"].__setitem__("cancel_409_410", "Success"))),
    ("introduce-second-serializer", lambda root: append(root, checker.ORDER_REQUEST, "\npub fn build_place_order_request() {}\n")),
    ("silently-accept-material-drift", lambda root: parity(root, lambda value: value["comparisons"].__setitem__("material_contract_drift", True))),
    ("ignore-unknown-order-status", lambda root: append(root, checker.MAPPER, "\n// unknown OrderStatus accepted\n")),
    ("omit-schedule-prerequisite", lambda root: snapshot(root, lambda value: value["instrument_prerequisites"].__setitem__("schedule_path", ""))),
    ("definitely-not-sent-same-request-retry", lambda root: descriptor(root, lambda value: value.__setitem__("definitely_not_sent_same_request_retry_allowed", True))),
    ("add-stage8-journal-owner", lambda root: descriptor(root, lambda value: value.__setitem__("stage8_journal_reducer_allocator_added", True))),
    ("stage5-scanner-sole-authority", lambda root: descriptor(root, lambda value: value.__setitem__("historical_stage5_scanner_sole_authority", True))),
    ("auto-fix-production-mapper", lambda root: descriptor(root, lambda value: value.__setitem__("production_fix_in_stage8a0", True))),
    ("unpin-negative-count", lambda root: descriptor(root, lambda value: value.__setitem__("negative_case_count", 35))),
    ("skip-workspace-regression", lambda root: descriptor(root, lambda value: value.__setitem__("workspace_regression_required", False))),
    ("self-accept-and-open-8a2", lambda root: descriptor(root, lambda value: (value.__setitem__("status", "ACCEPTED"), value.__setitem__("stage8a_2_through_8a_5_open", True)))),
]


def main() -> None:
    checker.check(ROOT, check_git_scope=False)
    if len(CASES) != 36:
        raise SystemExit(f"stage8a0-negative: FAIL inventory={len(CASES)}/36")
    passed = 0
    for name, mutate in CASES:
        with tempfile.TemporaryDirectory(prefix="stage8a0-negative-") as raw:
            root = Path(raw)
            copy_contract(root)
            mutate(root)
            try:
                checker.check(root, check_git_scope=False)
            except checker.GateFailure:
                passed += 1
                print(f"PASS {name}")
            else:
                raise SystemExit(f"stage8a0-negative: FAIL accepted mutation {name}")
    if passed != 36:
        raise SystemExit(f"stage8a0-negative: FAIL cases={passed}/36")
    print("stage8a0-negative: PASS cases=36/36")


if __name__ == "__main__":
    main()
