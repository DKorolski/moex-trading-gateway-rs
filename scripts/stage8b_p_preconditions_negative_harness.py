#!/usr/bin/env python3
"""Reject every declared Stage 8B-P preconditions R2 authority mutation."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage8b_p_preconditions_check.py"
A = "docs/stage-8/stage8b-p-preconditions-authority.json"
C = "docs/stage-8/stage8b-p-finam-contract-snapshot-2026-08-23.json"
B = "docs/stage-8/stage8b-p-build-identity-2026-08-23.json"
G = "docs/stage-8/stage8b-p-governance-observation-2026-08-23.json"
CI = ".github/workflows/ci.yml"


def replace(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    value = path.read_text(encoding="utf-8")
    if old not in value:
        raise RuntimeError(f"mutation anchor missing: {relative}: {old}")
    path.write_text(value.replace(old, new, 1), encoding="utf-8")


def mutate_json(root: Path, relative: str, keys: tuple[str, ...], value: Any) -> None:
    path = root / relative
    document = json.loads(path.read_text(encoding="utf-8"))
    target = document
    for key in keys[:-1]:
        target = target[key]
    target[keys[-1]] = value
    path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def delete_json(root: Path, relative: str, keys: tuple[str, ...]) -> None:
    path = root / relative
    document = json.loads(path.read_text(encoding="utf-8"))
    target = document
    for key in keys[:-1]:
        target = target[key]
    del target[keys[-1]]
    path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def policy_delete(key: str) -> Callable[[Path], None]:
    return lambda root: delete_json(root, G, ("required_governance_policy", key))


def case_replace(relative: str, old: str, new: str) -> Callable[[Path], None]:
    return lambda root: replace(root, relative, old, new)


POLICY_KEYS = (
    "active_main_ruleset_required",
    "pull_request_required",
    "one_independent_approval_required",
    "canonical_status_checks_required",
    "force_push_blocked_required",
    "branch_deletion_blocked_required",
    "empty_bypass_policy_required",
    "post_merge_exact_head_and_tree_verification_required",
    "current_tree_gate_required",
    "administrator_self_acceptance_for_p_forbidden",
)


def cases() -> list[tuple[str, Callable[[Path], None]]]:
    result: list[tuple[str, Callable[[Path], None]]] = [
        ("tls-ref", case_replace(A, "6cb179509fad97e8be56e31bb930b2a86caefc6a", "0cb179509fad97e8be56e31bb930b2a86caefc6a")),
        ("tls-tree", case_replace(A, "4900fd38d741ab24f643acf211e7d1f807d23792", "0900fd38d741ab24f643acf211e7d1f807d23792")),
        ("tls-archive", case_replace(A, "1066ab44b32451f921f2d3cdd49118471f78b214de7dd848a3273c95e19143b6", "0066ab44b32451f921f2d3cdd49118471f78b214de7dd848a3273c95e19143b6")),
        ("tree-not-identical", case_replace(A, '"accepted_tls_tree_identical_after_merge": true', '"accepted_tls_tree_identical_after_merge": false')),
        ("contract-response-removed", case_replace(C, '"name":"rest_schedule"', '"name":"removed_schedule"')),
        ("contract-http", case_replace(C, '"http_status":200', '"http_status":500')),
        ("contract-hash", case_replace(C, "0fc4494e2f06a9bc8aebb10eb0a7de0500b661c9988a9fdfda526364348ff589", "1fc4494e2f06a9bc8aebb10eb0a7de0500b661c9988a9fdfda526364348ff589")),
        ("material-drift", case_replace(C, '"material_contract_drift": false', '"material_contract_drift": true')),
        ("production-host", case_replace(C, '"production_host": "api.finam.ru"', '"production_host": "example.invalid"')),
        ("place-method", case_replace(C, '"method": "POST"', '"method": "GET"')),
        ("cancel-method", case_replace(C, '"method": "DELETE"', '"method": "GET"')),
        ("retry-open", case_replace(C, '"automatic_retry": false', '"automatic_retry": true')),
        ("build-archive", case_replace(B, "1066ab44b32451f921f2d3cdd49118471f78b214de7dd848a3273c95e19143b6", "2066ab44b32451f921f2d3cdd49118471f78b214de7dd848a3273c95e19143b6")),
        ("executable", case_replace(B, "677f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06", "777f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06")),
        ("one-build", case_replace(B, '"independent_clean_build_count": 2', '"independent_clean_build_count": 1')),
        ("nonreproducible", case_replace(B, '"all_executable_hashes_identical": true', '"all_executable_hashes_identical": false')),
        ("cargo-lock", case_replace(B, "8233fd447ee0d7bc1cc1983960af771f70c8e3b4db53a57fb4ffb453d8c529b6", "9233fd447ee0d7bc1cc1983960af771f70c8e3b4db53a57fb4ffb453d8c529b6")),
        ("rustc-commit", case_replace(B, "59807616e1fa2540724bfbac14d7976d7e4a3860", "09807616e1fa2540724bfbac14d7976d7e4a3860")),
        ("ruleset-disabled", lambda r: mutate_json(r, G, ("ruleset", "enforcement"), "disabled")),
        ("target-branch-removed", lambda r: mutate_json(r, G, ("ruleset", "conditions", "include"), [])),
        ("deletion-rule-removed", lambda r: mutate_json(r, G, ("ruleset", "rule_types"), ["non_fast_forward", "pull_request", "required_status_checks"])),
        ("force-push-rule-removed", lambda r: mutate_json(r, G, ("ruleset", "rule_types"), ["deletion", "pull_request", "required_status_checks"])),
        ("pull-request-rule-removed", lambda r: mutate_json(r, G, ("ruleset", "rule_types"), ["deletion", "non_fast_forward", "required_status_checks"])),
        ("approval-count-zero", lambda r: mutate_json(r, G, ("ruleset", "pull_request", "required_approving_review_count"), 0)),
        ("stale-review-retained", lambda r: mutate_json(r, G, ("ruleset", "pull_request", "dismiss_stale_reviews_on_push"), False)),
        ("last-push-approval-disabled", lambda r: mutate_json(r, G, ("ruleset", "pull_request", "require_last_push_approval"), False)),
        ("review-resolution-disabled", lambda r: mutate_json(r, G, ("ruleset", "pull_request", "required_review_thread_resolution"), False)),
        ("extra-approval-disabled", lambda r: mutate_json(r, G, ("ruleset", "pull_request", "require_extra_approval_for_unattributed_changes"), False)),
        ("squash-merge-opened", lambda r: mutate_json(r, G, ("ruleset", "pull_request", "allowed_merge_methods"), ["merge", "squash"])),
        ("redis-check-removed", lambda r: mutate_json(r, G, ("ruleset", "required_status_checks", "contexts"), ["rust"])),
        ("rust-check-removed", lambda r: mutate_json(r, G, ("ruleset", "required_status_checks", "contexts"), ["redis-smoke"])),
        ("strict-check-policy-disabled", lambda r: mutate_json(r, G, ("ruleset", "required_status_checks", "strict_required_status_checks_policy"), False)),
        ("bypass-added", lambda r: mutate_json(r, G, ("ruleset", "bypass_actors"), [{"actor_id": 1, "actor_type": "RepositoryRole", "bypass_mode": "always"}])),
        ("branch-protection-false", lambda r: mutate_json(r, G, ("branch_protected",), False)),
    ]
    result.extend((f"policy-key-deleted-{key}", policy_delete(key)) for key in POLICY_KEYS)
    result.extend(
        [
            ("unreviewed-policy-key-added", lambda r: mutate_json(r, G, ("required_governance_policy", "unreviewed_bypass"), True)),
            ("checkout-mutable", case_replace(CI, "actions/checkout@11d5960a326750d5838078e36cf38b85af677262", "actions/checkout@v4")),
            ("checkout-pin-drift", case_replace(CI, "actions/checkout@11d5960a326750d5838078e36cf38b85af677262", "actions/checkout@0000000000000000000000000000000000000000")),
            ("rust-action-mutable", case_replace(CI, "dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c", "dtolnay/rust-toolchain@stable")),
            ("rust-action-pin-drift", case_replace(CI, "dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c", "dtolnay/rust-toolchain@0000000000000000000000000000000000000000")),
            ("rust-release-stable", case_replace(CI, "toolchain: 1.95.0", "toolchain: stable")),
            ("gov-self-accept", lambda r: mutate_json(r, G, ("gov_p1_status",), "ACCEPTED")),
            ("open-p", lambda r: mutate_json(r, A, ("closed_surfaces", "stage8b_p"), False)),
            ("next-execution", lambda r: mutate_json(r, A, ("aggregate", "next_allowed_action"), "execute_stage8b_p")),
        ]
    )
    return result


def main() -> None:
    mutations = cases()
    if len(mutations) != 53:
        raise SystemExit(f"stage8b-p-preconditions-negative: FAIL inventory count={len(mutations)}")
    with tempfile.TemporaryDirectory(prefix="stage8b-p-preconditions-negative-") as tmp:
        base = Path(tmp) / "base"
        shutil.copytree(ROOT, base, ignore=shutil.ignore_patterns("target", ".git", "reports", "tmp"))
        for index, (name, mutation) in enumerate(mutations, 1):
            case = Path(tmp) / f"case-{index:02d}"
            shutil.copytree(base, case)
            mutation(case)
            result = subprocess.run(
                ["python3", CHECKER, "--no-git"], cwd=case, text=True, capture_output=True
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-preconditions-negative: FAIL mutation passed: {name}")
            print(f"PASS {index:02d}/53 {name}")
    print("stage8b-p-preconditions-negative: PASS 53/53")


if __name__ == "__main__":
    main()
