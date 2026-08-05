#!/usr/bin/env python3
"""Mutation matrix proving the Stage 5G-e-d-a R2 checker fails closed."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage5g_eda_r2_check.py"
FILES = [
    "crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.json",
    "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.md",
    "docs/stage-5/stage5g-lifecycle-entry-inventory.json",
    "docs/current-status.md",
    "docs/reviewer-onboarding-and-roadmap.md",
    "scripts/stage5g_eda_r2_gate.sh",
    "scripts/stage5g_eda_r2_negative_harness.py",
    "scripts/stage5g_ed_check.py",
    "scripts/stage5g_ed_negative_harness.py",
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
    with tempfile.TemporaryDirectory(prefix=f"stage5g-eda-r2-{name}-") as directory:
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
            raise SystemExit(f"stage5g-eda-r2-negative: FAIL: mutation survived: {name}")
        print(f"PASS {name}")


def main() -> None:
    source = FILES[0]
    contract = FILES[2]
    cases = [
        ("drop-json-disposition", lambda root: replace_once(
            root, contract, ',\n    "TerminalInconsistency"', "")),
        ("rename-rust-disposition", lambda root: replace_once(
            root, source, "    TerminalInconsistency,", "    TerminalConflict,")),
        ("drop-json-operational-field", lambda root: replace_once(
            root, contract, '    "command_consumer_generation",\n', "")),
        ("drop-rust-operational-field", lambda root: replace_once(
            root, source,
            "    command_consumer_generation: Stage5gFeedGeneration,\n",
            "")),
        ("remove-order-source-chronology", lambda root: replace_once(
            root, source, "source_ts > order.received_ts", "false")),
        ("remove-trade-source-chronology", lambda root: replace_once(
            root, source, "trade.source_ts > trade.received_ts", "false")),
        ("remove-position-source-chronology", lambda root: replace_once(
            root, source, "source_ts > position.received_ts", "false")),
        ("remove-orders-section-observation", lambda root: replace_once(
            root, source,
            "validate_section_observation(\n        package.orders_observed_at,",
            "skip_section_observation(\n        package.orders_observed_at,")),
        ("remove-trades-section-observation", lambda root: replace_once(
            root, source,
            "validate_section_observation(\n        package.trades_observed_at,",
            "skip_section_observation(\n        package.trades_observed_at,")),
        ("remove-positions-section-observation", lambda root: replace_once(
            root, source,
            "validate_section_observation(\n        package.positions_observed_at,",
            "skip_section_observation(\n        package.positions_observed_at,")),
        ("remove-section-post-restore-bound", lambda root: replace_once(
            root, source, "observed_at <= clean_restore_completed_at", "false")),
        ("remove-section-captured-at-bound", lambda root: replace_once(
            root, source, "observed_at > captured_at", "false")),
        ("remove-order-row-restore-bound", lambda root: replace_once(
            root, source, "order.received_ts < clean_restore_completed_at", "false")),
        ("remove-trade-row-restore-bound", lambda root: replace_once(
            root, source, "trade.received_ts < clean_restore_completed_at", "false")),
        ("remove-position-row-restore-bound", lambda root: replace_once(
            root, source, "position.received_ts < clean_restore_completed_at", "false")),
        ("remove-order-row-section-bound", lambda root: replace_once(
            root, source, "order.received_ts > package.orders_observed_at", "false")),
        ("remove-trade-row-section-bound", lambda root: replace_once(
            root, source, "trade.received_ts > package.trades_observed_at", "false")),
        ("remove-position-row-section-bound", lambda root: replace_once(
            root, source, "position.received_ts > package.positions_observed_at", "false")),
        ("restore-trim-only-identity-grammar", lambda root: replace_once(
            root, source,
            "!value.is_empty()\n        && value == value.trim()\n        && !value\n            .chars()\n            .any(|character| character.is_whitespace() || character.is_control())",
            "!value.trim().is_empty() && value == value.trim()")),
        ("restore-unchecked-validated-deserialize", lambda root: replace_once(
            root, source,
            "#[derive(Debug, Clone, PartialEq, Eq, Serialize)]\n#[serde(deny_unknown_fields)]\npub(crate) struct Stage5gOperationalIdentityV1",
            "#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\n#[serde(deny_unknown_fields)]\npub(crate) struct Stage5gOperationalIdentityV1")),
        ("restore-strict-position-key-dedup", lambda root: replace_once(
            root, source,
            "instrument_identity_matches(&previous.instrument, &position.instrument)",
            "instrument_key(&previous.instrument) == instrument_key(&position.instrument)")),
        ("remove-filled-complete-fill-rule", lambda root: replace_once(
            root, source,
            "matches!(order.status, OrderStatus::Filled) && order.filled_qty != order.qty",
            "false")),
        ("collapse-replay-authorities", lambda root: replace_once(
            root, source,
            "pub(crate) pre_restart_package_id: &'a str,",
            "pub(crate) last_reconciled_package_id: &'a str,")),
        ("allow-same-id-changed-fingerprint", lambda root: replace_once(
            root, source,
            "    } else {\n        Err(Stage5gFreshBrokerTruthError::FreshPackageIdentityConflict)\n    }\n}\n\nfn validate_replay_ledger(",
            "    } else {\n        Ok(exact)\n    }\n}\n\nfn validate_replay_ledger(")),
        ("public-module-leak", lambda root: replace_once(
            root, FILES[1], "mod stage5g_fresh_broker_truth;", "pub mod stage5g_fresh_broker_truth;")),
        ("open-reducer", lambda root: replace_once(
            root, contract, '"reconciliation_reducer": false', '"reconciliation_reducer": true')),
        ("runtime-callback-surface", lambda root: append(
            root, source, "\nfn forged_callback() { on_bar(); }\n")),
        ("redis-surface", lambda root: append(root, source, "\nuse redis::Commands;\n")),
    ]
    for name, mutation in cases:
        run_case(name, mutation)
    print(f"stage5g-eda-r2-negative: PASS ({len(cases)}/{len(cases)})")


if __name__ == "__main__":
    main()
