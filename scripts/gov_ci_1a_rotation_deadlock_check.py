#!/usr/bin/env python3
"""Prove that the requested in-band GOV-CI-1A rotation has no valid candidate."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BASE_REF = "0ce76a334f12bf7b13e682ca976c9a4cde6be137"
AUTHORITY_REF = "8ce0acd60c7cb5cc5d25a27f6553077240658b57"
CI = ".github/workflows/ci.yml"
AUTHORITY_WORKFLOW = ".github/workflows/stage5f-base-authority.yml"
CONTRACT = ROOT / "scripts/stage5f_base_authority_contract.py"


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def git_show(ref: str, path: str) -> bytes:
    return subprocess.check_output(["git", "show", f"{ref}:{path}"], cwd=ROOT)


def load_contract():
    spec = importlib.util.spec_from_file_location("stage5f_base_authority_contract", CONTRACT)
    require(spec is not None and spec.loader is not None, "cannot load protected-base contract")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def main() -> None:
    workflow = git_show(BASE_REF, AUTHORITY_WORKFLOW).decode("utf-8")
    contract_source = git_show(BASE_REF, "scripts/stage5f_base_authority_contract.py").decode("utf-8")
    require(f'ref: "{AUTHORITY_REF}"' in workflow, "fixed authority ref drift")
    require("pull_request_target:" in workflow, "protected-base trigger is not active")
    require(
        "require_entry(authority_entries, CI_WORKFLOW) != require_entry(candidate_entries, CI_WORKFLOW)"
        in contract_source,
        "canonical CI external-authority equality rule missing",
    )
    require('return relative == AUTHORITY_WORKFLOW' in contract_source, "workflow rotation scope rule missing")

    base_ci = git_show(BASE_REF, CI)
    authority_ci = git_show(AUTHORITY_REF, CI)
    base_sha = hashlib.sha256(base_ci).hexdigest()
    authority_sha = hashlib.sha256(authority_ci).hexdigest()
    require(base_sha != authority_sha, "deadlock precondition absent: canonical CI already equals authority")

    contract = load_contract()
    canonical_ci_change_allowed = contract.is_rotation_path_allowed(
        contract.CI_WORKFLOW, "5F-terminal-authority-retirement"
    )
    authority_workflow_change_allowed = contract.is_rotation_path_allowed(
        contract.AUTHORITY_WORKFLOW, "5F-terminal-authority-retirement"
    )
    require(canonical_ci_change_allowed is False, "canonical CI unexpectedly allowed in generic rotation")
    require(authority_workflow_change_allowed is True, "authority workflow unexpectedly forbidden")

    evidence = {
        "schema_version": 1,
        "stage": "GOV-CI-1A-rotation-deadlock-discovery",
        "base_ref": BASE_REF,
        "fixed_authority_ref": AUTHORITY_REF,
        "base_ci_sha256": base_sha,
        "fixed_authority_ci_sha256": authority_sha,
        "base_ci_equals_fixed_authority": False,
        "unchanged_candidate_ci_accepted": False,
        "canonical_ci_change_allowed_by_rotation": False,
        "authority_workflow_change_allowed_by_rotation": True,
        "candidate_contract_can_change_base_side_execution": False,
        "in_band_terminal_rotation_satisfiable": False,
        "stage8b_d_r2_authorized": False,
        "stage8b_s_authorized": False,
        "finam_execution_enabled": False,
        "redis_live_consumer_enabled": False,
        "runtime_live_enabled": False,
        "real_orders_enabled": False,
    }
    print(json.dumps(evidence, indent=2, sort_keys=True))
    print("gov-ci-1a-rotation-deadlock-check: PASS deadlock_proven=true")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"gov-ci-1a-rotation-deadlock-check: FAIL {error}") from error
