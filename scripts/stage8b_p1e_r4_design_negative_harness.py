#!/usr/bin/env python3
"""Redigested mutation harness for Stage 8B-P1-e R4 design."""

from __future__ import annotations

import copy
import csv
import io
import json
from typing import Any, Callable

import stage8b_p1e_r4_design_check as checker


BASE = checker.read_all()


def json_bytes(value: Any) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def replace_once(value: bytes, old: str, new: str) -> bytes:
    text = value.decode()
    if text.count(old) != 1:
        raise SystemExit(f"mutation source count for {old!r}: {text.count(old)}")
    return text.replace(old, new, 1).encode()


def mutate_json(blobs: dict[str, bytes], name: str, change: Callable[[dict[str, Any]], None]) -> None:
    value = json.loads(blobs[name])
    change(value)
    blobs[name] = json_bytes(value)


def redigest(blobs: dict[str, bytes]) -> None:
    evidence = json.loads(blobs["evidence"])
    for name in list(evidence["contract_sha256"]):
        evidence["contract_sha256"][name] = checker.sha256(blobs[name])
    blobs["evidence"] = json_bytes(evidence)


cases: list[tuple[str, dict[str, bytes], str | None]] = []


def add(name: str, target: str, change: Callable[[dict[str, bytes]], None], source: str | None = None) -> None:
    blobs = dict(BASE)
    change(blobs)
    redigest(blobs)
    cases.append((name, blobs, source))


# Design/scope mutations.
add("design-opens-source", "design", lambda b: b.__setitem__("design", replace_once(b["design"], "Status: design-only review candidate", "Status: source implementation authorized")))
add("design-removes-active-count", "design", lambda b: b.__setitem__("design", replace_once(b["design"], "182 active REQUIRED rows", "181 active REQUIRED rows")))
add("design-removes-source-first", "design", lambda b: b.__setitem__("design", replace_once(b["design"], "SOURCE_FIRST_TIMER_DEFERRED", "TIMER_FIRST_SOURCE_DEFERRED")))

# Active merge mutations.
add("active-reenables-r3-user-row", "active", lambda b: mutate_json(b, "active", lambda v: v["sources"][2]["superseded_rows"].remove("P1ER3-010")))
add("active-drops-supersession-map", "active", lambda b: mutate_json(b, "active", lambda v: v["supersession_map"].pop()))
add("active-total-forged", "active", lambda b: mutate_json(b, "active", lambda v: v["active_contract_expectation"].update(total_active_rows=183)))

