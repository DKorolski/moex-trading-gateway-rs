#!/usr/bin/env python3
"""Fail-closed checker for the Stage 8B-P1-f R1 design correction."""

from __future__ import annotations

import csv
import hashlib
import json
import subprocess
from pathlib import Path
from typing import Any

import stage8b_p1f_design_check as r0


ROOT = Path(__file__).resolve().parents[1]
BASE = "58bb4cafd3eb80f43c8d0bfd182f7be18ea92d00"
DOCUMENT = r0.DOCUMENT
INVENTORY = r0.INVENTORY
MATRIX = r0.MATRIX
TARGET = r0.TARGET
STATUS = r0.STATUS
ROADMAP = r0.ROADMAP
MODELS = "docs/stage-8/stage8b-p1f-operational-model-fixtures.json"
MATRIX_SHA256 = "a50a68335f52d119be44ee8f88183f239f2780af9f1be3c9d9f47f69d548c61d"
MODELS_SHA256 = "17bca2bb39d4e60b42f32db7bf871e85ada01c8b745b0a40df2b9851f4e5d8c1"

ALLOWED_CHANGES = {
    DOCUMENT,
    INVENTORY,
    MATRIX,
    MODELS,
    STATUS,
    ROADMAP,
    "scripts/make_stage8b_p1f_r1_design_handoff.py",
    "scripts/stage8b_p1f_r1_design_check.py",
    "scripts/stage8b_p1f_r1_design_gate.sh",
    "scripts/stage8b_p1f_r1_design_handoff_safety_check.py",
    "scripts/stage8b_p1f_r1_design_negative_harness.py",
}

