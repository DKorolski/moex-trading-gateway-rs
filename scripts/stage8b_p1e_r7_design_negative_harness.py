#!/usr/bin/env python3
"""Redigested adversarial mutations for Stage 8B-P1-e R7 design."""

from __future__ import annotations

import csv
import io
import json
from typing import Any, Callable

import stage8b_p1e_r7_design_check as checker


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


def redigest(blobs: dict[str, bytes], acceptance_changed: bool = False) -> None:
    if acceptance_changed:
        active = json.loads(blobs["active"])
        active["r7_source"]["sha256"] = checker.sha256(blobs["acceptance"])
        blobs["active"] = json_bytes(active)
    semantic = json.loads(blobs["semantic"])
    semantic["contract_bindings"]["latch_aware_source_seam_v1_sha256"] = checker.sha256(blobs["seam"])
    semantic["contract_bindings"]["supervisor_event_matrix_v2_sha256"] = checker.sha256(blobs["events"])
    semantic["contract_bindings"]["latch_race_test_matrix_v1_sha256"] = checker.sha256(blobs["tests"])
    blobs["semantic"] = json_bytes(semantic)
    evidence = json.loads(blobs["evidence"])
    for name in evidence["contract_sha256"]:
        evidence["contract_sha256"][name] = checker.sha256(blobs[name])
    blobs["evidence"] = json_bytes(evidence)


cases: list[tuple[str, dict[str, bytes], bool]] = []


def add(name: str, change: Callable[[dict[str, bytes]], None], acceptance_changed: bool = False) -> None:
    blobs = dict(BASE)
    change(blobs)
    redigest(blobs, acceptance_changed)
    cases.append((name, blobs, acceptance_changed))


