#!/usr/bin/env python3
"""Fail-closed checker for the Stage 8B-P1-f R2 design correction."""

from __future__ import annotations

import csv
import hashlib
import json
import re
import subprocess
from pathlib import Path
from typing import Any

import stage8b_p1f_r1_design_check as r1


ROOT = Path(__file__).resolve().parents[1]
BASE = "8feedfb3e1d6e4d0f24148abdbbb25bf8d90b0ed"
BASE_TREE = "4e03628f0bf5c732c7529f3fa64925a2934a8430"
REVIEW_SHA256 = "163f57e6f524356b5ca57c645b0a28f5c334d369c83a31c75a7d2d910df2ad82"
DOCUMENT = r1.DOCUMENT
INVENTORY = r1.INVENTORY
MATRIX = r1.MATRIX
TARGET = r1.TARGET
STATUS = r1.STATUS
ROADMAP = r1.ROADMAP
MODELS = r1.MODELS
MATRIX_SHA256 = "c9fb65c47f842110e987575e84a1e835f7242a911d88bfccc941f492996aaad7"
MODELS_SHA256 = "28d54f4e1595c8bc32ab2b91f0715214b780f4befee82f97ad90a649466e6ed7"

ALLOWED_CHANGES = {
    DOCUMENT, INVENTORY, MATRIX, MODELS, STATUS, ROADMAP,
    "scripts/make_stage8b_p1f_r2_design_handoff.py",
    "scripts/stage8b_p1f_r2_design_check.py",
    "scripts/stage8b_p1f_r2_design_gate.sh",
    "scripts/stage8b_p1f_r2_design_handoff_safety_check.py",
    "scripts/stage8b_p1f_r2_design_negative_harness.py",
}
TOP_LEVEL_KEYS = r1.TOP_LEVEL_KEYS
ARTIFACT_IDS = [
    "phase-authority-manifest", "fresh-first-boot-source",
    "o2-materialized-set-receipt", "synthetic-stage4-observation",
    "finam-stage4-observation", "signed-schedule-envelope",
    "canonical-synthetic-m10", "canonical-finam-m10",
    "phase-receipt-and-publisher-state",
]
SCRIPT_IDS = [
    "namespace-initialization-v1", "namespace-verify-v1", "m10-publication-v1",
    "command-publication-v1", "command-publication-revalidate-v1",
    "p1d4-command-publication-v1", "p1d4-command-publication-revalidate-v1",
    "atomic-stale-consumer-delete-v1",
]
SCRIPT_HASHES = {
    "namespace-initialization-v1": "c7db9e1660bb6519ab7502b63cc695ce6d3a5f453be248b4aaad4ddbc614dd3e",
    "namespace-verify-v1": "de29c3ca22ec8eb16925a48ada22bf1dd4fd5b6bd7fa58a0438000a0923e7d49",
    "m10-publication-v1": "ecf60a82c8efdb92c2b4a13a86cd607dee46a216bdbf3c7207ea3208989baf54",
    "command-publication-v1": "0c5b4d4cfbbe8615ed863e00bfdd3a3325e028f9fba78ee1bf2e5bb819346ce1",
    "command-publication-revalidate-v1": "f0c37828d3057d77243506c2eee21e9c12f8e67a8489c42071580ba378f71993",
    "p1d4-command-publication-v1": "ec80534b837877db2dc992ffda1803f47ad8569c966d399edc3265fbc21b51c6",
    "p1d4-command-publication-revalidate-v1": "f15f0f0ef1ad1397cceaf1158e421f49bbdd8d0ad956a97ae3fe46c947108358",
    "atomic-stale-consumer-delete-v1": "cfabc24e0563c490e9950e250fdb146f992403394d48953a1b5eb2a2ffd09b6b",
}
SOURCE_CONSTANTS = {
    "namespace-initialization-v1": "NAMESPACE_INITIALIZATION_LUA",
    "namespace-verify-v1": "NAMESPACE_VERIFY_LUA",
    "m10-publication-v1": "M10_PUBLICATION_LUA",
    "command-publication-v1": "COMMAND_PUBLICATION_LUA",
    "command-publication-revalidate-v1": "COMMAND_PUBLICATION_REVALIDATE_LUA",
    "p1d4-command-publication-v1": "P1D4_COMMAND_PUBLICATION_LUA",
    "p1d4-command-publication-revalidate-v1": "P1D4_COMMAND_PUBLICATION_REVALIDATE_LUA",
}
OPERATION_IDS = [
    "fresh-namespace", "verify-only-attach", "m10-publish", "retention-admission",
    "schedule-read", "stale-consumer-discovery", "stale-consumer-cleanup",
    "source-acquire-and-reclaim", "command-publication", "source-xack-last",
]
ROUTES = [
    ("PlainMarket", "CommandPublishedMarket"),
    ("GeneratedMarket", "CommandPublishedGeneratedMarket with reservation binding"),
    ("InitialLimit", "CommandPublishedInitialLimit"),
    ("Cancel", "CommandPublishedCancel"),
    ("ReadyWorkingLimit", "RoutedContinuation ReadyWorkingLimit"),
    ("ReadyDayExpiry", "source-free ReadyDayExpiry"),
]
REDIS_CONTRACT_DIGESTS = {
    "scripts": "4e152ea7cae84985c9821869f1bdecd57c754659a33d97503335f8c261e91ba0",
    "source_operations": "01e8f8d9aa86dd881239a1eaf63d21b141aa9a10e306f846af81cd71ab27a19d",
    "schedule_route_frontiers": "2b8d5e0290ba3f3c6d2b0ad822a4c2ee5f18dce6c9bac935ba78320e31c2c8e8",
    "roles": "1e7d73011415aca1d86e43a381e958646d104bfe2da569e8311774bbba90dd17",
}
O2_CONTRACT_DIGEST = "efc96095f42deeae7a09cd0164f78d0972d9085f0a97b6aad1d63f56464aa0cd"
SCHEDULE_SUPPLY_DIGEST = "1afdc8e356d52a25687728e036a9bf895ebfceb795ec4bac0a202350c5e2b43c"


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


