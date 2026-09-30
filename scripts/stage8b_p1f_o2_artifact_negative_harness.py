#!/usr/bin/env python3
"""Negative mutations for the Stage 8B-P1F O2 artifact contract."""

from __future__ import annotations

import copy
import json

import stage8b_p1f_o2_artifact_check as check


def main() -> None:
    original = check.load_json(check.ARTIFACT)
    check.validate_document(original, verify_files=True)
    check.validate_files()
    cases = [
        ("execution-open", lambda value: value.__setitem__("execution_authorized", True)),
        ("target-mutated", lambda value: value.__setitem__("target_mutation_performed", True)),
        ("contract-ref", lambda value: value.__setitem__("accepted_contract_ref", "0" * 40)),
        ("implementation-ref", lambda value: value.__setitem__("implementation_ref", "0" * 40)),
        ("implementation-tree", lambda value: value.__setitem__("implementation_tree", "0" * 40)),
        ("profile-id", lambda value: value["runtime_profile"].__setitem__("profile_id", "imoexf-hybrid-high180-paper-v1")),
        ("profile-hash", lambda value: value["runtime_profile"].__setitem__("canonical_sha256", "0" * 64)),
        ("profile-config", lambda value: value["runtime_profile"].__setitem__("runtime_config_fingerprint_sha256", "0" * 64)),
        ("operator-exit", lambda value: value.__setitem__("operator_runner_failure_exit_code", 72)),
        ("binary-duplicate", lambda value: value["build"]["binaries"][1].__setitem__("name", value["build"]["binaries"][0]["name"])),
        ("binary-foreign", lambda value: value["build"]["binaries"][1].__setitem__("name", "foreign")),
        ("bootstrap-runtime-drop", lambda value: value["build"]["binaries"].pop()),
        ("bootstrap-replacement-bypassed", lambda value: value["bootstrap_runtime_prerequisite"].__setitem__("replacement_required", False)),
        ("bootstrap-replacement-authorized", lambda value: value["bootstrap_runtime_prerequisite"].__setitem__("replacement_authorized", True)),
        ("bootstrap-old-hash", lambda value: value["bootstrap_runtime_prerequisite"].__setitem__("sha256", "cee324a4e4f251227a25d4a7b23dda332a94522b45407671982f4fc896614406")),
        ("authority-key", lambda value: value["authority"].__setitem__("public_key_ed25519_hex", "0" * 64)),
        ("private-key", lambda value: value["authority"].__setitem__("private_key_in_artifact", True)),
        ("raw-account", lambda value: value["account_boundary"].__setitem__("broker_account_id_in_artifact", True)),
        ("account-alias", lambda value: value["account_boundary"].__setitem__("broker_neutral_account_alias", "foreign")),
        ("role-drop", lambda value: value["roles"].pop()),
        ("binary-hash", lambda value: value["build"]["binaries"][0].__setitem__("sha256", "")),
        ("build-platform", lambda value: value["build"].__setitem__("platform", "linux/arm64")),
        ("get-write", lambda value: value["get_guard"].__setitem__("write_methods", True)),
        ("get-route", lambda value: value["get_guard"]["route_kinds"].append("orders-create")),
        ("redirect", lambda value: value["get_guard"].__setitem__("redirects", True)),
        ("proxy", lambda value: value["get_guard"].__setitem__("system_proxy", True)),
        ("poll", lambda value: value["supervision"].__setitem__("poll_interval_ms", 500)),
        ("command-unbounded", lambda value: value["supervision"].__setitem__("command_timeout_seconds", 0)),
        ("stopped-proof", lambda value: value["supervision"]["stopped_proof"].pop()),
        ("systemctl-exit", lambda value: value["supervision"].__setitem__("systemctl_exit_is_stopped_proof", True)),
        ("new-grace", lambda value: value["supervision"].__setitem__("restart_grants_new_grace", True)),
        ("terminal-reclassify", lambda value: value["supervision"].__setitem__("pending_terminal_replay", "classify-current-time")),
        ("runner-command", lambda value: value["commands"].__setitem__("runner_start", ["systemctl", "start", "foreign.service"])),
        ("secret-exclusion", lambda value: value["excluded"].remove("raw-finam-account-id")),
        ("redis-open", lambda value: value["closed_surfaces"].__setitem__("operational_redis_mutation", True)),
        ("finam-write-open", lambda value: value["closed_surfaces"].__setitem__("finam_write_or_order_execution", True)),
    ]
    accepted: list[str] = []
    for name, mutate in cases:
        candidate = copy.deepcopy(original)
        mutate(candidate)
        require_changed = json.dumps(candidate, sort_keys=True) != json.dumps(original, sort_keys=True)
        if not require_changed:
            accepted.append(f"{name}:no-op")
            continue
        try:
            check.validate_document(candidate, verify_files=False)
        except (check.ArtifactError, KeyError, TypeError):
            continue
        accepted.append(name)
    if accepted:
        raise SystemExit(f"stage8b-p1f-o2-artifact-negative-harness: FAIL accepted={accepted}")
    print(f"PASS stage8b-p1f-o2-artifact-negative-harness {len(cases)}/{len(cases)}")


if __name__ == "__main__":
    main()
