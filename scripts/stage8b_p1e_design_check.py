#!/usr/bin/env python3
"""Fail-closed scope/content checker for the Stage 8B-P1-e R0 design."""

from __future__ import annotations

import csv
import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "c2a9e1246dfdd59f3a6297268de907dedcb19903"
REVIEW_SHA256 = "93180e3f633c256ab9bb2cdfa43dd63c3fe7a1b7380eac435feae45cf8969142"
DESIGN = ROOT / "docs/stage-8/stage8b-p1e-deployable-supervisor-design.md"
MATRIX = ROOT / "docs/stage-8/stage8b-p1e-deployable-supervisor-acceptance-matrix.csv"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1e-deployable-supervisor-design-evidence.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"
EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-design-evidence.json",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-design.md",
    "scripts/make_stage8b_p1e_design_handoff.py",
    "scripts/stage8b_p1e_design_check.py",
    "scripts/stage8b_p1e_design_gate.sh",
    "scripts/stage8b_p1e_design_handoff_safety_check.py",
    "scripts/stage8b_p1e_design_negative_harness.py",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def changed_files() -> set[str]:
    tracked = subprocess.run(
        ["git", "diff", "--name-only", BASE], cwd=ROOT, check=True, text=True,
        capture_output=True,
    ).stdout.splitlines()
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT,
        check=True, text=True, capture_output=True,
    ).stdout.splitlines()
    return set(tracked) | set(untracked)


