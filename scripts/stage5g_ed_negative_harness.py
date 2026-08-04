#!/usr/bin/env python3
"""Mutation harness proving that the Stage 5G-e-d-a checker fails closed."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage5g_ed_check.py"
FILES = [
    "crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.json",
    "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.md",
    "docs/stage-5/stage5g-lifecycle-entry-inventory.json",
    "scripts/stage5g_ed_gate.sh",
]


def replace_once(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError(f"mutation anchor must occur once in {relative}: {old!r}")
    path.write_text(text.replace(old, new, 1))


def append(root: Path, relative: str, value: str) -> None:
    path = root / relative
    path.write_text(path.read_text() + value)


def run_case(name: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix=f"stage5g-ed-{name}-") as directory:
        root = Path(directory)
        for relative in FILES:
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        mutation(root)
        result = subprocess.run(
            ["python3", str(CHECKER), "--root", str(root)],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
        if result.returncode == 0:
            raise SystemExit(f"stage5g-ed-negative: FAIL: mutation survived: {name}")
        print(f"PASS {name}")


def main() -> None:
    cases = [
        (
            "renamed-frozen-grst-id",
            lambda root: replace_once(
                root,
                FILES[2],
                "GRST01_RESTART_BEFORE_ACK",
                "GRST01_RENAMED",
            ),
        ),
        (
            "incomplete-section-treated-as-absence",
            lambda root: replace_once(
                root,
                FILES[2],
                '"incomplete_section_means_absent_rows": false',
                '"incomplete_section_means_absent_rows": true',
            ),
        ),
        (
            "drop-package-freshness-check",
            lambda root: replace_once(
                root,
                FILES[0],
                "package.captured_at <= context.clean_restore_completed_at",
                "false",
            ),
        ),
        (
            "drop-operational-identity-check",
            lambda root: replace_once(
                root,
                FILES[0],
                "&package.operational_identity != context.expected_operational_identity",
                "false",
            ),
        ),
        (
            "public-module-leak",
            lambda root: replace_once(
                root,
                FILES[1],
                "mod stage5g_fresh_broker_truth;",
                "pub mod stage5g_fresh_broker_truth;",
            ),
        ),
        (
            "public-function-leak",
            lambda root: append(root, FILES[0], "\npub fn forged_stage5g_reconcile() {}\n"),
        ),
        (
            "redis-surface",
            lambda root: append(root, FILES[0], "\nuse redis::Commands;\n"),
        ),
        (
            "runtime-authority",
            lambda root: append(
                root,
                FILES[0],
                "\nuse crate::HybridIntradayRuntimeStrategy;\n",
            ),
        ),
        (
            "implemented-case-claim",
            lambda root: replace_once(
                root,
                FILES[2],
                '"implemented_restart_case_ids": []',
                '"implemented_restart_case_ids": ["GRST01_RESTART_BEFORE_ACK"]',
            ),
        ),
        (
            "drop-terminal-disposition",
            lambda root: replace_once(
                root,
                FILES[2],
                ',\n    "TerminalInconsistency"',
                "",
            ),
        ),
        (
            "move-predecessor-gate-to-current-tree",
            lambda root: replace_once(
                root,
                FILES[5],
                'git worktree add --detach "$snapshot_root" "$accepted_ec_ref"',
                'cp -R . "$snapshot_root"',
            ),
        ),
    ]
    for name, mutation in cases:
        run_case(name, mutation)
    print(f"stage5g-ed-negative: PASS ({len(cases)}/{len(cases)})")


if __name__ == "__main__":
    main()
