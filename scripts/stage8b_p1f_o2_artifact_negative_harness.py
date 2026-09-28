#!/usr/bin/env python3
"""Negative mutations for the Stage 8B-P1F O2 artifact contract."""

from __future__ import annotations

import copy
import json

import stage8b_p1f_o2_artifact_check as check


def main() -> None:
    original = check.load_json(check.ARTIFACT)
    cases = [
        ("execution-open", lambda value: value.__setitem__("execution_authorized", True)),
        ("target-mutated", lambda value: value.__setitem__("target_mutation_performed", True)),
        ("contract-ref", lambda value: value.__setitem__("accepted_contract_ref", "0" * 40)),
        ("implementation-ref", lambda value: value.__setitem__("implementation_ref", "0" * 40)),
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