def validate(
    design: str,
    matrix_text: str,
    evidence: dict[str, object],
    status: str,
    roadmap: str,
) -> None:
    required_design = (
        "Status: R0 design-only review candidate",
        BASE,
        REVIEW_SHA256,
        "P1-f owns isolated operational acceptance",
        "`stage8b-p1-paper-supervisor`",
        "not a `broker-cli` subcommand",
        "`validate-config CONFIG`",
        "`bootstrap CONFIG CONFIRMATION`",
        "`run CONFIG` is restart-only",
        "CREATE_NEW_STAGE8B_P1_DURABLE_ROOT",
        "No mode accepts secret bytes or a caller-selected credential filename/path",
        "The existing P0 units under `deploy/paper-shadow/` remain byte-for-byte",
        "loopback Redis URL with an explicit nonzero database",
        "The checked-in example uses DB15",
        "LoadCredential=stage8b-p1-lifecycle.key",
        "No Redis connection, stream creation, group creation, XADD, XACK or consumer",
        "S00 parse argv and canonical config",
        "S09 publish PaperReady and enter the single-owner loop",
        "One task owns the mutable P1 composition",
        "Internal task restart is forbidden",
        "exact `MAXLEN = 4096`",
        "The only readiness phases are `Starting`, `PaperReady`, `Degraded`, `Draining`",
        "The supervisor must never serialize `LiveReady`",
        "The binary handles SIGTERM and SIGINT directly",
        "A second signal does not bypass the\ndurable protocol",
        "Restart=on-failure",
        "TimeoutStopSec=100",
        "two clean isolated runs",
        "`0 < child_pid <= u32::MAX`",
        "does not authorize installing or starting the resulting service on a VPS",
    )
    for token in required_design:
        require(token in design, f"missing design invariant: {token}")

    rows = list(csv.DictReader(matrix_text.splitlines()))
    require(len(rows) == 48, f"acceptance row count drifted: {len(rows)}")
    require(list(rows[0]) == ["id", "area", "requirement", "status"], "matrix fields drifted")
    require([row["id"] for row in rows] == [f"P1E-{i:03d}" for i in range(1, 49)], "matrix IDs/order drifted")
    require(len({row["id"] for row in rows}) == 48, "duplicate matrix IDs")
    require(all(row["status"] == "REQUIRED" for row in rows), "acceptance weakened")
    by_id = {row["id"]: row["requirement"] for row in rows}
    for row_id, token in {
        "P1E-004": "no broker-finam or finam-gateway dependency",
        "P1E-007": "restart-only",
        "P1E-012": "nonzero database",
        "P1E-016": "before authenticated durable restart",
        "P1E-018": "claim scan completes before any fresh source read",
        "P1E-026": "MAXLEN 4096",
        "P1E-030": "LiveReady is impossible",
        "P1E-034": "next accepted authenticated restart boundary",
        "P1E-039": "timeout 100 seconds",
        "P1E-048": "P1-f isolated operational acceptance",
    }.items():
        require(token in by_id[row_id], f"matrix semantic drift: {row_id}")

    require(evidence.get("stage") == "Stage 8B-P1-e deployable paper supervisor design", "stage drifted")
    require(evidence.get("status") == "R0_DESIGN_REVIEW_CANDIDATE", "status drifted")
    require(evidence.get("accepted_p1d4_closure_ref") == BASE, "predecessor drifted")
    require(evidence.get("accepted_p1d4_review_sha256") == REVIEW_SHA256, "review binding drifted")
    require(evidence.get("acceptance_rows") == 48, "row evidence drifted")
    require(evidence.get("negative_cases") == 36, "negative inventory drifted")
    require(evidence.get("design_only") is True, "design-only marker missing")
    require(evidence.get("implementation_authorized") is False, "implementation opened")
    require(evidence.get("operational_activation_authorized") is False, "activation opened")
    binary = evidence.get("binary_contract")
    require(isinstance(binary, dict), "binary contract missing")
    require(binary.get("package") == "runtime-durable-service", "binary package drifted")
    require(binary.get("broker_cli_subcommand") is False, "broker-cli coupling opened")
    require(binary.get("prohibited_dependencies") == ["broker-finam", "finam-gateway"], "dependency boundary drifted")
    credential = evidence.get("credential_contract")
    require(isinstance(credential, dict), "credential contract missing")
    require(credential.get("expected_bytes") == 32, "credential size drifted")
    require(credential.get("argument_secret_allowed") is False, "argv secret opened")
    require(credential.get("environment_secret_allowed") is False, "env secret opened")
    require(evidence.get("readiness_phases") == ["Starting", "PaperReady", "Degraded", "Draining", "Stopped"], "readiness phases drifted")
    require(evidence.get("startup_phases") == [f"S{i:02d}" for i in range(10)], "startup phases drifted")
    closed = evidence.get("closed_surfaces")
    require(isinstance(closed, dict) and len(closed) == 12, "closed surface inventory drifted")
    require(all(value is False for value in closed.values()), "closed surface opened")
    p2 = evidence.get("deferred_p2")
    require(isinstance(p2, dict) and p2.get("blocks_p1e") is False, "P2 classification drifted")
    require(p2.get("required_bound") == "0 < child_pid <= u32::MAX", "P2 bound drifted")

    for token in (BASE, "active next candidate is the\ndesign-only P1-e", "P1-f retains ownership"):
        require(token in status, f"status drifted: {token}")
    for token in (BASE, "active P1-e R0 design-only", "Acceptance may open only P1-e source implementation"):
        require(token in roadmap, f"roadmap drifted: {token}")


def main() -> None:
    try:
        require(changed_files() == EXPECTED_CHANGED, f"changed path drift: {sorted(changed_files())}")
        validate(
            DESIGN.read_text(encoding="utf-8"),
            MATRIX.read_text(encoding="utf-8"),
            json.loads(EVIDENCE.read_text(encoding="utf-8")),
            STATUS.read_text(encoding="utf-8"),
            ROADMAP.read_text(encoding="utf-8"),
        )
    except (OSError, json.JSONDecodeError, subprocess.CalledProcessError, CheckFailure) as error:
        print(f"stage8b-p1e-design-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("PASS stage8b-p1e-design-scope files=10 rows=48 startup_phases=10 design_only=true")


if __name__ == "__main__":
    main()
