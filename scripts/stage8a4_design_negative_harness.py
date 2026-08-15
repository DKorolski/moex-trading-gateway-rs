#!/usr/bin/env python3
"""Exact inherited-48 plus R2-20 mutation harness for Stage 8A-4 design."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage8a4_design_check as scanner


def mutate_json(path: Path, mutation) -> None:
    value = json.loads(path.read_text())
    mutation(value)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def set_path(*path_and_value):
    *path, value = path_and_value

    def apply(document):
        target = document
        for key in path[:-1]:
            target = target[key]
        target[path[-1]] = value

    return apply


def remove_source(name: str):
    return lambda value: value["required_truth_sources"].remove(name)


def remove_outcome(name: str):
    return lambda value: value["outcomes"].remove(name)


def swap_correlation(first: int, second: int):
    def apply(value):
        order = value["correlation_precedence"]
        order[first], order[second] = order[second], order[first]

    return apply


STRUCTURAL_CASES = [
    ("accepted predecessor drift", set_path("accepted_predecessor", "forged")),
    ("accepted review drift", set_path("accepted_predecessor_review_sha256", "0" * 64)),
    ("design-only disabled", set_path("design_only", False)),
    ("implementation predeclared", set_path("production_reconciliation_implemented", True)),
    ("orders source removed", remove_source("orders")),
    ("trades source removed", remove_source("trades")),
    ("positions source removed", remove_source("positions")),
    ("instrument registry removed", remove_source("instrument_registry")),
    ("client identity demoted", swap_correlation(0, 1)),
    ("shape correlation promoted", swap_correlation(0, 2)),
    ("Conflict removed", remove_outcome("Conflict")),
    ("StillUnknown removed", remove_outcome("StillUnknown")),
    ("trades made authoritative", set_path("supporting_evidence_only", ["target_instrument_position"])),
    ("position made authoritative", set_path("supporting_evidence_only", ["trades"])),
    ("ProvenNoMatch opened", set_path("closed", "proven_no_match", False)),
    ("same-request retry opened", set_path("closed", "same_request_retry", False)),
    ("network transport opened", set_path("closed", "network_transport", False)),
    ("runtime-live opened", set_path("closed", "runtime_live", False)),
]

MARKER_CASES = list(scanner.FORBIDDEN_CONTRACT_MARKERS)

R2_STRUCTURAL_CASES = [
    (
        "saturated trade interval marked complete",
        set_path("source_completeness", "trades", "returned_count_greater_or_equal_limit_is_complete", True),
    ),
    (
        "sealed event-window start coverage disabled",
        set_path("source_completeness", "trades", "event_window_start_coverage_required", False),
    ),
    (
        "sealed event-window end coverage disabled",
        set_path("source_completeness", "trades", "event_window_end_coverage_required", False),
    ),
    (
        "caller-selected trade envelope enabled",
        set_path("source_completeness", "trades", "caller_selected_limit_or_window_allowed", True),
    ),
    (
        "unbounded trade subdivision enabled",
        set_path("source_completeness", "trades", "subdivision_bounded", False),
    ),
    (
        "exact GET-order missing proves no match",
        set_path("source_completeness", "known_broker_order_id_lookup", "not_found_or_unavailable_proves_no_match", True),
    ),
    (
        "exact GET-order replaces account-wide safety",
        set_path("source_completeness", "known_broker_order_id_lookup", "replaces_account_wide_orders_safety_snapshot", True),
    ),
    ("duplicate trade counted twice", set_path("trade_identity", "equal_duplicate_counted_once", False)),
    ("conflicting duplicate silently deduped", set_path("trade_identity", "conflicting_duplicate_outcome", "Dedup")),
    ("cancel partial fill lost", set_path("exact_outcome", "terminal_cancel_partial_fill_preserved", False)),
    ("expired partial fill lost", set_path("exact_outcome", "terminal_expired_partial_fill_preserved", False)),
    ("rejected nonzero fill accepted", set_path("quantity_invariants", "rejected_nonzero_fill_is_conflict", False)),
    ("filled status permits partial quantity", set_path("quantity_invariants", "filled_status_requires_full_quantity", False)),
    ("remaining quantity mismatch accepted", set_path("quantity_invariants", "remaining_equals_qty_minus_filled", False)),
    ("tier3 ignores order type", set_path("tier3_shape", "binds_order_type", False)),
    ("market limit cross-match enabled", set_path("tier3_shape", "market_limit_cross_match_allowed", True)),
    ("tier3 ignores exact limit price", set_path("tier3_shape", "binds_exact_normalized_limit_price", False)),
    ("tier3 ignores DAY TIF", set_path("tier3_shape", "binds_time_in_force_day", False)),
    ("caller-selected price tolerance enabled", set_path("tier3_shape", "caller_selected_price_tolerance_allowed", True)),
    ("missing tier3 field treated as match", set_path("tier3_shape", "missing_required_shape_field_outcome", "Match")),
]


def main() -> int:
    if len(STRUCTURAL_CASES) != 18 or len(MARKER_CASES) != 30 or len(R2_STRUCTURAL_CASES) != 20:
        raise SystemExit("negative case inventory drift")
    copied = scanner.ALLOWED_CHANGED_PATHS
    for index, (name, mutation) in enumerate(STRUCTURAL_CASES, 1):
        with tempfile.TemporaryDirectory(prefix="stage8a4-design-negative-") as raw:
            root = Path(raw)
            for relative in copied:
                source = scanner.ROOT / relative
                if source.is_file():
                    target = root / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(source, target)
            mutate_json(root / scanner.AUTHORITY, mutation)
            try:
                scanner.check(root, git_scope=False)
            except Exception:
                print(f"PASS {index:02d} {name}")
            else:
                print(f"FAIL {index:02d} {name}: mutation accepted")
                return 1

    for offset, marker in enumerate(MARKER_CASES, len(STRUCTURAL_CASES) + 1):
        with tempfile.TemporaryDirectory(prefix="stage8a4-design-negative-") as raw:
            root = Path(raw)
            for relative in copied:
                source = scanner.ROOT / relative
                if source.is_file():
                    target = root / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(source, target)
            contract = root / scanner.CONTRACT
            contract.write_text(contract.read_text() + f"\n{marker}\n")
            try:
                scanner.check(root, git_scope=False)
            except Exception:
                print(f"PASS {offset:02d} {marker}")
            else:
                print(f"FAIL {offset:02d} {marker}: mutation accepted")
                return 1

    for offset, (name, mutation) in enumerate(R2_STRUCTURAL_CASES, 49):
        with tempfile.TemporaryDirectory(prefix="stage8a4-design-r2-negative-") as raw:
            root = Path(raw)
            for relative in copied:
                source = scanner.ROOT / relative
                if source.is_file():
                    target = root / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(source, target)
            mutate_json(root / scanner.AUTHORITY, mutation)
            try:
                scanner.check(root, git_scope=False)
            except Exception:
                print(f"PASS {offset:02d} {name}")
            else:
                print(f"FAIL {offset:02d} {name}: mutation accepted")
                return 1

    print("stage8a4-design-r2-negative: PASS inherited=48/48 new=20/20 total=68/68")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