add("design-opens-source-now", lambda b: b.__setitem__("design", replace_once(b["design"], "R7 itself is design-only", "R7 source is implemented")))
add("design-selects-atomic-drain", lambda b: b.__setitem__("design", replace_once(b["design"], "selects review option A", "selects review option B")))
add("acceptance-optional", lambda b: mutate_csv(b, "acceptance", lambda rows: rows[-1].update(status="OPTIONAL")), True)
add("active-base-hash-drift", lambda b: mutate_json(b, "active", lambda v: v["base_active_contract"].update(sha256="0" * 64)))
add("active-supersession-omitted", lambda b: mutate_json(b, "active", lambda v: v["superseded_base_rows"].remove("P1ER6-018")))
add("semantic-retains-monolithic-reclaim", lambda b: mutate_json(b, "semantic", lambda v: v["replacement_active_authorities"].update({"redis.ReclaimRequiredContinuation":"fifteen-exact-owner-phase-routes-existing-wrapper-sole-reclaim-claim-idle-applies"})))
add("semantic-missing-linear-owner", lambda b: mutate_json(b, "semantic", lambda v: v["required_new_keys"].remove("redis.PostAcquisitionLinearOwner")))
add("semantic-contract-binding-drift", lambda b: mutate_json(b, "semantic", lambda v: v["contract_bindings"].update(extra="0" * 64)))
add("seam-option-b", lambda b: mutate_json(b, "seam", lambda v: v.update(selected_option="B-atomic-wrapper-drain")))
add("seam-source-hash-drift", lambda b: mutate_json(b, "seam", lambda v: v["current_source_baseline"].update(sha256="0" * 64)))
add("seam-claims-source-modified", lambda b: mutate_json(b, "seam", lambda v: v["current_source_baseline"].update(modified_in_r7_design=True)))
add("seam-allowlist-expansion", lambda b: mutate_json(b, "seam", lambda v: v["i0_authorization_after_independent_r7_acceptance"]["allowed_production_files"].append("crates/finam-gateway/src/lib.rs")))
add("seam-cargo-open", lambda b: mutate_json(b, "seam", lambda v: v["i0_authorization_after_independent_r7_acceptance"].update(cargo_change_allowed=True)))
add("seam-supervisor-binary-open", lambda b: mutate_json(b, "seam", lambda v: v["i0_authorization_after_independent_r7_acceptance"].update(supervisor_binary_allowed=True)))
add("owner-clone", lambda b: mutate_json(b, "seam", lambda v: v["opaque_protocol_types"].update(clone=True)))
add("owner-serialize", lambda b: mutate_json(b, "seam", lambda v: v["opaque_protocol_types"].update(serialize=True)))
add("owner-public-fields", lambda b: mutate_json(b, "seam", lambda v: v["opaque_protocol_types"].update(private_fields=False)))
add("owner-transport-getter", lambda b: mutate_json(b, "seam", lambda v: v["opaque_protocol_types"].update(payload_or_transport_getter=True)))
add("acquisition-select-cancellation", lambda b: mutate_json(b, "seam", lambda v: v["latch_protocol"].update(acquisition_future_select_cancellation_allowed=True)))
add("continuation-select-cancellation", lambda b: mutate_json(b, "seam", lambda v: v["latch_protocol"].update(continuation_future_select_cancellation_allowed=True)))
add("direct-lookup-xack-allowed", lambda b: mutate_json(b, "seam", lambda v: v["acquisition_body_forbidden_after_delivery"].remove("acknowledge_exact")))
add("reclaim-parse-allowed", lambda b: mutate_json(b, "seam", lambda v: v["acquisition_body_forbidden_after_delivery"].remove("parse_exact")))
add("reclaim-provider-allowed", lambda b: mutate_json(b, "seam", lambda v: v["acquisition_body_forbidden_after_delivery"].remove("provider")))
add("reclaim-schedule-allowed", lambda b: mutate_json(b, "seam", lambda v: v["acquisition_body_forbidden_after_delivery"].remove("schedule")))
add("continuation-second-acquisition", lambda b: mutate_json(b, "seam", lambda v: v["continuation_body_forbidden_acquisition"].remove("XAUTOCLAIM")))
add("legacy-wrapper-bypass", lambda b: mutate_json(b, "seam", lambda v: v.update(legacy_wrapper_rule="old-signatures-retained")))
add("zero-intent-latch-after-xack", lambda b: mutate_json(b, "seam", lambda v: v.update(zero_intent_order=["exact_delivery_for_evidence","acknowledge_exact","decide_stage8b_p1e_post_acquisition_latch"])))
add("reclaim-route-omitted", lambda b: mutate_json(b, "seam", lambda v: v["reclaim_required_routes"].pop()))
add("test-route-omitted", lambda b: mutate_json(b, "tests", lambda v: v["routes"].pop()))
add("test-callback-negative-omitted", lambda b: mutate_json(b, "tests", lambda v: v["required_source_body_negatives"].remove("reclaim-to-callback-before-latch")))
add("test-select-negative-omitted", lambda b: mutate_json(b, "tests", lambda v: v["required_source_body_negatives"].remove("external-select-cancels-continuation-permit")))
add("event-e18-allows-xack", lambda b: mutate_csv(b, "events", lambda rows: next(row for row in rows if row["id"] == "E18").update(allowed_next_effect="acknowledge_exact")))

if len(cases) != 32:
    raise SystemExit(f"R7 mutation inventory drifted: {len(cases)}")

escaped: list[str] = []
original_acceptance_sha = checker.R7_ACCEPTANCE_SHA
for name, blobs, acceptance_changed in cases:
    try:
        if acceptance_changed:
            checker.R7_ACCEPTANCE_SHA = checker.sha256(blobs["acceptance"])
        checker.validate({checker.FILES[key]: value for key, value in blobs.items()})
    except (checker.CheckFailure, checker.r6.CheckFailure, checker.r6.r5.CheckFailure, checker.r6.r5.r4.CheckFailure, ValueError, KeyError, TypeError, json.JSONDecodeError):
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")
    finally:
        checker.R7_ACCEPTANCE_SHA = original_acceptance_sha

if escaped:
    raise SystemExit("R7 mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1e-r7-design-negative-harness 32/32 redigested=true")
