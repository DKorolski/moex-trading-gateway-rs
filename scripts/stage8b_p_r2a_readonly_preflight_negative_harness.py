#!/usr/bin/env python3
"""Reject the 40 Stage 8B-P R2A contract mutations."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
AUTHORITY = "docs/stage-8/stage8b-p-r2a-readonly-preflight-authority.json"
CHECKER = "scripts/stage8b_p_r2a_readonly_preflight_check.py"


def change(root: Path, keys: tuple[str, ...], value: Any) -> None:
    path = root / AUTHORITY
    document = json.loads(path.read_text())
    target = document
    for key in keys[:-1]:
        target = target[key]
    target[keys[-1]] = value
    path.write_text(json.dumps(document, indent=2) + "\n")


def edit(root: Path, keys: tuple[str, ...], action: Callable[[list[Any]], None]) -> None:
    path = root / AUTHORITY
    document = json.loads(path.read_text())
    target = document
    for key in keys:
        target = target[key]
    action(target)
    path.write_text(json.dumps(document, indent=2) + "\n")


def scalar(keys: tuple[str, ...], value: Any) -> Callable[[Path], None]:
    return lambda root: change(root, keys, value)


def vector(keys: tuple[str, ...], action: Callable[[list[Any]], None]) -> Callable[[Path], None]:
    return lambda root: edit(root, keys, action)


def cases() -> list[tuple[str, Callable[[Path], None]]]:
    return [
        ("merge-ref", scalar(("lineage", "accepted_main_merge_ref"), "0" * 40)),
        ("r1b-ref", scalar(("lineage", "accepted_r1b_ref"), "0" * 40)),
        ("r1b-authority", scalar(("lineage", "r1b_authority_sha256"), "0" * 64)),
        ("r1b-network", scalar(("lineage", "r1b_network_authority_sha256"), "0" * 64)),
        ("r1b-run", scalar(("lineage", "r1b_run_authority_sha256"), "0" * 64)),
        ("build", scalar(("qualified_executable", "build_identity_sha256"), "0" * 64)),
        ("executable", scalar(("qualified_executable", "executable_sha256"), "0" * 64)),
        ("rebuild", scalar(("qualified_executable", "rebuild_allowed"), True)),
        ("alternate-executable", scalar(("qualified_executable", "alternate_executable_allowed"), True)),
        ("selection-present", scalar(("r2a_execution", "operator_selection_present"), True)),
        ("operation-expanded", vector(("operator_selection", "operation_enum"), lambda x: x.append("REPLACE"))),
        ("instrument", scalar(("operator_selection", "instrument"), "SBER@MISX")),
        ("market-place", scalar(("operator_selection", "place_order_type"), "MARKET")),
        ("quantity", scalar(("operator_selection", "place_quantity"), "2")),
        ("cancel-order", scalar(("operator_selection", "cancel_exact_broker_order_id_required"), False)),
        ("cancel-lifecycle", scalar(("operator_selection", "cancel_same_lifecycle_required"), False)),
        ("token-selection", scalar(("operator_selection", "token_forbidden_in_selection"), False)),
        ("raw-account", scalar(("operator_selection", "raw_account_forbidden_in_evidence"), False)),
        ("post-method", vector(("readonly_transport", "method_allowlist"), lambda x: x.append("POST"))),
        ("source-order", vector(("readonly_transport", "source_order"), lambda x: x.reverse())),
        ("route", scalar(("readonly_transport", "route_templates"), ["/unsafe"] * 4)),
        ("request-count", scalar(("readonly_transport", "max_requests"), 5)),
        ("timeout", scalar(("readonly_transport", "request_timeout_ms"), 30000)),
        ("interval", scalar(("readonly_transport", "min_request_interval_ms"), 0)),
        ("preflight-age", scalar(("readonly_transport", "preflight_max_age_ms"), 120000)),
        ("retry", scalar(("readonly_transport", "retry_disabled"), False)),
        ("redirect", scalar(("readonly_transport", "redirect_disabled"), False)),
        ("proxy", scalar(("readonly_transport", "proxy_disabled"), False)),
        ("background", scalar(("readonly_transport", "background_loop_disabled"), False)),
        ("raw-response", scalar(("readonly_transport", "raw_response_exported"), True)),
        ("current-input", vector(("required_current_inputs",), lambda x: x.pop())),
        ("cached-truth", scalar(("evidence_semantics", "caller_built_or_cached_truth_forbidden"), False)),
        ("r2-equals-k2", scalar(("evidence_semantics", "not_equal_to"), "R2ReadOnlyPreflightEvidence")),
        ("satisfy-k1", scalar(("evidence_semantics", "cannot_satisfy_k1"), False)),
        ("satisfy-k2", scalar(("evidence_semantics", "cannot_satisfy_k2"), False)),
        ("issue-arm", scalar(("evidence_semantics", "cannot_issue_arm"), False)),
        ("record-attempt", scalar(("evidence_semantics", "cannot_record_dispatch_attempt"), False)),
        ("effect-transport", scalar(("evidence_semantics", "cannot_enter_effect_transport"), False)),
        ("r2b-unlocked", scalar(("r2a_execution", "r2b_actual_get_unlocked"), True)),
        ("authorization", scalar(("authorization", "status"), "ISSUED")),
    ]


def main() -> None:
    mutations = cases()
    if len(mutations) != 40:
        raise SystemExit(f"stage8b-p-r2a-negative: FAIL inventory={len(mutations)}")
    with tempfile.TemporaryDirectory(prefix="stage8b-p-r2a-negative-") as temp:
        root = Path(temp) / "root"
        shutil.copytree(ROOT, root, ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports"))
        original = (root / AUTHORITY).read_bytes()
        for index, (name, mutation) in enumerate(mutations, 1):
            (root / AUTHORITY).write_bytes(original)
            mutation(root)
            result = subprocess.run(
                ["python3", CHECKER, "--no-git"], cwd=root, text=True,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r2a-negative: FAIL mutation passed: {name}")
            print(f"PASS {index:02d}/40 {name}")
    print("stage8b-p-r2a-negative: PASS 40/40 broker_get=false authorization=NOT_ISSUED")


if __name__ == "__main__":
    main()
