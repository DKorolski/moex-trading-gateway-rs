#!/usr/bin/env python3
"""Exact 36 negative mutations for the Gate 7->8 R3 contract."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path
from typing import Callable

import transition_gate_7_to_8_check as checker

ROOT = Path(__file__).resolve().parents[1]
FILES = {
    checker.DESCRIPTOR,
    checker.SPEC,
    checker.MATRIX,
    checker.SLICE_PLAN,
    checker.CONTRACT_SNAPSHOT,
    checker.CONTRACT_EVIDENCE,
    checker.ORDER_REQUEST_SOURCE,
    checker.ORDER_ENUM_FIXTURE,
    checker.CLOSURE_DESCRIPTOR,
    checker.ACCEPTANCE_RECORD,
    Path("docs/current-status.md"),
    Path("docs/roadmap.md"),
}


def copy_contracts(destination: Path) -> None:
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
    edit_json(root, checker.CONTRACT_SNAPSHOT, mutate)


def replace(root: Path, path: Path, old: str, new: str) -> None:
    target = root / path
    text = target.read_text()
    if old not in text:
        raise RuntimeError(f"mutation source missing: {path}: {old}")
    target.write_text(text.replace(old, new, 1))


def append(root: Path, path: Path, value: str) -> None:
    target = root / path
    target.write_text(target.read_text() + value)


CASES: list[tuple[str, Callable[[Path], None]]] = [
    (
        "self-accept-gate-r3",
        lambda root: descriptor(root, lambda value: value.__setitem__("status", "ACCEPTED")),
    ),
    (
        "open-stage8a-network-send",
        lambda root: descriptor(root, lambda value: value["decision_after_independent_acceptance"].__setitem__("stage8_production_rust_authorized", True)),
    ),
    (
        "open-stage8b-real-execution",
        lambda root: descriptor(root, lambda value: value["decision_after_independent_acceptance"].__setitem__("stage8b_real_execution", "open")),
    ),
    (
        "open-finam-post",
        lambda root: descriptor(root, lambda value: value["currently_open_surfaces"].__setitem__("finam_http_post", True)),
    ),
    (
        "open-finam-delete",
        lambda root: descriptor(root, lambda value: value["currently_open_surfaces"].__setitem__("finam_http_delete", True)),
    ),
    (
        "open-runtime-live",
        lambda root: descriptor(root, lambda value: value["currently_open_surfaces"].__setitem__("runtime_live", True)),
    ),
    (
        "allow-stop-sltp-bracket-multileg",
        lambda root: descriptor(root, lambda value: value["allowed_initial_commands"].append("STOP")),
    ),
    (
        "drop-accepted-stage7b-binding",
        lambda root: descriptor(root, lambda value: value["accepted_stage7b"].__setitem__("accepted_source_ref", "0" * 40)),
    ),
    (
        "remove-official-finam-contract-refresh",
        lambda root: snapshot(root, lambda value: value["official_source"].__setitem__("rest_documentation_url", "")),
    ),
    (
        "change-place-endpoint-path",
        lambda root: snapshot(root, lambda value: value["place_order"].__setitem__("path", "/v2/orders")),
    ),
    (
        "change-cancel-endpoint-path",
        lambda root: snapshot(root, lambda value: value["cancel_order"].__setitem__("path", "/v2/orders/{order_id}")),
    ),
    (
        "omit-time-in-force-from-place-contract",
        lambda root: snapshot(root, lambda value: value["place_order"]["documented_body_fields"].remove("time_in_force")),
    ),
    (
        "allow-non-day-initial-tif",
        lambda root: descriptor(root, lambda value: value["allowed_initial_time_in_force"].append("TIME_IN_FORCE_IOC")),
    ),
    (
        "allow-broker-generated-client-id-fallback",
        lambda root: descriptor(root, lambda value: value["safety_invariants"].__setitem__("broker_generated_client_order_id_fallback", True)),
    ),
    (
        "allow-client-order-id-over-20",
        lambda root: snapshot(root, lambda value: value["place_order"]["client_order_id"].__setitem__("maximum_characters", 21)),
    ),
    (
        "introduce-second-stage8-serializer",
        lambda root: descriptor(root, lambda value: value["safety_invariants"].__setitem__("second_stage8_serializer_allowed", True)),
    ),
    (
        "generic-all-4xx-broker-rejected",
        lambda root: descriptor(root, lambda value: value["safety_invariants"].__setitem__("generic_all_4xx_classifier_allowed", True)),
    ),
    (
        "cancel-400-already-executed-to-rejected",
        lambda root: replace(root, checker.SPEC, "documented 400 already executed", "documented 400 is BrokerRejected"),
    ),
    (
        "cancel-404-to-rejected",
        lambda root: replace(root, checker.SPEC, "documented 404 account/order not found", "documented 404 is BrokerRejected"),
    ),
    (
        "cancel-409-to-success",
        lambda root: replace(root, checker.SPEC, "undocumented 409 or 410", "HTTP conflict is success"),
    ),
    (
        "place-429-to-final-rejection",
        lambda root: replace(root, checker.SPEC, "429, 500, 503, 504 or default | `ReconciliationRequired`", "429, 500, 503, 504 or default | `BrokerRejected`"),
    ),
    (
        "place-malformed-2xx-to-accepted",
        lambda root: replace(root, checker.SPEC, "malformed, truncated or unknown 2xx", "unexpected 2xx is accepted"),
    ),
    (
        "timeout-without-proof-to-definitely-not-sent",
        lambda root: replace(root, checker.SPEC, "Only a pre-send/local connect failure with proof that no bytes could leave", "Any timeout"),
    ),
    (
        "empty-snapshot-to-proven-no-match",
        lambda root: replace(root, checker.SPEC, "missing, stale or merely absent truth always remains `StillUnknown`", "missing or stale truth becomes `ProvenNoMatch`"),
    ),
    (
        "enable-proven-no-match-constructor",
        lambda root: descriptor(root, lambda value: value["safety_invariants"].__setitem__("proven_no_match_constructible_in_stage8a", True)),
    ),
    (
        "multiple-candidates-choose-first",
        lambda root: replace(root, checker.SPEC, "multiple candidates mean conflict and no new live command", "multiple candidates choose first"),
    ),
    (
        "redispatch-old-ambiguous-request",
        lambda root: replace(root, checker.SPEC, "Reconciliation never redispatches an old ambiguous request", "Reconciliation may redispatch an old ambiguous request"),
    ),
    (
        "remove-kill-switch-pre-send-check",
        lambda root: replace(root, checker.SPEC, "immediately before transport", "after transport"),
    ),
    (
        "kill-switch-read-failure-run-allowed",
        lambda root: descriptor(root, lambda value: value["safety_invariants"].__setitem__("kill_switch_unreadable_or_stale_fails_closed", False)),
    ),
    (
        "remove-stage8b-kill-switch-requirement",
        lambda root: replace(root, checker.SPEC, "same kill-switch mechanism", "optional kill-switch mechanism"),
    ),
    (
        "allow-dual-alor-finam-ownership",
        lambda root: descriptor(root, lambda value: value["safety_invariants"].__setitem__("simultaneous_alor_finam_live_for_same_strategy", True)),
    ),
    (
        "reuse-stale-stage5-scanner-as-sole-authority",
        lambda root: replace(root, checker.SPEC, "historical Stage 5 `forbidden_surface_scan.sh` is not rebaselined here", "historical Stage 5 `forbidden_surface_scan.sh` is the sole authority"),
    ),
    (
        "cancel-401-broker-rejected-and-same-request-retry",
        lambda root: append(root, checker.SPEC, "\nCANCEL 401 -> BrokerRejected; same cancel request may be retried.\n"),
    ),
    (
        "definitely-not-sent-rearm-same-durable-request",
        lambda root: append(root, checker.SPEC, "\nDefinitelyNotSent: new arm may reuse the same durable request and resend the same durable request.\n"),
    ),
    (
        "gate-opens-8a1-or-later-directly",
        lambda root: descriptor(root, lambda value: value["decision_after_independent_acceptance"].__setitem__("stage8a_1_protected_capability", "authorized_no_send")),
    ),
    (
        "post-acceptance-transition-contradiction",
        lambda root: append(root, checker.SPEC, "\nUntil then, and even after acceptance until the relevant later gate: Stage 8 implementation CLOSED.\n"),
    ),
]


def main() -> None:
    checker.check(ROOT, check_git_scope=False)
    if len(CASES) != 36:
        raise SystemExit(f"transition-gate-7-to-8-negative: FAIL inventory={len(CASES)}/36")
    passed = 0
    for name, mutate in CASES:
        with tempfile.TemporaryDirectory(prefix="gate7-to-8-r3-negative-") as raw:
            root = Path(raw)
            copy_contracts(root)
            mutate(root)
            try:
                checker.check(root, check_git_scope=False)
            except checker.GateFailure:
                passed += 1
                print(f"PASS {name}")
            else:
                raise SystemExit(f"transition-gate-7-to-8-negative: FAIL accepted mutation {name}")
    if passed != 36:
        raise SystemExit(f"transition-gate-7-to-8-negative: FAIL cases={passed}/36")
    print("transition-gate-7-to-8-negative: PASS cases=36/36")


if __name__ == "__main__":
    main()
