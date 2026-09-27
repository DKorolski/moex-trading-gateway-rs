#!/usr/bin/env python3
"""Fail closed over retained Stage 8B-P1-f O1 operational evidence."""

from __future__ import annotations

import csv
import hashlib
import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BASE = "8864a2bbba64ef930073fae4e71dfcde82ceba58"
BRANCH = "stage8b-paper-shadow-resumption"
REVIEW = "FINAM_P1F_O1_PACKAGE_ACCEPT_8864a2b_2026-09-27.md"
REVIEW_SHA256 = "b3faa0eaceca62b2c7991791cdb348e0530359466b7aca29e9da4dafd8ae16e8"
OUTER_SHA256 = "d8f9695bdb7a29b220dfe1396e31856fa7e71fb8a932dea126cd13e79c78e985"
BUNDLE_SHA256 = "f90fea1357a0f959d119027ef07becd4e1175995223037c83ed9e70c93db73c1"
BINARY_SHA256 = "cee324a4e4f251227a25d4a7b23dda332a94522b45407671982f4fc896614406"
EVIDENCE = "docs/stage-8/stage8b-p1f-o1-operational-evidence.json"
DOCUMENT = "docs/stage-8/stage8b-p1f-o1-operational-evidence.md"
MATRIX = "docs/stage-8/stage8b-p1f-o1-operational-acceptance-matrix.csv"
PRE_RAW = "reports/stage8b/stage8b-p1f-o1-pre-o0-readonly-probe.txt"
POST_RAW = "reports/stage8b/stage8b-p1f-o1-post-install-readonly-probe.txt"
PROBE = "scripts/stage8b_p1f_o1_readonly_probe.sh"
COLLECTOR = "scripts/stage8b_p1f_o1_collect.py"
CHECKER = "scripts/stage8b_p1f_o1_operational_check.py"
NEGATIVE = "scripts/stage8b_p1f_o1_operational_negative_harness.py"
GATE = "scripts/stage8b_p1f_o1_operational_gate.sh"
HANDOFF = "scripts/make_stage8b_p1f_o1_operational_handoff.py"
SAFETY = "scripts/stage8b_p1f_o1_operational_handoff_safety_check.py"
STATUS = "docs/current-status.md"
ROADMAP = "docs/roadmap.md"
ALLOWED_CHANGES = {
    EVIDENCE, DOCUMENT, MATRIX, PRE_RAW, POST_RAW, PROBE, COLLECTOR,
    CHECKER, NEGATIVE, GATE, HANDOFF, SAFETY, STATUS, ROADMAP,
}


