#!/usr/bin/env python3
"""Stage 5G-e-d-c R3 exact source-proof field-map checker."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

import stage5g_edc_r2_check as r2

BASE = "95901eb9bf19e103e9acb82fb9726708f356b4cd"
BRANCH = "stage5g-lifecycle"
SOURCE_MAP = Path("docs/stage-5/stage5g-e-d-c-r3-source-proof-field-map.json")
DESIGN = Path("docs/stage-5/stage5g-e-d-c-r3-source-proof-field-map.md")
APP = r2.APP
REDUCER = r2.REDUCER
CLEAN = r2.CLEAN
SELF = Path("scripts/stage5g_edc_r3_check.py")

EXPECTED_SOURCE_FIELDS = [
    "scenario_id",
    "disposition",
    "reason",
    "operational_identity_commitment_sha256",
    "command_request_id",
    "parent_snapshot_id",
    "parent_snapshot_revision",
    "fresh_package_id",
    "fresh_snapshot_epoch",
    "fresh_captured_at",
    "fresh_package_fingerprint_sha256",
    "pre_restart_package_fingerprint_sha256",
    "reduction_pre_semantic_fingerprint_sha256",
    "ignored_terminal_order_count",
    "ignored_historical_trade_count",
]

R3_ORACLE_TESTS = [
    "stage5g_edc_r3_source_oracle_grst02_candidate",
    "stage5g_edc_r3_source_oracle_grst03_working",
    "stage5g_edc_r3_source_oracle_grst04_partial_fill",
    "stage5g_edc_r3_source_oracle_grst11_terminal_late_fill",
    "stage5g_edc_r3_source_oracle_nonzero_history_counts",
    "stage5g_edc_r3_fresh_captured_at_exact_watermark_binding",
    "stage5g_edc_r3_parent_revision_cross_binding",
    "stage5g_edc_r3_restored_evidence_source_oracle_equality",
    "stage5g_edc_r3_source_proof_cannot_be_rebuilt_from_evidence",
]


def require(ok: bool, message: str) -> None:
    if not ok:
        raise SystemExit(f"stage5g-edc-r3-check: FAIL: {message}")


def text(root: Path, path: Path) -> str:
    target = root / path
    require(target.is_file() and not target.is_symlink(), f"missing {path}")
    return target.read_text()


def normalize(value: str) -> str:
    return re.sub(r"\s+", "", value)


def constructor_region(source: str) -> str:
    start = source.find("fn from_application_parts(")
    require(start >= 0, "missing source-proof constructor")
    end = source.find("\n    fn commitment_sha256", start)
    require(end > start, "source-proof constructor region end missing")
    return source[start:end]


def check_source_map(root: Path, app: str) -> None:
    descriptor = json.loads(text(root, SOURCE_MAP))
    require(descriptor["stage"] == "5G-e-d-c R3", "source-map stage drift")
    require(descriptor["base_ref"] == BASE, "source-map base drift")
    require(descriptor["branch"] == BRANCH, "source-map branch drift")
    require(descriptor["schema_version"] == 1, "source-map schema drift")
    require(descriptor["source_proof_type"] == "Stage5gFreshTruthApplicationSourceProof",
            "source-map type drift")
    require(descriptor["constructor"] == "Stage5gFreshTruthApplicationSourceProof::from_application_parts",
            "source-map constructor drift")
    require(all(v is False for v in descriptor["closed_surfaces"].values()),
            "source-map opened a closed surface")
    require(descriptor["negative_floor"]["current_r3_new_minimum"] >= 18,
            "R3 current negative floor drift")
    require(descriptor["negative_floor"]["aggregate_minimum"] >= 540,
            "R3 aggregate negative floor drift")

    source_fields = r2.struct_fields(app, "Stage5gFreshTruthApplicationSourceProof")
    field_map = descriptor["field_map"]
    require(source_fields == EXPECTED_SOURCE_FIELDS, "source-proof field inventory drift")
    require(list(field_map.keys()) == EXPECTED_SOURCE_FIELDS, "source-map field order/inventory drift")

    region = constructor_region(app)
    compact = normalize(region)
    require(compact.count("fnfrom_application_parts(") == 1, "constructor definition drift")
    require(compact.count("from_application_parts(") == 1,
            "constructor recursively or indirectly rebuilds source proof")
    require(compact.count("Self{scenario_id:") == 1, "constructor must use one source-proof Self literal")
    require("Default::default" not in region, "constructor gained Default source")
    require(".." not in region, "constructor gained struct spread")
    require("Stage5gFreshTruthApplicationEvidenceV1" not in region and "evidence" not in region,
            "source proof constructor reads application evidence")
    require("fn from_evidence" not in app, "source proof can be reconstructed from evidence")
    require("::from_evidence" not in app, "source proof from_evidence route exists")
    require("impl From<&Stage5gFreshTruthApplicationEvidenceV1> for Stage5gFreshTruthApplicationSourceProof" not in app,
            "source proof From<evidence> route exists")

    for field, expression in field_map.items():
        expected = normalize(f"{field}: {expression},")
        require(expected in compact, f"source-map assignment drift for {field}")
        require(compact.count(normalize(f"{field}:")) == 1,
                f"source-map duplicate assignment for {field}")


def check(root: Path, check_git: bool) -> None:
    if check_git:
        head_parent = subprocess.check_output(
            ["git", "rev-parse", "HEAD^"], cwd=root, text=True
        ).strip()
        branch = subprocess.check_output(
            ["git", "branch", "--show-current"], cwd=root, text=True
        ).strip()
        require(head_parent == BASE, "HEAD is not one direct successor to 95901eb")
        require(branch == BRANCH, "wrong branch")

    r2.check(root, False)
    app = text(root, APP)
    reducer = text(root, REDUCER)
    clean = text(root, CLEAN)
    design = text(root, DESIGN)
    self_source = text(root, SELF)

    check_source_map(root, app)
    require('require(expected in compact, f"source-map assignment drift for {field}")' in self_source,
            "checker source-map enforcement line drift")
    bypass_marker = "True or " + "expected in compact"
    require(bypass_marker not in self_source, "checker source-map enforcement bypassed")

    require(app.count("Stage5gFreshTruthApplicationSourceProof::from_application_parts(&parts, &candidate)") == 1,
            "source proof construction callsite drift")
    require(app.count("fn from_application_parts(") == 1, "source proof constructor definition drift")
    require("Stage5gFreshTruthApplicationSourceProof::from_application_parts" in design,
            "R3 design lost constructor scope")
    require("machine-checked field-to-source contract" in design,
            "R3 design lost source-map authority")
    require("Policy B `ExactReplay` remains disabled" in design,
            "R3 design lost Policy-B closure")
    require("Stage 5G-f or Stage 6" in design, "R3 design lost closed-stage statement")

    for test_name in R3_ORACLE_TESTS:
        require(f"fn {test_name}(" in reducer, f"missing R3 source-oracle test {test_name}")

    require("stage5g_edc_r3_capture_source_oracle" in reducer,
            "independent source oracle helper missing")
    require("from_application_parts" not in reducer.split("fn stage5g_edc_r3_capture_source_oracle", 1)[1]
            .split("\n    fn ", 1)[0],
            "source oracle calls source-proof constructor")
    require("stage5g_application_parent_revision_matches_package_instance(" in clean,
            "parent revision/package-instance cross-binding missing")
    require(clean.count("stage5g_application_parent_revision_matches_package_instance(\n                    evidence,\n                    &projection.package_instance,\n                )") == 1,
            "parent revision/package-instance guard callsite drift")
    require(".previous_revision" in clean and "evidence.parent_snapshot_revision()" in clean,
            "parent revision cross-binding does not use independent package previous revision")
    require(clean.count("evidence.parent_snapshot_revision()") == 2,
            "parent revision must feed both package recompute and package-instance binding")
    require(clean.count("evidence.parent_snapshot_id()") == 1,
            "parent snapshot id must feed post-package recompute exactly once")

    forbidden = [
        "redis::", "reqwest", "Method::POST", "Method::DELETE", ".post(", ".delete(",
        "finam_client", "dispatch_order", "runtime_live", "on_broker_bar(", "on_timer(",
    ]
    for value in forbidden:
        require(value not in app, f"forbidden application surface: {value}")

    print("stage5g-edc-r3-check: PASS")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    check(args.root.resolve(), not args.skip_git)


if __name__ == "__main__":
    main()
