#!/usr/bin/env python3
"""Mutations that the Stage 8B-P1-d0 policy checker must reject."""

from __future__ import annotations

import copy
import csv
import importlib.util
import io
import json
import pathlib
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage8b_p1d0_check.py"
spec = importlib.util.spec_from_file_location("stage8b_p1d0_check", CHECKER)
assert spec and spec.loader
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)

policy = checker.POLICY.read_text(encoding="utf-8")
matrix = checker.MATRIX.read_text(encoding="utf-8")
evidence = json.loads(checker.EVIDENCE.read_text(encoding="utf-8"))
status = checker.STATUS.read_text(encoding="utf-8")
roadmap = checker.ROADMAP.read_text(encoding="utf-8")


def replace_once(source: str, old: str, new: str) -> str:
    if source.count(old) != 1:
        raise SystemExit(f"fixture drift: expected one occurrence of {old!r}")
    return source.replace(old, new, 1)


def mutate_evidence(path: tuple[str, ...], value: object) -> dict[str, object]:
    result = copy.deepcopy(evidence)
    target: dict[str, object] = result
    for key in path[:-1]:
        target = target[key]  # type: ignore[assignment]
    target[path[-1]] = value
    return result


matrix_rows = list(csv.DictReader(matrix.splitlines()))
matrix_buffer = io.StringIO()
writer = csv.DictWriter(matrix_buffer, fieldnames=matrix_rows[0].keys(), lineterminator="\n")
writer.writeheader()
writer.writerows(matrix_rows[:-1])

cases: list[tuple[str, str, str, dict[str, object], str, str]] = [
    ("same-bar-execution", replace_once(policy, "That command is never evaluated against `B[n]`.", "That command may be evaluated against `B[n]`."), matrix, evidence, status, roadmap),
    ("allow-unproven-gap", replace_once(policy, "An unproven missing eligible interval is `ExecutionBarGap`.", "An unproven missing eligible interval is skipped."), matrix, evidence, status, roadmap),
    ("ad-hoc-schedule-reparse", replace_once(policy, "Redis composition and the paper\nprovider must not parse calendars", "Redis composition and the paper\nprovider may parse calendars"), matrix, evidence, status, roadmap),
    ("allow-history-execution", replace_once(policy, "History and warmup bars never execute operational paper commands.", "History bars may execute operational paper commands."), matrix, evidence, status, roadmap),
    ("cancel-before-fill", replace_once(policy, "Fill-before-cancel is the frozen chronology for that case.", "Cancel-before-fill is the frozen chronology for that case."), matrix, evidence, status, roadmap),
    ("remove-predispatch-gate", replace_once(policy, "fail-closed execution-eligibility preflight before that transition", "execution check after that transition"), matrix, evidence, status, roadmap),
    ("open-dispatch-while-waiting", replace_once(policy, "DispatchAttemptRecorded   forbidden", "DispatchAttemptRecorded   allowed"), matrix, evidence, status, roadmap),
    ("open-provider-while-waiting", replace_once(policy, "paper provider call       forbidden", "paper provider call       allowed"), matrix, evidence, status, roadmap),
    ("blind-provider-retry", replace_once(policy, "It must not blindly call the\nprovider again.", "It may blindly call the\nprovider again."), matrix, evidence, status, roadmap),
    ("market-close-fill", replace_once(policy, "fill_price = execution_bar.open", "fill_price = execution_bar.close"), matrix, evidence, status, roadmap),
    ("buy-limit-worse-price", replace_once(policy, "fill_price = min(bar.open, limit)", "fill_price = max(bar.open, limit)"), matrix, evidence, status, roadmap),
    ("sell-limit-worse-price", replace_once(policy, "fill_price = max(bar.open, limit)", "fill_price = min(bar.open, limit)"), matrix, evidence, status, roadmap),
    ("enable-partial-fills", replace_once(policy, "Synthetic partial fills are disabled in policy v1.", "Synthetic partial fills are enabled in policy v1."), matrix, evidence, status, roadmap),
    ("configurable-slippage", replace_once(policy, "market slippage adjustment = 0 ticks", "market slippage adjustment = configured ticks"), matrix, evidence, status, roadmap),
    ("nonzero-commission", replace_once(policy, "commission                 = 0", "commission                 = configured"), matrix, evidence, status, roadmap),
    ("redis-derived-order-id", replace_once(policy, "Paper IDs are derived from immutable durable identity, never from Redis IDs,", "Paper IDs are derived from Redis IDs,"), matrix, evidence, status, roadmap),
    ("second-lifecycle-authority", replace_once(policy, "Stage 7B remains the sole durable command-lifecycle authority.", "Stage 7B and P0 are durable command-lifecycle authorities."), matrix, evidence, status, roadmap),
    ("remove-acceptance-row", policy, matrix_buffer.getvalue(), evidence, status, roadmap),
    ("open-implementation", policy, matrix, mutate_evidence(("implementation_authorized",), True), status, roadmap),
    ("open-finam", policy, matrix, mutate_evidence(("closed_surfaces", "finam_post_delete"), True), status, roadmap),
    ("consume-execution-bar-as-semantic", replace_once(policy, "must not use `XREADGROUP`, add the bar to a PEL, XACK it or invoke the Hybrid", "may use `XREADGROUP`, add the bar to a PEL, XACK it and invoke the Hybrid"), matrix, evidence, status, roadmap),
]

failures: list[str] = []
for name, case_policy, case_matrix, case_evidence, case_status, case_roadmap in cases:
    try:
        checker.validate(case_policy, case_matrix, case_evidence, case_status, case_roadmap)
    except checker.CheckFailure:
        print(f"PASS {name}")
    else:
        failures.append(name)
        print(f"FAIL {name}")

if failures:
    print("mutations escaped: " + ", ".join(failures), file=sys.stderr)
    raise SystemExit(1)
print(f"PASS stage8b-p1d0-negative-harness {len(cases)}/{len(cases)}")