TOP_LEVEL_KEYS = {
    "schema_version", "stage", "status", "correction_parent", "accepted_predecessor",
    "target_baseline", "model_fixtures", "isolation", "phases", "freshness_contract",
    "artifact_contracts", "operator_authority", "phase_lifecycle", "session_policy",
    "synthetic_phase", "finam_bars_phase", "publisher_continuity",
    "redis_capabilities", "restart_scenarios", "evidence", "closed_surfaces",
}
ISOLATION = {
    "p1_redis_database": 15,
    "p0_redis_database": 0,
    "p1_namespace": "finam_imoexf_paper:{finam-imoexf-p1}:",
    "p1_service_user": "moex-p1-paper",
    "p1_config_root": "/etc/moex-finam-p1-paper",
    "p1_state_root": "/var/lib/moex-finam-p1-paper/state",
    "p1_binary": "/usr/local/libexec/moex/stage8b-p1-paper-supervisor",
    "p0_services_must_remain_unchanged": True,
    "db0_write_allowed": False,
    "non_loopback_redis_allowed": False,
}
PHASES = [
    {"id":"P1F-I","name":"source implementation","remote_mutation":False,"service_start":False,"requires_prior_acceptance":"P1F-R1-design"},
    {"id":"P1F-O0","name":"immutable read-only target preflight","remote_mutation":False,"service_start":False,"requires_prior_acceptance":"P1F-I-source"},
    {"id":"P1F-O1","name":"non-activating provisioning","remote_mutation":True,"service_start":False,"requires_prior_acceptance":"P1F-O0-preflight"},
    {"id":"P1F-O2","name":"fresh materialization then network-isolated one-shot bootstrap","remote_mutation":True,"service_start":False,"requires_prior_acceptance":"P1F-O1-provisioning"},
    {"id":"P1F-O3","name":"bounded synthetic paper session and restart matrix","remote_mutation":True,"service_start":True,"requires_prior_acceptance":"P1F-O2-bootstrap"},
    {"id":"P1F-O4","name":"bounded read-only FINAM bars paper session","remote_mutation":True,"service_start":True,"requires_prior_acceptance":"P1F-O3-synthetic"},
    {"id":"P1F-A","name":"aggregate operational acceptance","remote_mutation":False,"service_start":False,"requires_prior_acceptance":"P1F-O4-finam-bars"},
]
EVIDENCE_KEYS = {
    "target_preflight", "installed_file_manifest", "systemd_security_and_activation_state",
    "phase_manifest_claim_receipt_and_deadline", "artifact_supply_chain_and_freshness",
    "publisher_sequence_revision_and_consumer_highwater", "redis_role_command_audit",
    "db15_exact_inventory_groups_streams_and_pel", "p0_unit_config_identity_and_no_p1_action",
    "p0_nonprefix_negative_and_legitimate_change_controls", "durable_tree_metadata_and_hashes",
    "health_readiness_and_runtime_state", "m10_command_ack_order_trade_position_and_pel",
    "process_exit_restart_deadline_and_network_endpoints", "resource_growth_and_disk_pressure",
    "redacted_logs_and_no_secret_scan", "pre_and_post_stop_state",
}
CLOSED_SURFACES = r0.CLOSED_SURFACES
ARTIFACT_IDS = [
    "phase-authority-manifest", "fresh-first-boot-source", "synthetic-stage4-observation",
    "finam-stage4-observation", "signed-schedule-envelope", "canonical-synthetic-m10",
    "canonical-finam-m10", "phase-receipt-and-publisher-state",
]
ARTIFACT_KEYS = {
    "id", "producer", "phase_authority", "source_and_provenance", "canonical_identity",
    "freshness", "signer_and_trust", "custody", "allowed_outputs", "restart_policy",
}
ROLE_IDS = [
    "provisioner", "synthetic-m10-feeder", "finam-bars-feeder", "schedule-publisher",
    "supervisor", "read-only-auditor", "phase-guardian", "broker-truth-observer",
]
EXPECTED_ROLES = [
    {"role":"provisioner", "db":15, "allowed":["PING", "TYPE exact-key", "GET deployment-manifest", "SET deployment-manifest NX exact-canonical-bytes", "EVAL pinned-namespace-initialization-script exact-M10-and-command-keys", "XADD exact-nonconsumed-output-stream provisioning-marker once", "XINFO exact-stream-or-group"], "writes":"exact manifest inventory only"},
    {"role":"synthetic-m10-feeder", "db":15, "allowed":["PING", "TYPE canonical-m10", "XADD canonical-m10 exact-payload"], "writes":"canonical M10 stream only"},
    {"role":"finam-bars-feeder", "db":15, "allowed":["PING", "TYPE canonical-m10", "XADD canonical-m10 exact-payload"], "writes":"canonical M10 stream only"},
    {"role":"schedule-publisher", "db":15, "allowed":["PING", "TYPE market-schedule", "XADD market-schedule NOMKSTREAM MAXLEN = 4096 payload"], "writes":"market-schedule stream only"},
    {"role":"supervisor", "db":15, "allowed":["PING", "TYPE exact-key", "GET exact-marker-or-manifest", "SET exact-settlement-marker", "XINFO exact-stream-or-group", "XPENDING exact-M10-group", "XAUTOCLAIM exact-M10-group bounded", "XREADGROUP exact-M10-group bounded", "XRANGE exact-lifecycle-stream bounded", "XREVRANGE market-schedule COUNT 1", "XADD exact-command-lifecycle-telemetry-stream NOMKSTREAM bounded", "EVAL pinned-settlement-script exact-keys", "XACK exact-source-group last"], "writes":"accepted P1 lifecycle only"},
    {"role":"read-only-auditor", "db":"0-and-15-read-only", "allowed":["PING", "INFO persistence-or-memory", "TYPE", "EXISTS", "SCAN bounded", "GET", "XLEN", "XRANGE bounded", "XREVRANGE bounded", "XINFO", "XPENDING", "PTTL", "DUMP for hash only"], "writes":"none"},
    {"role":"phase-guardian", "db":"none", "allowed":[], "writes":"local phase receipt and process control only"},
    {"role":"broker-truth-observer", "db":"none", "allowed":[], "writes":"local exact-response hashes and Stage4 report only"},
]
FORBIDDEN_COMMANDS = [
    "FLUSHALL", "FLUSHDB", "CONFIG", "ACL", "MODULE", "DEBUG", "SHUTDOWN", "SAVE",
    "BGSAVE", "BGREWRITEAOF", "MIGRATE", "RESTORE", "MOVE", "SWAPDB", "RENAME",
    "RENAMENX", "DEL", "UNLINK", "XDEL", "XTRIM",
]
RESTART_IDS = [
    "clean-sigterm", "sigkill-before-semantic-effect", "stale-pel-reclaim",
    "exact-duplicate-idempotence", "conflicting-duplicate-fail-closed",
    "paper-provider-uncertain-outcome-recovery",
]


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(root: Path, relative: str) -> dict[str, Any]:
    try:
        value = json.loads((root / relative).read_text(), object_pairs_hook=strict_object)
    except (OSError, json.JSONDecodeError) as error:
        raise CheckFailure(f"cannot read {relative}: {error}") from error
    require(type(value) is dict, f"{relative} must contain an object")
    return value


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def exact(actual: Any, expected: Any, label: str) -> None:
    require(type(actual) is type(expected), f"{label} type drift")
    if type(expected) is dict:
        require(set(actual) == set(expected), f"{label} key inventory drift")
        for key in expected:
            exact(actual[key], expected[key], f"{label}.{key}")
    elif type(expected) is list:
        require(len(actual) == len(expected), f"{label} length drift")
        for index, item in enumerate(expected):
            exact(actual[index], item, f"{label}[{index}]")
    else:
        require(actual == expected, f"{label} value drift")


