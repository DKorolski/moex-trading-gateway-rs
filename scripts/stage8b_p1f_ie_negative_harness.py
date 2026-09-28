#!/usr/bin/env python3
"""Mutation harness for Stage 8B-P1-f Ie aggregate source closure."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
from pathlib import Path

import stage8b_p1f_ie_check as check


def replace(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError(f"mutation anchor count for {relative}: {old!r}")
    path.write_text(text.replace(old, new, 1))


CASES = (
    ("self-accept", check.INVENTORY, "REVIEW_CANDIDATE_LINKED_COMPOSITION_CORRECTION_NO_ACTIVATION", "ACCEPTED"),
    ("ia-commit", check.INVENTORY, check.ACCEPTED_SOURCES[0]["commit"], "0" * 40),
    ("ib-commit", check.INVENTORY, check.ACCEPTED_SOURCES[1]["commit"], "1" * 40),
    ("ic-commit", check.INVENTORY, check.ACCEPTED_SOURCES[2]["commit"], "2" * 40),
    ("id-commit", check.INVENTORY, check.ACCEPTED_SOURCES[3]["commit"], "3" * 40),
    ("ia-review", check.INVENTORY, check.ACCEPTED_SOURCES[0]["review_sha256"], "4" * 64),
    ("id-review", check.INVENTORY, check.ACCEPTED_SOURCES[3]["review_sha256"], "5" * 64),
    ("correction-review", check.INVENTORY, check.CORRECTION_REVIEW["sha256"], "6" * 64),
    ("fixture-count", check.INVENTORY, '"aggregate_regression_fixture_count": 9', '"aggregate_regression_fixture_count": 8'),
    ("production-change", check.INVENTORY, '"new_production_rust_files": 0', '"new_production_rust_files": 1'),
    ("cargo-change", check.INVENTORY, '"new_cargo_changes": 0', '"new_cargo_changes": 1'),
    ("operational-redis", check.INVENTORY, '"operational_redis_db15_or_db0": false', '"operational_redis_db15_or_db0": true'),
    ("next-boundary", check.INVENTORY, "P1F-O0 immutable read-only target preflight; operational activation remains closed", "P1F-O0 activated"),
    ("matrix-row", check.MATRIX, "P1FIE-024,roadmap,Only independently accepted Ie may unlock separate O0 read-only target preflight,REQUIRED\n", ""),
    ("o2-link", check.LINKED, "o2_materialization_finalizes_only_source_hash_and_replays_exactly", "removed_o2_materialization_link"),
    ("supervision-link", check.LINKED, "local_supervision_starts_after_admission_and_stops_on_sigterm", "removed_local_supervision_link"),
    ("producer-link", check.LINKED, "id_linked_real_redis_response_loss_restarts_prepared_without_duplicate", "removed_producer_redis_link"),
    ("resource-link", check.LINKED, "resource_probe_uses_real_bounded_redis_reads_and_hash_only_audit", "removed_resource_audit_link"),
    ("runtime-link", check.LINKED, "production_v5_bootstrap_runs_continuous_market_lifecycle_and_readmits_exactly", "removed_runtime_link"),
    ("linked-witness", check.WITNESS, check.WITNESS_SELECTOR, "removed_linked_composition_witness"),
    (
        "qualified-selector",
        check.LINKED,
        "stage8b_p1f_guardian::tests::o2_materialization_finalizes_only_source_hash_and_replays_exactly",
        "wrong_module::tests::o2_materialization_finalizes_only_source_hash_and_replays_exactly",
    ),
    ("exact-selection-guard", check.EXACT_RUNNER, 'grep -Foc "$selector"', 'grep -Foc "running 1 test"'),
    ("status", check.STATUS, "The active source candidate is now the P1F-Ie linked-composition correction", "The active source candidate is now P1F-O0"),
    ("roadmap", check.ROADMAP, "P1F-O0 stays closed", "P1F-O0 is open"),
)


def zero_selection_guard() -> None:
    with tempfile.TemporaryDirectory(prefix="stage8b-p1f-ie-zero-selection-") as raw:
        fake_bin = Path(raw)
        cargo = fake_bin / "cargo"
        cargo.write_text(
            "#!/usr/bin/env bash\n"
            "echo 'running 0 tests'\n"
            "echo 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out'\n"
            "exit 0\n"
        )
        cargo.chmod(0o755)
        environment = os.environ.copy()
        environment["PATH"] = str(fake_bin) + os.pathsep + environment["PATH"]
        result = subprocess.run(
            [
                "bash",
                str(check.ROOT / check.EXACT_RUNNER),
                "runtime-durable-service",
                check.RUNTIME_REGRESSIONS[0],
            ],
            cwd=check.ROOT,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
        if result.returncode == 0:
            raise SystemExit("FAIL zero-selection exact runner returned success")
        print("PASS zero-selection-exact-runner-rejected")


def copy_contract(root: Path) -> None:
    for relative in check.ALLOWED_CHANGES:
        source = check.ROOT / relative
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="stage8b-p1f-ie-negative-") as raw:
        root = Path(raw)
        copy_contract(root)
        check.validate(root, check_lineage=False)
        print("PASS positive-control")
        document = root / check.DOCUMENT
        document.write_text(document.read_text() + "\n")
        check.validate(root, check_lineage=False)
        print("PASS nonsemantic-control")
    zero_selection_guard()
    for name, relative, old, new in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-p1f-ie-{name}-") as raw:
            root = Path(raw)
            copy_contract(root)
            replace(root, relative, old, new)
            try:
                check.validate(root, check_lineage=False)
            except (check.CheckFailure, OSError, UnicodeDecodeError, KeyError, TypeError):
                print(f"PASS {name}")
            else:
                raise SystemExit(f"FAIL mutation survived: {name}")
    print(f"PASS stage8b-p1f-ie-negative-harness {len(CASES)}/{len(CASES)}")


if __name__ == "__main__":
    main()
