#!/usr/bin/env python3
"""Focused redigested negative mutations for the P1-e R3 design."""

from __future__ import annotations

import copy
import csv
import io

import stage8b_p1e_r3_design_check as checker


def csv_text(rows: list[dict[str, str]], fields: list[str]) -> str:
    stream = io.StringIO(newline="")
    writer = csv.DictWriter(stream, fieldnames=fields, lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    return stream.getvalue()


design = checker.DESIGN.read_text(encoding="utf-8")
r1_acceptance = checker.R1_ACCEPTANCE.read_text(encoding="utf-8")
r2_acceptance = checker.R2_ACCEPTANCE.read_text(encoding="utf-8")
r3_acceptance = checker.R3_ACCEPTANCE.read_text(encoding="utf-8")
active = checker.load_json(checker.ACTIVE)
outer = checker.OUTER.read_text(encoding="utf-8")
operational = checker.OPERATIONAL.read_text(encoding="utf-8")
transaction = checker.load_json(checker.TRANSACTION)
receipt = checker.load_json(checker.RECEIPT)
package = checker.load_json(checker.PACKAGE)
acquisition = checker.load_json(checker.ACQUISITION)
redis_policy = checker.load_json(checker.REDIS_POLICY)
evidence = checker.load_json(checker.EVIDENCE)
status = checker.STATUS.read_text(encoding="utf-8")
roadmap = checker.ROADMAP.read_text(encoding="utf-8")
recovery_source = checker.RECOVERY_SOURCE.read_text(encoding="utf-8")


def base() -> list[object]:
    return [
        design, r1_acceptance, r2_acceptance, r3_acceptance,
        copy.deepcopy(active), outer, operational, copy.deepcopy(transaction),
        copy.deepcopy(receipt), copy.deepcopy(package), copy.deepcopy(acquisition),
        copy.deepcopy(redis_policy), copy.deepcopy(evidence), status, roadmap,
        recovery_source,
    ]


def replace_once(text: str, old: str, new: str) -> str:
    if text.count(old) != 1:
        raise SystemExit(f"mutation source count for {old!r}: {text.count(old)}")
    return text.replace(old, new, 1)


def redigest(args: list[object]) -> None:
    r3_text = args[3]
    active_value = args[4]
    operational_value = args[6]
    transaction_value = args[7]
    receipt_value = args[8]
    package_value = args[9]
    acquisition_value = args[10]
    evidence_value = args[12]
    assert isinstance(r3_text, str)
    assert isinstance(active_value, dict)
    assert isinstance(operational_value, str)
    assert isinstance(transaction_value, dict)
    assert isinstance(receipt_value, dict)
    assert isinstance(package_value, dict)
    assert isinstance(acquisition_value, dict)
    assert isinstance(evidence_value, dict)
    active_value["sources"][2]["file_sha256"] = checker.text_sha256(r3_text)
    hashes = {
        "active_acceptance_contract_v3_sha256": checker.canonical_json_sha256(active_value),
        "r3_acceptance_matrix_file_sha256": checker.text_sha256(r3_text),
        "first_boot_transaction_v2_sha256": checker.canonical_json_sha256(transaction_value),
        "first_boot_receipt_v1_sha256": checker.canonical_json_sha256(receipt_value),
        "authenticated_restart_package_v2_sha256": checker.canonical_json_sha256(package_value),
        "source_acquisition_seam_v1_sha256": checker.canonical_json_sha256(acquisition_value),
        "operational_pretransition_matrix_v3_file_sha256": checker.text_sha256(operational_value),
    }
    checker.CONTRACT_HASHES = hashes
    evidence_value["canonical_contracts"] = hashes


cases: list[tuple[str, list[object]]] = []

for name, old, new in [
    ("design-authorize-source", "Status: R3 design-only review candidate", "Status: source implementation authorized"),
    ("design-direct-recovery", "Direct invocation", "Direct invocation is allowed;"),
    ("design-preview-callback", "No preview callback exists", "A preview callback exists"),
    ("design-second-acquire", "Neither facade may call `XPENDING`, `XAUTOCLAIM` or `XREADGROUP`", "Facades may reacquire the delivery"),
]:
    args = base()
    args[0] = replace_once(design, old, new)
    cases.append((name, args))

r3_rows = checker.csv_rows(r3_acceptance)
r3_fields = list(r3_rows[0])
for name, mutate in [
    ("acceptance-delete", lambda rows: rows.pop()),
    ("acceptance-optional", lambda rows: rows[42].update(status="OPTIONAL")),
]:
    rows = copy.deepcopy(r3_rows)
    mutate(rows)
    args = base()
    args[3] = csv_text(rows, r3_fields)
    redigest(args)
    cases.append((name, args))

for name, source_name, row_id in [
    ("active-conflicting-ready-row", "r1", "P1ER1-043"),
    ("active-conflicting-start-limit-row", "r1", "P1ER1-074"),
]:
    args = base()
    value = args[4]
    assert isinstance(value, dict)
    source = next(item for item in value["sources"] if item["name"] == source_name)
    source["superseded_rows"].remove(row_id)
    for item in value["supersession_map"]:
        if row_id in item["superseded"]:
            item["superseded"].remove(row_id)
            break
    value["active_contract_expectation"][f"{source_name}_active_rows"] += 1
    value["active_contract_expectation"]["total_active_rows"] += 1
    redigest(args)
    cases.append((name, args))

args = base()
value = args[4]
assert isinstance(value, dict)
value["supersession_map"].pop()
redigest(args)
cases.append(("active-unmapped-supersession", args))

for name, mutate in [
    ("transaction-direct-recovery", lambda value: value["administrative_boundary"].update(direct_manual_execution="accepted")),
    ("transaction-missing-credential-mutates", lambda value: value["administrative_boundary"].update(missing_credentials_directory="continue")),
    ("transaction-open-action", lambda value: value["administrative_boundary"].update(action_grammar=value["administrative_boundary"]["action_grammar"] + "|delete-root")),
    ("transaction-selector-auto-reinterpret", lambda value: value.update(recovery_entry_rule="reinterpret-stale-selector")),
    ("transaction-delete-classification", lambda value: value["classifications"].pop()),
    ("transaction-receipt-temp-auto-adopt", lambda value: value["classifications"][5].update(required_action_selector="adopt-committed-root")),
]:
    args = base()
    value = args[7]
    assert isinstance(value, dict)
    mutate(value)
    redigest(args)
    cases.append((name, args))

for name, mutate in [
    ("receipt-caller-key", lambda value: value.update(hmac_key="argv-key")),
    ("receipt-drop-transaction", lambda value: value["fields_in_canonical_order"].remove("transaction_id_sha256")),
    ("receipt-weak-commit", lambda value: value["persistence"].update(commit_point="rename-only")),
    ("receipt-cross-attempt", lambda value: value["replay_rejection"].remove("cross-attempt-generation")),
    ("receipt-existing-conflict-accepted", lambda value: value["persistence"].update(existing_nonexact_receipt="accepted")),
]:
    args = base()
    value = args[8]
    assert isinstance(value, dict)
    mutate(value)
    redigest(args)
    cases.append((name, args))

for name, mutate in [
    ("package-drop-provenance", lambda value: value["package_fields_in_order"].remove("first_boot_provenance_v1")),
    ("package-hmac-omits-provenance", lambda value: value["restart_commitment_v2"]["ordered_inputs"].remove("first_boot_provenance_canonical_sha256")),
    ("package-v1-downgrade", lambda value: value["decode_policy"].update(p1e_run_accepts_schema_versions=[1, 2], v1_downgrade_allowed=True)),
    ("package-regenerate-provenance", lambda value: value["construction"].update(provenance_regeneration_allowed=True)),
    ("package-seal-drops-provenance", lambda value: value["construction"].update(seal_advance="rebuild-without-provenance")),
]:
    args = base()
    value = args[9]
    assert isinstance(value, dict)
    mutate(value)
    redigest(args)
    cases.append((name, args))

for name, mutate in [
    ("acquisition-clone", lambda value: value.update(clone_allowed=True)),
    ("acquisition-preview-result", lambda value: value["forbidden_pre_transition_observations"].remove("strategy_callback_result")),
    ("acquisition-working-limit-reclaim", lambda value: value["consumers"]["process_claimed_working_limit"].update(redis_acquisition_calls_allowed=["XAUTOCLAIM"])),
    ("acquisition-ready-source-reread", lambda value: value["consumers"]["process_claimed_ready_source"].update(redis_acquisition_calls_allowed=["XREADGROUP"])),
    ("acquisition-shutdown-parse", lambda value: value["consumers"]["retain_after_shutdown_latch"].update(parse_allowed=True)),
    ("acquisition-duplicate-callback", lambda value: value.update(callback_rule="preview-then-real-callback")),
]:
    args = base()
    value = args[10]
    assert isinstance(value, dict)
    mutate(value)
    redigest(args)
    cases.append((name, args))

operational_rows = checker.csv_rows(operational)
operational_fields = list(operational_rows[0])
for name, mutate in [
    ("operational-result-discriminator", lambda rows: rows[1].update(redis_pel_state="one_exact_claimable_later_untouched_zero_intent")),
    ("operational-s08-reacquire", lambda rows: rows[7].update(first_legal_transition="call_process_next_and_XREADGROUP_again")),
    ("operational-cancel-generic", lambda rows: next(row for row in rows if row["id"] == "OC28").update(s06r_completion_boundary="s_truth_then_xack")),
    ("operational-delete", lambda rows: rows.pop()),
]:
    rows = copy.deepcopy(operational_rows)
    mutate(rows)
    args = base()
    args[6] = csv_text(rows, operational_fields)
    redigest(args)
    cases.append((name, args))

args = base()
value = args[12]
assert isinstance(value, dict)
value["implementation_authorized"] = True
cases.append(("evidence-open-source", args))

if len(cases) != 36:
    raise SystemExit(f"R3 mutation inventory drifted: {len(cases)}")

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
    raise SystemExit("R3 mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1e-r3-design-negative-harness 36/36 redigested=true")
