#!/usr/bin/env python3
"""Redigested adversarial mutations for Stage 8B-P1-e R8 design."""

from __future__ import annotations

import csv
import io
import json
from typing import Any, Callable

import stage8b_p1e_r8_design_check as checker


BASE = checker.read_all()


def json_bytes(value: Any) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def mutate_json(blobs: dict[str, bytes], name: str, change: Callable[[dict[str, Any]], None]) -> None:
    value = json.loads(blobs[name])
    change(value)
    blobs[name] = json_bytes(value)


def mutate_csv(blobs: dict[str, bytes], name: str, change: Callable[[list[dict[str, str]]], None]) -> None:
    rows = checker.csv_rows(blobs[name])
    fields = list(rows[0])
    change(rows)
    stream = io.StringIO(newline="")
    writer = csv.DictWriter(stream, fieldnames=fields, lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    blobs[name] = stream.getvalue().encode()


def replace_once(value: bytes, old: str, new: str) -> bytes:
    text = value.decode()
    if text.count(old) != 1:
        raise SystemExit(f"mutation source count for {old!r}: {text.count(old)}")
    return text.replace(old, new, 1).encode()


def route(blobs: dict[str, bytes], cell_id: str) -> dict[str, Any]:
    return next(row for row in json.loads(blobs["routes"])["rows"] if row["cell_id"] == cell_id)


def mutate_route(blobs: dict[str, bytes], cell_id: str, **changes: Any) -> None:
    def change(value: dict[str, Any]) -> None:
        next(row for row in value["rows"] if row["cell_id"] == cell_id).update(changes)
    mutate_json(blobs, "routes", change)


def remove_route(blobs: dict[str, bytes], cell_id: str) -> None:
    def change(value: dict[str, Any]) -> None:
        value["rows"] = [row for row in value["rows"] if row["cell_id"] != cell_id]
        value["row_count"] = len(value["rows"])
        value["class_counts"] = {
            kind: sum(row["class"] == kind for row in value["rows"])
            for kind in ("reclaim", "terminal")
        }
    mutate_json(blobs, "routes", change)


def redigest(blobs: dict[str, bytes]) -> None:
    active = json.loads(blobs["active"])
    active["r8_source"]["sha256"] = checker.sha256(blobs["acceptance"])
    blobs["active"] = json_bytes(active)
    semantic = json.loads(blobs["semantic"])
    bindings = semantic["contract_bindings"]
    for key, name in (
        ("latch_aware_source_seam_v2_sha256", "seam"),
        ("supervisor_event_matrix_v3_sha256", "events"),
        ("latch_route_transition_matrix_v1_sha256", "routes"),
        ("shutdown_intent_v1_sha256", "shutdown"),
        ("i0_regression_gate_v1_sha256", "regression"),
    ):
        bindings[key] = checker.sha256(blobs[name])
    blobs["semantic"] = json_bytes(semantic)
    evidence = json.loads(blobs["evidence"])
    for name in evidence["contract_sha256"]:
        evidence["contract_sha256"][name] = checker.sha256(blobs[name])
    blobs["evidence"] = json_bytes(evidence)


cases: list[tuple[str, dict[str, bytes]]] = []


def add(name: str, change: Callable[[dict[str, bytes]], None]) -> None:
    blobs = dict(BASE)
    change(blobs)
    redigest(blobs)
    cases.append((name, blobs))


add("design-opens-source-now", lambda b: b.__setitem__("design", replace_once(b["design"], "R8 opens no supervisor binary", "R8 implements source and opens supervisor binary")))
add("design-removes-thirty-variant-proof", lambda b: b.__setitem__("design", replace_once(b["design"], "all 30 material variants", "twenty logical routes")))
add("acceptance-optional", lambda b: mutate_csv(b, "acceptance", lambda rows: rows[-1].update(status="OPTIONAL")))
add("active-base-hash-drift", lambda b: mutate_json(b, "active", lambda v: v["base_active_contract"].update(sha256="0" * 64)))
add("active-supersession-omitted", lambda b: mutate_json(b, "active", lambda v: v["superseded_base_rows"].remove("P1ER7-020")))
add("active-count-drift", lambda b: mutate_json(b, "active", lambda v: v["active_contract_expectation"].update(total_active_rows=280)))
add("semantic-terminal-old-value", lambda b: mutate_json(b, "semantic", lambda v: v["replacement_active_authorities"].update({"redis.TerminalSourceResolution": v["superseded_active_authorities"]["redis.TerminalSourceResolution"]})))
add("semantic-shutdown-key-omitted", lambda b: mutate_json(b, "semantic", lambda v: v["required_new_keys"].remove("shutdown.CauseCarryingIntent")))
add("semantic-binding-extra", lambda b: mutate_json(b, "semantic", lambda v: v["contract_bindings"].update(extra="0" * 64)))
add("seam-option-b", lambda b: mutate_json(b, "seam", lambda v: v.update(selected_option="B-atomic-wrapper-drain")))
add("seam-source-hash-drift", lambda b: mutate_json(b, "seam", lambda v: v["current_source_baseline"].update(sha256="0" * 64)))
add("seam-claims-source-modified", lambda b: mutate_json(b, "seam", lambda v: v["current_source_baseline"].update(modified_in_r8_design=True)))
add("seam-allowlist-expansion", lambda b: mutate_json(b, "seam", lambda v: v["i0_authorization_after_independent_r8_acceptance"]["allowed_production_files"].append("crates/finam-gateway/src/lib.rs")))
add("seam-cargo-open", lambda b: mutate_json(b, "seam", lambda v: v["i0_authorization_after_independent_r8_acceptance"].update(cargo_change_allowed=True)))
add("seam-supervisor-open", lambda b: mutate_json(b, "seam", lambda v: v["i0_authorization_after_independent_r8_acceptance"].update(supervisor_binary_allowed=True)))
add("owner-clone", lambda b: mutate_json(b, "seam", lambda v: v["opaque_protocol_types"].update(clone=True)))
add("owner-public-fields", lambda b: mutate_json(b, "seam", lambda v: v["opaque_protocol_types"].update(private_fields=False)))
add("acquisition-select-cancellation", lambda b: mutate_json(b, "seam", lambda v: v["latch_protocol"].update(acquisition_future_select_cancellation_allowed=True)))
add("continuation-select-cancellation", lambda b: mutate_json(b, "seam", lambda v: v["latch_protocol"].update(continuation_future_select_cancellation_allowed=True)))
add("absolute-xpending-ban-restored", lambda b: mutate_json(b, "seam", lambda v: v["continuation_body_forbidden_delivery_acquisition"].append("XPENDING")))
add("xautoclaim-post-permit-allowed", lambda b: mutate_json(b, "seam", lambda v: v["continuation_body_forbidden_delivery_acquisition"].remove("XAUTOCLAIM")))
add("second-exact-delivery-allowed", lambda b: mutate_json(b, "seam", lambda v: v["continuation_body_forbidden_delivery_acquisition"].remove("exact_delivery_for_evidence")))
add("second-owner-allowed", lambda b: mutate_json(b, "seam", lambda v: v["post_permit_terminal_resolution_observations"].update(second_delivery_owner_forbidden=False)))
add("terminal-xinfo-omitted", lambda b: mutate_json(b, "seam", lambda v: v["post_permit_terminal_resolution_observations"]["allowed_once_per_terminal_permit"].remove("XINFO-GROUPS")))
add("terminal-xpending-omitted", lambda b: mutate_json(b, "seam", lambda v: v["post_permit_terminal_resolution_observations"]["allowed_once_per_terminal_permit"].remove("XPENDING-exact-source")))
add("terminal-conflict-class-omitted", lambda b: mutate_json(b, "seam", lambda v: v["post_permit_terminal_resolution_observations"]["classification"].pop("conflict")))
add("p1d4-revalidation-before-latch", lambda b: mutate_json(b, "seam", lambda v: v["p1d4_terminal_revalidation"].update(before_latch_forbidden=False)))
add("p1d4-revalidation-optional", lambda b: mutate_json(b, "seam", lambda v: v["p1d4_terminal_revalidation"].update(omission_forbidden=False)))
add("p1d4-revalidation-xpending-omitted", lambda b: mutate_json(b, "seam", lambda v: v["p1d4_terminal_revalidation"]["read_only_operations"].remove("XPENDING-exact-source")))
add("route-lr12-candidate-omitted", lambda b: remove_route(b, "LR12-candidate-source"))
add("route-lr15-terminal-omitted", lambda b: remove_route(b, "LR15-s_terminal"))
add("route-terminal-already-omitted", lambda b: remove_route(b, "LT03-already-acknowledged"))
add("route-due-timer-omitted", lambda b: remove_route(b, "LT05-due-day-timer"))
add("route-next-boundary-altered", lambda b: mutate_route(b, "LR03-default", exact_next_covering_boundary="p1d2_s_truth"))
add("route-stops-at-current-boundary", lambda b: mutate_route(b, "LR03-default", exact_next_covering_boundary="p1d2_pre_ack"))
add("route-illegal-xack-at-s-ack", lambda b: mutate_route(b, "LR13-default", xack_legality="exactly-one-XACK"))
add("route-pending-source-not-resolved", lambda b: mutate_route(b, "LT04-pending", source_pel_disposition="pending"))
add("route-already-treated-as-pending", lambda b: mutate_route(b, "LT04-already-acknowledged", xack_legality="exactly-one-XACK-reply-1"))
add("route-p1d4-revalidation-omitted", lambda b: mutate_route(b, "LT03-pending", first_post_permit_effect="terminal-resolution-XINFO-XRANGE-XPENDING"))
add("route-test-id-duplicated", lambda b: mutate_route(b, "LR02-default", test_id=route(b, "LR01-default")["test_id"]))
add("route-preset-latch-effect", lambda b: mutate_json(b, "routes", lambda v: v["test_protocol"].update(preset_latch="RetainForRestart then parse source")))
add("shutdown-telemetry-downgraded", lambda b: mutate_json(b, "shutdown", lambda v: v["causes"].update(TelemetryFailure=0)))
add("shutdown-second-signal-replaces", lambda b: mutate_json(b, "shutdown", lambda v: v["rules"].update(second_signal="replace-with-external-signal")))
add("event-e18-clean-exit", lambda b: mutate_csv(b, "events", lambda rows: next(row for row in rows if row["id"] == "E18").update(exit_code="0")))
add("i0-full-p1d4-gate-omitted", lambda b: mutate_json(b, "regression", lambda v: v["p1d4_full_gate"].update(command="", clean_runs=1)))

if len(cases) != 45:
    raise SystemExit(f"R8 mutation inventory drifted: {len(cases)}")

escaped: list[str] = []
for name, blobs in cases:
    try:
        checker.validate({checker.FILES[key]: value for key, value in blobs.items()})
    except (checker.CheckFailure, checker.r7.CheckFailure, checker.r7.r6.CheckFailure, checker.r7.r6.r5.CheckFailure, checker.r7.r6.r5.r4.CheckFailure, ValueError, KeyError, TypeError, json.JSONDecodeError):
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")

if escaped:
    raise SystemExit("R8 mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1e-r8-design-negative-harness 45/45 redigested=true")
