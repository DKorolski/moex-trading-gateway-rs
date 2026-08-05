#!/usr/bin/env python3
"""Mutation matrix proving the Stage 5G-e-d-a R3 checker fails closed."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage5g_eda_r3_check.py"
SOURCE = "crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs"
LIB = "crates/strategy-runtime-core/src/lib.rs"
CONTRACT = "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.json"
FILES = [
    SOURCE,
    LIB,
    CONTRACT,
    "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.md",
    "docs/stage-5/stage5g-e-d-a-r3-current-head-invariants.json",
    "docs/stage-5/stage5g-lifecycle-entry-inventory.json",
    "docs/current-status.md",
    "docs/reviewer-onboarding-and-roadmap.md",
    "scripts/stage5g_eda_r3_gate.sh",
    "scripts/stage5g_eda_r3_negative_harness.py",
    "scripts/make_stage5g_ed_handoff_archive.py",
    "scripts/stage5g_eda_r3_preseal_check.py",
]


def replace_once(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError(
            f"mutation anchor must occur once in {relative}: {old!r}; got {text.count(old)}"
        )
    path.write_text(text.replace(old, new, 1))


def insert_before_tests(root: Path, value: str) -> None:
    replace_once(root, SOURCE, "#[cfg(test)]\nmod tests {", f"{value}\n#[cfg(test)]\nmod tests {{")


def append(root: Path, relative: str, value: str) -> None:
    path = root / relative
    path.write_text(path.read_text() + value)


def run_case(name: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix=f"stage5g-eda-r3-{name}-") as directory:
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
            raise SystemExit(f"stage5g-eda-r3-negative: FAIL: mutation survived: {name}")
        print(f"PASS {name}")


def main() -> None:
    cases = [
        ("drop-json-disposition", lambda root: replace_once(
            root, CONTRACT, ',\n    "TerminalInconsistency"', "")),
        ("rename-rust-disposition", lambda root: replace_once(
            root, SOURCE, "    TerminalInconsistency,", "    TerminalConflict,")),
        ("drop-json-operational-field", lambda root: replace_once(
            root, CONTRACT, '    "command_consumer_generation",\n', "")),
        ("drop-rust-operational-field", lambda root: replace_once(
            root, SOURCE, "    command_consumer_generation: Stage5gFeedGeneration,\n", "")),
        ("remove-order-source-chronology", lambda root: replace_once(
            root, SOURCE, "source_ts > order.received_ts", "false")),
        ("remove-trade-source-chronology", lambda root: replace_once(
            root, SOURCE, "trade.source_ts > trade.received_ts", "false")),
        ("remove-position-source-chronology", lambda root: replace_once(
            root, SOURCE, "source_ts > position.received_ts", "false")),
        ("remove-orders-section-observation", lambda root: replace_once(
            root, SOURCE, "validate_section_observation(\n        package.orders_observed_at,",
            "skip_section_observation(\n        package.orders_observed_at,")),
        ("remove-trades-section-observation", lambda root: replace_once(
            root, SOURCE, "validate_section_observation(\n        package.trades_observed_at,",
            "skip_section_observation(\n        package.trades_observed_at,")),
        ("remove-positions-section-observation", lambda root: replace_once(
            root, SOURCE, "validate_section_observation(\n        package.positions_observed_at,",
            "skip_section_observation(\n        package.positions_observed_at,")),
        ("remove-section-post-restore-bound", lambda root: replace_once(
            root, SOURCE, "observed_at <= clean_restore_completed_at", "false")),
        ("remove-section-captured-at-bound", lambda root: replace_once(
            root, SOURCE, "observed_at > captured_at", "false")),
        ("remove-order-row-restore-bound", lambda root: replace_once(
            root, SOURCE, "order.received_ts < clean_restore_completed_at", "false")),
        ("remove-trade-row-restore-bound", lambda root: replace_once(
            root, SOURCE, "trade.received_ts < clean_restore_completed_at", "false")),
        ("remove-position-row-restore-bound", lambda root: replace_once(
            root, SOURCE, "position.received_ts < clean_restore_completed_at", "false")),
        ("remove-order-row-section-bound", lambda root: replace_once(
            root, SOURCE, "order.received_ts > package.orders_observed_at", "false")),
        ("remove-trade-row-section-bound", lambda root: replace_once(
            root, SOURCE, "trade.received_ts > package.trades_observed_at", "false")),
        ("remove-position-row-section-bound", lambda root: replace_once(
            root, SOURCE, "position.received_ts > package.positions_observed_at", "false")),
        ("restore-trim-only-identity-grammar", lambda root: replace_once(
            root, SOURCE,
            "!value.is_empty()\n        && value == value.trim()\n        && !value\n            .chars()\n            .any(|character| character.is_whitespace() || character.is_control())",
            "!value.trim().is_empty() && value == value.trim()")),
        ("restore-unchecked-validated-deserialize", lambda root: replace_once(
            root, SOURCE,
            "#[derive(Debug, Clone, PartialEq, Eq, Serialize)]\n#[serde(deny_unknown_fields)]\npub(crate) struct Stage5gOperationalIdentityV1",
            "#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\n#[serde(deny_unknown_fields)]\npub(crate) struct Stage5gOperationalIdentityV1")),
        ("restore-strict-position-key-dedup", lambda root: replace_once(
            root, SOURCE,
            "instrument_identity_matches(&previous.instrument, &position.instrument)",
            "instrument_key(&previous.instrument) == instrument_key(&position.instrument)")),
        ("remove-filled-complete-fill-rule", lambda root: replace_once(
            root, SOURCE,
            "matches!(order.status, OrderStatus::Filled) && order.filled_qty != order.qty",
            "false")),
        ("collapse-replay-authorities", lambda root: replace_once(
            root, SOURCE, "pub(crate) pre_restart_package_id: &'a str,",
            "pub(crate) last_reconciled_package_id: &'a str,")),
        ("allow-same-id-changed-fingerprint", lambda root: replace_once(
            root, SOURCE,
            "    } else {\n        Err(Stage5gFreshBrokerTruthError::FreshPackageIdentityConflict)\n    }\n}\n\nfn validate_replay_ledger(",
            "    } else {\n        Ok(exact)\n    }\n}\n\nfn validate_replay_ledger(")),
        ("public-module-leak", lambda root: replace_once(
            root, LIB, "mod stage5g_fresh_broker_truth;", "pub mod stage5g_fresh_broker_truth;")),
        ("open-reducer", lambda root: replace_once(
            root, CONTRACT, '"reconciliation_reducer": false', '"reconciliation_reducer": true')),
        ("runtime-callback-surface", lambda root: insert_before_tests(
            root, "fn forged_callback() { on_bar(); }")),
        ("redis-surface", lambda root: insert_before_tests(root, "use redis::Commands;")),
        ("remove-schema-version-guard", lambda root: replace_once(
            root, SOURCE,
            "package.schema_version != STAGE5G_FRESH_BROKER_TRUTH_SCHEMA_VERSION", "false")),
        ("remove-package-id-constructor", lambda root: replace_once(
            root, SOURCE, "Stage5gPackageId::parse(package.package_id.clone())",
            "Stage5gPackageId::parse(\"forged-package\")")),
        ("remove-snapshot-epoch-constructor", lambda root: replace_once(
            root, SOURCE, "Stage5gSnapshotEpoch::parse(package.snapshot_epoch.clone())",
            "Stage5gSnapshotEpoch::parse(\"forged-epoch\")")),
        ("remove-package-id-replay-guard", lambda root: replace_once(
            root, SOURCE, "package_id.as_str() == context.pre_restart_package_id", "false")),
        ("remove-snapshot-epoch-replay-guard", lambda root: replace_once(
            root, SOURCE, "snapshot_epoch.as_str() == context.pre_restart_snapshot_epoch", "false")),
        ("remove-package-post-restore-guard", lambda root: replace_once(
            root, SOURCE, "package.captured_at <= context.clean_restore_completed_at", "false")),
        ("remove-package-validation-upper-bound", lambda root: replace_once(
            root, SOURCE, "package.captured_at > context.validation_observed_at", "false")),
        ("remove-operational-identity-equality", lambda root: replace_once(
            root, SOURCE, "&operational_identity != context.expected_operational_identity", "false")),
        ("remove-order-account-binding", lambda root: replace_once(
            root, SOURCE, "&order.account_id != account", "false")),
        ("remove-trade-account-binding", lambda root: replace_once(
            root, SOURCE, "&trade.account_id != account", "false")),
        ("remove-position-account-binding", lambda root: replace_once(
            root, SOURCE, "&position.account_id != account", "false")),
        ("remove-order-lifecycle-agreement", lambda root: replace_once(
            root, SOURCE, "order.lifecycle != BrokerOrderSnapshot::lifecycle_for(&order.status)",
            "false")),
        ("remove-exact-remaining-rule", lambda root: replace_once(
            root, SOURCE, "remaining != order.qty - order.filled_qty", "false")),
        ("remove-active-zero-remaining-rule", lambda root: replace_once(
            root, SOURCE, "order.is_inconsistent_active_zero_remaining()", "false")),
        ("remove-market-order-shape-rule", lambda root: replace_once(
            root, SOURCE, "OrderType::Market if order.limit_price.is_some()",
            "OrderType::Market if false")),
        ("remove-limit-order-shape-rule", lambda root: replace_once(
            root, SOURCE, "Some(price) => price <= Decimal::ZERO", "Some(_) => false")),
        ("remove-canonical-order-id-rule", lambda root: replace_once(
            root, SOURCE,
            "if order\n            .broker_order_id\n            .as_ref()\n            .is_some_and(|id| !canonical_native_id(id.as_str()))",
            "if false && order\n            .broker_order_id\n            .as_ref()\n            .is_some_and(|id| !canonical_native_id(id.as_str()))")),
        ("remove-canonical-trade-id-rule", lambda root: replace_once(
            root, SOURCE, "!canonical_native_id(trade.broker_trade_id.as_str())", "false")),
        ("remove-positive-trade-values-rule", lambda root: replace_once(
            root, SOURCE, "trade.qty <= Decimal::ZERO || trade.price <= Decimal::ZERO", "false")),
        ("remove-unique-broker-order-rule", lambda root: replace_once(
            root, SOURCE, "!broker_order_ids.insert(id.as_str().to_owned())", "false")),
        ("remove-unique-client-order-rule", lambda root: replace_once(
            root, SOURCE, "!client_order_ids.insert(id.as_str().to_owned())", "false")),
        ("remove-unique-trade-rule", lambda root: replace_once(
            root, SOURCE, "!trade_ids.insert(trade.broker_trade_id.as_str().to_owned())", "false")),
        ("remove-replay-ledger-uniqueness-rule", lambda root: replace_once(
            root, SOURCE,
            "!ids.insert(entry.package_id.as_str()) || !epochs.insert(entry.snapshot_epoch.as_str())",
            "false")),
        ("append-real-source-reducer", lambda root: insert_before_tests(
            root,
            "pub(crate) fn forged_reducer(\n"
            "    _package: Stage5gValidatedFreshBrokerTruthPackage,\n"
            ") -> Stage5gRestartReconciliationDisposition {\n"
            "    Stage5gRestartReconciliationDisposition::ExactReplay\n"
            "}")),
        ("swap-grst-all-first-two", lambda root: replace_once(
            root, SOURCE,
            "        Self::Grst01RestartBeforeAck,\n        Self::Grst02RestartAfterAckBeforeOrder,",
            "        Self::Grst02RestartAfterAckBeforeOrder,\n        Self::Grst01RestartBeforeAck,")),
        ("remove-grst-all-entry", lambda root: replace_once(
            root, SOURCE, "        Self::Grst06RestartAfterTerminalPositionApplied,\n", "")),
        ("duplicate-grst-all-entry", lambda root: replace_once(
            root, SOURCE, "        Self::Grst06RestartAfterTerminalPositionApplied,\n",
            "        Self::Grst06RestartAfterTerminalPositionApplied,\n"
            "        Self::Grst06RestartAfterTerminalPositionApplied,\n")),
        ("change-frozen-grst-mapping", lambda root: replace_once(
            root, SOURCE,
            'Self::Grst07RestartAtTimerCheckpoint => "GRST07_RESTART_AT_TIMER_CHECKPOINT"',
            'Self::Grst07RestartAtTimerCheckpoint => "GRST07_RESTART_AT_TIMER_CHECKPOINT_DRIFT"')),
    ]
    for name, mutation in cases:
        run_case(name, mutation)
    if len(cases) < 42:
        raise SystemExit("stage5g-eda-r3-negative: FAIL: fewer than 42 mutations")
    print(f"stage5g-eda-r3-negative: PASS ({len(cases)}/{len(cases)})")


if __name__ == "__main__":
    main()