def validate_lineage(root: Path) -> None:
    try:
        subprocess.run(["git", "merge-base", "--is-ancestor", BASE, "HEAD"], cwd=root, check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        changed = set(subprocess.check_output(["git", "diff", "--name-only", BASE, "--"], cwd=root, text=True).splitlines())
        changed |= set(subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True).splitlines())
    except (OSError, subprocess.CalledProcessError) as error:
        raise CheckFailure(f"cannot verify P1-f R1 lineage: {error}") from error
    require(changed == ALLOWED_CHANGES, "P1-f R1 changed-path inventory drift")


def validate_inventory(root: Path) -> dict[str, Any]:
    value = read_json(root, INVENTORY)
    require(set(value) == TOP_LEVEL_KEYS, "design top-level key inventory drift")
    require(type(value["schema_version"]) is int and value["schema_version"] == 2, "design schema drift")
    require(value["stage"] == "Stage 8B-P1-f R1 isolated operational acceptance design correction", "stage drift")
    require(value["status"] == "DESIGN_CORRECTION_REVIEW_CANDIDATE_NO_ACTIVATION", "design self-accepted")
    exact(value["correction_parent"], {
        "commit": BASE,
        "tree": "3c771d40df6303a10e19c9ed9ff652894075ac83",
        "review_sha256": "ea5b184ad7c412e63229c8b98bacda895da151c09fc666057dc512968cc24ba5",
    }, "correction parent")
    exact(value["accepted_predecessor"], {
        "commit": "3f171d997de5616cb9a07311d7776e446456c0c1",
        "tree": "12a13b9c3418ec844219f8f9130904896e7a622e",
        "accepted_aggregate_commit": "a9bcd940635b62c2a13f8d378453e6ca21511e30",
        "aggregate_review_sha256": "c5be3e66cadd6c45f6678669afa3f9a194c31f6288aae1c3d29fa59aac47ca54",
    }, "accepted predecessor")
    require(value["target_baseline"] == TARGET and value["model_fixtures"] == MODELS, "input binding drift")

    exact(value["isolation"], ISOLATION, "isolation")
    exact(value["phases"], PHASES, "phases")

    freshness = value["freshness_contract"]
    require(type(freshness) is dict and set(freshness) == {
        "first_boot_truth_max_age_seconds", "schedule_transport_max_age_ms",
        "schedule_cross_source_max_skew_ms", "o2_materialization", "schedule_supply",
    }, "freshness contract key inventory drift")
    require(type(freshness["first_boot_truth_max_age_seconds"]) is int and freshness["first_boot_truth_max_age_seconds"] == 300, "F00 freshness drift")
    require(type(freshness["schedule_transport_max_age_ms"]) is int and freshness["schedule_transport_max_age_ms"] == 5000, "transport freshness drift")
    require(type(freshness["schedule_cross_source_max_skew_ms"]) is int and freshness["schedule_cross_source_max_skew_ms"] == 5000, "cross-source skew drift")
    o2 = freshness["o2_materialization"]
    require(type(o2) is dict and set(o2) == {"subphase", "authority", "network_policy", "output_binding", "bootstrap_subphase", "bootstrap_network_policy", "stale_before_bootstrap", "review_wait_policy"}, "O2 materialization inventory drift")
    require(o2["subphase"] == "P1F-O2-M" and o2["bootstrap_subphase"] == "P1F-O2-B", "O2 subphase drift")
    require("read-only FINAM GET only" in o2["network_policy"] and "no Redis" in o2["network_policy"], "O2 materializer authority drift")
    require("PrivateNetwork" in o2["bootstrap_network_policy"] and "AF_UNIX" in o2["bootstrap_network_policy"], "O2 bootstrap isolation drift")
    require("fail phase without bootstrap mutation" in o2["stale_before_bootstrap"] and "never reused" in o2["review_wait_policy"], "O2 stale policy drift")
    supply = freshness["schedule_supply"]
    require(type(supply) is dict and set(supply) == {"publication_interval_max_ms", "stage4_observation_refresh_max_ms", "missing_or_stale_action", "expired_stage4_action", "synthetic_clock", "committed_v4_recovery"}, "schedule supply inventory drift")
    require(type(supply["publication_interval_max_ms"]) is int and supply["publication_interval_max_ms"] == 2000, "schedule cadence drift")
    require(type(supply["stage4_observation_refresh_max_ms"]) is int and supply["stage4_observation_refresh_max_ms"] == 30000, "Stage4 refresh drift")
    require("PEL retained" in supply["missing_or_stale_action"] and "before callback" in supply["expired_stage4_action"], "stale schedule action drift")
    require("historical continuation only" in supply["committed_v4_recovery"], "committed V4 recovery drift")

    artifacts = value["artifact_contracts"]
    require(type(artifacts) is list and [row.get("id") for row in artifacts] == ARTIFACT_IDS, "artifact inventory drift")
    for row in artifacts:
        require(type(row) is dict and set(row) == ARTIFACT_KEYS, f"artifact schema drift: {row.get('id')}")
        require(all(type(row[key]) is str and row[key] for key in ARTIFACT_KEYS), f"artifact field type drift: {row['id']}")
    by_artifact = {row["id"]: row for row in artifacts}
    require("300 seconds" in by_artifact["fresh-first-boot-source"]["freshness"], "first-boot artifact freshness drift")
    require("no test constructor" in by_artifact["synthetic-stage4-observation"]["restart_policy"], "synthetic production path drift")
    require("separate from bars feeder" in by_artifact["finam-stage4-observation"]["phase_authority"], "O4 observer authority drift")
    require("sequence plus one" in by_artifact["signed-schedule-envelope"]["restart_policy"], "publisher sequence policy drift")

    exact(value["operator_authority"], {
        "one_signed_phase_manifest_per_operational_phase": True,
        "manifest_single_use": True,
        "manifest_binds_source_tree_target_host_phase_deadline_inputs_config_and_installation": True,
        "durable_claim_before_first_effect": True,
        "second_controller_rejected": True,
        "automatic_phase_escalation": False,
        "unattended_activation": False,
    }, "operator authority")
    lifecycle = value["phase_lifecycle"]
    require(type(lifecycle) is dict and set(lifecycle) == {"states", "claim", "deadline", "restart", "terminal_action"}, "phase lifecycle inventory drift")
    exact(lifecycle["states"], ["Unclaimed", "Active", "Stopping", "Completed", "Failed", "Expired"], "phase states")
    require("fsync file and parent directory" in lifecycle["claim"]["operation"], "durable claim drift")
    exact(lifecycle["claim"]["receipt_binds"], ["manifest_sha256", "source_tree", "target_host_key", "phase_id", "input_sha256", "config_sha256", "installation_manifest_sha256", "started_at_utc", "deadline_utc", "controller_id"], "claim receipt bindings")
    require(lifecycle["claim"]["second_controller"] == "reject without effect", "second controller policy drift")
    deadline = lifecycle["deadline"]
    require(type(deadline["restart_extends_deadline"]) is bool and deadline["restart_extends_deadline"] is False, "deadline extension opened")
    require(deadline["synthetic_seconds"] == 1800 and deadline["finam_bars_seconds"] == 10800 and deadline["shutdown_grace_seconds"] == 30, "deadline bounds drift")
    require(deadline["local_enforcer_required"] is True and deadline["independent_of_ssh"] is True, "local deadline enforcement drift")
    exact(lifecycle["restart"], {"allowed_state":"Active", "requires_exact_receipt_and_identities":True, "before_deadline_only":True, "uses_original_deadline":True, "terminal_state_autorestart":False}, "restart authority")
    terminal = lifecycle["terminal_action"]
    require(terminal["retain_source_pel"] is True and terminal["retain_db15_and_durable_evidence"] is True, "terminal retention drift")
    require("never XACK" in terminal["telemetry_failure"], "telemetry failure authority drift")

    exact(value["session_policy"], {"readiness":"PaperReady", "live_ready_forbidden":True, "paper_only_required":True, "synthetic_session_max_minutes":30, "finam_bars_session_max_minutes":180, "stop_and_disable_after_each_session":True, "retain_db15_and_durable_state_for_review":True}, "session policy")
    exact(value["synthetic_phase"], {"test_only_constructors_allowed":False, "production_verification_path_required":True, "fixture_digest_bound_to_manifest":True, "schedule_and_m10_clock_contract_shared":True, "publisher_refresh_continues_for_entire_session":True}, "synthetic phase")
    exact(value["finam_bars_phase"], {"requires_separate_acceptance_after_synthetic":True, "bars_feeder_market_data_only":True, "broker_truth_observer_is_separate_role":True, "broker_truth_observer_get_only":True, "read_only_tokens_required":True, "finam_order_http_allowed":False, "command_consumer_allowed":False, "broker_dispatch_allowed":False, "main_supervisor_has_finam_dependency":False}, "FINAM bars phase")
    exact(value["publisher_continuity"], {"source_generation":"1", "schedule_key_generation":2, "o3_to_o4_uses_same_durable_high_water":True, "o4_requires_new_phase_manifest":True, "publication_sequence_strictly_increases":True, "semantic_revision_changes_only_with_semantic_identity":True, "consumer_v4_progression_retained":True, "missing_or_conflicting_state":"fail closed; no first-publication reset"}, "publisher continuity")

    redis = value["redis_capabilities"]
    require(type(redis) is dict and set(redis) == {"enforcement", "mutating_roles_database", "mutating_roles_key_pattern", "global_forbidden_commands", "roles", "p0_protection_evidence", "resource_limits"}, "Redis capability inventory drift")
    require("typed role adapters" in redis["enforcement"] and "no raw Redis connection escapes" in redis["enforcement"], "Redis enforcement drift")
    require(type(redis["mutating_roles_database"]) is int and redis["mutating_roles_database"] == 15, "mutating Redis DB drift")
    require(redis["mutating_roles_key_pattern"] == "finam_imoexf_paper:{finam-imoexf-p1}:*", "mutating key pattern drift")
    exact(redis["global_forbidden_commands"], FORBIDDEN_COMMANDS, "forbidden Redis commands")
    roles = redis["roles"]
    require(type(roles) is list and [row.get("role") for row in roles] == ROLE_IDS, "Redis role inventory drift")
    for row in roles:
        require(type(row) is dict and set(row) == {"role", "db", "allowed", "writes"}, f"Redis role schema drift: {row.get('role')}")
        require(type(row["allowed"]) is list and all(type(item) is str for item in row["allowed"]), f"Redis role commands drift: {row['role']}")
    exact(roles, EXPECTED_ROLES, "Redis roles")
    by_role = {row["role"]: row for row in roles}
    require(all(by_role[name]["db"] == 15 for name in ("provisioner", "synthetic-m10-feeder", "finam-bars-feeder", "schedule-publisher", "supervisor")), "mutating role DB drift")
    require(by_role["phase-guardian"]["allowed"] == [] and by_role["broker-truth-observer"]["allowed"] == [], "non-Redis role gained command")
    require(any("XACK exact-source-group last" == command for command in by_role["supervisor"]["allowed"]), "XACK-last capability drift")
    require(not any(command.split()[0] in FORBIDDEN_COMMANDS for row in roles for command in row["allowed"]), "forbidden command granted")
    exact(redis["p0_protection_evidence"], {"unit_and_config_hashes_before_after":True, "p1_initiated_p0_service_actions_must_be_zero":True, "command_audit_proves_no_db0_write":True, "existing_nonprefix_p0_key_mutation_negative_control":True, "legitimate_p0_db0_change_positive_control":True, "full_db0_byte_equality_required":False}, "P0 protection evidence")
    exact(redis["resource_limits"], {"telemetry_and_schedule_stream_maxlen":4096, "total_pel_fail_stop_threshold":64, "db15_evidence_budget_bytes":536870912, "minimum_root_free_bytes":10737418240, "resource_poll_interval_seconds":5, "limit_action":"stop P1 phase only; never alter Redis process config or P0"}, "resource limits")

    restarts = value["restart_scenarios"]
    require(type(restarts) is list and [row.get("id") for row in restarts] == RESTART_IDS, "restart scenario inventory drift")
    for row in restarts:
        require(type(row) is dict and set(row) == {"id", "start_frontier", "fault_trigger", "expected_exit", "counter_delta", "pel_final", "checkpoint"}, f"restart schema drift: {row.get('id')}")
        require(all(type(item) is str and item for item in row.values()), f"restart field drift: {row['id']}")
    conflict = {row["id"]: row for row in restarts}["conflicting-duplicate-fail-closed"]
    require("zero" in conflict["counter_delta"] and "pending" in conflict["pel_final"], "conflicting duplicate boundary drift")

    evidence = value["evidence"]
    require(type(evidence) is dict and set(evidence) == EVIDENCE_KEYS, "evidence key inventory drift")
    require(all(type(flag) is bool and flag is True for flag in evidence.values()), "evidence strict boolean drift")
    surfaces = value["closed_surfaces"]
    require(type(surfaces) is dict and set(surfaces) == CLOSED_SURFACES, "closed surface inventory drift")
    require(all(type(flag) is bool and flag is False for flag in surfaces.values()), "closed surface opened")
    return value


