#!/usr/bin/env python3
"""Fetch and validate the exact public GitHub GOV-P1 ruleset state."""

from __future__ import annotations

import argparse
import hashlib
import json
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "docs/stage-8/stage8b-p-governance-observation-2026-08-23.json"
REPOSITORY = "DKorolski/moex-trading-gateway-rs"
RULESET_ID = 20111805
API = f"https://api.github.com/repos/{REPOSITORY}"
CHECKOUT_SHA = "11d5960a326750d5838078e36cf38b85af677262"
RUST_ACTION_SHA = "4360b52568e2003a75bf9bc1d59f33a8e3fc893c"
RUST_RELEASE = "1.95.0"
REQUIRED_CHECKS = ["redis-smoke", "rust"]


def fetch(path: str) -> dict[str, Any]:
    request = urllib.request.Request(
        API + path,
        headers={
            "Accept": "application/vnd.github+json",
            "User-Agent": "moex-stage8b-p-governance-refresh",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        if response.status != 200:
            raise RuntimeError(f"GitHub API status {response.status}: {path}")
        return json.load(response)


def normalize_rules(rules: list[dict[str, Any]]) -> dict[str, Any]:
    by_type = {item.get("type"): item for item in rules}
    pull = by_type.get("pull_request", {}).get("parameters", {})
    checks = by_type.get("required_status_checks", {}).get("parameters", {})
    return {
        "rule_types": sorted(str(value) for value in by_type),
        "deletion_blocked": "deletion" in by_type,
        "force_push_blocked": "non_fast_forward" in by_type,
        "pull_request": {
            "required_approving_review_count": pull.get("required_approving_review_count"),
            "dismiss_stale_reviews_on_push": pull.get("dismiss_stale_reviews_on_push"),
            "require_last_push_approval": pull.get("require_last_push_approval"),
            "required_review_thread_resolution": pull.get("required_review_thread_resolution"),
            "require_extra_approval_for_unattributed_changes": pull.get(
                "require_extra_approval_for_unattributed_changes"
            ),
            "allowed_merge_methods": sorted(pull.get("allowed_merge_methods", [])),
        },
        "required_status_checks": {
            "contexts": sorted(
                str(item.get("context"))
                for item in checks.get("required_status_checks", [])
                if item.get("context")
            ),
            "strict_required_status_checks_policy": checks.get(
                "strict_required_status_checks_policy"
            ),
        },
    }


def material_observation() -> dict[str, Any]:
    repository = fetch("")
    branch = fetch("/branches/main")
    ruleset = fetch(f"/rulesets/{RULESET_ID}")
    normalized_rules = normalize_rules(ruleset.get("rules", []))
    conditions = ruleset.get("conditions", {}).get("ref_name", {})
    material = {
        "repository": REPOSITORY,
        "default_branch": repository.get("default_branch"),
        "observed_main_head": branch.get("commit", {}).get("sha"),
        "branch_protected": branch.get("protected"),
        "ruleset": {
            "id": ruleset.get("id"),
            "name": ruleset.get("name"),
            "target": ruleset.get("target"),
            "source_type": ruleset.get("source_type"),
            "enforcement": ruleset.get("enforcement"),
            "conditions": {
                "include": sorted(conditions.get("include", [])),
                "exclude": sorted(conditions.get("exclude", [])),
            },
            "bypass_actors": ruleset.get("bypass_actors", []),
            **normalized_rules,
        },
    }
    material["compliant"] = compliant(material)
    return material


def compliant(value: dict[str, Any]) -> bool:
    ruleset = value["ruleset"]
    pull = ruleset["pull_request"]
    checks = ruleset["required_status_checks"]
    return all(
        (
            value.get("default_branch") == "main",
            value.get("branch_protected") is True,
            ruleset.get("id") == RULESET_ID,
            ruleset.get("name") == "moex-trading-project",
            ruleset.get("target") == "branch",
            ruleset.get("source_type") == "Repository",
            ruleset.get("enforcement") == "active",
            ruleset.get("conditions") == {"include": ["~DEFAULT_BRANCH"], "exclude": []},
            ruleset.get("bypass_actors") == [],
            ruleset.get("rule_types")
            == ["deletion", "non_fast_forward", "pull_request", "required_status_checks"],
            ruleset.get("deletion_blocked") is True,
            ruleset.get("force_push_blocked") is True,
            pull
            == {
                "required_approving_review_count": 1,
                "dismiss_stale_reviews_on_push": True,
                "require_last_push_approval": True,
                "required_review_thread_resolution": True,
                "require_extra_approval_for_unattributed_changes": True,
                "allowed_merge_methods": ["merge"],
            },
            checks
            == {
                "contexts": REQUIRED_CHECKS,
                "strict_required_status_checks_policy": True,
            },
        )
    )


def document(material: dict[str, Any]) -> dict[str, Any]:
    ci = ROOT / ".github/workflows/ci.yml"
    return {
        "schema_version": 2,
        "observation_kind": "stage8b_p_enforced_governance_precondition",
        "observed_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        **material,
        "canonical_ci": {
            "sha256": hashlib.sha256(ci.read_bytes()).hexdigest(),
            "actions_checkout_sha": CHECKOUT_SHA,
            "rust_toolchain_action_sha": RUST_ACTION_SHA,
            "rust_release": RUST_RELEASE,
            "mutable_references_present": False,
        },
        "required_governance_policy": {
            "active_main_ruleset_required": True,
            "pull_request_required": True,
            "one_independent_approval_required": True,
            "canonical_status_checks_required": True,
            "force_push_blocked_required": True,
            "branch_deletion_blocked_required": True,
            "empty_bypass_policy_required": True,
            "post_merge_exact_head_and_tree_verification_required": True,
            "current_tree_gate_required": True,
            "administrator_self_acceptance_for_p_forbidden": True,
        },
        "gov_p1_status": "READY_FOR_INDEPENDENT_ACCEPTANCE"
        if material["compliant"]
        else "PENDING_RULESET_ACTIVATION",
        "workflow_modified_by_this_slice": True,
        "stage8b_p_authorized": False,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    material = material_observation()
    candidate = document(material)
    if args.write:
        OUTPUT.write_text(json.dumps(candidate, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    else:
        recorded = json.loads(OUTPUT.read_text(encoding="utf-8"))
        for key in (
            "repository",
            "default_branch",
            "observed_main_head",
            "branch_protected",
            "ruleset",
            "compliant",
        ):
            if recorded.get(key) != candidate.get(key):
                raise SystemExit(f"stage8b-p-governance-refresh: FAIL live drift: {key}")
    if not material["compliant"]:
        raise SystemExit("stage8b-p-governance-refresh: FAIL GOV-P1 ruleset is not compliant")
    print(
        "stage8b-p-governance-refresh: PASS "
        f"ruleset={RULESET_ID} enforcement=active checks={','.join(REQUIRED_CHECKS)} "
        "stage8b_p=false"
    )


if __name__ == "__main__":
    main()
