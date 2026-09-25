#!/usr/bin/env python3
"""Fail-closed checker for Stage 8B-P1-f I source implementation."""

from __future__ import annotations

import csv
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

import stage8b_p1f_r4_design_check as r4


ROOT = Path(__file__).resolve().parents[1]
BASE = "5d81b8e212300858246237a227a95d115dd67c2d"
SOURCE = "crates/runtime-durable-service/src/stage8b_p1f_guardian.rs"
LIB = "crates/runtime-durable-service/src/lib.rs"
CARGO = "crates/runtime-durable-service/Cargo.toml"
FIRST_BOOT = "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs"
DOCUMENT = "docs/stage-8/stage8b-p1f-source-implementation.md"
INVENTORY = "docs/stage-8/stage8b-p1f-source-implementation.json"
MATRIX = "docs/stage-8/stage8b-p1f-source-acceptance-matrix.csv"
STATUS = "docs/current-status.md"
ROADMAP = "docs/roadmap.md"
MULTI_UID = "scripts/stage8b_p1f_multi_uid_custody_harness.sh"
REDIS_SOURCE = "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs"
STALE_DELETE_LUA = "docs/stage-8/stage8b-p1e-atomic-stale-consumer-delete-v1.lua"

DESIGN_HASHES = {
    r4.DOCUMENT: "e67f6be9c36caed6975c131a01356062d8eb52fc7a66e4f24256c6b05232b990",
    r4.INVENTORY: "0077a7dbfdca4cbf2323726bffa2208125427b7c7cafc20cc0a9e848cdf6b0cd",
    r4.MATRIX: "3107b84010949be4cc5b464069f4ccc653d9ab6657b97796b8bcb8d103600d41",
    r4.MODELS: "867cf023e66bab80322363a5998b7b2464b5e862b32c5e7c7c950449aa29ee69",
    r4.TARGET: "62ad9188e8232107725162e467d284e924f7800f6eabac95a3f535e147c6231b",
}