def validate_models(root: Path) -> None:
    require(sha256(root / MODELS) == MODELS_SHA256, "model fixture bytes drift")
    value = read_json(root, MODELS)
    require(set(value) == {"schema_version", "domain", "status", "cases"}, "model top-level inventory drift")
    require(type(value["schema_version"]) is int and value["schema_version"] == 1, "model schema drift")
    require(value["domain"] == "moex.stage8b.p1f.operational-model-fixtures.v1" and value["status"] == "DESIGN_MODEL_ONLY_NO_OPERATION", "model identity drift")
    cases = value["cases"]
    require(type(cases) is list and [row.get("id") for row in cases] == [f"P1F-M{index:02}" for index in range(1, 21)], "model case inventory drift")
    for row in cases:
        require(type(row) is dict and set(row) == {"id", "area", "given", "event", "expected"}, f"model case schema drift: {row.get('id')}")
        require(all(type(item) is str and item for item in row.values()), f"model case field drift: {row['id']}")
    expected = {row["id"]: row["expected"] for row in cases}
    require("zero bootstrap mutation" in expected["P1F-M02"], "301-second model drift")
    require("no new fresh schedule admission" in expected["P1F-M08"], "historical continuation model drift")
    require("loser rejected before effect" in expected["P1F-M09"], "two-controller model drift")
    require("original deadline" in expected["P1F-M13"] and "no child start" in expected["P1F-M14"], "restart deadline model drift")
    require("zero telemetry-driven XACK" in expected["P1F-M16"], "telemetry model drift")
    require("no false incident" in expected["P1F-M18"], "legitimate P0 change model drift")


