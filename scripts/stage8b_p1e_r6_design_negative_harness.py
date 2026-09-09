#!/usr/bin/env python3
"""Redigested adversarial mutations for Stage 8B-P1-e R6 design."""

from __future__ import annotations

import csv
import io
import json
from typing import Any, Callable

import stage8b_p1e_r6_design_check as checker


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
        active["r6_source"]["sha256"] = checker.sha256(blobs["acceptance"])
        blobs["active"] = json_bytes(active)
    semantic = json.loads(blobs["semantic"])
    binding_names = {
        "acquisition": "acquisition_model_v3_sha256",
        "operational": "operational_overlay_v6_sha256",
        "transaction": "first_boot_transaction_v5_sha256",
        "precedence": "source_timer_precedence_v3_sha256",
    }
    for name, key in binding_names.items():
        semantic["contract_bindings"][key] = checker.sha256(blobs[name])
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


# Design and active contract.
add("design-opens-source", lambda b: b.__setitem__("design", replace_once(b["design"], "Status: design-only review candidate", "Status: source implementation authorized")))
add("design-active-count-drift", lambda b: b.__setitem__("design", replace_once(b["design"], "V6 active rows                  242", "V6 active rows                  243")))
add("design-terminal-autoclaim", lambda b: b.__setitem__("design", replace_once(b["design"], "They never call XAUTOCLAIM", "They call XAUTOCLAIM")))
add("active-reenables-r4-acquisition", lambda b: mutate_json(b, "active", lambda v: v["superseded_base_rows"].remove("P1ER4-033")))
add("active-drops-supersession-map", lambda b: mutate_json(b, "active", lambda v: v["supersession_map"].pop()))
add("active-total-forged", lambda b: mutate_json(b, "active", lambda v: v["active_contract_expectation"].update(total_active_rows=243)))


def optional_acceptance(blobs: dict[str, bytes]) -> None:
    mutate_csv(blobs, "acceptance", lambda rows: rows[-1].update(status="OPTIONAL"))


add("acceptance-row-optional", optional_acceptance, True)
add("active-base-hash-drift", lambda b: mutate_json(b, "active", lambda v: v["base_active_contract"].update(sha256="0" * 64)))

# Semantic composition.
add("semantic-retains-old-nonready-owner", lambda b: mutate_json(b, "semantic", lambda v: v["replacement_active_authorities"].update({"redis.NonReadyAcquisitionOwner": "S06-observation-only-existing-resume-wrapper-sole-reclaim"})))
add("semantic-missing-terminal-key", lambda b: mutate_json(b, "semantic", lambda v: v["required_new_keys"].remove("redis.TerminalSourceResolution")))
add("semantic-terminal-claim-idle-enabled", lambda b: mutate_json(b, "semantic", lambda v: v["new_active_authorities"].update({"redis.TerminalClaimIdleDependency": "required"})))
add("semantic-reclaim-route-no-claim", lambda b: mutate_json(b, "semantic", lambda v: v["new_active_authorities"].update({"redis.ReclaimRequiredContinuation": "no-claim-lookup"})))
add("semantic-contract-binding-drift", lambda b: mutate_json(b, "semantic", lambda v: v["contract_bindings"].update(extra_unbound_contract="0" * 64)))

# Acquisition partition and accepted source API.
add("acquisition-terminal-autoclaim", lambda b: mutate_json(b, "acquisition", lambda v: v["terminal_resolution_rule"].update(XAUTOCLAIM_total=1)))
add("acquisition-terminal-claim-idle", lambda b: mutate_json(b, "acquisition", lambda v: v["terminal_resolution_rule"].update(claim_idle_ms_dependency=True)))
add("acquisition-terminal-reclaim-helper", lambda b: mutate_json(b, "acquisition", lambda v: v["terminal_source_resolution_continuations"][0].update(lookup="reclaim_exact_evidence")))
add("acquisition-terminal-function-drift", lambda b: mutate_json(b, "acquisition", lambda v: v["terminal_source_resolution_continuations"][1].update(function="resume_stage8b_p1d2_ack_with_redis")))
add("acquisition-terminal-route-omitted", lambda b: mutate_json(b, "acquisition", lambda v: v["terminal_source_resolution_continuations"].pop()))
add("acquisition-reclaim-owner-no-claim-wrapper", lambda b: mutate_json(b, "acquisition", lambda v: v["reclaim_required_semantic_continuations"][0].update(wrapper="exact_delivery_for_evidence")))
add("acquisition-owner-overlap", lambda b: mutate_json(b, "acquisition", lambda v: v["reclaim_required_semantic_continuations"][0].update(owner="P1d2TruthCommitted", phase="p1d2_s_truth")))
add("acquisition-s06-allows-autoclaim", lambda b: mutate_json(b, "acquisition", lambda v: v["s06_non_ready_observation"]["forbidden_redis_operations"].remove("XAUTOCLAIM")))
add("acquisition-source-oracle-hash-drift", lambda b: mutate_json(b, "acquisition", lambda v: v["accepted_source_oracle"].update(sha256="0" * 64)))


