#!/usr/bin/env python3
"""Focused redigested negative mutations for the P1-e R2 design."""

from __future__ import annotations

import copy
import csv
import io

import stage8b_p1e_r2_design_check as checker


def csv_text(rows: list[dict[str, str]], fields: list[str]) -> str:
    stream = io.StringIO(newline="")
    writer = csv.DictWriter(stream, fieldnames=fields, lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    return stream.getvalue()


design = checker.DESIGN.read_text(encoding="utf-8")
acceptance = checker.ACCEPTANCE.read_text(encoding="utf-8")
outer = checker.OUTER.read_text(encoding="utf-8")
operational = checker.OPERATIONAL.read_text(encoding="utf-8")
transaction = checker.load_json(checker.TRANSACTION)
redis_policy = checker.load_json(checker.REDIS_POLICY)
evidence = checker.load_json(checker.EVIDENCE)
status = checker.STATUS.read_text(encoding="utf-8")
roadmap = checker.ROADMAP.read_text(encoding="utf-8")
recovery_source = checker.RECOVERY_SOURCE.read_text(encoding="utf-8")


def base() -> list[object]:
    return [
        design, acceptance, outer, operational, copy.deepcopy(transaction),
        copy.deepcopy(redis_policy), copy.deepcopy(evidence), status, roadmap,
        recovery_source,
    ]


def replace_once(text: str, old: str, new: str) -> str:
    if text.count(old) != 1:
        raise SystemExit(f"mutation source count for {old!r}: {text.count(old)}")
    return text.replace(old, new, 1)


def redigest(args: list[object]) -> None:
    outer_value = args[2]
    operational_value = args[3]
    transaction_value = args[4]
    redis_value = args[5]
    evidence_value = args[6]
    assert isinstance(outer_value, str)
    assert isinstance(operational_value, str)
    assert isinstance(transaction_value, dict)
    assert isinstance(redis_value, dict)
    assert isinstance(evidence_value, dict)
    hashes = {
        "first_boot_transaction_v1_sha256": checker.canonical_json_sha256(transaction_value),
        "redis_runtime_policy_v1_sha256": checker.canonical_json_sha256(redis_value),
        "restart_outer_matrix_v2_file_sha256": checker.text_sha256(outer_value),
        "operational_continuation_matrix_v2_file_sha256": checker.text_sha256(operational_value),
    }
    checker.CONTRACT_HASHES = hashes
    evidence_value["canonical_contracts"] = hashes


cases: list[tuple[str, list[object]]] = []

for name, old, new in [
    ("design-authorize-source", "Status: R2 design-only review candidate", "Status: source implementation authorized"),
    ("design-unverified-delete", "authenticated incomplete-bootstrap ceremony", "operator rm-rf ceremony"),
    ("design-adopt-without-final-restart", "CommittedRootResponseLost", "CommittedRootAutoAccepted"),
    ("design-discard-ready-pel", "54-row operational continuation matrix v2", "Ready has no continuation"),
    ("design-generalize-cancel", "must commit/reread exact `S_cancel_recovered`", "may commit generic `S_truth`"),
    ("design-second-fresh-read", "same delivery is processed in the same ownership invocation", "delivery may wait for next poll"),
    ("design-open-implementation", "does not authorize that implementation", "authorizes implementation"),
]:
    args = base()
    args[0] = replace_once(design, old, new)
    cases.append((name, args))

acceptance_rows = checker.csv_rows(acceptance)
acceptance_fields = list(acceptance_rows[0])
for name, mutate in [
    ("acceptance-delete", lambda rows: rows.pop()),
    ("acceptance-optional", lambda rows: rows[47].update(status="OPTIONAL")),
    ("acceptance-bootstrap-after-root", lambda rows: rows[7].update(requirement="marker may follow root creation")),
    ("acceptance-ack-stops-before-truth", lambda rows: rows[32].update(requirement="AckCommitted may XACK immediately")),
]:
    rows = copy.deepcopy(acceptance_rows)
    mutate(rows)
    args = base()
    args[1] = csv_text(rows, acceptance_fields)
    cases.append((name, args))

outer_rows = checker.csv_rows(outer)
outer_fields = list(outer_rows[0])
for name, mutate in [
    ("outer-delete-variant", lambda rows: rows.pop()),
    ("outer-ready-none", lambda rows: rows[0].update(first_legal_transition="none")),
    ("outer-p1d2-ack-xack", lambda rows: rows[6].update(s06r_completion_boundary="s_ack_then_xack")),
    ("outer-generated-ack-xack", lambda rows: rows[13].update(s06r_completion_boundary="generated_s_ack_then_xack")),
    ("outer-cancel-generic-truth", lambda rows: rows[19].update(s06r_completion_boundary="s_truth_then_xack")),
]:
    rows = copy.deepcopy(outer_rows)
    mutate(rows)
    args = base()
    args[2] = csv_text(rows, outer_fields)
    redigest(args)
    cases.append((name, args))

operational_rows = checker.csv_rows(operational)
operational_fields = list(operational_rows[0])
for name, mutate in [
    ("operational-delete", lambda rows: rows.pop()),
    ("operational-ready-premature", lambda rows: rows[0].update(paper_ready_legality="immediate")),
    ("operational-unclaimable-fresh", lambda rows: rows[4].update(fresh_poll_legality="allowed")),
    ("operational-ambiguous-dispatch", lambda rows: rows[5].update(first_legal_transition="claim_first_entry")),
    ("operational-expiry-reissue-after-commit", lambda rows: rows[7].update(equivalent_authority_reissue="required_day_authority")),
    ("operational-timer-before-source", lambda rows: rows[8].update(first_legal_transition="expire_before_pending_source")),
    ("operational-s08-second-read", lambda rows: rows[9].update(fresh_poll_legality="second_read_allowed")),
    ("operational-cancel-generic", lambda rows: rows[27].update(s06r_completion_boundary="s_truth_then_xack")),
    ("operational-already-ack-xack", lambda rows: rows[31].update(xack_legality="repeat_xack")),
]:
    rows = copy.deepcopy(operational_rows)
    mutate(rows)
    args = base()
    args[3] = csv_text(rows, operational_fields)
    redigest(args)
    cases.append((name, args))

for name, mutate in [
    ("transaction-bootstrap-over-marker", lambda value: value.update(bootstrap_entry_rule="bootstrap-over-existing-marker")),
    ("transaction-remove-binding", lambda value: value["marker_authentication"]["required_bindings"].pop()),
    ("transaction-nonatomic-marker", lambda value: value.update(marker_update_protocol=["write"])),
    ("transaction-auto-delete", lambda value: value["quarantine_protocol"].update(automatic_delete_allowed=True)),
    ("transaction-quarantine-committed", lambda value: value["quarantine_protocol"]["preconditions"].remove("no-committed-seal")),
    ("transaction-adopt-without-restart", lambda value: value["adoption_protocol"]["preconditions"].remove("fresh-restart-from-final-canonical-path-returns-exact-TimerReady-owner")),
    ("transaction-provenance-sidecar-only", lambda value: value["first_boot_provenance"].update(storage="unauthenticated-sidecar")),
    ("transaction-drop-sigkill-window", lambda value: value["required_sigkill_hooks"].pop()),
]:
    args = base()
    value = args[4]
    assert isinstance(value, dict)
    mutate(value)
    redigest(args)
    cases.append((name, args))

for name, mutate in [
    ("policy-threshold-too-long", lambda value: value.update(claim_idle_ms=59000)),
    ("policy-pel-two", lambda value: value["startup_claim"].update(maximum_accepted_pel_count=2)),
    ("policy-nonterminal-fallthrough", lambda value: value["startup_claim"].update(nonterminal_cursor_after_max_pages="continue-fresh-read")),
    ("policy-env-override", lambda value: value.update(ordinary_environment_override_allowed=True)),
    ("policy-second-fresh", lambda value: value["fresh_read"].update(second_fresh_read_before_first_delivery_resolution=True)),
    ("policy-delete-pending-consumer", lambda value: value["stale_consumer_hygiene"].update(nonzero_pending_delete_allowed=True)),
]:
    args = base()
    value = args[5]
    assert isinstance(value, dict)
    mutate(value)
    redigest(args)
    cases.append((name, args))

args = base()
value = args[6]
assert isinstance(value, dict)
value["implementation_authorized"] = True
cases.append(("evidence-open-source", args))

if len(cases) != 40:
    raise SystemExit(f"R2 mutation inventory drifted: {len(cases)}")

original_hashes = copy.deepcopy(checker.CONTRACT_HASHES)
escaped: list[str] = []
for name, args in cases:
    try:
        checker.validate(*args)  # type: ignore[arg-type]
    except checker.CheckFailure:
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")
    finally:
        checker.CONTRACT_HASHES = copy.deepcopy(original_hashes)

if escaped:
    raise SystemExit("R2 mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1e-r2-design-negative-harness 40/40 redigested=true")
