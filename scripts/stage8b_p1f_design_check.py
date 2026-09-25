#!/usr/bin/env python3
"""Fail-closed checker for Stage 8B-P1-f R0 design."""

from __future__ import annotations

import csv
import hashlib
import json
import subprocess
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
BASE = "3f171d997de5616cb9a07311d7776e446456c0c1"
DOCUMENT = "docs/stage-8/stage8b-p1f-isolated-operational-acceptance-design.md"
INVENTORY = "docs/stage-8/stage8b-p1f-isolated-operational-acceptance-design.json"
MATRIX = "docs/stage-8/stage8b-p1f-isolated-operational-acceptance-matrix.csv"
TARGET = "docs/stage-8/stage8b-p1f-target-vps-baseline.json"
STATUS = "docs/current-status.md"
ROADMAP = "docs/roadmap.md"

ALLOWED_CHANGES = {
    DOCUMENT,
    INVENTORY,
    MATRIX,
    TARGET,
    STATUS,
    ROADMAP,
    "scripts/make_stage8b_p1f_design_handoff.py",
    "scripts/stage8b_p1f_design_check.py",
    "scripts/stage8b_p1f_design_gate.sh",
    "scripts/stage8b_p1f_design_handoff_safety_check.py",
    "scripts/stage8b_p1f_design_negative_harness.py",
}
TARGET_SHA256 = "62ad9188e8232107725162e467d284e924f7800f6eabac95a3f535e147c6231b"
MATRIX_SHA256 = "c00a0b20e324d0fd3e349a309a2443056f305e292365b0ab87be295cbca30b67"
PHASES = [
    ("P1F-I", "source implementation", False, False, "P1F-R0-design"),
    ("P1F-O0", "immutable read-only target preflight", False, False, "P1F-I-source"),
    ("P1F-O1", "non-activating provisioning", True, False, "P1F-O0-preflight"),
    ("P1F-O2", "network-isolated one-shot bootstrap", True, False, "P1F-O1-provisioning"),
    ("P1F-O3", "bounded synthetic paper session and restart matrix", True, True, "P1F-O2-bootstrap"),
    ("P1F-O4", "bounded read-only FINAM bars paper session", True, True, "P1F-O3-synthetic"),
    ("P1F-A", "aggregate operational acceptance", False, False, "P1F-O4-finam-bars"),
]
RESTART_SCENARIOS = [
    "clean-sigterm",
    "sigkill-before-semantic-effect",
    "stale-pel-reclaim",
    "exact-duplicate-idempotence",
    "conflicting-duplicate-fail-closed",
    "paper-provider-uncertain-outcome-recovery",
]
CLOSED_SURFACES = {
    "design_performs_remote_mutation",
    "p1f_source_implementation_authorized",
    "operational_installation_authorized",
    "systemd_reload_enable_start_authorized",
    "operational_redis_db15_authorized",
    "operational_redis_db0_authorized",
    "finam_post_delete_send_authorized",
    "broker_dispatch_authorized",
    "runtime_live_authorized",
    "real_orders_authorized",
}


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


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_json(root: Path, relative: str) -> dict[str, Any]:
    try:
        value = json.loads((root / relative).read_text(), object_pairs_hook=strict_object)
    except (OSError, json.JSONDecodeError) as error:
        raise CheckFailure(f"cannot read {relative}: {error}") from error
    require(isinstance(value, dict), f"{relative} must contain an object")
    return value


