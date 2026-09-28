#!/usr/bin/env python3
"""Redigested adversarial mutations for Stage 8B-P1-e R5 design."""

from __future__ import annotations

import copy
import csv
import io
import json
from typing import Any, Callable

import stage8b_p1e_r5_design_check as checker


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


def redigest(blobs: dict[str, bytes]) -> None:
    evidence = json.loads(blobs["evidence"])
    for name in evidence["contract_sha256"]:
        evidence["contract_sha256"][name] = checker.sha256(blobs[name])
    blobs["evidence"] = json_bytes(evidence)


cases: list[tuple[str, dict[str, bytes], bool]] = []


def add(name: str, change: Callable[[dict[str, bytes]], None], acceptance_changed: bool = False) -> None:
    blobs = dict(BASE)
    change(blobs)
    redigest(blobs)
    cases.append((name, blobs, acceptance_changed))


# Design and active-contract boundary.
add("design-opens-source", lambda b: b.__setitem__("design", replace_once(b["design"], "Status: design-only review candidate", "Status: source implementation authorized")))
add("design-active-count-drift", lambda b: b.__setitem__("design", replace_once(b["design"], "217 active rows", "216 active rows")))
add("design-receipt-wins", lambda b: b.__setitem__("design", replace_once(b["design"], "a valid receipt is a durable prerequisite", "a valid receipt immediately authorizes run")))
add("active-reenables-r4-row", lambda b: mutate_json(b, "active", lambda v: v["sources"][3]["superseded_rows"].remove("P1ER4-002")))
add("active-drops-map", lambda b: mutate_json(b, "active", lambda v: v["supersession_map"].pop()))
add("active-total-forged", lambda b: mutate_json(b, "active", lambda v: v["active_contract_expectation"].update(total_active_rows=218)))

def optional_acceptance(blobs: dict[str, bytes]) -> None:
    mutate_csv(blobs, "acceptance", lambda rows: rows[-1].update(status="OPTIONAL"))
    active = json.loads(blobs["active"])
    active["sources"][4]["file_sha256"] = checker.sha256(blobs["acceptance"])
    blobs["active"] = json_bytes(active)

add("acceptance-row-optional", optional_acceptance, True)

# Execution-level semantic authority.
add("semantic-main-address-conflict", lambda b: mutate_json(b, "semantic", lambda v: v["authorities"].append({"key":"systemd.MainAddressFamilies","value":"AF_UNIX","source":"mutation","active":True})))
add("semantic-main-private-network-drift", lambda b: mutate_json(b, "semantic", lambda v: v["expected_active_values"].update({"systemd.MainPrivateNetwork":"true-private-namespace"})))
add("semantic-bootstrap-isolation-omitted", lambda b: mutate_json(b, "semantic", lambda v: v["required_keys"].remove("systemd.BootstrapNetworkIsolation")))
add("semantic-overlap-authority-reactivated", lambda b: mutate_json(b, "semantic", lambda v: next(x for x in v["authorities"] if x["key"] == "firstboot.ClassificationDisjointnessOrPrecedence" and not x["active"]).update(active=True)))
add("semantic-phase-set-nonsense", lambda b: mutate_json(b, "semantic", lambda v: v["expected_active_values"].update({"restart.P1d3TruthCommittedPhaseSet":"nonsense"})))
add("semantic-cancel-continuation-conflict", lambda b: mutate_json(b, "semantic", lambda v: v["authorities"].append({"key":"restart.CancelRecoveredOnlyContinuation","value":"generic-resume","source":"mutation","active":True})))
add("semantic-endpoint-key-omitted", lambda b: mutate_json(b, "semantic", lambda v: v["required_keys"].remove("systemd.MainRedisEndpointPolicy")))

# Main versus bootstrap/recovery network composition.
add("network-main-missing-ipv4", lambda b: mutate_json(b, "identity", lambda v: v["mode_specific_network_contracts"]["main_run"]["RestrictAddressFamilies"].remove("AF_INET")))
add("network-main-missing-ipv6", lambda b: mutate_json(b, "identity", lambda v: v["mode_specific_network_contracts"]["main_run"]["RestrictAddressFamilies"].remove("AF_INET6")))
add("network-main-private", lambda b: mutate_json(b, "identity", lambda v: v["mode_specific_network_contracts"]["main_run"].update(PrivateNetwork=True)))
add("network-nonloopback-allowed", lambda b: mutate_json(b, "identity", lambda v: v["mode_specific_network_contracts"]["main_run"]["IPAddressAllow"].append("0.0.0.0/0")))
add("network-wrong-db", lambda b: mutate_json(b, "identity", lambda v: v["mode_specific_network_contracts"]["main_run"]["redis_url_allowlist"].__setitem__(0, "redis://127.0.0.1:6379/0")))
add("network-wrong-port", lambda b: mutate_json(b, "identity", lambda v: v["mode_specific_network_contracts"]["main_run"].update(destination_port=6380)))
add("network-bootstrap-host-namespace", lambda b: mutate_json(b, "identity", lambda v: v["mode_specific_network_contracts"]["bootstrap"].update(PrivateNetwork=False)))
add("network-bootstrap-inet", lambda b: mutate_json(b, "identity", lambda v: v["mode_specific_network_contracts"]["bootstrap"]["RestrictAddressFamilies"].append("AF_INET")))
add("network-recovery-redis-contact", lambda b: mutate_json(b, "identity", lambda v: v["mode_specific_network_contracts"]["bootstrap_recover"].update(redis_contact_allowed=True)))
add("network-policy-returned-to-shared", lambda b: mutate_json(b, "identity", lambda v: v["shared_unit_contract"].update(PrivateNetwork=True)))

