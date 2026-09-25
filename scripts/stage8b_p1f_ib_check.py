#!/usr/bin/env python3
"""Fail-closed source checker for Stage 8B-P1-f Ib local supervision."""

from __future__ import annotations

import csv
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
BASE = "9be356b04a38e627337ed148ccc9fbdaebae8d4a"
REVIEW_SHA256 = "db684cb6ba10cb951801cc917c317d2817c99c3f10874d5a1d959c053f6ebb60"
GUARDIAN = "crates/runtime-durable-service/src/stage8b_p1f_guardian.rs"
SUPERVISION = "crates/runtime-durable-service/src/stage8b_p1f_local_supervision.rs"
BINARY = "crates/runtime-durable-service/src/bin/stage8b-p1f-local-supervisor.rs"
LIB = "crates/runtime-durable-service/src/lib.rs"
DOCUMENT = "docs/stage-8/stage8b-p1f-ib-local-supervision.md"
INVENTORY = "docs/stage-8/stage8b-p1f-ib-local-supervision.json"
MATRIX = "docs/stage-8/stage8b-p1f-ib-local-supervision-matrix.csv"
STATUS = "docs/current-status.md"
ROADMAP = "docs/roadmap.md"
CHECKER = "scripts/stage8b_p1f_ib_check.py"
NEGATIVE = "scripts/stage8b_p1f_ib_negative_harness.py"
GATE = "scripts/stage8b_p1f_ib_gate.sh"
SAFETY = "scripts/stage8b_p1f_ib_handoff_safety_check.py"
BUILDER = "scripts/make_stage8b_p1f_ib_handoff.py"
ALLOWED_CHANGES = {
    GUARDIAN,
    SUPERVISION,
    BINARY,
    LIB,
    DOCUMENT,
    INVENTORY,
    MATRIX,
    STATUS,
    ROADMAP,
    CHECKER,
    NEGATIVE,
    GATE,
    SAFETY,
    BUILDER,
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
        raise CheckFailure(f"cannot verify Ib lineage: {error}") from error
    require(changed == ALLOWED_CHANGES, f"Ib changed-path drift: {sorted(changed ^ ALLOWED_CHANGES)}")


def validate_inventory(root: Path) -> None:
    value = read_json(root, INVENTORY)
    require(
        set(value)
        == {
            "schema_version",
            "stage",
            "status",
            "accepted_guardian_commit",
            "accepted_guardian_review_sha256",
            "production_module",
            "entry",
            "child",
            "stop_contract",
            "evidence",
            "closed_surfaces",
            "next_after_acceptance",
        },
        "Ib inventory key set drift",
    )
    require(value["schema_version"] == 1 and type(value["schema_version"]) is int, "schema drift")
    require(value["stage"] == "Stage 8B-P1-f Ib local supervision composition", "stage drift")
    require(value["status"] == "REVIEW_CANDIDATE_LOCAL_SUPERVISION_ONLY", "Ib self-accepted")
    require(value["accepted_guardian_commit"] == BASE, "Ia commit binding drift")
    require(value["accepted_guardian_review_sha256"] == REVIEW_SHA256, "Ia review binding drift")
    require(value["production_module"] == SUPERVISION, "production module drift")
    require(
        value["entry"]
        == {
            "binary": "stage8b-p1f-local-supervisor",
            "grammar": "run MANIFEST_SHA256",
            "permitted_phases": ["O3_SYNTHETIC_PAPER", "O4_FINAM_READ_ONLY"],
        },
        "entry contract drift",
    )
    require(
        value["child"]
        == {
            "args": ["run", "/etc/moex-finam-p1-paper/supervisor.json"],
            "binary": "/usr/local/libexec/moex/stage8b-p1-paper-supervisor",
            "linux_parent_death_signal": "SIGKILL",
            "max_starts_per_600_seconds": 5,
            "process_group": "dedicated",
            "restart_delay_seconds": 5,
            "user_group": "moex-p1-paper:moex-p1-paper",
        },
        "fixed child contract drift",
    )
    require(
        value["stop_contract"]
        == {
            "force_kill_scope": "entire child process group",
            "grace_seconds": 30,
            "monotonic_witness": "original linear Ia run permit",
            "signals": ["SIGTERM", "SIGINT"],
        },
        "stop contract drift",
    )
    require(value["evidence"] == {
        "checker": CHECKER,
        "gate": GATE,
        "negative_harness": NEGATIVE,
        "real_process_tests": 8,
    }, "evidence inventory drift")
    closed = value["closed_surfaces"]
    require(type(closed) is dict and len(closed) == 8, "closed surface inventory drift")
    require(all(flag is False for flag in closed.values()), "operational surface opened")
    require(
        value["next_after_acceptance"]
        == "P1F-Ic fixed producers and retained high-water; P1F-O0 remains closed",
        "next boundary drift",
    )


def validate_matrix(root: Path) -> None:
    try:
        with (root / MATRIX).open(newline="") as handle:
            rows = list(csv.DictReader(handle))
    except OSError as error:
        raise CheckFailure(f"cannot read Ib matrix: {error}") from error
    require(rows and list(rows[0]) == ["id", "area", "requirement", "status"], "matrix header drift")
    require([row["id"] for row in rows] == [f"P1FIB-{index:03}" for index in range(1, 21)], "matrix rows drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "optional Ib row")


def validate_source(root: Path) -> None:
    guardian = (root / GUARDIAN).read_text()
    source = (root / SUPERVISION).read_text()
    binary = (root / BINARY).read_text()
    library = (root / LIB).read_text()
    for fragment in (
        "pub(crate) fn request_local_stop(",
        "self.admitted_monotonic.elapsed()",
        "elapsed.checked_add(StdDuration::from_secs(30))",
        "let force_kill_after_elapsed = StdDuration::ZERO;",
        "stopping_reason_code",
    ):
        require(fragment in guardian, f"guardian composition seam missing: {fragment}")
    for fragment in (
        'pub const STAGE8B_P1F_I1_BINARY_PATH: &str = "/usr/local/libexec/moex/stage8b-p1-paper-supervisor"',
        "OsString::from(STAGE8B_P1E_SUPERVISOR_CONFIG_PATH)",
        "const MAX_STARTS_PER_WINDOW: usize = 5;",
        "const RESTART_WINDOW: StdDuration = StdDuration::from_secs(600);",
        "const RESTART_DELAY: StdDuration = StdDuration::from_secs(5);",
        ".process_group(0)",
        "libc::setgroups(0, std::ptr::null())",
        "libc::setgid(gid)",
        "libc::setuid(uid)",
        "libc::PR_SET_PDEATHSIG",
        "libc::getppid() as u32 != parent_pid",
        "libc::kill(-self.process_group, signal)",
        "permit.request_local_stop",
        "store.admit_active_phase(manifest_sha256, trusted_now)",
        "store.resume_stopping_phase(manifest_sha256, trusted_now)",
        "Stage8bP1fPhaseV1::O3SyntheticPaper | Stage8bP1fPhaseV1::O4FinamReadOnly",
        "child.force_kill_and_reap()",
        "Stage8bP1fLocalSupervisionErrorV1::RecoveryTerminated",
        "Stage8bP1fLocalSupervisionErrorV1::SignalTask",
    ):
        require(fragment in source, f"Ib source contract missing: {fragment}")
    for forbidden in (
        "redis::",
        "reqwest::",
        "broker_finam",
        "finam_gateway",
        "Command::new(executable)",
        "pub fn supervise_admitted_child",
        "pub fn spawn",
    ):
        require(forbidden not in source, f"forbidden generic/operational surface: {forbidden}")
    require(source.count("store.admit_active_phase(manifest_sha256, trusted_now)") == 1,
            "phase admission count drift")
    require("tokio::spawn(forward_unix_signals(signal_sender))" in source, "signals not installed first")
    require(source.index("tokio::spawn(forward_unix_signals(signal_sender))") < source.index("resolve_service_identity()?"),
            "signal registration moved after admission boundary")
    for test in (
        "local_supervision_starts_after_admission_and_stops_on_sigterm",
        "retained_pre_spawn_signal_prevents_child_start",
        "local_supervision_restarts_child_under_same_permit_then_handles_sigint",
        "local_supervision_force_kills_noncooperative_process_group",
        "recovered_guardian_never_starts_a_new_child",
        "signal_supervision_loss_is_nonzero_and_leaves_no_child",
        "child_restart_budget_is_bounded_without_readmission",
        "linux_guardian_death_signal_kills_child",
    ):
        require(f"fn {test}" in guardian, f"real process case missing: {test}")
    require('Some("run"), Some(manifest), None' in binary, "CLI grammar drift")
    require("run_stage8b_p1f_local_supervisor_v1(&manifest_sha256)" in binary, "CLI route drift")
    require("mod stage8b_p1f_local_supervision;" in library, "Ib module not sealed in crate")
    require("run_stage8b_p1f_local_supervisor_v1" in library, "Ib public entry missing")


def validate_documents(root: Path) -> None:
    document = (root / DOCUMENT).read_text()
    status = (root / STATUS).read_text()
    roadmap = (root / ROADMAP).read_text()
    for fragment in (
        "REVIEW_CANDIDATE_LOCAL_SUPERVISION_ONLY",
        BASE,
        "PR_SET_PDEATHSIG=SIGKILL",
        "starts in 600 seconds",
        "P1F-Ic producer/high-water composition",
        "through O4 and P1F-A remain closed",
    ):
        require(fragment in document, f"Ib document missing: {fragment}")
    for text, name in ((status, STATUS), (roadmap, ROADMAP)):
        require(BASE in text, f"{name}: accepted Ia ref missing")
        require("P1F-Ib" in text and "active source" in text, f"{name}: active Ib status missing")
        require("P1F-Ic" in text and "P1F-Ie" in text, f"{name}: remaining source sequence missing")
    require("P1F-O0 is not unlocked" in status, f"{STATUS}: operational boundary missing")
    require("P1F-O0 stays closed" in roadmap, f"{ROADMAP}: operational boundary missing")


def validate(root: Path = ROOT, *, verify_lineage: bool = True) -> None:
    if verify_lineage:
        validate_lineage(root)
    validate_inventory(root)
    validate_matrix(root)
    validate_source(root)
    validate_documents(root)


def main() -> int:
    try:
        validate()
    except (CheckFailure, OSError, UnicodeDecodeError) as error:
        print(f"FAIL stage8b-p1f-ib-check: {error}")
        return 1
    print("PASS stage8b-p1f-ib-check scenarios=20 real_process_tests=8 operational=false")
    return 0


if __name__ == "__main__":
    sys.exit(main())
