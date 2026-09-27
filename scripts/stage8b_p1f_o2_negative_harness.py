#!/usr/bin/env python3
"""Mutation controls for the Stage 8B-P1-f O2 contract."""

from __future__ import annotations

import copy
import json

import stage8b_p1f_o2_check as check


def rejected(document: dict[str, object], mutation) -> bool:
    candidate = copy.deepcopy(document)
    mutation(candidate)
    try:
        check.validate(candidate)
    except (ValueError, KeyError, TypeError):
        return True
    return False


def main() -> None:
    document = json.loads((check.ROOT / check.CONTRACT).read_text(), object_pairs_hook=check.strict_object)
    cases = [
        ("execute", lambda value: value["package_boundary"].update(execution_authorized=True)),
        ("remote-mutation", lambda value: value["package_boundary"].update(remote_mutation_performed=True)),
        ("redis", lambda value: value["closed_surfaces"].update(redis_db15_or_db0_mutation=True)),
        ("finam-post", lambda value: value["closed_surfaces"].update(finam_post_or_delete=True)),
        ("runtime-live", lambda value: value["closed_surfaces"].update(runtime_live=True)),
        ("freshness", lambda value: value["freshness"].update(broker_truth_max_age_seconds_at_bootstrap=301)),
        ("o2m-redis", lambda value: value["subphases"][0].update(network="FINAM and Redis")),
        ("o2b-network", lambda value: value["subphases"][1].update(network="AF_INET allowed")),
        ("enable", lambda value: value["bootstrap"].update(enable_allowed=True)),
        ("main-start", lambda value: value["bootstrap"].update(ordinary_service_start_allowed=True)),
        ("path", lambda value: value["fixed_paths"].update(config="/tmp/supervisor.json")),
        ("wire", lambda value: value["source_contract"].update(wire_schema_version=1)),
        ("target", lambda value: value["target"].update(ipv4="127.0.0.1")),
        ("facade-drop", lambda value: value["facades_required_in_later_execution_artifact"].pop()),
        ("closed-empty", lambda value: value.update(closed_surfaces={})),
        ("boundary-empty", lambda value: value.update(package_boundary={})),
        ("orders-route-drop", lambda value: value["materializer_network_policy"]["route_allowlist"].pop(1)),
        ("orders-assumed", lambda value: value["materializer_network_policy"].update(complete_orders_truth_may_be_assumed=True)),
        ("write-method", lambda value: value["materializer_network_policy"]["method_allowlist"].append("POST")),
        ("unlisted-route", lambda value: value["materializer_network_policy"].update(unlisted_routes_allowed=True)),
        ("account-unbound", lambda value: value["materializer_network_policy"].update(exact_account_binding="any account")),
        ("blocking-runner", lambda value: value["bootstrap_supervision"].pop("permit_and_unit_poll_interval_ms")),
        ("client-exit-proof", lambda value: value["bootstrap_supervision"].update(systemctl_child_exit_is_unit_stop_proof=True)),
        ("no-runner-cleanup", lambda value: value["bootstrap_supervision"].update(runner_loss_action="none")),
        ("failed-after-deadline", lambda value: value["terminal_outcomes"].update(deadline_reached_or_terminal_recovery_at_or_after_deadline="Failed")),
        ("terminal-retry-open", lambda value: value["terminal_outcomes"].update(new_admission_while_nonterminal_or_pending=True)),
        ("key-ordering", lambda value: value["authority_key_ordering"].update(current_patch_generates_or_reuses_private_key=True)),
        ("control-root-content", lambda value: value["control_root_skeleton"]["permitted_initial_entries"].append("authority")),
    ]
    failures = [name for name, mutation in cases if not rejected(document, mutation)]
    if failures:
        raise SystemExit(f"stage8b-p1f-o2-negative-harness: FAIL accepted={failures}")
    print(f"PASS stage8b-p1f-o2-negative-harness {len(cases)}/{len(cases)}")


if __name__ == "__main__":
    main()
