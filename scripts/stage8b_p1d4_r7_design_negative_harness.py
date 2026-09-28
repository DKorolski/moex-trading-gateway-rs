#!/usr/bin/env python3
"""Targeted mutation harness for the P1-d4 R7 design contract."""

from __future__ import annotations

import copy
import csv
import hashlib
import io
import json
import struct

import stage8b_p1d4_r7_design_check as checker


def csv_rows(value: str) -> tuple[list[str], list[dict[str, str]]]:
    reader = csv.DictReader(value.splitlines())
    return list(reader.fieldnames or []), list(reader)


def emit_csv(fields: list[str], rows: list[dict[str, str]]) -> str:
    stream = io.StringIO(newline="")
    writer = csv.DictWriter(stream, fieldnames=fields, lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    return stream.getvalue()


def mutate_cell(value: str, key: str, identity: str, field: str, replacement: str) -> str:
    fields, rows = csv_rows(value)
    matches = [row for row in rows if row[key] == identity]
    if len(matches) != 1:
        raise SystemExit(f"mutation target {key}={identity} is not unique")
    matches[0][field] = replacement
    return emit_csv(fields, rows)


def replace_once(value: str, old: str, new: str) -> str:
    if value.count(old) != 1:
        raise SystemExit(f"expected one occurrence of {old!r}, got {value.count(old)}")
    return value.replace(old, new)


baseline = list(checker.read_inputs())
cases: list[tuple[str, list[object]]] = []


def add(name: str, index: int, replacement: object) -> None:
    inputs = copy.deepcopy(baseline)
    inputs[index] = replacement
    cases.append((name, inputs))


# 28 matrix and graph mutations.
fields, rows = csv_rows(baseline[0])
add("general-row-missing", 0, emit_csv(fields, rows[:-1]))
fields, rows = csv_rows(baseline[1])
add("base-row-missing", 1, emit_csv(fields, rows[:-1]))
add("base-duplicate-coverage", 1, mutate_cell(baseline[1], "cell_id", "P1D4C-001", "duplicate_variant_required", "false"))
add("base-conflict-coverage", 1, mutate_cell(baseline[1], "cell_id", "P1D4C-092", "conflict_variant_required", "false"))
fields, rows = csv_rows(baseline[2])
add("gm-row-missing", 2, emit_csv(fields, rows[:-1]))
add("gm-id-drift", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-001", "cell_id", "P1D4GM-999"))
add("gm-frontier-drift", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-004", "frontier_id", "GM99"))
add("gm-parent-drift", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-006", "parent_scenario_id", "S04"))
add("gm-reservation-optional", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-008", "publication_reservation_expectation", "optional"))
add("gm-prexadd-binding-present", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-001", "publication_binding_expectation", "mandatory_hmac_covered_exact_binding"))
add("gm-pre-sack-binding-weak", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-003", "publication_binding_expectation", "marker_only"))
add("gm-post-sack-binding-absent", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-011", "publication_binding_expectation", "absent"))
add("gm-len1-route-fallback", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-004", "classifier_route", "ordinary_p1d2"))
add("gm-len2-route-fallback", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-006", "classifier_route", "ordinary_p1d2"))
add("gm-len3-route-fallback", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-007", "classifier_route", "ordinary_p1d2"))
add("gm-len4-route-fallback", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-009", "classifier_route", "ordinary_p1d2"))
add("gm-ack-route-reclassified", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-011", "classifier_route", "ordinary_p1d2"))
add("gm-truth-route-reclassified", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-013", "classifier_route", "ordinary_p1d2"))
add("gm-auto-id-continuation", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-001", "only_legal_continuation", "xadd_star"))
add("gm-source-before-missing", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-005", "source_pel_before", "exact_source_absent"))
add("gm-source-after-missing", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-010", "source_pel_after", "exact_source_absent"))
add("gm-early-xack", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-012", "xack_delta", "+1_exact"))
add("gm-terminal-xack-missing", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-013", "xack_delta", "0"))
add("gm-duplicate-coverage", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-003", "duplicate_variant_required", "false"))
add("gm-conflict-coverage", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-012", "conflict_variant_required", "false"))
fields, rows = csv_rows(baseline[3])
add("amendment-row-missing", 3, emit_csv(fields, rows[:-1]))
add("amendment-id-drift", 3, mutate_cell(baseline[3], "id", "P1D4R7-020", "id", "P1D4R7-999"))
add("amendment-optional", 3, mutate_cell(baseline[3], "id", "P1D4R7-040", "status", "OPTIONAL"))

# 14 design, discovery, source-shape and closed-surface mutations.
add("design-explicit-id-lost", 4, replace_once(baseline[4], "explicit `XADD command_stream reserved_command_entry_id`", "dynamic command publication"))
add("design-xadd-star-opened", 4, replace_once(baseline[4], "never `XADD *`", "may use `XADD *`"))
add("design-byte-identical-e2-lost", 4, baseline[4].replace("byte-identical E2", "replacement entry"))
add("design-invalid-fallback-opened", 4, replace_once(baseline[4], "hard Blocked/Corrupt; no classifier fallback", "ordinary P1-d2 fallback"))
add("design-precedence-lost", 4, replace_once(baseline[4], "P1-d4 V1 routing before ordinary P1-d2", "ordinary P1-d2 before P1-d4"))
add("design-generation-relation-lost", 4, replace_once(baseline[4], "W1 = W0 + 1", "W1 may skip W0"))
add("discovery-auto-id-fact-lost", 5, replace_once(baseline[5], "accepted Lua command publication uses `XADD *`", "accepted Lua uses explicit IDs"))
add("discovery-invalid-fallback-opened", 5, replace_once(baseline[5], "`PresentInvalid` blocks without fallback", "`PresentInvalid` falls back"))
shape = json.loads(baseline[6])
mutated = copy.deepcopy(shape)
mutated["facts"]["existing_command_publication_uses_auto_id"] = False
add("shape-auto-id-fact", 6, json.dumps(mutated))
mutated = copy.deepcopy(shape)
mutated["facts"]["existing_p1d2_classifier_suffix_lengths"] = [1, 2, 3, 4]
add("shape-classifier-suffix", 6, json.dumps(mutated))
mutated = copy.deepcopy(shape)
mutated["facts"]["existing_restart_route_order"] = ["P1d3V3", "GenericP1", "P1d2V1"]
add("shape-route-order", 6, json.dumps(mutated))
mutated = copy.deepcopy(shape)
mutated["facts"]["r7_source_change_requires_explicit_reserved_id"] = False
add("shape-explicit-id-correction", 6, json.dumps(mutated))
mutated = copy.deepcopy(shape)
mutated["facts"]["r7_source_change_requires_package_aware_routing"] = False
add("shape-package-aware-correction", 6, json.dumps(mutated))
mutated = copy.deepcopy(baseline[8])
mutated["closed_surfaces"]["operational_redis_db0"] = True
add("evidence-opens-db0", 8, mutated)

# 12 canonical fixture and authenticated identity mutations.
fixture = json.loads(baseline[7])
mutated = copy.deepcopy(fixture)
mutated["reservation"]["unexpected"] = "field"
add("fixture-reservation-extra-field", 7, json.dumps(mutated))
mutated = copy.deepcopy(fixture)
mutated["reservation"]["domain"] = "moex.stage8b.p1d4.command-publication-reservation.v2"
add("fixture-reservation-domain", 7, json.dumps(mutated))
mutated = copy.deepcopy(fixture)
mutated["reservation"]["command_stream_predecessor_id"] = "01785999999999-3"
add("fixture-predecessor-leading-zero", 7, json.dumps(mutated))
mutated = copy.deepcopy(fixture)
mutated["reservation"]["reserved_command_entry_id"] = "1785999999999-5"
add("fixture-non-successor", 7, json.dumps(mutated))
mutated = copy.deepcopy(fixture)
mutated["binding"]["command_entry_id"] = "1785999999999-5"
add("fixture-binding-entry-mismatch", 7, json.dumps(mutated))
mutated = copy.deepcopy(fixture)
mutated["reservation"]["strategy_request_id"] = "00112233-4455-6677-8899-AABBCCDDEEFF"
add("fixture-uppercase-uuid", 7, json.dumps(mutated))
mutated = copy.deepcopy(fixture)
mutated["binding"]["canonical_command_sha256"] = "A" * 64
add("fixture-uppercase-digest", 7, json.dumps(mutated))
mutated = copy.deepcopy(fixture)
mutated["reservation"]["prepublication_package_generation"] = -1
add("fixture-negative-generation", 7, json.dumps(mutated))
mutated = copy.deepcopy(fixture)
mutated["binding"]["prepublication_seal_generation"] = 43
add("fixture-generation-skip", 7, json.dumps(mutated))
mutated = copy.deepcopy(fixture)
mutated["reservation_canonical_hex"] = "00" + mutated["reservation_canonical_hex"][2:]
add("fixture-reservation-canonical-bytes", 7, json.dumps(mutated))
mutated = copy.deepcopy(fixture)
mutated["binding_canonical_hex"] = "00" + mutated["binding_canonical_hex"][2:]
add("fixture-binding-canonical-bytes", 7, json.dumps(mutated))
mutated = copy.deepcopy(fixture)
mutated["reservation"]["command_stream_predecessor_id"] = "1785999999999-4"
mutated["reservation"]["reserved_command_entry_id"] = "1785999999999-5"
mutated["binding"]["command_stream_predecessor_id"] = "1785999999999-4"
mutated["binding"]["command_entry_id"] = "1785999999999-5"
reservation_bytes = checker.reservation_bytes(mutated["reservation"])
mutated["reservation_canonical_hex"] = reservation_bytes.hex()
reservation_hash = hashlib.sha256(reservation_bytes).hexdigest()
mutated["reservation"]["publication_reservation_sha256"] = reservation_hash
mutated["binding"]["publication_reservation_sha256"] = reservation_hash
binding_bytes = checker.binding_bytes(mutated["binding"])
mutated["binding_canonical_hex"] = binding_bytes.hex()
mutated["binding"]["publication_binding_sha256"] = hashlib.sha256(binding_bytes).hexdigest()
add("fixture-self-consistent-e2-substitution", 7, json.dumps(mutated))

# Six independently pinned accepted source-shape components.
for suffix, path, old, new in (
    ("recovery", "crates/runtime-durable-service/src/recovery.rs", "let p1d2_candidate = classify_stage8b_p1d2_journal_ahead_candidate(", "let p1d2_candidate = classify_generic_before_p1d2("),
    ("redis", "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs", "redis.call('XADD', command_stream, '*', 'payload', envelope_payload)", "redis.call('XADD', command_stream, reserved_id, 'payload', envelope_payload)"),
    ("stage6", "crates/strategy-runtime-core/src/stage6d_live_core.rs", "if !matches!(suffix_len, 3 | 4)", "if !matches!(suffix_len, 1 | 2 | 3 | 4)"),
    ("feedback", "crates/strategy-runtime-core/src/stage8b_p1d2_market_feedback.rs", ".stage8b_p1d2_allocate_sequence_pair()", ".allocate_unbound_pair()"),
    ("ack", "crates/strategy-runtime-core/src/stage5g_mock_ack.rs", "P1-d2's sole sequence-pair allocator", "one of several sequence allocators"),
    ("provider", "crates/strategy-runtime-core/src/stage8b_p1d1_paper_provider.rs", "reconstruct_stage8b_p1d1_market_outcome_evidence", "reconstruct_unbound_market_evidence"),
):
    sources = copy.deepcopy(baseline[11])
    sources[path] = replace_once(sources[path], old, new)
    add(f"source-shape-{suffix}", 11, sources)

if len(cases) != 60:
    raise SystemExit(f"R7 mutation inventory drifted: {len(cases)}")

escaped: list[str] = []
for name, inputs in cases:
    try:
        checker.validate(*inputs, verify_hashes=False)
    except (checker.CheckFailure, json.JSONDecodeError, KeyError, TypeError, ValueError, struct.error):
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")

if escaped:
    raise SystemExit("mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1d4-r7-design-negative-harness 60/60 inherited_r3=128 aggregate_contract=188 reservation=exact routing=package_aware")
