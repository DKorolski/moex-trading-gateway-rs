#!/usr/bin/env python3
"""Fail-closed source/archive safety checks for review handoffs."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import stat
import zipfile
from pathlib import Path, PurePosixPath


EXCLUDED_PARTS = {".git", "target", "tmp", "reports", "__pycache__", "__MACOSX"}
FORBIDDEN_NAME_PATTERNS = (
    re.compile(r"^\.env(?:\..*)?$"),
    re.compile(r".*\.log$"),
    re.compile(r".*\.local\..*$"),
)
FORBIDDEN_CONTENT = re.compile(
    rb"(75" rb"02[A-Z0-9]*|190" rb"9892|63" rb"170[A-Z0-9/]*|"
    rb"tapi_[sa]k_[A-Za-z0-9_-]+|"
    rb"eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{10,})"
)


def path_is_excluded(path: PurePosixPath) -> bool:
    return any(part in EXCLUDED_PARTS for part in path.parts) or any(
        pattern.fullmatch(path.name) for pattern in FORBIDDEN_NAME_PATTERNS
    ) or path.name == ".DS_Store"


def check_payload(name: str, payload: bytes) -> None:
    if b"\x00" in payload:
        return
    match = FORBIDDEN_CONTENT.search(payload)
    if match:
        raise SystemExit(f"handoff safety: forbidden live-like literal in {name}")


def check_source_tree(root: Path) -> None:
    for path in root.rglob("*"):
        relative = PurePosixPath(path.relative_to(root).as_posix())
        if path_is_excluded(relative):
            continue
        if path.is_symlink():
            raise SystemExit(f"handoff safety: included symlink in source tree: {relative}")
        if path.is_file():
            check_payload(str(relative), path.read_bytes())
    print("handoff-source-safety: ok")


def check_archive(path: Path) -> None:
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise SystemExit("handoff safety: duplicate ZIP entries")
        for info in archive.infolist():
            pure = PurePosixPath(info.filename)
            if pure.is_absolute() or ".." in pure.parts:
                raise SystemExit(f"handoff safety: unsafe ZIP path: {info.filename}")
            if path_is_excluded(pure):
                raise SystemExit(f"handoff safety: excluded artifact in ZIP: {info.filename}")
            mode = info.external_attr >> 16
            if stat.S_ISLNK(mode):
                raise SystemExit(f"handoff safety: symlink in ZIP: {info.filename}")
            if not info.is_dir():
                check_payload(info.filename, archive.read(info))

        required = {"handoff-commit.txt", "handoff-manifest.json"}
        missing = sorted(required - set(names))
        if missing:
            raise SystemExit(f"handoff safety: missing generated markers: {missing}")
        try:
            manifest = json.loads(archive.read("handoff-manifest.json"))
        except json.JSONDecodeError as exc:
            raise SystemExit(f"handoff safety: malformed handoff manifest JSON: {exc}") from exc
        if not isinstance(manifest, dict):
            raise SystemExit("handoff safety: handoff manifest must be a JSON object")
        if manifest.get("schema_version") != 1:
            raise SystemExit("handoff safety: unsupported handoff manifest schema_version")
        review_stage = manifest.get("review_stage")
        if not isinstance(review_stage, str) or not review_stage:
            raise SystemExit("handoff safety: missing review_stage")
        archive_name = manifest.get("archive_name")
        if not isinstance(archive_name, str) or not archive_name:
            raise SystemExit("handoff safety: missing archive_name")
        stage5d_manifest_name = "docs/stage-5/stage-5d-additive-freeze-manifest.json"
        stage5d_manifest = json.loads(archive.read(stage5d_manifest_name))
        if review_stage != stage5d_manifest.get("stage"):
            raise SystemExit("handoff safety: review_stage/freeze-stage mismatch")
        for field, member in [
            ("stage5c_checker_sha256", "scripts/stage5c_api_freeze_check.py"),
            ("stage5d_checker_sha256", "scripts/stage5d_additive_freeze_check.py"),
            ("stage5d_manifest_sha256", stage5d_manifest_name),
        ]:
            expected = manifest.get(field)
            if not isinstance(expected, str) or not re.fullmatch(r"[0-9a-f]{64}", expected):
                raise SystemExit(f"handoff safety: missing or invalid {field}")
            actual = hashlib.sha256(archive.read(member)).hexdigest()
            if actual != expected:
                raise SystemExit(f"handoff safety: {field} mismatch")
        current_review_stage = manifest.get("current_review_stage")
        stage5e_declared = any(
            key in manifest
            for key in [
                "stage5e_checker_sha256",
                "stage5e_inventory_sha256",
                "stage5e_plan_sha256",
                "stage5e_gate_result_sha256",
            ]
        )
        if stage5e_declared and (
            not isinstance(current_review_stage, str)
            or not current_review_stage.startswith("5E-")
        ):
            raise SystemExit("handoff safety: current_review_stage/Stage 5E inventory mismatch")
        if isinstance(current_review_stage, str) and current_review_stage.startswith("5E-"):
            stage5e_inventory_name = (
                "docs/stage-5/stage5e-lifecycle-event-time-attachment-inventory.json"
            )
            stage5e_plan_name = "docs/stage-5/5e-a-lifecycle-event-time-attachment-plan.md"
            stage5e_checker_name = "scripts/stage5e_lifecycle_event_time_freeze_check.py"
            stage5e_gate_result_name = "handoff-stage5e-gate-result.json"
            for member in [
                stage5e_inventory_name,
                stage5e_plan_name,
                stage5e_checker_name,
                stage5e_gate_result_name,
            ]:
                if member not in names:
                    raise SystemExit(f"handoff safety: missing Stage 5E member: {member}")
            for field, member in [
                ("stage5e_checker_sha256", stage5e_checker_name),
                ("stage5e_inventory_sha256", stage5e_inventory_name),
                ("stage5e_plan_sha256", stage5e_plan_name),
                ("stage5e_gate_result_sha256", stage5e_gate_result_name),
            ]:
                expected = manifest.get(field)
                if not isinstance(expected, str) or not re.fullmatch(r"[0-9a-f]{64}", expected):
                    raise SystemExit(f"handoff safety: missing or invalid {field}")
                actual = hashlib.sha256(archive.read(member)).hexdigest()
                if actual != expected:
                    raise SystemExit(f"handoff safety: {field} mismatch")
            stage5e_inventory = json.loads(archive.read(stage5e_inventory_name))
            if not isinstance(stage5e_inventory, dict):
                raise SystemExit("handoff safety: Stage 5E inventory must be a JSON object")
            if current_review_stage != stage5e_inventory.get("stage"):
                raise SystemExit("handoff safety: current_review_stage/Stage 5E inventory mismatch")
            if (
                stage5e_inventory.get("source_stage5d_aggregate_closure_r2_ref")
                != "9ebbfd29d0346be5149dac746225866f0c8d0257"
            ):
                raise SystemExit("handoff safety: Stage 5E source baseline ref mismatch")
            if stage5e_inventory.get("baseline_ref") != "9ebbfd29d0346be5149dac746225866f0c8d0257":
                raise SystemExit("handoff safety: Stage 5E baseline_ref mismatch")
            closed = stage5e_inventory.get("closed_surfaces")
            if not isinstance(closed, dict) or any(value is not False for value in closed.values()):
                raise SystemExit("handoff safety: Stage 5E closed-surface mismatch")
            gate_result = json.loads(archive.read(stage5e_gate_result_name))
            if not isinstance(gate_result, dict):
                raise SystemExit("handoff safety: Stage 5E gate result must be a JSON object")
            if gate_result.get("gate_id") != "stage5e_lifecycle_event_time":
                raise SystemExit("handoff safety: Stage 5E gate result id mismatch")
            if gate_result.get("exit_code") != 0:
                raise SystemExit("handoff safety: Stage 5E gate did not pass")
        source_commit = manifest.get("source_commit")
        source_ref = manifest.get("source_ref")
        if not isinstance(source_commit, str) or not re.fullmatch(
            r"[0-9a-f]{7,12}", source_commit
        ):
            raise SystemExit("handoff safety: missing or invalid source_commit")
        if not isinstance(source_ref, str) or not re.fullmatch(r"[0-9a-f]{40}", source_ref):
            raise SystemExit("handoff safety: missing or invalid source_ref")
        if not source_ref.startswith(source_commit):
            raise SystemExit("handoff safety: source short/full commit mismatch")
        marker = archive.read("handoff-commit.txt").decode().splitlines()
        expected_marker = [
            f"source_commit={source_commit}",
            f"source_ref={source_ref}",
            f"archive_name={archive_name}",
        ]
        if marker != expected_marker or archive_name != path.name:
            raise SystemExit("handoff safety: provenance marker/manifest mismatch")
        if isinstance(current_review_stage, str) and current_review_stage.startswith("5E-"):
            gate_result = json.loads(archive.read("handoff-stage5e-gate-result.json"))
            if gate_result.get("source_ref") != source_ref:
                raise SystemExit("handoff safety: Stage 5E gate source_ref mismatch")
    print("handoff-archive-safety: ok")


def main() -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--source-tree", type=Path)
    group.add_argument("--archive", type=Path)
    args = parser.parse_args()
    if args.source_tree:
        check_source_tree(args.source_tree.resolve())
    else:
        check_archive(args.archive.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