def canonical_digest(value: Any) -> str:
    raw = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(raw).hexdigest()


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
        subprocess.run(["git", "merge-base", "--is-ancestor", BASE, "HEAD"], cwd=root,
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        changed = set(subprocess.check_output(
            ["git", "diff", "--name-only", BASE, "--"], cwd=root, text=True).splitlines())
        changed |= set(subprocess.check_output(
            ["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True).splitlines())
    except (OSError, subprocess.CalledProcessError) as error:
        raise CheckFailure(f"cannot verify P1-f R2 lineage: {error}") from error
    require(changed == ALLOWED_CHANGES, "P1-f R2 changed-path inventory drift")


def validate_o2(value: dict[str, Any]) -> None:
    freshness = value["freshness_contract"]
    require(type(freshness) is dict and set(freshness) == {
        "first_boot_truth_max_age_seconds", "schedule_transport_max_age_ms",
        "schedule_cross_source_max_skew_ms", "o2_materialization", "schedule_supply",
    }, "freshness contract key inventory drift")
    require(freshness["first_boot_truth_max_age_seconds"] == 300
            and type(freshness["first_boot_truth_max_age_seconds"]) is int,
            "F00 freshness drift")
    o2 = freshness["o2_materialization"]
    require(type(o2) is dict and set(o2) == {
        "subphase", "authority", "network_policy", "output_binding", "bootstrap_subphase",
        "bootstrap_network_policy", "materialization_states", "signed_policy",
        "identity_chain", "source_custody", "final_identity", "bootstrap_admission",
        "stale_before_bootstrap", "review_wait_policy", "new_admission_recovery",
        "historical_v5_recovery", "administrative_v5_recovery",
    }, "O2 materialization inventory drift")
    exact(o2["materialization_states"],
          ["Claimed", "SourceCommitted", "ConfigCommitted", "ReadyForBootstrap"],
          "O2 materialization states")
    exact(o2["signed_policy"], {
        "domain": "moex.stage8b.p1f.o2-materialization-policy.v1",
        "binds": ["source_tree", "target_host_key", "phase_id",
                  "installation_manifest_sha256", "config_template_sha256",
                  "immutable_config_fields_sha256", "source_path", "config_path",
                  "receipt_path"],
        "only_finalizable_config_field": "first_boot_source_bundle_sha256",
        "finalizer": "canonical supervisor JSON with source sha256 substituted into the signed template",
    }, "O2 signed policy")
    chain = o2["identity_chain"]
    require(type(chain) is list and [row.get("state") for row in chain] == o2["materialization_states"],
            "O2 identity chain drift")
    require(all(type(row) is dict and set(row) == {"state", "path", "identity", "commit"}
                and all(type(item) is str and item for item in row.values()) for row in chain),
            "O2 identity row schema drift")
    require(chain[1]["path"] == "/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json",
            "O2 source path drift")
    require(chain[2]["path"] == "/etc/moex-finam-p1-paper/supervisor.json",
            "O2 config path drift")
    require("root:moex-p1-paper 0440" in o2["source_custody"]
            and "0750" in o2["source_custody"], "O2 nonroot custody drift")
    require("ReadyForBootstrap" in o2["bootstrap_admission"]
            and "cross-validation" in o2["bootstrap_admission"], "O2 admission drift")
    require("deterministic incomplete materialization" in o2["new_admission_recovery"]
            and "never bootstrap" in o2["new_admission_recovery"], "O2 partial recovery drift")
    require("V5 marker" in o2["historical_v5_recovery"]
            and "schedule V4" in o2["historical_v5_recovery"], "V5 historical recovery drift")
    require("never read F00" in o2["administrative_v5_recovery"],
            "V5 administrative recovery drift")
    supply = freshness["schedule_supply"]
    require(type(supply) is dict and set(supply) == {
        "publication_interval_max_ms", "stage4_observation_refresh_max_ms",
        "missing_or_stale_action", "expired_stage4_action", "synthetic_clock",
        "committed_v4_recovery",
    }, "schedule supply inventory drift")
    require(supply["publication_interval_max_ms"] == 2000
            and supply["stage4_observation_refresh_max_ms"] == 30000,
            "schedule cadence drift")
    require("route-specific" in supply["missing_or_stale_action"]
            and "existing source PEL" in supply["missing_or_stale_action"],
            "schedule failure disposition drift")
    require("distinct from V5" in supply["committed_v4_recovery"],
            "V4/V5 recovery conflated")
    require(canonical_digest(o2) == O2_CONTRACT_DIGEST,
            "O2 materialization contract digest drift")
    require(canonical_digest(supply) == SCHEDULE_SUPPLY_DIGEST,
            "schedule supply contract digest drift")


def rust_script_hashes(root: Path) -> dict[str, str]:
    text = (root / "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs").read_text()
    hashes: dict[str, str] = {}
    for identifier, constant in SOURCE_CONSTANTS.items():
        match = re.search(rf'const {constant}: &str = r#"(.*?)"#;', text, re.DOTALL)
        require(match is not None, f"source script missing: {constant}")
        hashes[identifier] = hashlib.sha256(match.group(1).encode()).hexdigest()
    stale = root / "docs/stage-8/stage8b-p1e-atomic-stale-consumer-delete-v1.lua"
    hashes["atomic-stale-consumer-delete-v1"] = sha256(stale)
    return hashes


def validate_redis(root: Path, value: dict[str, Any]) -> None:
    redis = value["redis_capabilities"]
    require(type(redis) is dict and set(redis) == {
        "enforcement", "mutating_roles_database", "mutating_roles_key_pattern",
        "global_forbidden_commands", "scripts", "source_operations",
        "schedule_route_frontiers", "roles", "p0_protection_evidence", "resource_limits",
    }, "Redis capability inventory drift")
    require(redis["mutating_roles_database"] == 15
            and redis["mutating_roles_key_pattern"] == "finam_imoexf_paper:{finam-imoexf-p1}:*",
            "Redis DB/key boundary drift")
    for key, expected_digest in REDIS_CONTRACT_DIGESTS.items():
        require(canonical_digest(redis[key]) == expected_digest,
                f"Redis {key} contract digest drift")
    exact(redis["global_forbidden_commands"], r1.FORBIDDEN_COMMANDS,
          "forbidden Redis commands")
    scripts = redis["scripts"]
    require(type(scripts) is list and [row.get("id") for row in scripts] == SCRIPT_IDS,
            "Redis script inventory drift")
    for row in scripts:
        require(type(row) is dict and set(row) == {
            "id", "sha256", "keys", "argv", "nested_commands", "effect"},
            f"Redis script schema drift: {row.get('id')}")
        require(type(row["nested_commands"]) is list
                and all(type(item) is str and item for item in row["nested_commands"]),
                f"Redis script nested command drift: {row['id']}")
        require(row["sha256"] == SCRIPT_HASHES[row["id"]],
                f"Redis script hash drift: {row['id']}")
    exact(rust_script_hashes(root), SCRIPT_HASHES, "production Redis script hashes")
    operations = redis["source_operations"]
    require(type(operations) is list and [row.get("id") for row in operations] == OPERATION_IDS,
            "source operation inventory drift")
    for row in operations:
        require(type(row) is dict and set(row) == {
            "id", "role", "source", "command", "response_loss"},
            f"source operation schema drift: {row.get('id')}")
        require(all(type(item) is str and item for item in row.values()),
                f"source operation field drift: {row['id']}")
    by_operation = {row["id"]: row for row in operations}
    require("COUNT 64" in by_operation["schedule-read"]["command"],
            "schedule COUNT 64 drift")
    require("XLEN" in by_operation["retention-admission"]["command"],
            "retention probe drift")
    require("equal payload" in by_operation["m10-publish"]["response_loss"],
            "M10 response-loss drift")
    routes = redis["schedule_route_frontiers"]
    require(type(routes) is list and [(row.get("route"), row.get("start_frontier"))
                                      for row in routes] == ROUTES,
            "schedule route frontier drift")
    for row in routes:
        require(type(row) is dict and set(row) == {
            "route", "start_frontier", "allowed_predecessor_effects", "failure_disposition"},
            f"schedule route schema drift: {row.get('route')}")
        require("retain" in row["failure_disposition"].lower()
                and "XACK" in row["failure_disposition"],
                f"schedule route fail-closed drift: {row['route']}")
    roles = redis["roles"]
    require(type(roles) is list and [row.get("role") for row in roles] == r1.ROLE_IDS,
            "Redis role inventory drift")
    for row in roles:
        require(type(row) is dict and set(row) == {"role", "db", "allowed", "writes"},
                f"Redis role schema drift: {row.get('role')}")
        require(type(row["allowed"]) is list
                and all(type(item) is str for item in row["allowed"]),
                f"Redis role command drift: {row['role']}")
    by_role = {row["role"]: row for row in roles}
    for role in ("synthetic-m10-feeder", "finam-bars-feeder"):
        exact(by_role[role]["allowed"], [
            "PING", "EVAL namespace-verify-v1 exact-keys-argv",
            "EVAL m10-publication-v1 exact-key-argv",
            "XRANGE canonical-m10 exact-id exact-id", "XLEN canonical-m10",
        ], f"{role} capabilities")
    supervisor = by_role["supervisor"]["allowed"]
    require("XREVRANGE market-schedule + - COUNT 64" in supervisor,
            "supervisor schedule capability drift")
    require("XLEN canonical-m10" in supervisor, "supervisor XLEN capability missing")
    require("EVAL namespace-verify-v1 exact-keys-argv" in supervisor,
            "verify-only attach capability missing")
    require("EVAL atomic-stale-consumer-delete-v1 exact-key-argv" in supervisor,
            "stale cleanup capability missing")
    require(not any(item == "EVAL" or "arbitrary" in item for row in roles
                    for item in row["allowed"]), "arbitrary EVAL capability opened")
    exact(redis["p0_protection_evidence"], {
        "unit_and_config_hashes_before_after": True,
        "p1_initiated_p0_service_actions_must_be_zero": True,
        "command_audit_proves_no_db0_write": True,
        "existing_nonprefix_p0_key_mutation_negative_control": True,
        "legitimate_p0_db0_change_positive_control": True,
        "full_db0_byte_equality_required": False,
    }, "P0 protection evidence")
    exact(redis["resource_limits"], {
        "telemetry_and_schedule_stream_maxlen": 4096,
        "total_pel_fail_stop_threshold": 64,
        "db15_evidence_budget_bytes": 536870912,
        "minimum_root_free_bytes": 10737418240,
        "resource_poll_interval_seconds": 5,
        "limit_action": "stop P1 phase only; never alter Redis process config or P0",
    }, "resource limits")


def validate_inventory(root: Path) -> dict[str, Any]:
    value = read_json(root, INVENTORY)
    require(set(value) == TOP_LEVEL_KEYS, "design top-level key inventory drift")
    require(type(value["schema_version"]) is int and value["schema_version"] == 3,
            "design schema drift")
    require(value["stage"] == "Stage 8B-P1-f R2 isolated operational acceptance design correction",
            "stage drift")
    require(value["status"] == "DESIGN_CORRECTION_REVIEW_CANDIDATE_NO_ACTIVATION",
            "design self-accepted")
    exact(value["correction_parent"], {
        "commit": BASE, "tree": BASE_TREE, "review_sha256": REVIEW_SHA256,
    }, "correction parent")
    exact(value["accepted_predecessor"], {
        "commit": "3f171d997de5616cb9a07311d7776e446456c0c1",
        "tree": "12a13b9c3418ec844219f8f9130904896e7a622e",
        "accepted_aggregate_commit": "a9bcd940635b62c2a13f8d378453e6ca21511e30",
        "aggregate_review_sha256": "c5be3e66cadd6c45f6678669afa3f9a194c31f6288aae1c3d29fa59aac47ca54",
    }, "accepted predecessor")
    exact(value["isolation"], r1.ISOLATION, "isolation")
    phases = list(r1.PHASES)
    phases[0] = dict(phases[0], requires_prior_acceptance="P1F-R2-design")
    exact(value["phases"], phases, "phases")
    validate_o2(value)
    artifacts = value["artifact_contracts"]
    require(type(artifacts) is list and [row.get("id") for row in artifacts] == ARTIFACT_IDS,
            "artifact inventory drift")
    for row in artifacts:
        require(type(row) is dict and set(row) == r1.ARTIFACT_KEYS,
                f"artifact schema drift: {row.get('id')}")
        require(all(type(row[key]) is str and row[key] for key in r1.ARTIFACT_KEYS),
                f"artifact field type drift: {row['id']}")
    require("root:moex-p1-paper 0440" in artifacts[1]["custody"],
            "first-boot source custody drift")
    require("ReadyForBootstrap" in artifacts[2]["allowed_outputs"],
            "materialized receipt output drift")
    exact(value["operator_authority"], {
        "one_signed_phase_manifest_per_operational_phase": True,
        "manifest_single_use": True,
        "manifest_binds_source_tree_target_host_phase_deadline_policy_template_and_installation": True,
        "o2_manifest_binds_policy_template_not_dynamic_source_or_final_config": True,
        "o2_materialized_receipt_binds_source_final_config_and_claim": True,
        "durable_claim_before_first_effect": True,
        "second_controller_rejected": True,
        "automatic_phase_escalation": False,
        "unattended_activation": False,
    }, "operator authority")
    lifecycle = value["phase_lifecycle"]
    exact(lifecycle, {
        "states": ["Unclaimed", "Active", "Stopping", "Completed", "Failed", "Expired"],
        "claim": {
            "operation": "create-new receipt then fsync file and parent directory before first effect",
            "receipt_binds": ["manifest_sha256", "source_tree", "target_host_key", "phase_id",
                              "materialization_policy_sha256", "config_template_sha256",
                              "installation_manifest_sha256", "started_at_utc", "deadline_utc",
                              "controller_id"],
            "crash_after_claim": "same local phase guardian may resume Active receipt before original deadline",
            "second_controller": "reject without effect",
        },
        "deadline": {
            "starts_at": "durable Active claim", "restart_extends_deadline": False,
            "synthetic_seconds": 1800, "finam_bars_seconds": 10800,
            "shutdown_grace_seconds": 30, "local_enforcer_required": True,
            "independent_of_ssh": True,
            "clock_rollback_or_untrusted_time": "fail closed to Expired",
            "reboot_policy": "no child autostart; expire stale Active receipt before any manual continuation",
        },
        "restart": {"allowed_state": "Active", "requires_exact_receipt_and_identities": True,
                    "before_deadline_only": True, "uses_original_deadline": True,
                    "terminal_state_autorestart": False},
        "terminal_action": {
            "process_order": ["market-data feeder", "broker-truth observer",
                              "schedule publisher", "signer", "supervisor"],
            "force_kill_after_grace": True, "retain_source_pel": True,
            "retain_db15_and_durable_evidence": True,
            "telemetry_failure": "persist local terminal receipt then stop; never XACK to report telemetry",
        },
    }, "phase lifecycle")
    exact(value["session_policy"], {
        "readiness":"PaperReady", "live_ready_forbidden":True, "paper_only_required":True,
        "synthetic_session_max_minutes":30, "finam_bars_session_max_minutes":180,
        "stop_and_disable_after_each_session":True,
        "retain_db15_and_durable_state_for_review":True,
    }, "session policy")
    exact(value["synthetic_phase"], {
        "test_only_constructors_allowed":False, "production_verification_path_required":True,
        "fixture_digest_bound_to_manifest":True, "schedule_and_m10_clock_contract_shared":True,
        "publisher_refresh_continues_for_entire_session":True,
    }, "synthetic phase")
    exact(value["finam_bars_phase"], {
        "requires_separate_acceptance_after_synthetic":True, "bars_feeder_market_data_only":True,
        "broker_truth_observer_is_separate_role":True, "broker_truth_observer_get_only":True,
        "read_only_tokens_required":True, "finam_order_http_allowed":False,
        "command_consumer_allowed":False, "broker_dispatch_allowed":False,
        "main_supervisor_has_finam_dependency":False,
    }, "FINAM bars phase")
    exact(value["publisher_continuity"], {
        "source_generation":"1", "schedule_key_generation":2,
        "o3_to_o4_uses_same_durable_high_water":True, "o4_requires_new_phase_manifest":True,
        "publication_sequence_strictly_increases":True,
        "semantic_revision_changes_only_with_semantic_identity":True,
        "consumer_v4_progression_retained":True,
        "missing_or_conflicting_state":"fail closed; no first-publication reset",
    }, "publisher continuity")
    validate_redis(root, value)
    restarts = value["restart_scenarios"]
    require(type(restarts) is list and [row.get("id") for row in restarts] == r1.RESTART_IDS,
            "restart scenario inventory drift")
    for row in restarts:
        require(type(row) is dict and set(row) == {
            "id", "start_frontier", "fault_trigger", "expected_exit", "counter_delta",
            "pel_final", "checkpoint"}, f"restart schema drift: {row.get('id')}")
    require(set(value["evidence"]) == r1.EVIDENCE_KEYS
            and all(type(flag) is bool and flag for flag in value["evidence"].values()),
            "evidence strict inventory drift")
    require(set(value["closed_surfaces"]) == r1.CLOSED_SURFACES
            and all(type(flag) is bool and flag is False
                    for flag in value["closed_surfaces"].values()), "closed surface opened")
    return value


def validate_models(root: Path) -> None:
    require(sha256(root / MODELS) == MODELS_SHA256, "model fixture bytes drift")
    value = read_json(root, MODELS)
    exact({key: value[key] for key in ("schema_version", "domain", "status")}, {
        "schema_version": 2,
        "domain": "moex.stage8b.p1f.operational-model-fixtures.v2",
        "status": "DESIGN_MODEL_ONLY_NO_OPERATION",
    }, "model header")
    require(set(value) == {"schema_version", "domain", "status", "cases"},
            "model top-level inventory drift")
    cases = value["cases"]
    require(type(cases) is list and [row.get("id") for row in cases]
            == [f"P1F-M{index:02}" for index in range(1, 31)], "model case inventory drift")
    for row in cases:
        require(type(row) is dict and set(row) == {"id", "area", "given", "event", "expected"},
                f"model case schema drift: {row.get('id')}")
        require(all(type(item) is str and item for item in row.values()),
                f"model case field drift: {row['id']}")
    expected = {row["id"]: row["expected"] for row in cases}
    require("ReadyForBootstrap" in expected["P1F-M04"]
            and "cross-validate" in expected["P1F-M05"], "O2 crash model drift")
    require("V5 marker-bound" in expected["P1F-M07"]
            and "reads no F00" in expected["P1F-M08"], "V5 model drift")
    require("COUNT 64" in expected["P1F-M13"], "schedule reader model drift")
    require("DELCONSUMER" in expected["P1F-M14"], "consumer hygiene model drift")
    require(all("retain" in expected[f"P1F-M{index:02}"].lower()
                for index in range(16, 22)), "route-specific retention model drift")
    require("force kill" in expected["P1F-M27"], "terminal grace model drift")


def validate_matrix(root: Path) -> None:
    require(sha256(root / MATRIX) == MATRIX_SHA256, "acceptance matrix bytes drift")
    with (root / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(rows and list(rows[0]) == ["id", "area", "requirement", "status"],
            "matrix header drift")
    require([row["id"] for row in rows] == [f"P1F-R2-{index:03}" for index in range(1, 65)],
            "matrix row inventory drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "optional acceptance row")


def validate_documents(root: Path) -> None:
    document = (root / DOCUMENT).read_text()
    status = (root / STATUS).read_text()
    roadmap = (root / ROADMAP).read_text()
    for fragment in (
        "Stage 8B-P1-f R2", BASE, "root:moex-p1-paper 0440", "ReadyForBootstrap",
        "pre-seal V5 first-boot marker", "V4 never substitutes for V5",
        "XREVRANGE + - COUNT 64", "Arbitrary `EVAL` is never allowed",
        "does not claim a universal pre-callback schedule check",
        "opens only `P1F-I`", "not completed operational scenarios",
    ):
        require(fragment in document, f"design document fragment missing: {fragment}")
    require("active P1-f R2 design/checker correction candidate" in status,
            "current status R2 boundary missing")
    require("grants no operational authority" in status, "status opened operation")
    require("P1-f R0 and R1 were held" in roadmap and "P1F-I remains closed" in roadmap,
            "roadmap R2 hold boundary missing")


def validate(root: Path = ROOT, *, verify_lineage: bool = True) -> None:
    if verify_lineage:
        validate_lineage(root)
    r1.r0.validate_target(root)
    validate_inventory(root)
    validate_models(root)
    validate_matrix(root)
    validate_documents(root)


def main() -> int:
    try:
        validate()
    except (CheckFailure, r1.CheckFailure, r1.r0.CheckFailure, OSError,
            UnicodeDecodeError) as error:
        print(f"stage8b-p1f-r2-design-check: FAIL {error}")
        return 1
    print("stage8b-p1f-r2-design-check: PASS rows=64 models=30 artifacts=9 "
          "scripts=8 operations=10 routes=6 roles=8 phases=7 activation=false")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