# First-boot partition and ordinary-run authority.
add("firstboot-post-receipt-state-omitted", lambda b: mutate_json(b, "transaction", lambda v: v["base_classifications"].__setitem__(8, v["base_classifications"][9])))
add("firstboot-post-receipt-run-allowed", lambda b: mutate_json(b, "transaction", lambda v: next(x for x in v["base_classifications"] if x["classification"] == "ReceiptCommittedMarkerUpdatePending").update(run_allowed=True)))
add("firstboot-adopted-allows-marker-temp", lambda b: mutate_json(b, "transaction", lambda v: next(x for x in v["base_classifications"] if x["classification"] == "AdoptedCommittedRoot").update(precondition="valid-receipt-v2-package-v2-seal-provenance-ready-owner-binding-canonical-marker-phase-adopted-marker-temp-present-or-absent")))
add("firstboot-precedence-enabled", lambda b: mutate_json(b, "transaction", lambda v: v["classification_model"].update(precedence_allowed=True)))
add("firstboot-receipt-sufficient", lambda b: mutate_json(b, "transaction", lambda v: v["adoption_protocol"].update(receipt_is_sufficient_without_adopted_marker=True)))
add("firstboot-adopted-temp-exclusivity-omitted", lambda b: mutate_json(b, "transaction", lambda v: next(x for x in v["marker_update_temp_classifications"] if x["classification"] == "SealCommittedToAdoptedMarkerTempPending").pop("exclusive_precondition")))
add("firstboot-multiple-match-runs", lambda b: mutate_json(b, "transaction", lambda v: v["classification_model"].update(multiple_matches="ordinary-run")))
add("firstboot-run-before-adopted", lambda b: mutate_json(b, "transaction", lambda v: v["adoption_protocol"].update(ordinary_run_requires_marker_phase="seal_committed")))

# Exact outer routing for P1d3 cancel-recovered.
add("outer-cancel-route-missing", lambda b: mutate_csv(b, "outer", lambda rows: rows.remove(next(x for x in rows if x["authenticated_package_phase"] == "p1d3_s_cancel_recovered"))))
add("outer-cancel-phase-nonsense", lambda b: mutate_csv(b, "outer", lambda rows: next(x for x in rows if x["authenticated_package_phase"] == "p1d3_s_cancel_recovered").update(authenticated_package_phase="nonsense")))
add("outer-cancel-truth-replay", lambda b: mutate_csv(b, "outer", lambda rows: next(x for x in rows if x["authenticated_package_phase"] == "p1d3_s_cancel_recovered").update(truth_replay_legality="allowed")))
add("outer-cancel-generic-transition", lambda b: mutate_csv(b, "outer", lambda rows: next(x for x in rows if x["authenticated_package_phase"] == "p1d3_s_cancel_recovered").update(first_legal_transition="generic_ready_fallback")))

# Four operational cancel-recovered cells.
add("operational-cancel-cell-missing", lambda b: mutate_csv(b, "operational", lambda rows: rows.remove(next(x for x in rows if x["id"] == "OC55"))))
add("operational-already-acked-second-xack", lambda b: mutate_csv(b, "operational", lambda rows: next(x for x in rows if x["id"] == "OC56").update(xack_legality="exact_source_xack_only")))
add("operational-not-claimable-fresh-read", lambda b: mutate_csv(b, "operational", lambda rows: next(x for x in rows if x["id"] == "OC57").update(fresh_poll_legality="one_bounded_s08_poll")))
add("operational-due-timer-first", lambda b: mutate_csv(b, "operational", lambda rows: next(x for x in rows if x["id"] == "OC58").update(first_legal_transition="issue_day_expiry")))
add("operational-premature-paper-ready", lambda b: mutate_csv(b, "operational", lambda rows: next(x for x in rows if x["id"] == "OC58").update(paper_ready_legality="immediate")))

# Cross-artifact precedence binding.
add("precedence-cancel-phase-nonsense", lambda b: mutate_json(b, "precedence", lambda v: v["source_bearing_owner_derivation"]["logical_cancel_recovered_representation"].update(authenticated_package_phase="nonsense")))
add("precedence-cancel-truth-replay", lambda b: mutate_json(b, "precedence", lambda v: v["source_bearing_owner_derivation"]["logical_cancel_recovered_representation"].update(truth_replay_allowed=True)))
add("precedence-due-route-omitted", lambda b: mutate_json(b, "precedence", lambda v: v["cancel_recovered_exact_contract"]["legal_source_transitions"].pop()))

if len(cases) != 44:
    raise SystemExit(f"R5 mutation inventory drifted: {len(cases)}")

escaped: list[str] = []
original_acceptance_sha = checker.R5_ACCEPTANCE_SHA
for name, blobs, acceptance_changed in cases:
    try:
        if acceptance_changed:
            checker.R5_ACCEPTANCE_SHA = checker.sha256(blobs["acceptance"])
        checker.validate({checker.FILES[key]: value for key, value in blobs.items()})
    except (checker.CheckFailure, checker.r4.CheckFailure, ValueError, KeyError, TypeError, json.JSONDecodeError):
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")
    finally:
        checker.R5_ACCEPTANCE_SHA = original_acceptance_sha

if escaped:
    raise SystemExit("R5 mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1e-r5-design-negative-harness 44/44 redigested=true")
