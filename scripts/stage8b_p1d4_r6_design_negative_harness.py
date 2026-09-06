#!/usr/bin/env python3
"""Targeted mutation harness for the P1-d4 R6 design/source contract."""

from __future__ import annotations

import copy
import csv
import io
import json

import stage8b_p1d4_r6_design_check as checker


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


# Matrix/state-graph mutations.
fields, rows = csv_rows(baseline[0])
add("general-row-missing", 0, emit_csv(fields, rows[:-1]))
fields, rows = csv_rows(baseline[1])
add("base-row-missing", 1, emit_csv(fields, rows[:-1]))
fields, rows = csv_rows(baseline[2])
add("gm-row-missing", 2, emit_csv(fields, rows[:-1]))
add("gm-id-drift", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-001", "cell_id", "P1D4GM-999"))
add("gm-frontier-drift", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-004", "frontier_id", "GM99"))
add("gm-parent-drift", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-006", "parent_scenario_id", "S04"))
add("gm-dispatch-frontier-lost", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-004", "stage6_durable_frontier", "none"))
add("gm-dispatch-owner-lost", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-004", "expected_restart_disposition", "P1d4GeneratedMarketPrepublicationPending"))
add("gm-order-owner-lost", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-006", "expected_restart_disposition", "P1d4GeneratedMarketDispatchPending"))
add("gm-prefinal-owner-lost", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-007", "expected_restart_disposition", "P1d4GeneratedMarketPreAckPending"))
add("gm-generic-owner", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-011", "expected_restart_disposition", "Ready"))
add("gm-source-before-missing", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-005", "source_pel_before", "exact_source_absent"))
add("gm-source-after-missing", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-010", "source_pel_after", "exact_source_absent"))
add("gm-early-xack", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-011", "xack_delta", "+1_exact"))
add("gm-terminal-xack-missing", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-013", "xack_delta", "0"))
add("gm-duplicate-coverage", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-003", "duplicate_variant_required", "false"))
add("gm-conflict-coverage", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-012", "conflict_variant_required", "false"))
add("gm-prexadd-binding-present", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-001", "publication_binding_expectation", "mandatory_binding"))
add("gm-post-sack-binding-absent", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-011", "publication_binding_expectation", "absent"))
add("gm-pair-before-finalization", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-007", "sequence_expectation", "exact_pair_allocated"))
add("gm-preallocator-pair-present", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-008", "sequence_expectation", "exact_pair_allocated"))
add("gm-postallocator-frontier-lost", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-009", "sequence_expectation", "pair_not_allocated"))
add("gm-prekill-comparison-lost", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-010", "sequence_expectation", "exact_adjacent_pair"))
add("gm-second-dispatch", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-004", "dispatch_v1_total", "+1_second_dispatch"))
add("gm-second-order", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-006", "only_legal_continuation", "append_order_and_trade"))
add("gm-trade-append-lost", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-006", "trade_v1_total", "0"))
add("gm-finalization-append-lost", 2, mutate_cell(baseline[2], "cell_id", "P1D4GM-007", "request_finalized_v1_total", "0"))

# Acceptance/design/evidence mutations.
fields, rows = csv_rows(baseline[3])
add("amendment-row-missing", 3, emit_csv(fields, rows[:-1]))
add("amendment-optional", 3, mutate_cell(baseline[3], "id", "P1D4R6-015", "status", "OPTIONAL"))
add("design-v3-authorized", 4, replace_once(baseline[4], "There is no generated-Market Stage6 V3 record", "There is one generated-Market Stage6 V3 record"))
add("design-second-source", 4, replace_once(baseline[4], "one retained M10", "two independent M10 sources"))
add("design-binding-type-lost", 4, baseline[4].replace("Stage8bP1d4CommandPublicationBindingV1", "UnboundPublication"))
add("design-entry-id-lost", 4, baseline[4].replace("command_entry_id", "command_entry_reference"))
add("design-duplicate-entry-equivalent", 4, replace_once(baseline[4], "a byte-identical duplicate entry is not equivalent", "a byte-identical duplicate entry is equivalent"))
add("discovery-classifier-shape", 5, replace_once(baseline[5], "suffix lengths 3 or 4", "suffix lengths 1 through 4"))
add("discovery-wip-included", 5, replace_once(baseline[5], "saved uncommitted source work remains excluded", "saved uncommitted source work is included"))

shape = json.loads(baseline[6])
mutated = copy.deepcopy(shape)
mutated["facts"]["existing_p1d2_classifier_suffix_lengths"] = [1, 2, 3, 4]
add("shape-classifier-suffix", 6, json.dumps(mutated))
mutated = copy.deepcopy(shape)
mutated["facts"]["generated_market_v3_authorized"] = True
add("shape-v3-opened", 6, json.dumps(mutated))
mutated = copy.deepcopy(shape)
mutated["facts"]["generated_market_journal_version"] = "V3"
add("shape-journal-version", 6, json.dumps(mutated))

evidence = baseline[7]
mutated = copy.deepcopy(evidence)
mutated["active_positive_cells"] = 102
add("evidence-old-cell-target", 7, mutated)
mutated = copy.deepcopy(evidence)
mutated["composition_contract"]["owners"] = mutated["composition_contract"]["owners"][:4]
add("evidence-owner-inventory", 7, mutated)
mutated = copy.deepcopy(evidence)
mutated["composition_contract"]["command_publication_binding"] = "none"
add("evidence-publication-binding", 7, mutated)
mutated = copy.deepcopy(evidence)
mutated["closed_surfaces"]["operational_redis_db0"] = True
add("evidence-opens-db0", 7, mutated)

# Each accepted source-shape component is independently pinned.
for suffix, path, old, new in (
    ("stage6", "crates/strategy-runtime-core/src/stage6d_live_core.rs", "if !matches!(suffix_len, 3 | 4)", "if !matches!(suffix_len, 1 | 2 | 3 | 4)"),
    ("feedback", "crates/strategy-runtime-core/src/stage8b_p1d2_market_feedback.rs", ".stage8b_p1d2_allocate_sequence_pair()", ".allocate_unbound_pair()"),
    ("ack", "crates/strategy-runtime-core/src/stage5g_mock_ack.rs", "P1-d2's sole sequence-pair allocator", "one of several sequence allocators"),
    ("provider", "crates/strategy-runtime-core/src/stage8b_p1d1_paper_provider.rs", "reconstruct_stage8b_p1d1_market_outcome_evidence", "reconstruct_unbound_market_evidence"),
    ("redis", "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs", "pub command_entry_id: String", "pub command_entry_reference: String"),
):
    sources = copy.deepcopy(baseline[10])
    sources[path] = replace_once(sources[path], old, new)
    add(f"source-shape-{suffix}", 10, sources)

if len(cases) != 48:
    raise SystemExit(f"R6 mutation inventory drifted: {len(cases)}")

escaped: list[str] = []
for name, inputs in cases:
    try:
        checker.validate(*inputs, verify_hashes=False)
    except (checker.CheckFailure, json.JSONDecodeError):
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")

if escaped:
    raise SystemExit("mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1d4-r6-design-negative-harness 48/48 inherited_r3=128 aggregate_contract=176 source_shape=5")