def validate_lineage(root: Path) -> None:
    try:
        subprocess.run(
            ["git", "merge-base", "--is-ancestor", BASE, "HEAD"],
            cwd=root,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        changed = set(
            subprocess.check_output(["git", "diff", "--name-only", BASE, "--"], cwd=root, text=True).splitlines()
        )
        changed |= set(
            subprocess.check_output(
                ["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True
            ).splitlines()
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise CheckFailure(f"cannot verify P1-f lineage: {error}") from error
    require(changed == ALLOWED_CHANGES, "P1-f changed-path inventory drift")


def validate_target(root: Path) -> None:
    require(sha256(root / TARGET) == TARGET_SHA256, "target baseline bytes drift")
    target = read_json(root, TARGET)
    require(target["schema_version"] == 1, "target schema drift")
    require(target["target_id"] == "stage8b-p1f-isolated-vps-1", "target id drift")
    require(target["hostname"] == "nektodk1.ispvds.com" and target["ipv4"] == "45.150.11.252", "target host drift")
    require(
        target["ssh_ed25519_fingerprint"] == "SHA256:8fOAPkfvZ61LYmldUBs6A5ZC+BFr6O3yCvUrBAWZsAo",
        "target host key drift",
    )
    require(target["platform"] == {
        "os": "Ubuntu 24.04.4 LTS",
        "architecture": "x86_64",
        "systemd_major": 255,
        "cpu_count": 2,
        "memory_gib_minimum": 3.5,
        "root_free_gib_minimum": 20,
    }, "target platform drift")
    redis = target["redis"]
    require(redis["listen_addresses"] == ["127.0.0.1", "::1"], "Redis listen drift")
    require(redis["port"] == 6379 and redis["protected_mode"] is True and redis["appendonly"] is True, "Redis policy drift")
    require(redis["p1_database"] == 15 and redis["p1_database_observed_empty"] is True, "P1 DB drift")
    require(redis["p0_database"] == 0 and redis["p0_database_must_not_be_mutated_by_p1"] is True, "P0 DB boundary drift")
    require([row["unit"] for row in target["coexisting_services"]] == [
        "moex-finam-paper-runtime.service", "moex-finam-paper-ws.service"
    ], "coexisting service inventory drift")
    require(target["observation_is_authority"] is False and target["fresh_preflight_required_before_mutation"] is True, "stale observation became authority")


def validate_inventory(root: Path) -> None:
    value = read_json(root, INVENTORY)
    require(set(value) == {
        "schema_version", "stage", "status", "accepted_predecessor", "target_baseline", "isolation",
        "phases", "operator_authority", "session_policy", "finam_bars_phase", "restart_scenarios", "evidence", "closed_surfaces"
    }, "design inventory keys drift")
    require(value["schema_version"] == 1, "design schema drift")
    require(value["stage"] == "Stage 8B-P1-f R0 isolated operational acceptance design", "stage drift")
    require(value["status"] == "DESIGN_REVIEW_CANDIDATE_NO_ACTIVATION", "design self-accepted")
    require(value["accepted_predecessor"] == {
        "commit": BASE,
        "tree": "12a13b9c3418ec844219f8f9130904896e7a622e",
        "accepted_aggregate_commit": "a9bcd940635b62c2a13f8d378453e6ca21511e30",
        "aggregate_review_sha256": "c5be3e66cadd6c45f6678669afa3f9a194c31f6288aae1c3d29fa59aac47ca54",
    }, "accepted predecessor drift")
    require(value["target_baseline"] == TARGET, "target baseline binding drift")
    isolation = value["isolation"]
    require(isolation["p1_redis_database"] == 15 and isolation["p0_redis_database"] == 0, "database isolation drift")
    require(isolation["p1_namespace"] == "finam_imoexf_paper:{finam-imoexf-p1}:", "namespace drift")
    require(isolation["p1_service_user"] == "moex-p1-paper", "service identity drift")
    require(isolation["p0_services_must_remain_unchanged"] is True, "P0 service boundary opened")
    require(isolation["db0_write_allowed"] is False and isolation["non_loopback_redis_allowed"] is False, "Redis isolation opened")
    phases = [(row["id"], row["name"], row["remote_mutation"], row["service_start"], row["requires_prior_acceptance"]) for row in value["phases"]]
    require(phases == PHASES, "phase sequence drift")
    require(value["operator_authority"] == {
        "one_signed_phase_manifest_per_operational_phase": True,
        "manifest_single_use": True,
        "manifest_binds_source_tree_target_host_phase_and_deadline": True,
        "automatic_phase_escalation": False,
        "unattended_activation": False,
    }, "operator authority drift")
    require(value["session_policy"] == {
        "readiness": "PaperReady",
        "live_ready_forbidden": True,
        "paper_only_required": True,
        "synthetic_session_max_minutes": 30,
        "finam_bars_session_max_minutes": 180,
        "stop_and_disable_after_each_session": True,
        "retain_db15_and_durable_state_for_review": True,
    }, "session policy drift")
    require(value["finam_bars_phase"] == {
        "requires_separate_acceptance_after_synthetic": True,
        "market_data_only": True,
        "read_only_token_required": True,
        "finam_order_http_allowed": False,
        "command_consumer_allowed": False,
        "broker_dispatch_allowed": False,
        "main_supervisor_has_finam_dependency": False,
    }, "FINAM-bars boundary drift")
    require(value["restart_scenarios"] == RESTART_SCENARIOS, "restart scenario drift")
    require(all(flag is True for flag in value["evidence"].values()) and len(value["evidence"]) == 11, "evidence inventory drift")
    surfaces = value["closed_surfaces"]
    require(isinstance(surfaces, dict) and set(surfaces) == CLOSED_SURFACES, "closed surface inventory drift")
    require(all(flag is False for flag in surfaces.values()), "closed surface opened")


def validate_matrix(root: Path) -> None:
    require(sha256(root / MATRIX) == MATRIX_SHA256, "acceptance matrix bytes drift")
    try:
        with (root / MATRIX).open(newline="") as handle:
            rows = list(csv.DictReader(handle))
    except OSError as error:
        raise CheckFailure(f"cannot read acceptance matrix: {error}") from error
    require(rows and list(rows[0]) == ["id", "area", "requirement", "status"], "matrix header drift")
    require([row["id"] for row in rows] == [f"P1F-{index:03}" for index in range(1, 37)], "matrix row drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "optional acceptance row")


def validate_documents(root: Path) -> None:
    document = (root / DOCUMENT).read_text()
    status = (root / STATUS).read_text()
    roadmap = (root / ROADMAP).read_text()
    for fragment in (
        "DESIGN REVIEW CANDIDATE — NO ACTIVATION",
        BASE,
        "P1 uses only initially empty DB15",
        "P1F-O0",
        "maximum 30-minute synthetic",
        "maximum 180-minute",
        "Independent acceptance of this R0 package opens only P1F-I source",
    ):
        require(fragment in document, f"design document fragment missing: {fragment}")
    require("active P1-f R0 design candidate" in status, "current status design boundary missing")
    require("grants no operational authority" in status, "status opened operation")
    require("P1-f R0 is opened as a design-only candidate" in roadmap, "roadmap design boundary missing")
    require("Each operational transition remains fail closed" in roadmap, "roadmap phase authority weakened")


def validate(root: Path = ROOT, *, verify_lineage: bool = True) -> None:
    if verify_lineage:
        validate_lineage(root)
    validate_target(root)
    validate_inventory(root)
    validate_matrix(root)
    validate_documents(root)


def main() -> int:
    try:
        validate()
    except (CheckFailure, OSError, UnicodeDecodeError) as error:
        print(f"stage8b-p1f-design-check: FAIL {error}")
        return 1
    print("stage8b-p1f-design-check: PASS rows=36 phases=7 restart=6 closed_surfaces=10 activation=false")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