def mutate_source(blobs: dict[str, bytes]) -> None:
    blobs["source"] = replace_once(blobs["source"], ".exact_delivery_for_evidence(&evidence, pending.operational_identity_sha256())", ".reclaim_exact_evidence(&evidence)")
    acquisition = json.loads(blobs["acquisition"])
    acquisition["accepted_source_oracle"]["sha256"] = checker.sha256(blobs["source"])
    blobs["acquisition"] = json_bytes(acquisition)
    evidence = json.loads(blobs["evidence"])
    evidence["accepted_source_oracle_sha256"] = checker.sha256(blobs["source"])
    blobs["evidence"] = json_bytes(evidence)


add("source-terminal-body-reclaim", mutate_source)
add("acquisition-conflict-allows-xack", lambda b: mutate_json(b, "acquisition", lambda v: v["terminal_resolution_rule"].update(conflict="fails-closed-with-one-XACK")))
add("acquisition-idle-test-omitted", lambda b: mutate_json(b, "acquisition", lambda v: v["idle_boundary_tests"].pop()))
add("acquisition-terminal-latch-omitted", lambda b: mutate_json(b, "acquisition", lambda v: v["terminal_resolution_rule"].update(post_lookup_latch="none")))

# Operational overlay.
add("operational-retains-cancel-not-claimable", lambda b: mutate_json(b, "operational", lambda v: v["excluded_base_rows"].remove("OC57")))
add("operational-terminal-claimable", lambda b: mutate_json(b, "operational", lambda v: v["replacement_rows"][0].update(redis_source_state="one_exact_claimable_source")))
add("operational-autoclaim-enabled", lambda b: mutate_json(b, "operational", lambda v: v["replacement_defaults"].update(XAUTOCLAIM_total=1)))
add("operational-claim-idle-enabled", lambda b: mutate_json(b, "operational", lambda v: v["replacement_defaults"].update(claim_idle_ms_dependency=True)))
add("operational-cancel-cell-omitted", lambda b: mutate_json(b, "operational", lambda v: v["replacement_rows"].pop()))
add("operational-timer-before-source", lambda b: mutate_json(b, "operational", lambda v: v["replacement_rows"][-1].update(first_legal_transition="issue_day_expiry")))
add("operational-already-acked-second-xack", lambda b: mutate_json(b, "operational", lambda v: v["replacement_rows"][1].update(source_disposition="one_exact_XACK")))
add("operational-premature-paper-ready", lambda b: mutate_json(b, "operational", lambda v: v["replacement_rows"][-1].update(paper_ready_legality="immediate")))

# Post-receipt stale temp.
add("transaction-receipt-temp-absence-omitted", lambda b: mutate_json(b, "transaction", lambda v: v["predicate_replacements"]["ReceiptCommittedMarkerUpdatePending"].update(new_precondition=v["predicate_replacements"]["ReceiptCommittedMarkerUpdatePending"]["old_precondition"])))
add("transaction-adopted-temp-receipt-absence-omitted", lambda b: mutate_json(b, "transaction", lambda v: v["predicate_replacements"]["SealCommittedToAdoptedMarkerTempPending"].update(new_exclusive_precondition=v["predicate_replacements"]["SealCommittedToAdoptedMarkerTempPending"]["old_exclusive_precondition"])))
add("transaction-stale-temp-marker-mutation", lambda b: mutate_json(b, "transaction", lambda v: v["stale_receipt_temp_rule"].update(marker_mutation_allowed=True)))
add("transaction-stale-temp-accepted", lambda b: mutate_json(b, "transaction", lambda v: v["stale_receipt_temp_rule"].update(classification="ReceiptCommittedMarkerUpdatePending")))

# Source/timer composition.
add("precedence-terminal-claim-idle", lambda b: mutate_json(b, "precedence", lambda v: v["terminal_resolution_amendment"].update(claim_idle_ms_dependency=True)))
add("precedence-cancel-not-claimable-cell", lambda b: mutate_json(b, "precedence", lambda v: v["cancel_recovered_exact_contract"]["operational_rows"].append("OC57-not-yet-claimable")))

if len(cases) != 40:
    raise SystemExit(f"R6 mutation inventory drifted: {len(cases)}")

escaped: list[str] = []
original_acceptance_sha = checker.R6_ACCEPTANCE_SHA
for name, blobs, acceptance_changed in cases:
    try:
        if acceptance_changed:
            checker.R6_ACCEPTANCE_SHA = checker.sha256(blobs["acceptance"])
        checker.validate({checker.FILES[key]: value for key, value in blobs.items()})
    except (checker.CheckFailure, checker.r5.CheckFailure, checker.r5.r4.CheckFailure, ValueError, KeyError, TypeError, json.JSONDecodeError):
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")
    finally:
        checker.R6_ACCEPTANCE_SHA = original_acceptance_sha

if escaped:
    raise SystemExit("R6 mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1e-r6-design-negative-harness 40/40 redigested=true")