class CheckError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckError(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def strict_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(path: Path) -> dict[str, object]:
    value = json.loads(path.read_text(), object_pairs_hook=strict_object)
    require(isinstance(value, dict), "evidence must be an object")
    return value


def validate(root: Path = ROOT) -> None:
    evidence = read_json(root / EVIDENCE)
    require(evidence.get("schema_version") == 1, "schema drift")
    require(evidence.get("stage") == "Stage 8B-P1-f O1 non-activating provisioning operational evidence", "stage drift")
    require(evidence.get("status") == "O1_OPERATIONAL_EVIDENCE_REVIEW_CANDIDATE", "status drift")
    accepted = evidence.get("accepted_package")
    require(isinstance(accepted, dict), "accepted package missing")
    require(accepted == {
        "source_ref": BASE,
        "outer_sha256": OUTER_SHA256,
        "nested_bundle_sha256": BUNDLE_SHA256,
        "binary_sha256": BINARY_SHA256,
        "acceptance_review": REVIEW,
        "acceptance_review_sha256": REVIEW_SHA256,
    }, "accepted package drift")

    execution = evidence.get("installation_execution")
    require(isinstance(execution, dict), "installation execution missing")
    require(execution.get("performed") is True and execution.get("remote_mutation_performed") is True, "installation not declared")
    require(execution.get("installer_exit_code") == 0, "installer did not exit zero")
    require(execution.get("installer_result") == "INSTALLED_OR_ALREADY_EXACT", "installer result drift")
    require(execution.get("post_installer_wrapper_exit_code") == 1, "wrapper disclosure drift")
    require("no second install was run" in execution.get("post_installer_wrapper_failure", ""), "single-install disclosure missing")
    require("EXACT_INSTALLED" in execution.get("verification", ""), "status verification missing")

    pre = evidence.get("pre_o0")
    post = evidence.get("post_install")
    require(isinstance(pre, dict) and isinstance(post, dict), "pre/post evidence missing")
    pre_raw = (root / PRE_RAW).read_bytes()
    post_raw = (root / POST_RAW).read_bytes()
    require(pre.get("raw_probe_path") == PRE_RAW and pre.get("raw_probe_sha256") == sha256(pre_raw), "pre raw mismatch")
    require(post.get("raw_probe_path") == POST_RAW and post.get("raw_probe_sha256") == sha256(post_raw), "post raw mismatch")
    require(pre.get("all_required_checks_passed") is True, "fresh O0 did not pass")
    require(pre.get("db15_size") == 0, "pre DB15 not empty")
    require(post.get("status") == {
        "activation_performed": False,
        "result": "EXACT_INSTALLED",
        "root": "/",
        "units": [
            "moex-finam-p1-paper.service",
            "moex-finam-p1-paper-bootstrap.service",
            "moex-finam-p1-paper-bootstrap-recover@.service",
        ],
    }, "status result drift")
    manifest = post.get("installation_manifest_content")
    require(isinstance(manifest, dict), "manifest evidence missing")
    require(manifest.get("binary_sha256") == BINARY_SHA256, "manifest binary drift")
    for key in (
        "activation_performed", "daemon_reload_performed", "redis_contact_performed",
        "finam_contact_performed", "operator_config_installed",
        "first_boot_source_installed", "lifecycle_credential_installed",
    ):
        require(manifest.get(key) is False, f"manifest boundary opened: {key}")
    require(len(post.get("managed_payloads", [])) == 6, "managed payload inventory drift")
    require(len(post.get("persistent_directories", [])) == 6, "persistent directory inventory drift")
    require(all(item.get("present") is False for item in post.get("operator_files", [])), "operator material present")
    require(post.get("state_extra_entry_count") == 0 and post.get("quarantine_entry_count") == 0, "durable state initialized")
    require(post.get("p1_process_count") == 0 and post.get("p1_recovery_instances") == "", "P1 process/recovery active")
    require(all(item.get("active") != "active" and item.get("enabled") != "enabled" for item in post.get("p1_units", [])), "P1 unit activated")
    require(all(item.get("active_state") == "active" and item.get("sub_state") == "running" for item in post.get("p0_services", {}).values()), "P0 not running")
    require(post.get("redis", {}).get("db15_size") == 0, "post DB15 not empty")
    require(evidence.get("all_required_checks_passed") is True, "required checks did not pass")
    checks = evidence.get("checks")
    require(isinstance(checks, dict) and len(checks) == 15 and all(value is True for value in checks.values()), "check matrix drift")
    closed = evidence.get("closed_surfaces")
    require(isinstance(closed, dict) and len(closed) == 9 and all(value is False for value in closed.values()), "surface opened")

    with (root / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 20 and [row["id"] for row in rows] == [f"O1O-{index:02d}" for index in range(1, 21)], "acceptance matrix drift")
    document = (root / DOCUMENT).read_text()
    for phrase in (
        "OPERATIONAL EVIDENCE REVIEW CANDIDATE",
        "installed once",
        "EXACT_INSTALLED",
        "No `systemctl daemon-reload`, enable or start was performed",
        "O1 does not authorize O2",
        "FINAM POST/DELETE",
    ):
        require(phrase in document, f"documentation boundary missing: {phrase}")
    probe = (root / PROBE).read_text()
    for forbidden in ("redis-cli SET", "redis-cli XADD", "systemctl daemon-reload", "systemctl enable", "systemctl start"):
        require(forbidden not in probe, f"mutating probe command present: {forbidden}")
    status = (root / STATUS).read_text()
    roadmap = (root / ROADMAP).read_text()
    require("O1 operational evidence review candidate" in status, "current status not updated")
    require("O1 operational evidence review candidate" in roadmap, "roadmap not updated")


def main() -> None:
    try:
        validate()
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError, CheckError) as error:
        print(f"stage8b-p1f-o1-operational-check: FAIL {error}")
        raise SystemExit(1) from error
    print("PASS stage8b-p1f-o1-operational-check matrix=20 checks=15 activation=false db15=empty")


if __name__ == "__main__":
    main()