ALLOWED_CHANGES = {
    CARGO,
    LIB,
    FIRST_BOOT,
    SOURCE,
    DOCUMENT,
    INVENTORY,
    MATRIX,
    STATUS,
    ROADMAP,
    MULTI_UID,
    "scripts/stage8b_p1f_source_check.py",
    "scripts/stage8b_p1f_source_negative_harness.py",
    "scripts/stage8b_p1f_source_gate.sh",
    "scripts/stage8b_p1f_source_handoff_safety_check.py",
    "scripts/make_stage8b_p1f_source_handoff.py",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


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
    require(type(value) is dict, f"{relative} must be an object")
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
            subprocess.check_output(
                ["git", "diff", "--name-only", BASE, "--"], cwd=root, text=True
            ).splitlines()
        )
        changed |= set(
            subprocess.check_output(
                ["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True
            ).splitlines()
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise CheckFailure(f"cannot verify source lineage: {error}") from error
    require(changed == ALLOWED_CHANGES, f"source changed-path inventory drift: {sorted(changed ^ ALLOWED_CHANGES)}")


def validate_design_freeze(root: Path) -> None:
    for path, expected in DESIGN_HASHES.items():
        require(sha256(root / path) == expected, f"accepted R4 design drift: {path}")
    r4.validate_inventory(root)
    r4.validate_models(root)
    r4.validate_matrix(root)


def validate_inventory(root: Path) -> None:
    value = read_json(root, INVENTORY)
    require(
        set(value)
        == {
            "schema_version",
            "stage",
            "status",
            "accepted_design_commit",
            "production_module",
            "authority_control_root",
            "config_root",
            "implemented_contracts",
            "inherited_contracts",
            "executable_evidence",
            "closed_surfaces",
            "remaining_p1fi_milestones",
            "next_after_independent_source_acceptance",
        },
        "source inventory key set drift",
    )
    require(value["schema_version"] == 1 and type(value["schema_version"]) is int, "schema drift")
    require(value["stage"] == "Stage 8B-P1-f Ia guardian foundation correction R2", "stage drift")
    require(value["status"] == "REVIEW_CANDIDATE_GUARDIAN_FOUNDATION_R2_ONLY", "source self-accepted")
    require(value["accepted_design_commit"] == BASE, "accepted design binding drift")
    require(value["production_module"] == SOURCE, "production module drift")
    require(value["authority_control_root"] == "/var/lib/moex-finam-p1-paper-control", "control root drift")
    require(value["config_root"] == "/etc/moex-finam-p1-paper", "config root drift")
    require(len(value["implemented_contracts"]) == 8, "implemented contract inventory drift")
    require(value["inherited_contracts"] == {
        "first_boot_recovery": "V5",
        "committed_schedule_recovery": "V4",
        "redis_operation_inventory": "accepted P1-e I1 exact traces and capabilities",
        "p0_isolation": "no P0 control or DB0 write capability added",
    }, "inherited contract drift")
    require(value["executable_evidence"] == {
        "guardian_test_module": "stage8b_p1f_guardian::tests",
        "multi_uid_harness": MULTI_UID,
        "source_gate": "scripts/stage8b_p1f_source_gate.sh",
        "source_checker": "scripts/stage8b_p1f_source_check.py",
        "negative_harness": "scripts/stage8b_p1f_source_negative_harness.py",
    }, "executable evidence inventory drift")
    closed = value["closed_surfaces"]
    require(type(closed) is dict and len(closed) == 9, "closed surface inventory drift")
    require(all(flag is False for flag in closed.values()), "operational surface opened")
    require(value["remaining_p1fi_milestones"] == [
        "P1F-Ib local deadline and child supervision composition",
        "P1F-Ic phase-gated bootstrap synthetic and GET-only observer producers with retained high-water",
        "P1F-Id eight role adapters resource polling and command audit",
        "P1F-Ie aggregate source closure",
    ], "remaining P1F-I milestones drift")
    require(
        value["next_after_independent_source_acceptance"]
        == "P1F-Ib local supervision composition; P1F-O0 remains closed",
        "next boundary drift",
    )


def validate_matrix(root: Path) -> None:
    try:
        with (root / MATRIX).open(newline="") as handle:
            rows = list(csv.DictReader(handle))
    except OSError as error:
        raise CheckFailure(f"cannot read matrix: {error}") from error
    require(rows and list(rows[0]) == ["id", "area", "requirement", "status"], "matrix header drift")
    require([row["id"] for row in rows] == [f"P1FI-{index:03}" for index in range(1, 31)], "matrix row inventory drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "optional matrix row")


def validate_source(root: Path) -> None:
    source = (root / SOURCE).read_text()
    library = (root / LIB).read_text()
    cargo = (root / CARGO).read_text()
    harness = (root / MULTI_UID).read_text()
    required = (
        'pub const STAGE8B_P1F_AUTHORITY_CONTROL_ROOT: &str = "/var/lib/moex-finam-p1-paper-control"',
        'pub const STAGE8B_P1F_CONFIG_ROOT: &str = "/etc/moex-finam-p1-paper"',
        "if unsafe { libc::geteuid() } != 0",
        "if unsafe { libc::geteuid() } != self.expected_uid",
        "libc::LOCK_EX | libc::LOCK_NB",
        "libc::O_NOFOLLOW | libc::O_CLOEXEC",
        "libc::fchown",
        "libc::chown",
        "SIGNED_GENESIS_DOMAIN",
        "SIGNED_ACTIVATION_DOMAIN",
        "SIGNED_PHASE_DOMAIN",
        "PENDING_CLAIM_FILE",
        "PENDING_MATERIALIZATION_FILE",
        "PENDING_TERMINAL_FILE",
        "PENDING_STOPPING_FILE",
        'event.event_kind == "PHASE_STOPPING"',
        "head.state != projected_state",
        "PendingRecoveryRequired",
        "acquire_execution_lock",
        "resume_stopping_phase",
        "poll_deadline_at_elapsed",
        "force_kill_after_elapsed",
        "current_boot_id",
        "force_kill_at = stopping_started_at + chrono::Duration::seconds(30)",
        "sha256_hex(&retained_manifest_bytes) != event.manifest_sha256",
        "Stage8bP1fClaimDispositionV1::ContinuedExisting",
        "!(0..=300).contains(&age)",
        "parse_stage8b_p1e_first_boot_source_v1",
        "Stage8bP1fDeadlineDecisionV1::ForceKill",
        "RestoreOverlapsControlRoot",
        "validate_prepared_pending_event",
        "self.continue_stopping_transaction(&pending)?",
        "pub fn execute_stage8b_p1f_permitted_restore_v1(\n    plan: &Stage8bP1fRestorePlanV1,\n    writes: &[Stage8bP1fRestoreWriteV1]",
        "openat_restore_component",
        "write_restore_file_at",
        "rebind_authorized: false",
    )
    for fragment in required:
        require(fragment in source, f"source contract missing: {fragment}")
    require("#[derive(Debug)]\npub struct Stage8bP1fRunPermitV1" in source, "run permit became cloneable or serializable")
    for test in (
        "stopping_monotonic_bound_rejects_frozen_and_backward_wall_clock",
        "active_readmission_without_monotonic_witness_fails_closed",
        "public_transitions_recover_exact_event_temp_and_pending_stopping",
        "restore_allows_sibling_and_binds_leaf_and_parent_by_descriptor",
    ):
        require(f"fn {test}()" in source, f"executable correction case missing: {test}")
    require("#[derive(Debug)]\npub struct Stage8bP1fAuthorityStoreV1" not in source, "authority store derivation drift")
    for forbidden in ("redis::Client", "reqwest::", "std::process::Command", "tokio::process", "clear_quarantine", "rebind_authority"):
        require(forbidden not in source, f"forbidden operational or rebind surface: {forbidden}")
    for forbidden in (
        "UNSIGNED_PHASE_DOMAIN",
        "REMOVED_CLAIM_FILE",
        "REMOVED_MATERIALIZATION_FILE",
        "REMOVED_TERMINAL_FILE",
        "libc::fchmod",
        "!(0..=301).contains(&age)",
    ):
        require(forbidden not in source, f"mutated source primitive present: {forbidden}")
    require(source.count("custom_flags(libc::O_CLOEXEC)") == 0,
            "authority open lost O_NOFOLLOW")
    require(source.count("libc::LOCK_EX | libc::LOCK_NB") >= 2,
            "guardian or execution lock lost nonblocking exclusivity")
    for fragment, minimum in {
        "if unsafe { libc::geteuid() } != self.expected_uid": 2,
        "libc::fchown": 2,
        "SIGNED_PHASE_DOMAIN": 3,
        "PENDING_CLAIM_FILE": 9,
        "PENDING_MATERIALIZATION_FILE": 5,
        "PENDING_TERMINAL_FILE": 6,
        "!(0..=300).contains(&age)": 2,
    }.items():
        require(source.count(fragment) >= minimum,
                f"source structural coverage reduced: {fragment}")
    require("mod stage8b_p1f_guardian;" in library and "Stage8bP1fRunPermitV1" in library, "library export drift")
    dependencies = cargo.split("[dev-dependencies]", 1)[0]
    require("ed25519-dalek.workspace = true" in dependencies, "production signature dependency missing")
    for fragment in (
        "unlink-authority",
        "rename-authority",
        "mutate-authority",
        "create-authority",
        "parent-substitution",
        "guardian-lock-read",
        "source-transition-positive-read",
        "multi_uid_root_transition_source_probe",
        "runuser -u",
    ):
        require(fragment in harness, f"multi-UID case missing: {fragment}")


def validate_documents(root: Path) -> None:
    document = (root / DOCUMENT).read_text()
    status = (root / STATUS).read_text()
    roadmap = (root / ROADMAP).read_text()
    for fragment in (
        "REVIEW_CANDIDATE_GUARDIAN_FOUNDATION_R2_ONLY",
        BASE,
        "exact recovery",
        "0..=300",
        "30-second",
        "No clear/rebind API exists",
        "P1F-Ib local supervision composition",
    ):
        require(fragment in document, f"source document fragment missing: {fragment}")
    require("P1F-Ia guardian-foundation correction R2 candidate" in status, "current status source boundary missing")
    require("P1F-Ia guardian foundation correction R2 is the active review candidate" in roadmap, "roadmap source boundary missing")


def validate(root: Path = ROOT, *, verify_lineage: bool = True) -> None:
    if verify_lineage:
        validate_lineage(root)
    validate_design_freeze(root)
    validate_inventory(root)
    validate_matrix(root)
    validate_source(root)
    validate_documents(root)


def main() -> int:
    try:
        validate()
    except (CheckFailure, r4.CheckFailure, OSError, UnicodeDecodeError) as error:
        print(f"stage8b-p1f-source-check: FAIL {error}")
        return 1
    print("PASS stage8b-p1f-source-check boundary=P1F-Ia rows=30 guardian=true genesis=true replay=true pending=true o2=true durable_stop=true restore=true operational=false")
    return 0


if __name__ == "__main__":
    sys.exit(main())