def optional_r4(blobs: dict[str, bytes]) -> None:
    rows = checker.csv_rows(blobs["r4"])
    rows[-1]["status"] = "OPTIONAL"
    stream = io.StringIO(newline="")
    writer = csv.DictWriter(stream, fieldnames=list(rows[0]), lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    blobs["r4"] = stream.getvalue().encode()
    active = json.loads(blobs["active"])
    active["sources"][3]["file_sha256"] = checker.sha256(blobs["r4"])
    blobs["active"] = json_bytes(active)

add("r4-row-optional", "r4", optional_r4, "r4")

# Real semantic-key registry mutations.
add("semantic-user-conflict", "semantic", lambda b: mutate_json(b, "semantic", lambda v: v["authorities"].append({"key":"systemd.User","value":"other-user","source":"mutation","active":True})))
add("semantic-binary-drift", "semantic", lambda b: mutate_json(b, "semantic", lambda v: v["expected_active_values"].update({"systemd.BinaryPath":"/usr/local/bin/supervisor"})))
add("semantic-credential-drift", "semantic", lambda b: mutate_json(b, "semantic", lambda v: v["expected_active_values"].update({"systemd.CredentialPath":"/tmp/key"})))
add("semantic-key-omitted", "semantic", lambda b: mutate_json(b, "semantic", lambda v: v["required_keys"].remove("firstboot.MarkerUpdateProtocol")))
add("semantic-old-acquisition-active", "semantic", lambda b: mutate_json(b, "semantic", lambda v: next(x for x in v["authorities"] if x["key"] == "redis.NonReadyAcquisitionOwner" and not x["active"]).update(active=True)))
add("semantic-unlisted-fallback", "semantic", lambda b: mutate_json(b, "semantic", lambda v: next(x for x in v["authorities"] if x["key"] == "classifier.UnlistedTupleDisposition" and x["active"]).update(value="fallthrough-Ready")))

# Deployment identity and custody mutations.
add("identity-user-drift", "identity", lambda b: mutate_json(b, "identity", lambda v: v["service_identity"].update(user="moex-finam-paper")))
add("identity-group-drift", "identity", lambda b: mutate_json(b, "identity", lambda v: v["service_identity"].update(group="moex-finam-paper")))
add("identity-binary-drift", "identity", lambda b: mutate_json(b, "identity", lambda v: v["paths"].update(binary="/usr/local/bin/stage8b-p1-paper-supervisor")))
add("identity-config-drift", "identity", lambda b: mutate_json(b, "identity", lambda v: v["paths"].update(config="/etc/moex/stage8b-p1-paper-supervisor.json")))
add("identity-credential-drift", "identity", lambda b: mutate_json(b, "identity", lambda v: v["paths"].update(credential_source="/etc/moex/credentials/stage8b-p1-lifecycle.key")))
add("identity-parent-owner-drift", "identity", lambda b: mutate_json(b, "identity", lambda v: next(x for x in v["ownership_table"] if x["object"] == "durable_parent").update(owner="moex-p1-paper")))
add("identity-marker-root-owned", "identity", lambda b: mutate_json(b, "identity", lambda v: next(x for x in v["ownership_table"] if x["object"] == "transaction_marker").update(owner="root")))
add("identity-world-writable", "identity", lambda b: mutate_json(b, "identity", lambda v: next(x for x in v["ownership_table"] if x["object"] == "accepted_receipt").update(mode="0666")))
add("identity-recovery-binary-differs", "identity", lambda b: mutate_json(b, "identity", lambda v: v["units"]["bootstrap_recover"].update(exec_start="/usr/local/bin/stage8b-p1-paper-supervisor bootstrap-recover /etc/moex-finam-p1-paper/supervisor.json %i RECOVER_EXISTING_STAGE8B_P1_FIRST_BOOT_V3")))

# Marker phase-update/recovery mutations.
add("marker-temp-class-omitted", "transaction", lambda b: mutate_json(b, "transaction", lambda v: v["marker_update_temp_classifications"].pop()))
add("marker-phase-pair-incompatible", "transaction", lambda b: mutate_json(b, "transaction", lambda v: v["marker_update_temp_classifications"][0].update(expected_temp_phase="journal_durable")))
add("marker-repeated-effect-allowed", "transaction", lambda b: mutate_json(b, "transaction", lambda v: v["marker_update_temp_classifications"][1].update(forbidden_repeated_effect="")))
add("marker-cross-transaction-conflict-omitted", "transaction", lambda b: mutate_json(b, "transaction", lambda v: v["marker_temp_conflicts"].remove("different-transaction-id")))
add("marker-sigkill-frontier-omitted", "transaction", lambda b: mutate_json(b, "transaction", lambda v: v["required_sigkill_hooks"].pop()))
add("marker-completion-reexecutes-effect", "transaction", lambda b: mutate_json(b, "transaction", lambda v: v["marker_temp_completion_protocol"].__setitem__(5, "reexecute-corresponding-phase-effect")))
add("recovery-direct-manual-allowed", "transaction", lambda b: mutate_json(b, "transaction", lambda v: v["administrative_boundary"].update(direct_manual_execution="accepted")))
add("recovery-open-action-grammar", "transaction", lambda b: mutate_json(b, "transaction", lambda v: v["administrative_boundary"].update(action_grammar=v["administrative_boundary"]["action_grammar"] + "|delete-root")))

# Receipt custody/binding/replay mutations.
add("receipt-omits-provenance", "receipt", lambda b: mutate_json(b, "receipt", lambda v: v["fields_in_storage_order"].remove("first_boot_provenance_canonical_sha256")))
add("receipt-root-owned", "receipt", lambda b: mutate_json(b, "receipt", lambda v: v["persistence"].update(owner="root")))
add("receipt-rename-only-commit", "receipt", lambda b: mutate_json(b, "receipt", lambda v: v["persistence"].update(commit_point="final-rename-only")))
add("receipt-cross-provenance-replay", "receipt", lambda b: mutate_json(b, "receipt", lambda v: v["replay_rejection"].remove("cross-provenance")))

# Byte-exact digest mutations.
add("digest-transaction-field-order", "digests", lambda b: mutate_json(b, "digests", lambda v: v["transaction_id_sha256"]["fields_in_order"].reverse()))
add("digest-domain-drift", "digests", lambda b: mutate_json(b, "digests", lambda v: v["canonical_root_identity_sha256"].update(domain_ascii="moex.stage8b.p1e.root.v1")))
add("digest-output-uppercase", "digests", lambda b: mutate_json(b, "digests", lambda v: v["record_encoding"].update(digest_output_encoding="64-uppercase-hex")))
add("golden-expected-digest-forged", "golden", lambda b: mutate_json(b, "golden", lambda v: v["samples"]["transaction_id_sha256"].update(expected_sha256="00" * 32)))
add("receipt-hmac-preimage-omits-provenance", "digests", lambda b: mutate_json(b, "digests", lambda v: v["first_boot_receipt_hmac_preimage"]["fields_in_order"].pop(7)))

# Redis acquisition/preview/latch mutations.
add("s06-nonready-autoclaim", "acquisition", lambda b: mutate_json(b, "acquisition", lambda v: v["s06_non_ready"]["forbidden_redis_operations"].remove("XAUTOCLAIM")))
add("nonready-second-reclaim", "acquisition", lambda b: mutate_json(b, "acquisition", lambda v: v["non_ready_continuation_rule"].update(maximum_successful_reclaims=2)))
add("linear-owner-drops-payload", "acquisition", lambda b: mutate_json(b, "acquisition", lambda v: v["linear_delivery_v2"]["fields"].pop(3)))
add("processing-reacquires", "acquisition", lambda b: mutate_json(b, "acquisition", lambda v: v["ready_non_acquiring_consumers"]["process_claimed_ready_source"].update(allowed_acquisition_operations=["XREADGROUP"])))
add("preview-callback-reintroduced", "acquisition", lambda b: mutate_json(b, "acquisition", lambda v: v["pre_transition_forbidden"].remove("strategy-callback-preview")))

# Source-first/deferred-timer and closed fallback mutations.
add("timer-before-source", "precedence", lambda b: mutate_json(b, "precedence", lambda v: v.update(rule="TIMER_FIRST_SOURCE_DEFERRED")))
add("unlisted-falls-through-ready", "precedence", lambda b: mutate_json(b, "precedence", lambda v: v["unlisted_tuple"].update(ready_fallback_allowed=True)))
add("stale-filled-timer-executes", "precedence", lambda b: mutate_json(b, "precedence", lambda v: v["post_source_timer_reclassification"]["discard_as_stale_if"].remove("order-filled")))
add("nonready-owner-timer-gap", "precedence", lambda b: mutate_json(b, "precedence", lambda v: v["source_bearing_owner_derivation"]["non_ready"].pop()))

if len(cases) != 48:
    raise SystemExit(f"R4 mutation inventory drifted: {len(cases)}")

escaped: list[str] = []
original_sources = copy.deepcopy(checker.SOURCE_EXPECTATIONS)
for name, blobs, changed_source in cases:
    try:
        if changed_source:
            count, _, excluded = checker.SOURCE_EXPECTATIONS[changed_source]
            checker.SOURCE_EXPECTATIONS[changed_source] = (count, checker.sha256(blobs[changed_source]), excluded)
        checker.validate({checker.FILES[key]: value for key, value in blobs.items()})
    except (checker.CheckFailure, ValueError, KeyError, TypeError, json.JSONDecodeError):
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")
    finally:
        checker.SOURCE_EXPECTATIONS = copy.deepcopy(original_sources)

if escaped:
    raise SystemExit("R4 mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1e-r4-design-negative-harness 48/48 redigested=true")
