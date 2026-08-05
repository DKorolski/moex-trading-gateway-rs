#!/usr/bin/env python3
"""Production-frozen mutation matrix for Stage 5G-e-d-a R4."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

import stage5g_eda_r3_negative_harness as r3


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage5g_eda_r4_check.py"
SOURCE = r3.SOURCE
CONTRACT = r3.CONTRACT
FREEZE = "docs/stage-5/stage5g-e-d-a-r4-production-freeze.json"
FILES = sorted(set(r3.FILES + [
    "docs/stage-5/stage5g-e-d-a-r4-production-freeze.json",
    "scripts/stage5g_eda_r4_check.py",
    "scripts/stage5g_eda_r4_gate.sh",
    "scripts/stage5g_eda_r4_negative_harness.py",
    "scripts/stage5g_eda_r4_preseal_check.py",
]))


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


def run_case(name: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix=f"stage5g-eda-r4-{name}-") as directory:
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
            raise SystemExit(f"stage5g-eda-r4-negative: FAIL: mutation survived: {name}")
        print(f"PASS {name}")


def r4_mutation_cases() -> list[tuple[str, object]]:
    return [
        ("contract-orders-completeness-false", lambda root: replace_once(
            root, CONTRACT, '"orders_completeness_explicit": true',
            '"orders_completeness_explicit": false')),
        ("contract-trades-completeness-false", lambda root: replace_once(
            root, CONTRACT, '"trades_completeness_explicit": true',
            '"trades_completeness_explicit": false')),
        ("contract-positions-completeness-false", lambda root: replace_once(
            root, CONTRACT, '"positions_completeness_explicit": true',
            '"positions_completeness_explicit": false')),
        ("contract-incomplete-means-absence", lambda root: replace_once(
            root, CONTRACT, '"incomplete_section_means_absent_rows": false',
            '"incomplete_section_means_absent_rows": true')),
        ("contract-validated-package-serializable", lambda root: replace_once(
            root, CONTRACT, '"validated_package_serializable": false',
            '"validated_package_serializable": true')),
        ("contract-package-callback-authority", lambda root: replace_once(
            root, CONTRACT, '"validated_package_owns_callback_authority": false',
            '"validated_package_owns_callback_authority": true')),
        ("remove-closed-surface-key", lambda root: replace_once(
            root, CONTRACT, '    "stage6": false,\n', "")),
        ("add-unreviewed-closed-surface-key", lambda root: replace_once(
            root, CONTRACT, '    "deployment": false\n',
            '    "deployment": false,\n    "unreviewed_surface": false\n')),
        ("remove-account-token-application", lambda root: replace_once(
            root, SOURCE, "!canonical_identity_token(input.account_id.as_str())", "false")),
        ("remove-target-symbol-token-application", lambda root: replace_once(
            root, SOURCE, "!canonical_identity_token(&input.target_instrument.symbol)", "false")),
        ("remove-pre-restart-token-application", lambda root: replace_once(
            root, SOURCE, "!canonical_identity_token(context.pre_restart_package_id)", "false")),
        ("remove-order-positive-qty", lambda root: replace_once(
            root, SOURCE, "order.qty <= Decimal::ZERO", "false")),
        ("remove-order-negative-filled", lambda root: replace_once(
            root, SOURCE, "order.filled_qty < Decimal::ZERO", "false")),
        ("remove-order-overfilled", lambda root: replace_once(
            root, SOURCE, "order.filled_qty > order.qty", "false")),
        ("remove-negative-remaining", lambda root: replace_once(
            root, SOURCE, "remaining < Decimal::ZERO", "false")),
        ("remove-orphan-order-duplicate", lambda root: replace_once(
            root, SOURCE, "!orphan_order_identities.insert(order_identity(order))", "false")),
        ("remove-reused-snapshot-epoch", lambda root: replace_once(
            root, SOURCE, ".any(|entry| entry.snapshot_epoch == *snapshot_epoch)",
            ".any(|_| false)")),
        ("allow-known-historical-unaccepted", lambda root: replace_once(
            root, SOURCE,
            "return Err(Stage5gFreshBrokerTruthError::HistoricalReplayNotAccepted);",
            "return Ok(Stage5gFreshPackageLineage::NewFresh);")),
        ("remove-market-data-generation-validation", lambda root: replace_once(
            root, SOURCE, "Stage5gFeedGeneration::parse(input.market_data_generation)?",
            "Stage5gFeedGeneration(1)")),
        ("remove-command-generation-validation", lambda root: replace_once(
            root, SOURCE,
            "Stage5gFeedGeneration::parse(\n                input.command_consumer_generation,\n            )?",
            "Stage5gFeedGeneration(1)")),
        ("remove-instrument-map-sha-validation", lambda root: replace_once(
            root, SOURCE,
            "Stage5gSha256::parse(\n                input.instrument_map_fingerprint_sha256,\n            )?",
            "Stage5gSha256(\"a\".repeat(64))")),
        ("append-alias-based-source-reducer", lambda root: insert_before_tests(
            root,
            "type FreshTruth = Stage5gValidatedFreshBrokerTruthPackage;\n"
            "type Decision = Stage5gRestartReconciliationDisposition;\n\n"
            "pub(crate) fn classify_truth(_package: FreshTruth) -> Decision {\n"
            "    Stage5gRestartReconciliationDisposition::ExactReplay\n"
            "}")),
        ("change-production-freeze-hash", lambda root: replace_once(
            root, FREEZE,
            "f2c1d9d104e3351e5d3c0eef300ca8e27081cb7568bd32e6eac8e0f421bd359f",
            "0000000000000000000000000000000000000000000000000000000000000000")),
    ]


def main() -> None:
    inherited = r3.mutation_cases()
    cases = inherited + r4_mutation_cases()
    if len(inherited) != 56:
        raise SystemExit(f"stage5g-eda-r4-negative: FAIL: inherited {len(inherited)} != 56")
    if len(cases) < 76:
        raise SystemExit(f"stage5g-eda-r4-negative: FAIL: only {len(cases)} cases")
    names = [name for name, _ in cases]
    if len(names) != len(set(names)):
        raise SystemExit("stage5g-eda-r4-negative: FAIL: duplicate mutation names")
    for name, mutation in cases:
        run_case(name, mutation)
    print(f"stage5g-eda-r4-negative: PASS ({len(cases)}/{len(cases)})")


if __name__ == "__main__":
    main()
