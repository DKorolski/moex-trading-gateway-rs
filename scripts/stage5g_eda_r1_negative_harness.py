#!/usr/bin/env python3
"""Required fail-closed mutations for Stage 5G-e-d-a R1."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage5g_eda_r1_check.py"
FILES = [
    "crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.json",
    "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.md",
    "docs/stage-5/stage5g-lifecycle-entry-inventory.json",
    "docs/current-status.md",
    "docs/reviewer-onboarding-and-roadmap.md",
    "scripts/stage5g_eda_r1_gate.sh",
    "scripts/stage5g_eda_r1_negative_harness.py",
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
    with tempfile.TemporaryDirectory(prefix=f"stage5g-eda-r1-{name}-") as directory:
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
            raise SystemExit(f"stage5g-eda-r1-negative: FAIL: mutation survived: {name}")
        print(f"PASS {name}")


def main() -> None:
    source = FILES[0]
    cases = [
        ("remove-row-lower-freshness-bound", lambda root: replace_once(
            root, source, "order.received_ts < clean_restore_completed_at", "false")),
        ("remove-complete-empty-observation-proof", lambda root: replace_once(
            root, source, "validate_section_observation(\n        package.orders_observed_at,",
            "validate_section_observation_if_nonempty(\n        package.orders_observed_at,")),
        ("restore-strict-position-key-dedup", lambda root: replace_once(
            root, source,
            "instrument_identity_matches(&previous.instrument, &position.instrument)",
            "instrument_key(&previous.instrument) == instrument_key(&position.instrument)")),
        ("restore-unchecked-validated-deserialize", lambda root: replace_once(
            root, source,
            "#[derive(Debug, Clone, PartialEq, Eq, Serialize)]\n#[serde(deny_unknown_fields)]\npub(crate) struct Stage5gOperationalIdentityV1",
            "#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\n#[serde(deny_unknown_fields)]\npub(crate) struct Stage5gOperationalIdentityV1")),
        ("remove-filled-complete-fill-rule", lambda root: replace_once(
            root, source,
            "matches!(order.status, OrderStatus::Filled) && order.filled_qty != order.qty",
            "false")),
        ("collapse-pre-restart-and-last-reconciled", lambda root: replace_once(
            root, source,
            "pub(crate) pre_restart_package_id: &'a str,",
            "pub(crate) last_reconciled_package_id: &'a str,")),
        ("allow-changed-fingerprint-for-same-package-id", lambda root: replace_once(
            root, source,
            "    } else {\n        Err(Stage5gFreshBrokerTruthError::FreshPackageIdentityConflict)\n    }\n}\n\nfn validate_replay_ledger(",
            "    } else {\n        Ok(exact)\n    }\n}\n\nfn validate_replay_ledger(")),
        ("public-module-leak", lambda root: replace_once(
            root, FILES[1], "mod stage5g_fresh_broker_truth;", "pub mod stage5g_fresh_broker_truth;")),
        ("redis-surface", lambda root: append(root, source, "\nuse redis::Commands;\n")),
        ("runtime-callback-surface", lambda root: append(root, source, "\nfn forged() { on_bar(); }\n")),
        ("open-reducer", lambda root: replace_once(
            root, FILES[2], '"reconciliation_reducer": false', '"reconciliation_reducer": true')),
    ]
    for name, mutation in cases:
        run_case(name, mutation)
    print(f"stage5g-eda-r1-negative: PASS ({len(cases)}/{len(cases)})")


if __name__ == "__main__":
    main()
