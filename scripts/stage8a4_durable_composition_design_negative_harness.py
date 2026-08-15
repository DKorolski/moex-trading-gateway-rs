#!/usr/bin/env python3
"""Run Stage 8A-4 durable-composition design R1 negative mutations."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage8a4_durable_composition_design_check as checker


def json_value(path: Path, keys: tuple[str, ...], value: object):
    def mutate(root: Path) -> None:
        target = root / path
        data = json.loads(target.read_text())
        node = data
        for key in keys[:-1]:
            node = node[key]
        node[keys[-1]] = value
        target.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
    return mutate


def json_list_remove(path: Path, key: str, value: str):
    def mutate(root: Path) -> None:
        target = root / path
        data = json.loads(target.read_text())
        data[key].remove(value)
        target.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
    return mutate


def main() -> None:
    a = checker.AUTHORITY
    mutations = [
        ("accepted-ref", json_value(a, ("accepted_reducer_ref",), "0" * 40), None),
        ("review-hash", json_value(a, ("accepted_reducer_review_sha256",), "0" * 64), None),
        ("forged-accepted", json_value(a, ("status",), "accepted"), None),
        ("production-rust-enabled", json_value(a, ("production_rust_changed",), True), None),
        ("diagnostic-authority", json_value(a, ("authoritative_result", "public_diagnostic_is_authority"), True), None),
        ("result-public", json_value(a, ("authoritative_result", "private"), False), None),
        ("caller-constructible", json_value(a, ("authoritative_result", "caller_constructible"), True), None),
        ("partial-identity-merge", json_value(a, ("partial_identity_policy",), "material_merge"), None),
        ("not-found-no-match", json_value(a, ("documented_not_found_proves_no_match",), True), None),
        ("unavailable-no-match", json_value(a, ("unavailable_proves_no_match",), True), None),
        ("proven-no-match", json_value(a, ("proven_no_match_available",), True), None),
        ("unknown-safety-removed", json_list_remove(a, "account_safety_summary", "unknown_status_orders"), None),
        ("orphan-safety-removed", json_list_remove(a, "account_safety_summary", "orphan_orders"), None),
        ("seal-revalidation-removed", json_list_remove(a, "apply_time_revalidation", "current_recovery_seal"), None),
        ("arm-revalidation-removed", json_list_remove(a, "apply_time_revalidation", "operator_arm_generation"), None),
        ("kill-switch-revalidation-removed", json_list_remove(a, "apply_time_revalidation", "kill_switch_state"), None),
        ("hold-advances-lifecycle", json_value(a, ("conflict_or_unknown_advances_order_lifecycle",), True), None),
        ("replay-non-idempotent", json_value(a, ("replay_is_idempotent",), False), None),
        ("ack-before-append", json_value(a, ("ack_after_durable_transition_only",), False), None),
        ("retry-rearm-resend", json_value(a, ("result_grants_retry_rearm_or_resend",), True), None),
        ("implementation-open", json_value(a, ("closed", "durable_composition_implementation"), False), None),
        ("finam-open", json_value(a, ("closed", "finam_post_delete"), False), None),
        ("runtime-live-open", json_value(a, ("closed", "runtime_live"), False), None),
        ("production-path-added", lambda root: None, checker.ALLOWED_CHANGED_PATHS | {"crates/finam-gateway/src/lib.rs"}),
    ]
    if len(mutations) != 24:
        raise SystemExit("stage8a4-durable-composition-design-negative: FAIL inventory")
    with tempfile.TemporaryDirectory(prefix="stage8a4-durable-design-negative-") as raw:
        root = Path(raw) / "repo"
        shutil.copytree(checker.ROOT, root, ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports", "__pycache__"))
        for name, mutate, override in mutations:
            original = (root / a).read_text()
            mutate(root)
            try:
                checker.check(root, git_scope=False, changed_paths_override=override)
            except checker.CheckFailure:
                print(f"PASS {name}")
            else:
                raise SystemExit(f"stage8a4-durable-composition-design-negative: FAIL survived={name}")
            finally:
                (root / a).write_text(original)
    print("stage8a4-durable-composition-design-negative: PASS 24/24")


if __name__ == "__main__":
    main()
