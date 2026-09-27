#!/usr/bin/env python3
"""Validate the Stage 8B-P1-f O2 execution-contract package."""

from __future__ import annotations

import csv
import hashlib
import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONTRACT = "docs/stage-8/stage8b-p1f-o2-execution-package.json"
MATRIX = "docs/stage-8/stage8b-p1f-o2-acceptance-matrix.csv"
PREDECESSOR = "e11744e31f11d567633716f21211c484bb9045eb"
EXPECTED_HASHES = {
    "docs/stage-8/stage8b-p1f-o1-governance-closure.json": "6d663cce47a38c859e5e0649774eb453a6d1d433abfe0fcf36e40be8fa7034fa",
    "docs/stage-8/stage8b-p1f-isolated-operational-acceptance-design.json": "0077a7dbfdca4cbf2323726bffa2208125427b7c7cafc20cc0a9e848cdf6b0cd",
    "docs/stage-8/stage8b-p1e-first-boot-source-plan-v2.json": "2a507577075b8b5315a462ffeee221dd0a7f8a8f61d42516fbdb9346cc3464ca",
    "docs/stage-8/stage8b-p1e-deployment-identity-v2.json": "428415fdedd3fd24ac128ee2ca703a6572e57644cad0cb40b30a9c96ea62a038",
    "deploy/stage8b-p1e/moex-finam-p1-paper-bootstrap.service": "f5f6db6e08d45f39fa3976701f78526a55e1485f3398f4140d13eaba1b2bc62b",
}
EXPECTED_PACKAGE_BOUNDARY = {
    "execution_authorized": False,
    "remote_mutation_performed": False,
    "key_generation_performed": False,
    "finam_contact_performed": False,
    "redis_contact_performed": False,
    "systemd_reload_or_start_performed": False,
}
EXPECTED_CLOSED_SURFACES = {
    "o2_execution": False,
    "o3_synthetic_session": False,
    "o4_finam_bars_session": False,
    "ordinary_p1_service": False,
    "redis_db15_or_db0_mutation": False,
    "paper_provider_order_execution": False,
    "finam_post_or_delete": False,
    "broker_dispatch": False,
    "runtime_live": False,
    "real_orders": False,
}
EXPECTED_FACADES = [
    "offline canonical Ed25519 signer with no secret output",
    "root target guardian CLI over accepted Stage8bP1fAuthorityStoreV1 APIs only",
    "read-only wire-V2 first-boot source materializer over accepted parsers/oracles",
    "root O2-B systemd-supervised runner holding the guardian permit and polling the exact bootstrap unit",
    "read-only redacted O2 evidence collector with no lifecycle authority",
]
EXPECTED_ROUTE_ALLOWLIST = [
    {"method": "GET", "route": "/v1/accounts/{account_id}", "query_keys": []},
    {
        "method": "GET",
        "route": "/v1/accounts/{account_id}/orders",
        "query_keys": [],
    },
    {
        "method": "GET",
        "route": "/v1/assets/{venue_symbol}/params",
        "query_keys": ["account_id"],
    },
    {
        "method": "GET",
        "route": "/v1/assets/{venue_symbol}/schedule",
        "query_keys": [],
    },
    {
        "method": "GET",
        "route": "/v1/instruments/{venue_symbol}/bars",
        "query_keys": ["timeframe", "interval.start_time", "interval.end_time"],
    },
]
EXPECTED_MATRIX_ROWS = [
    ("O2R0-01", "lineage", "O1 operational and governance closure identities are exact", "REQUIRED"),
    ("O2R0-02", "scope", "Contract commit changes no production Rust Cargo deploy config workflow or remote state", "REQUIRED"),
    ("O2R0-03", "target", "Target host and SSH fingerprint are exact", "REQUIRED"),
    ("O2R0-04", "package", "Private key FINAM token and fresh source bytes are excluded and public key is selected before artifact finalization", "REQUIRED"),
    ("O2R0-05", "facade", "Offline signer only creates canonical accepted guardian documents and private bytes stay offline", "REQUIRED"),
    ("O2R0-06", "facade", "Target guardian CLI delegates only to accepted authority APIs after exact empty control-root skeleton validation", "REQUIRED"),
    ("O2R0-07", "facade", "Materializer enforces the exact-account GET route/query allowlist including complete orders snapshot before transport and cannot reach Redis or write routes", "REQUIRED"),
    ("O2R0-08", "facade", "Systemd-supervised O2-B runner polls permit and unit every 250 ms and controls the unit rather than treating systemctl client exit as stopped proof", "REQUIRED"),
    ("O2R0-09", "authority", "Genesis uses one offline Prepared generation and exact Activated certificate", "REQUIRED"),
    ("O2R0-10", "authority", "O2 claim is durable before FINAM source collection", "REQUIRED"),
    ("O2R0-11", "authority", "Manifest binds source tree host sequence predecessor policy template installation and deadline", "REQUIRED"),
    ("O2R0-12", "materialization", "Only first_boot_source_bundle_sha256 is finalized in signed template", "REQUIRED"),
    ("O2R0-13", "materialization", "Source config and ReadyForBootstrap receipt use accepted fixed paths and custody", "REQUIRED"),
    ("O2R0-14", "materialization", "Partial materialization resumes exact same claim only", "REQUIRED"),
    ("O2R0-15", "source", "Broker truth is flat exact-account IMOEXF@RTSX with complete observed positions and GET orders snapshot and price-step 0.5", "REQUIRED"),
    ("O2R0-16", "source", "History has at least 121 explicit Moscow sessions via accepted M1-to-M10 path", "REQUIRED"),
    ("O2R0-17", "source", "Riskgate has at least 120 source-compatible cross-validated observations", "REQUIRED"),
    ("O2R0-18", "source", "Candidate is one later canonical final M10 with ten-M1 provenance and zero intents", "REQUIRED"),
    ("O2R0-19", "freshness", "Broker truth age at O2-B is at most 300 seconds", "REQUIRED"),
    ("O2R0-20", "freshness", "Age 301 fails with zero bootstrap mutation and requires new manifest", "REQUIRED"),
    ("O2R0-21", "credential", "Lifecycle key is generated once installed root 0400 and never logged or archived", "REQUIRED"),
    ("O2R0-22", "bootstrap", "Bootstrap and runner unit bytes hashes commands cleanup mode and unit-state proof are exact in the later artifact", "REQUIRED"),
    ("O2R0-23", "bootstrap", "O2-B has PrivateNetwork yes AF_UNIX only and no Redis or FINAM contact", "REQUIRED"),
    ("O2R0-24", "bootstrap", "One daemon reload is bounded after exact reread no unit is enabled and runner cleanup is independent of SSH", "REQUIRED"),
    ("O2R0-25", "bootstrap", "Ordinary P1 service remains inactive and not enabled after O2", "REQUIRED"),
    ("O2R0-26", "evidence", "Receipts source config credential metadata durable root and unit result are retained redacted", "REQUIRED"),
    ("O2R0-27", "isolation", "DB15 remains empty and P0 identities remain unchanged", "REQUIRED"),
    ("O2R0-28", "recovery", "Lost response or runner uses stopped-unit proof then accepted V5 classification and exact Completed Failed Expired deadline rules with no blind retry", "REQUIRED"),
    ("O2R0-29", "closed", "Current contract authorizes no key generation remote mutation FINAM call Redis call reload or start", "REQUIRED"),
    ("O2R0-30", "next", "O2 execution artifact and execution each require independent acceptance", "REQUIRED"),
]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def strict_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate(document: dict[str, object], root: Path = ROOT) -> None:
    require(document.get("schema_version") == 1, "schema drift")
    require(
        document.get("status")
        == "R1_EXECUTION_CONTRACT_CORRECTION_REVIEW_CANDIDATE_DO_NOT_EXECUTE",
        "status opened",
    )
    predecessor = document["accepted_predecessor"]
    require(predecessor["o1_governance_closure_commit"] == PREDECESSOR, "O1 closure drift")
    require(
        predecessor["o1_operational_commit"]
        == "997e8a1d201048fcdec0e948660f32a0bee3cceb",
        "O1 operational drift",
    )
    require(
        predecessor["installed_binary_sha256"]
        == "cee324a4e4f251227a25d4a7b23dda332a94522b45407671982f4fc896614406",
        "installed binary drift",
    )
    target = document["target"]
    require(target == {
        "target_id": "stage8b-p1f-isolated-vps-1",
        "hostname": "nektodk1.ispvds.com",
        "ipv4": "45.150.11.252",
        "ssh_ed25519_fingerprint": "SHA256:8fOAPkfvZ61LYmldUBs6A5ZC+BFr6O3yCvUrBAWZsAo",
    }, "target drift")
    subphases = document["subphases"]
    require([item["id"] for item in subphases] == ["P1F-O2-M", "P1F-O2-B"], "subphase drift")
    require(
        subphases[0]["network"]
        == "exact method-plus-route allowlist including GET account orders snapshot; no Redis and no order execution endpoint",
        "O2-M network drift",
    )
    require(subphases[1]["network"] == "PrivateNetwork=yes and AF_UNIX only", "O2-B network widened")
    paths = document["fixed_paths"]
    require(paths == {
        "control_root": "/var/lib/moex-finam-p1-paper-control",
        "config": "/etc/moex-finam-p1-paper/supervisor.json",
        "source": "/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json",
        "lifecycle_credential": "/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key",
        "durable_state": "/var/lib/moex-finam-p1-paper/state",
        "bootstrap_unit": "moex-finam-p1-paper-bootstrap.service",
    }, "fixed path drift")
    require(document["freshness"]["broker_truth_max_age_seconds_at_bootstrap"] == 300, "freshness widened")
    require(document["authority_key_ordering"] == {
        "current_patch_generates_or_reuses_private_key": False,
        "private_key_preparation_timing": "after R1 design acceptance and before execution-artifact finalization",
        "final_execution_artifact_requires_preselected_public_key": True,
        "private_bytes_location": "designated offline medium only",
        "private_bytes_in_git_handoff_target_or_logs": False,
    }, "authority key ordering drift")
    require(document["control_root_skeleton"] == {
        "required_before_open_production": True,
        "path": "/var/lib/moex-finam-p1-paper-control",
        "create_if_absent": True,
        "owner": "root",
        "group": "moex-p1-paper",
        "mode": "0750",
        "permitted_initial_entries": [],
        "existing_nonempty_or_wrong_custody_action": "fail before open_production and genesis",
        "repeated_genesis_or_authority_replacement_allowed": False,
    }, "control-root skeleton drift")
    network = document["materializer_network_policy"]
    require(network["base_url"] == "https://api.finam.ru", "base URL drift")
    require(network["exact_account_binding"] == "configured account id only", "account binding drift")
    require(network["exact_venue_symbol"] == "IMOEXF@RTSX", "venue symbol drift")
    require(network["method_allowlist"] == ["GET"], "method allowlist drift")
    require(network["route_allowlist"] == EXPECTED_ROUTE_ALLOWLIST, "route allowlist drift")
    require(network["bars_timeframe"] == "TIME_FRAME_M1", "bars timeframe drift")
    require(network["host_or_ip_filter_alone_is_sufficient"] is False, "host-only enforcement accepted")
    require(network["complete_orders_truth_may_be_assumed"] is False, "orders truth assumed")
    require(network["orders_snapshot_is_read_only_truth_not_execution"] is True, "orders GET classification drift")
    require(network["forbidden_methods"] == ["POST", "PUT", "PATCH", "DELETE"], "write method inventory drift")
    require(network["unlisted_routes_allowed"] is False, "unlisted route opened")
    require(network["redirects_allowed"] is False and network["system_proxy_allowed"] is False, "transport widened")
    require(network["redis_allowed"] is False, "Redis opened")
    require(document["source_contract"]["wire_schema_version"] == 2, "wire version drift")
    require("active_orders_complete is observed and never assumed" in document["source_contract"]["broker_truth"], "complete orders truth missing")
    require(document["bootstrap"] == {
        "daemon_reload_allowed_once_after_full_reread": True,
        "enable_allowed": False,
        "command": "/usr/bin/systemctl start --wait moex-finam-p1-paper-bootstrap.service",
        "ordinary_service_start_allowed": False,
        "redis_contact_allowed": False,
        "finam_contact_allowed": False,
    }, "bootstrap boundary drift")
    supervision = document["bootstrap_supervision"]
    require(supervision == {
        "runner_unit": "moex-finam-p1-paper-o2-bootstrap-runner.service",
        "runner_unit_material": "exact bytes and hash required in later execution artifact",
        "runner_process": "one root foreground runner independent of SSH",
        "bootstrap_start_child": "/usr/bin/systemctl start --wait moex-finam-p1-paper-bootstrap.service",
        "systemctl_child_exit_is_unit_stop_proof": False,
        "permit_and_unit_poll_interval_ms": 250,
        "permit_poll_method": "Stage8bP1fRunPermitV1::poll_deadline",
        "unit_observation": ["ActiveState", "SubState", "Result", "ExecMainStatus", "MainPID", "ControlPID", "Job", "ControlGroup"],
        "begin_stopping_command": "/usr/bin/systemctl stop --no-block moex-finam-p1-paper-bootstrap.service",
        "force_kill_command": "/usr/bin/systemctl kill --kill-who=all --signal=SIGKILL moex-finam-p1-paper-bootstrap.service",
        "stopped_proof": "no pending job; MainPID=0; ControlPID=0; inactive-or-failed state; empty ControlGroup cgroup.procs",
        "stop_grace_seconds": 30,
        "runner_signal_action": "stop bootstrap unit immediately; prove stopped; terminalize Failed before deadline or Expired at-or-after deadline",
        "runner_loss_action": "systemd ExecStopPost invokes the same pinned runner binary cleanup mode to stop then bounded-kill and prove the bootstrap unit absent",
        "host_restart_action": "bootstrap and runner remain disabled; recovery proves no unit process or job before V5 classification",
        "cleanup_failure_action": "retain Active-or-Stopping authority plus diagnostics; no terminal receipt and no new admission",
        "ssh_loss_changes_supervision": False,
    }, "bootstrap supervision drift")
    require(document["terminal_outcomes"] == {
        "verified_success_before_deadline": "Completed",
        "verified_failure_or_operator_stop_before_deadline": "Failed",
        "deadline_reached_or_terminal_recovery_at_or_after_deadline": "Expired",
        "completed_or_failed_at_or_after_deadline_allowed": False,
        "terminal_receipt_write_failure": "retain exact pending-terminal transaction; replay only identical state and reason after stopped proof",
        "lost_bootstrap_response": "prove unit stopped then use exact durable marker and receipt with accepted V5 classification; never blind retry",
        "new_admission_while_nonterminal_or_pending": False,
        "deadline_extension_allowed": False,
    }, "terminal outcome drift")
    require(document["package_boundary"] == EXPECTED_PACKAGE_BOUNDARY, "package boundary drift")
    require(document["closed_surfaces"] == EXPECTED_CLOSED_SURFACES, "closed surface drift")
    require(document["facades_required_in_later_execution_artifact"] == EXPECTED_FACADES, "facade inventory drift")
    for path, digest in EXPECTED_HASHES.items():
        require(sha256(root / path) == digest, f"accepted input drift: {path}")
    guardian = (root / "crates/runtime-durable-service/src/stage8b_p1f_guardian.rs").read_text()
    for marker in (
        "pub fn initialize_authority(",
        "pub fn activate_authority(",
        "pub fn claim_phase(",
        "pub fn materialize_o2(",
        "pub fn admit_active_phase(",
        "pub fn finish_phase(",
        "pub fn poll_deadline(",
        "Stage8bP1fPhaseStateV1::Expired",
        'state: "ReadyForBootstrap".to_string()',
        "if !(0..=300).contains(&age)",
    ):
        require(marker in guardian, f"guardian seam missing: {marker}")
    unit = (root / "deploy/stage8b-p1e/moex-finam-p1-paper-bootstrap.service").read_text()
    for marker in ("Type=oneshot", "PrivateNetwork=yes", "RestrictAddressFamilies=AF_UNIX"):
        require(marker in unit, f"bootstrap unit drift: {marker}")


def main() -> None:
    document = json.loads((ROOT / CONTRACT).read_text(), object_pairs_hook=strict_object)
    validate(document)
    with (ROOT / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    actual_rows = [tuple(row[field] for field in ("id", "area", "requirement", "status")) for row in rows]
    require(actual_rows == EXPECTED_MATRIX_ROWS, "acceptance matrix drift")
    print("PASS stage8b-p1f-o2-check revision=R1 rows=30 execution=false")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"stage8b-p1f-o2-check: FAIL {error}")
        raise SystemExit(1)
