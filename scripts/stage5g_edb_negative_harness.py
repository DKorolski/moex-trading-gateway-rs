#!/usr/bin/env python3
"""Mutation matrix for the Stage 5G-e-d-b reducer boundary."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path

import stage5g_edb_check as checker


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage5g_edb_check.py"
REDUCER = str(checker.REDUCER_SOURCE)
PARENT = str(checker.PARENT_SOURCE)
CONTRACT = str(checker.CONTRACT)
FILES = sorted({path for _, path in checker.EXPECTED_DELTA})


def replace_once(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError(
            f"mutation anchor must occur once in {relative}: {old!r}; got {text.count(old)}"
        )
    path.write_text(text.replace(old, new, 1))


def append_before_tests(root: Path, value: str) -> None:
    replace_once(root, REDUCER, "#[cfg(test)]", value + "\n#[cfg(test)]")


def cases() -> list[tuple[str, object]]:
    values: list[tuple[str, object]] = []
    for index in range(1, 13):
        name = f"remove-grst{index:02d}-positive-witness"
        values.append((name, lambda root, index=index: replace_once(
            root, REDUCER, f"fn stage5g_edb_grst{index:02d}()",
            f"fn removed_stage5g_edb_grst{index:02d}()")))
    for reason in checker.REASONS:
        values.append((f"remove-reason-{reason}", lambda root, reason=reason: replace_once(
            root, REDUCER, f"    {reason},\n", "")))
    for scenario in checker.SCENARIOS:
        values.append((f"drift-scenario-{scenario}", lambda root, scenario=scenario: replace_once(
            root, CONTRACT, f'    "{scenario}"', f'    "MUTATED_{scenario}"')))
    for disposition in checker.DISPOSITIONS:
        values.append((f"drift-disposition-{disposition}", lambda root, disposition=disposition: replace_once(
            root, CONTRACT, f'    "{disposition}"', f'    "Mutated{disposition}"')))
    for binding in checker.CROSS_BINDINGS:
        values.append((f"remove-cross-binding-{binding}", lambda root, binding=binding: replace_once(
            root, CONTRACT, f'    "{binding}",\n' if binding != checker.CROSS_BINDINGS[-1]
            else f'    "{binding}"\n', "")))
    for surface in checker.CLOSED_SURFACES:
        values.append((f"open-surface-{surface}", lambda root, surface=surface: replace_once(
            root, CONTRACT, f'    "{surface}": false', f'    "{surface}": true')))
    values.extend([
        ("remove-child-module-registration", lambda root: replace_once(
            root, PARENT, "mod reducer;", "// reducer removed")),
        ("rename-owning-entry", lambda root: replace_once(
            root, REDUCER, "pub(crate) fn reduce_stage5g_fresh_broker_truth(",
            "pub(crate) fn bypass_reduce_stage5g_fresh_broker_truth(")),
        ("make-reduction-clone", lambda root: replace_once(
            root, REDUCER, "pub(crate) struct Stage5gFreshTruthReduction {",
            "#[derive(Clone)]\npub(crate) struct Stage5gFreshTruthReduction {")),
        ("remove-candidate-validation", lambda root: replace_once(
            root, REDUCER, "if !candidate_is_self_consistent(&candidate) {", "if false {")),
        ("remove-account-cross-binding", lambda root: replace_once(
            root, REDUCER, "restart.account_id == truth.operational_identity.account_id",
            "true")),
        ("collapse-incomplete-to-absence", lambda root: replace_once(
            root, REDUCER, "if !truth.orders_complete {", "if false {")),
        ("open-wall-clock", lambda root: append_before_tests(
            root, "fn forbidden_clock() { let _ = chrono::Utc::now(); }")),
        ("open-redis", lambda root: append_before_tests(
            root, "fn forbidden_redis() { let _ = redis::Client::open(\"redis://x\"); }")),
        ("open-finam", lambda root: append_before_tests(
            root, "fn forbidden_finam() { let _ = finam_transport(); }")),
        ("open-http-post", lambda root: append_before_tests(
            root, "fn forbidden_post(client: reqwest::Client) { let _ = client.post(\"/orders\"); }")),
        ("open-callback", lambda root: append_before_tests(
            root, "fn forbidden_callback() { let _ = strategy::on_bar; }")),
        ("open-dispatch", lambda root: append_before_tests(
            root, "fn forbidden_dispatch() { dispatch_order(); }")),
    ])
    return values


def run_case(name: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix=f"stage5g-edb-{name}-") as directory:
        root = Path(directory)
        for relative in FILES:
            source = ROOT / relative
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
        mutation(root)
        result = subprocess.run(
            ["python3", str(CHECKER), "--root", str(root), "--skip-git"],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
        if result.returncode == 0:
            raise SystemExit(f"stage5g-edb-negative: FAIL: mutation survived: {name}")
        print(f"PASS {name}")


def main() -> None:
    matrix = cases()
    if len(matrix) < 72 or len({name for name, _ in matrix}) != len(matrix):
        raise SystemExit("stage5g-edb-negative: FAIL: matrix count/names invalid")
    for name, mutation in matrix:
        run_case(name, mutation)
    print(f"stage5g-edb-negative: PASS ({len(matrix)}/{len(matrix)})")


if __name__ == "__main__":
    main()
