#!/usr/bin/env python3
"""Named mutation matrix for Stage 5G-e-d-c R3 source-map sealing."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

import stage5g_edc_r2_negative_harness as r2_negative

ROOT = Path(__file__).resolve().parents[1]
APP = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/application.rs")
CLEAN = Path("crates/strategy-runtime-core/src/stage5g_clean_restart.rs")
SOURCE_MAP = Path("docs/stage-5/stage5g-e-d-c-r3-source-proof-field-map.json")
DESIGN = Path("docs/stage-5/stage5g-e-d-c-r3-source-proof-field-map.md")
CHECKER = Path("scripts/stage5g_edc_r3_check.py")


def replace_once(source: str, old: str, new: str) -> str:
    if source.count(old) < 1:
        raise RuntimeError(f"mutation anchor missing: {old[:160]!r}")
    return source.replace(old, new, 1)


def direct_source_map_cases() -> list[tuple[str, Path, str, str]]:
    return [
        (
            "wrong-scenario-source",
            APP,
            "scenario_id: parts.scenario_id.frozen_id().to_string(),",
            'scenario_id: "FORGED-SCENARIO".to_owned(),',
        ),
        (
            "wrong-disposition-source",
            APP,
            "disposition: disposition_id(parts.disposition).to_string(),",
            'disposition: "forged_disposition".to_owned(),',
        ),
        (
            "wrong-reason-source",
            APP,
            "reason: reason_id(parts.reason),",
            'reason: "forged_reason".to_owned(),',
        ),
        (
            "wrong-operational-identity-source",
            APP,
            "operational_identity_commitment_sha256: parts\n                .truth\n                .operational_binding_commitment_sha256\n                .clone(),",
            'operational_identity_commitment_sha256: "b".repeat(64),',
        ),
        (
            "wrong-command-request-source",
            APP,
            "command_request_id: candidate.command_request_id().to_owned(),",
            'command_request_id: "FORGED-REQUEST".to_owned(),',
        ),
        (
            "wrong-parent-snapshot-id-source",
            APP,
            "parent_snapshot_id: parts\n                .restart\n                .stage5g_application_parent_snapshot_binding()\n                .0,",
            'parent_snapshot_id: "FORGED-PARENT".to_owned(),',
        ),
        (
            "wrong-parent-snapshot-revision-source",
            APP,
            "parent_snapshot_revision: parts\n                .restart\n                .stage5g_application_parent_snapshot_binding()\n                .1,",
            "parent_snapshot_revision: parts\n                .restart\n                .stage5g_application_parent_snapshot_binding()\n                .1 + 1,",
        ),
        (
            "wrong-fresh-package-id-source",
            APP,
            "fresh_package_id: parts.truth.package.package_id.as_str().to_string(),",
            'fresh_package_id: "FORGED-PACKAGE".to_owned(),',
        ),
        (
            "wrong-fresh-snapshot-epoch-source",
            APP,
            "fresh_snapshot_epoch: parts.truth.package.snapshot_epoch.as_str().to_string(),",
            'fresh_snapshot_epoch: "FORGED-EPOCH".to_owned(),',
        ),
        (
            "swap-fresh-id-and-epoch-source",
            APP,
            "fresh_package_id: parts.truth.package.package_id.as_str().to_string(),",
            "fresh_package_id: parts.truth.package.snapshot_epoch.as_str().to_string(),",
        ),
        (
            "wrong-fresh-captured-at-source",
            APP,
            "fresh_captured_at: parts.truth.package.captured_at,",
            "fresh_captured_at: parts.truth.package.captured_at + chrono::Duration::seconds(1),",
        ),
        (
            "wrong-fresh-package-fingerprint-source",
            APP,
            "fresh_package_fingerprint_sha256: parts\n                .truth\n                .package\n                .canonical_fingerprint_sha256\n                .clone(),",
            'fresh_package_fingerprint_sha256: "b".repeat(64),',
        ),
        (
            "wrong-pre-restart-fingerprint-source",
            APP,
            "pre_restart_package_fingerprint_sha256: parts\n                .restart\n                .stage5g_pre_restart_package_fingerprint_sha256(),",
            'pre_restart_package_fingerprint_sha256: "b".repeat(64),',
        ),
        (
            "wrong-reduction-pre-fingerprint-source",
            APP,
            "reduction_pre_semantic_fingerprint_sha256: parts\n                .pre_semantic_fingerprint_sha256\n                .clone(),",
            'reduction_pre_semantic_fingerprint_sha256: "b".repeat(64),',
        ),
        (
            "wrong-terminal-history-count-source",
            APP,
            "ignored_terminal_order_count: parts.ignored_unrelated_terminal_order_count,",
            "ignored_terminal_order_count: parts.ignored_unrelated_terminal_order_count + 1,",
        ),
        (
            "wrong-trade-history-count-source",
            APP,
            "ignored_historical_trade_count: parts.ignored_unrelated_historical_trade_count,",
            "ignored_historical_trade_count: parts.ignored_unrelated_historical_trade_count + 1,",
        ),
        (
            "source-proof-built-from-evidence-instead-of-reduction",
            APP,
            "reason: reason_id(parts.reason),",
            "reason: evidence.reason.clone(),",
        ),
        (
            "source-map-descriptor-fresh-fingerprint-drift",
            SOURCE_MAP,
            '"fresh_package_fingerprint_sha256": "parts.truth.package.canonical_fingerprint_sha256.clone()"',
            '"fresh_package_fingerprint_sha256": "\\"b\\".repeat(64)"',
        ),
        (
            "source-map-negative-floor-lowered",
            SOURCE_MAP,
            '"aggregate_minimum": 540',
            '"aggregate_minimum": 1',
        ),
        (
            "parent-revision-package-binding-removed",
            CLEAN,
            "stage5g_application_parent_revision_matches_package_instance(\n                    evidence,\n                    &projection.package_instance,\n                )",
            "true",
        ),
        (
            "source-oracle-helper-uses-source-proof-constructor",
            APP,
            "Self {\n            scenario_id:",
            "let _ = Stage5gFreshTruthApplicationSourceProof::from_application_parts(parts, candidate);\n        Self {\n            scenario_id:",
        ),
        (
            "r3-design-policy-b-closure-removed",
            DESIGN,
            "Policy B `ExactReplay` remains disabled.",
            "Policy B exact replay policy omitted.",
        ),
        (
            "r3-checker-field-map-enforcement-disabled",
            CHECKER,
            "require(expected in compact, f\"source-map assignment drift for {field}\")",
            "require(True or expected in compact, f\"source-map assignment drift for {field}\")",
        ),
    ]


def cases() -> list[tuple[str, Path, str, str]]:
    return r2_negative.cases() + direct_source_map_cases()


def main() -> None:
    matrix = cases()
    if len(direct_source_map_cases()) < 18:
        raise SystemExit("stage5g-edc-r3-negative: FAIL: direct R3 cases below floor")
    with tempfile.TemporaryDirectory(prefix="stage5g-edc-r3-negative-") as temp:
        work = Path(temp) / "repo"
        shutil.copytree(
            ROOT,
            work,
            ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports", "*.zip"),
        )
        originals = {path: (ROOT / path).read_text() for _, path, _, _ in matrix}
        for name, path, old, new in matrix:
            target = work / path
            target.write_text(replace_once(originals[path], old, new))
            result = subprocess.run(
                ["python3", "scripts/stage5g_edc_r3_check.py", "--root", str(work), "--skip-git"],
                cwd=work,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.STDOUT,
                check=False,
            )
            target.write_text(originals[path])
            if result.returncode == 0:
                raise SystemExit(f"stage5g-edc-r3-negative: FAIL: survived {name}")
            print(f"PASS {name}")
    current = len(direct_source_map_cases())
    print(f"stage5g-edc-r3-negative: PASS current={current}/{current} aggregate={len(matrix)}/{len(matrix)}")


if __name__ == "__main__":
    main()
