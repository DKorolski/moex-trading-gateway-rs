#!/usr/bin/env python3
"""Reject every declared Stage 8B-P preconditions authority mutation."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage8b_p_preconditions_check.py"
A = "docs/stage-8/stage8b-p-preconditions-authority.json"
C = "docs/stage-8/stage8b-p-finam-contract-snapshot-2026-08-23.json"
B = "docs/stage-8/stage8b-p-build-identity-2026-08-23.json"
G = "docs/stage-8/stage8b-p-governance-observation-2026-08-23.json"


def replace(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    value = path.read_text()
    if old not in value:
        raise RuntimeError(f"mutation anchor missing: {relative}: {old}")
    path.write_text(value.replace(old, new, 1))


MUTATIONS = [
    ("tls-ref", A, "6cb179509fad97e8be56e31bb930b2a86caefc6a", "0cb179509fad97e8be56e31bb930b2a86caefc6a"),
    ("tls-tree", A, "4900fd38d741ab24f643acf211e7d1f807d23792", "0900fd38d741ab24f643acf211e7d1f807d23792"),
    ("tls-archive", A, "1066ab44b32451f921f2d3cdd49118471f78b214de7dd848a3273c95e19143b6", "0066ab44b32451f921f2d3cdd49118471f78b214de7dd848a3273c95e19143b6"),
    ("tree-not-identical", A, '"accepted_tls_tree_identical_after_merge": true', '"accepted_tls_tree_identical_after_merge": false'),
    ("contract-response-removed", C, '"name":"rest_schedule"', '"name":"removed_schedule"'),
    ("contract-http", C, '"http_status":200', '"http_status":500'),
    ("contract-hash", C, "0fc4494e2f06a9bc8aebb10eb0a7de0500b661c9988a9fdfda526364348ff589", "1fc4494e2f06a9bc8aebb10eb0a7de0500b661c9988a9fdfda526364348ff589"),
    ("material-drift", C, '"material_contract_drift": false', '"material_contract_drift": true'),
    ("production-host", C, '"production_host": "api.finam.ru"', '"production_host": "example.invalid"'),
    ("place-method", C, '"method": "POST"', '"method": "GET"'),
    ("cancel-method", C, '"method": "DELETE"', '"method": "GET"'),
    ("retry-open", C, '"automatic_retry": false', '"automatic_retry": true'),
    ("build-archive", B, "1066ab44b32451f921f2d3cdd49118471f78b214de7dd848a3273c95e19143b6", "2066ab44b32451f921f2d3cdd49118471f78b214de7dd848a3273c95e19143b6"),
    ("executable", B, "677f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06", "777f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06"),
    ("one-build", B, '"independent_clean_build_count": 2', '"independent_clean_build_count": 1'),
    ("nonreproducible", B, '"all_executable_hashes_identical": true', '"all_executable_hashes_identical": false'),
    ("cargo-lock", B, "8233fd447ee0d7bc1cc1983960af771f70c8e3b4db53a57fb4ffb453d8c529b6", "9233fd447ee0d7bc1cc1983960af771f70c8e3b4db53a57fb4ffb453d8c529b6"),
    ("rustc-commit", B, "59807616e1fa2540724bfbac14d7976d7e4a3860", "09807616e1fa2540724bfbac14d7976d7e4a3860"),
    ("false-protection", G, '"branch_protected": false', '"branch_protected": true'),
    ("false-ruleset", G, '"ruleset_enforcement": "disabled"', '"ruleset_enforcement": "active"'),
    ("mutable-inventory", G, '"actions/checkout@v4"', '"actions/checkout@immutable"'),
    ("gov-self-accept", G, '"gov_p1_status": "PENDING_INDEPENDENT_ACCEPTANCE_OR_RULESET_ENABLEMENT"', '"gov_p1_status": "ACCEPTED"'),
    ("open-p", A, '"stage8b_p": true', '"stage8b_p": false'),
    ("next-execution", A, "independent_review_of_this_design_only_preconditions_package", "execute_stage8b_p"),
]


def main() -> None:
    if len(MUTATIONS) != 24:
        raise SystemExit("stage8b-p-preconditions-negative: FAIL inventory count")
    with tempfile.TemporaryDirectory(prefix="stage8b-p-preconditions-negative-") as tmp:
        base = Path(tmp) / "base"
        shutil.copytree(ROOT, base, ignore=shutil.ignore_patterns("target", ".git", "reports", "tmp"))
        for index, (name, relative, old, new) in enumerate(MUTATIONS, 1):
            case = Path(tmp) / f"case-{index:02d}"
            shutil.copytree(base, case)
            replace(case, relative, old, new)
            result = subprocess.run(
                ["python3", CHECKER, "--no-git"], cwd=case, text=True, capture_output=True
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-preconditions-negative: FAIL mutation passed: {name}")
            print(f"PASS {index:02d}/24 {name}")
    print("stage8b-p-preconditions-negative: PASS 24/24")


if __name__ == "__main__":
    main()