def validate_matrix(root: Path) -> None:
    require(sha256(root / MATRIX) == MATRIX_SHA256, "acceptance matrix bytes drift")
    try:
        with (root / MATRIX).open(newline="") as handle:
            rows = list(csv.DictReader(handle))
    except OSError as error:
        raise CheckFailure(f"cannot read acceptance matrix: {error}") from error
    require(rows and list(rows[0]) == ["id", "area", "requirement", "status"], "matrix header drift")
    require([row["id"] for row in rows] == [f"P1F-R1-{index:03}" for index in range(1, 65)], "matrix row inventory drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "optional acceptance row")


def validate_documents(root: Path) -> None:
    document = (root / DOCUMENT).read_text()
    status = (root / STATUS).read_text()
    roadmap = (root / ROADMAP).read_text()
    for fragment in (
        "DESIGN CORRECTION REVIEW CANDIDATE — NO ACTIVATION", BASE,
        "O1 never prepares reusable \"fresh\" F00 truth", "P1F-O2-M", "P1F-O2-B",
        "Unclaimed -> Active -> Stopping -> Completed | Failed | Expired",
        "never extends it", "no raw Redis", "whole-DB equality",
        "opens only `P1F-I`", "not completed operational scenarios",
    ):
        require(fragment in document, f"design document fragment missing: {fragment}")
    require("active P1-f R1 design-correction candidate" in status, "current status R1 boundary missing")
    require("grants no operational authority" in status, "status opened operation")
    require("P1-f R0 was held" in roadmap and "P1F-I remains closed" in roadmap, "roadmap hold boundary missing")
    require("absolute deadline independent of SSH" in roadmap, "roadmap deadline boundary missing")


def validate(root: Path = ROOT, *, verify_lineage: bool = True) -> None:
    if verify_lineage:
        validate_lineage(root)
    r0.validate_target(root)
    validate_inventory(root)
    validate_models(root)
    validate_matrix(root)
    validate_documents(root)


def main() -> int:
    try:
        validate()
    except (CheckFailure, r0.CheckFailure, OSError, UnicodeDecodeError) as error:
        print(f"stage8b-p1f-r1-design-check: FAIL {error}")
        return 1
    print("stage8b-p1f-r1-design-check: PASS rows=64 models=20 artifacts=8 roles=8 phases=7 activation=false")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
