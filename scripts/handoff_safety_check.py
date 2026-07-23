#!/usr/bin/env python3
"""Fail-closed source/archive safety checks for review handoffs."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import stat
import sys
import zipfile
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath

sys.path.insert(0, str(Path(__file__).resolve().parent))
from stage5e_descriptor import descriptor_for_stage
from stage5e_b_no_io_lifecycle_check import (
    EXPECTED_ALLOWED_CHANGED_PATHS as STAGE5E_B_ALLOWED_CHANGED_PATHS,
    EXPECTED_TOP_LEVEL_KEYS as STAGE5E_B_TOP_LEVEL_KEYS,
)


EXCLUDED_PARTS = {".git", "target", "tmp", "reports", "__pycache__", "__MACOSX"}
FORBIDDEN_NAME_PATTERNS = (
    re.compile(r"^\.env$"),
    re.compile(r"^\.env\.(?!example$).+"),
    re.compile(r".*\.log$"),
    re.compile(r".*\.local\..*$"),
)
FORBIDDEN_CONTENT = re.compile(
    rb"(75" rb"02[A-Z0-9]*|190" rb"9892|63" rb"170[A-Z0-9/]*|"
    rb"tapi_[sa]k_[A-Za-z0-9_-]+|"
    rb"eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{10,})"
)
HEX64 = re.compile(r"[0-9a-f]{64}")
HEX40 = re.compile(r"[0-9a-f]{40}")
ISO_UTC = re.compile(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z")


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


def canonical_sha256(value: object) -> str:
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def parse_utc_timestamp(value: object, field: str) -> datetime:
    if not isinstance(value, str) or not ISO_UTC.fullmatch(value):
        raise SystemExit(f"handoff safety: invalid Stage 5E gate timestamp: {field}")
    return datetime.fromisoformat(value.replace("Z", "+00:00")).astimezone(timezone.utc)


def require_hex64(value: object, field: str) -> None:
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        raise SystemExit(f"handoff safety: missing or invalid {field}")


def git_blob_sha1(payload: bytes) -> bytes:
    return hashlib.sha1(b"blob " + str(len(payload)).encode() + b"\0" + payload).digest()


def git_tree_sha1(entries: dict[str, tuple[str, bytes]]) -> str:
    tree: dict[str, object] = {}
    for path, (mode, object_hash) in entries.items():
        parts = path.split("/")
        cursor = tree
        for part in parts[:-1]:
            cursor = cursor.setdefault(part, {})  # type: ignore[assignment]
            if not isinstance(cursor, dict):
                raise SystemExit("handoff safety: invalid source-tree path nesting")
        cursor[parts[-1]] = (mode, object_hash)

    def digest_node(node: dict[str, object]) -> bytes:
        body = bytearray()
        def sort_key(name: str) -> str:
            return f"{name}/" if isinstance(node[name], dict) else name

        for name in sorted(node, key=sort_key):
            value = node[name]
            if isinstance(value, dict):
                mode = "40000"
                digest = digest_node(value)
            else:
                mode, digest = value  # type: ignore[misc]
            body.extend(mode.encode())
            body.extend(b" ")
            body.extend(name.encode())
            body.extend(b"\0")
            body.extend(digest)
        payload = bytes(body)
        return hashlib.sha1(b"tree " + str(len(payload)).encode() + b"\0" + payload).digest()

    return digest_node(tree).hex()


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

        required = {
            "handoff-commit.txt",
            "handoff-manifest.json",
            "handoff-source-tree-manifest.json",
        }
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
                "stage5e_design_scope_sha256",
            ]
        )
        if stage5e_declared and (
            not isinstance(current_review_stage, str)
            or not current_review_stage.startswith("5E-")
        ):
            raise SystemExit("handoff safety: current_review_stage/Stage 5E inventory mismatch")
        if isinstance(current_review_stage, str) and current_review_stage.startswith("5E-"):
            active_descriptor_name = "docs/stage-5/stage5e-active-descriptor.json"
            if active_descriptor_name not in names:
                raise SystemExit("handoff safety: missing active Stage 5E descriptor")
            active_descriptor = json.loads(archive.read(active_descriptor_name))
            if set(active_descriptor) != {"schema_version", "stage"} or active_descriptor.get("schema_version") != 1:
                raise SystemExit("handoff safety: active Stage 5E descriptor schema mismatch")
            try:
                selected = descriptor_for_stage(active_descriptor.get("stage"))
            except ValueError as exc:
                raise SystemExit(f"handoff safety: {exc}") from exc
            if selected["stage"] != current_review_stage:
                raise SystemExit("handoff safety: current_review_stage/Stage 5E inventory mismatch")
            stage5e_inventory_name = selected["inventory"]
            stage5e_plan_name = selected["plan"]
            stage5e_checker_name = selected["checker"]
            if current_review_stage == "5E-b-no-io-lifecycle-capability":
                expected_stage5e_baseline_ref = "23362c291279d45189b108ab8e8fdc8e7f5958d3"
                expected_stage5e_a_freeze_ref = "eb03695dc407b02bb8327de57fde6acea077d96b"
            else:
                expected_stage5e_baseline_ref = "9ebbfd29d0346be5149dac746225866f0c8d0257"
                expected_stage5e_a_freeze_ref = None
            stage5e_gate_result_name = "handoff-stage5e-gate-result.json"
            stage5e_stdout_name = "handoff-stage5e-gate-stdout.txt"
            stage5e_stderr_name = "handoff-stage5e-gate-stderr.txt"
            source_tree_manifest_name = "handoff-source-tree-manifest.json"
            for member in [
                stage5e_inventory_name,
                stage5e_plan_name,
                stage5e_checker_name,
                active_descriptor_name,
                stage5e_gate_result_name,
                stage5e_stdout_name,
                stage5e_stderr_name,
                source_tree_manifest_name,
            ]:
                if member not in names:
                    raise SystemExit(f"handoff safety: missing Stage 5E member: {member}")
            for field, member in [
                ("stage5e_checker_sha256", stage5e_checker_name),
                ("stage5e_inventory_sha256", stage5e_inventory_name),
                ("stage5e_plan_sha256", stage5e_plan_name),
                ("stage5e_gate_result_sha256", stage5e_gate_result_name),
                ("source_tree_manifest_sha256", source_tree_manifest_name),
            ]:
                expected = manifest.get(field)
                if not isinstance(expected, str) or not HEX64.fullmatch(expected):
                    raise SystemExit(f"handoff safety: missing or invalid {field}")
                actual = hashlib.sha256(archive.read(member)).hexdigest()
                if actual != expected:
                    raise SystemExit(f"handoff safety: {field} mismatch")
            stage5e_inventory = json.loads(archive.read(stage5e_inventory_name))
            if not isinstance(stage5e_inventory, dict):
                raise SystemExit("handoff safety: Stage 5E inventory must be a JSON object")
            if current_review_stage != stage5e_inventory.get("stage"):
                raise SystemExit("handoff safety: current_review_stage/Stage 5E inventory mismatch")
            if current_review_stage == "5E-b-no-io-lifecycle-capability":
                if set(stage5e_inventory) != STAGE5E_B_TOP_LEVEL_KEYS:
                    raise SystemExit("handoff safety: Stage 5E-b inventory key set drift")
                if stage5e_inventory.get("allowed_changed_paths") != STAGE5E_B_ALLOWED_CHANGED_PATHS:
                    raise SystemExit("handoff safety: Stage 5E-b allowed_changed_paths drift")
            if stage5e_inventory.get("source_stage5d_aggregate_closure_r2_ref") != "9ebbfd29d0346be5149dac746225866f0c8d0257":
                raise SystemExit("handoff safety: Stage 5E source baseline ref mismatch")
            if stage5e_inventory.get("baseline_ref") != expected_stage5e_baseline_ref:
                raise SystemExit("handoff safety: Stage 5E baseline_ref mismatch")
            if expected_stage5e_a_freeze_ref is not None and stage5e_inventory.get("stage5e_a_freeze_ref") != expected_stage5e_a_freeze_ref:
                raise SystemExit("handoff safety: Stage 5E-a freeze ref mismatch")
            closed = stage5e_inventory.get("closed_surfaces")
            if not isinstance(closed, dict) or any(value is not False for value in closed.values()):
                raise SystemExit("handoff safety: Stage 5E closed-surface mismatch")
            gate_result = json.loads(archive.read(stage5e_gate_result_name))
            if not isinstance(gate_result, dict):
                raise SystemExit("handoff safety: Stage 5E gate result must be a JSON object")
            expected_gate_keys = {
                "command",
                "design_scope",
                "exit_code",
                "finished_at_utc",
                "gate_id",
                "input_sha256",
                "schema_version",
                "source_ref",
                "source_tree_manifest_sha256",
                "source_tree_member_count",
                "started_at_utc",
                "stderr_member",
                "stderr_line_count",
                "stderr_sha256",
                "stdout_member",
                "stdout_line_count",
                "stdout_sha256",
            }
            if set(gate_result) != expected_gate_keys:
                raise SystemExit("handoff safety: Stage 5E gate result key set drift")
            if gate_result.get("schema_version") != 1:
                raise SystemExit("handoff safety: unsupported Stage 5E gate result schema_version")
            if gate_result.get("gate_id") != "stage5e_lifecycle_event_time":
                raise SystemExit("handoff safety: Stage 5E gate result id mismatch")
            if gate_result.get("command") != ["bash", "scripts/stage5e_lifecycle_event_time_gate.sh"]:
                raise SystemExit("handoff safety: Stage 5E gate command mismatch")
            if gate_result.get("exit_code") != 0:
                raise SystemExit("handoff safety: Stage 5E gate did not pass")
            started_at = parse_utc_timestamp(gate_result.get("started_at_utc"), "started_at_utc")
            finished_at = parse_utc_timestamp(gate_result.get("finished_at_utc"), "finished_at_utc")
            if finished_at < started_at:
                raise SystemExit("handoff safety: Stage 5E gate timestamp order invalid")
            require_hex64(gate_result.get("stdout_sha256"), "Stage 5E gate stdout_sha256")
            require_hex64(gate_result.get("stderr_sha256"), "Stage 5E gate stderr_sha256")
            if gate_result.get("stdout_member") != stage5e_stdout_name:
                raise SystemExit("handoff safety: Stage 5E gate stdout member mismatch")
            if gate_result.get("stderr_member") != stage5e_stderr_name:
                raise SystemExit("handoff safety: Stage 5E gate stderr member mismatch")
            if hashlib.sha256(archive.read(stage5e_stdout_name)).hexdigest() != gate_result.get(
                "stdout_sha256"
            ):
                raise SystemExit("handoff safety: Stage 5E gate stdout hash mismatch")
            if hashlib.sha256(archive.read(stage5e_stderr_name)).hexdigest() != gate_result.get(
                "stderr_sha256"
            ):
                raise SystemExit("handoff safety: Stage 5E gate stderr hash mismatch")
            for field in ["stdout_line_count", "stderr_line_count"]:
                if not isinstance(gate_result.get(field), int) or gate_result[field] < 0:
                    raise SystemExit(f"handoff safety: invalid Stage 5E gate {field}")
            input_sha256 = gate_result.get("input_sha256")
            if not isinstance(input_sha256, dict):
                raise SystemExit("handoff safety: Stage 5E gate input hashes must be an object")
            expected_input_keys = {
                "stage5c_checker",
                "stage5d_checker",
                "stage5d_manifest",
                "stage5e_active_descriptor",
                "stage5e_checker",
                "stage5e_descriptor_registry",
                "stage5e_inventory",
                "stage5e_plan",
            }
            if set(input_sha256) != expected_input_keys:
                raise SystemExit("handoff safety: Stage 5E gate input hash key set drift")
            for key, manifest_field, member in [
                ("stage5c_checker", "stage5c_checker_sha256", "scripts/stage5c_api_freeze_check.py"),
                ("stage5d_checker", "stage5d_checker_sha256", "scripts/stage5d_additive_freeze_check.py"),
                ("stage5d_manifest", "stage5d_manifest_sha256", stage5d_manifest_name),
                ("stage5e_active_descriptor", None, active_descriptor_name),
                ("stage5e_checker", "stage5e_checker_sha256", stage5e_checker_name),
                ("stage5e_descriptor_registry", None, "scripts/stage5e_descriptor.py"),
                ("stage5e_inventory", "stage5e_inventory_sha256", stage5e_inventory_name),
                ("stage5e_plan", "stage5e_plan_sha256", stage5e_plan_name),
            ]:
                value = input_sha256.get(key)
                require_hex64(value, f"Stage 5E gate input hash {key}")
                if manifest_field is not None and value != manifest.get(manifest_field):
                    raise SystemExit(f"handoff safety: Stage 5E gate input/manifest mismatch: {key}")
                actual = hashlib.sha256(archive.read(member)).hexdigest()
                if value != actual:
                    raise SystemExit(f"handoff safety: Stage 5E gate input/archive mismatch: {key}")
            design_scope = gate_result.get("design_scope")
            if not isinstance(design_scope, dict):
                raise SystemExit("handoff safety: Stage 5E design scope must be an object")
            expected_design_keys = {
                "baseline_ref",
                "changed_paths",
                "changed_paths_sha256",
                "head_tree",
                "source_ref",
            }
            if set(design_scope) != expected_design_keys:
                raise SystemExit("handoff safety: Stage 5E design scope key set drift")
            require_hex64(manifest.get("stage5e_design_scope_sha256"), "stage5e_design_scope_sha256")
            if canonical_sha256(design_scope) != manifest.get("stage5e_design_scope_sha256"):
                raise SystemExit("handoff safety: Stage 5E design scope hash mismatch")
            if design_scope.get("baseline_ref") != stage5e_inventory.get("baseline_ref"):
                raise SystemExit("handoff safety: Stage 5E design scope baseline mismatch")
            if not isinstance(design_scope.get("source_ref"), str) or not HEX40.fullmatch(design_scope["source_ref"]):
                raise SystemExit("handoff safety: Stage 5E design scope source_ref invalid")
            if not isinstance(design_scope.get("head_tree"), str) or not HEX40.fullmatch(design_scope["head_tree"]):
                raise SystemExit("handoff safety: Stage 5E design scope head_tree invalid")
            changed_paths = design_scope.get("changed_paths")
            if not isinstance(changed_paths, list) or not all(isinstance(item, str) for item in changed_paths):
                raise SystemExit("handoff safety: Stage 5E changed_paths must be a string list")
            if len(changed_paths) != len(set(changed_paths)):
                raise SystemExit("handoff safety: Stage 5E changed_paths contains duplicates")
            if hashlib.sha256(
                json.dumps(changed_paths, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest() != design_scope.get("changed_paths_sha256"):
                raise SystemExit("handoff safety: Stage 5E changed_paths hash mismatch")
            allowed = stage5e_inventory.get("allowed_changed_paths")
            if not isinstance(allowed, list) or not set(changed_paths).issubset(set(allowed)):
                raise SystemExit("handoff safety: Stage 5E design scope allowlist mismatch")
            if changed_paths != allowed:
                raise SystemExit("handoff safety: Stage 5E design scope changed-path set mismatch")
            source_tree_manifest = json.loads(archive.read(source_tree_manifest_name))
            if not isinstance(source_tree_manifest, dict):
                raise SystemExit("handoff safety: source-tree manifest must be a JSON object")
            expected_source_tree_keys = {
                "baseline_ref",
                "changed_paths",
                "excluded_generated_members",
                "head_tree",
                "members",
                "schema_version",
                "source_ref",
            }
            if set(source_tree_manifest) != expected_source_tree_keys:
                raise SystemExit("handoff safety: source-tree manifest key set drift")
            if source_tree_manifest.get("schema_version") != 1:
                raise SystemExit("handoff safety: unsupported source-tree manifest schema_version")
            if source_tree_manifest.get("source_ref") != gate_result.get("source_ref"):
                raise SystemExit("handoff safety: source-tree manifest source_ref mismatch")
            if source_tree_manifest.get("head_tree") != design_scope.get("head_tree"):
                raise SystemExit("handoff safety: source-tree manifest head_tree mismatch")
            if source_tree_manifest.get("baseline_ref") != design_scope.get("baseline_ref"):
                raise SystemExit("handoff safety: source-tree manifest baseline_ref mismatch")
            if source_tree_manifest.get("changed_paths") != changed_paths:
                raise SystemExit("handoff safety: source-tree manifest changed_paths mismatch")
            if gate_result.get("source_tree_manifest_sha256") != manifest.get(
                "source_tree_manifest_sha256"
            ):
                raise SystemExit("handoff safety: gate/source-tree manifest hash mismatch")
            if gate_result.get("source_tree_manifest_sha256") != hashlib.sha256(
                archive.read(source_tree_manifest_name)
            ).hexdigest():
                raise SystemExit("handoff safety: source-tree manifest hash mismatch")
            generated = source_tree_manifest.get("excluded_generated_members")
            if not isinstance(generated, list) or not all(isinstance(item, str) for item in generated):
                raise SystemExit("handoff safety: source-tree generated member list invalid")
            if set(generated) != {
                "handoff-commit.txt",
                "handoff-manifest.json",
                "handoff-stage5e-gate-result.json",
                "handoff-stage5e-gate-stderr.txt",
                "handoff-stage5e-gate-stdout.txt",
                "handoff-source-tree-manifest.json",
            }:
                raise SystemExit("handoff safety: source-tree generated member set drift")
            source_members = source_tree_manifest.get("members")
            if not isinstance(source_members, list):
                raise SystemExit("handoff safety: source-tree members must be a list")
            source_member_map: dict[str, tuple[str, str]] = {}
            for row in source_members:
                if not isinstance(row, dict) or set(row) != {"git_mode", "path", "sha256"}:
                    raise SystemExit("handoff safety: source-tree member row key set drift")
                member_path = row["path"]
                member_sha = row["sha256"]
                git_mode = row["git_mode"]
                if not isinstance(member_path, str) or not member_path:
                    raise SystemExit("handoff safety: source-tree member path invalid")
                if git_mode not in {"100644", "100755"}:
                    raise SystemExit("handoff safety: source-tree member git_mode invalid")
                require_hex64(member_sha, f"source-tree member sha256 {member_path}")
                if member_path in source_member_map:
                    raise SystemExit("handoff safety: duplicate source-tree member")
                source_member_map[member_path] = (git_mode, member_sha)
            if gate_result.get("source_tree_member_count") != len(source_member_map):
                raise SystemExit("handoff safety: source-tree member count mismatch")
            archive_files = {
                info.filename
                for info in archive.infolist()
                if not info.is_dir()
            }
            expected_archive_files = set(source_member_map) | set(generated)
            if archive_files != expected_archive_files:
                raise SystemExit("handoff safety: source-tree/archive member set mismatch")
            git_entries: dict[str, tuple[str, bytes]] = {}
            for member_path, (git_mode, expected_sha) in source_member_map.items():
                payload = archive.read(member_path)
                if hashlib.sha256(payload).hexdigest() != expected_sha:
                    raise SystemExit(f"handoff safety: source-tree member hash mismatch: {member_path}")
                git_entries[member_path] = (git_mode, git_blob_sha1(payload))
            if git_tree_sha1(git_entries) != design_scope.get("head_tree"):
                raise SystemExit("handoff safety: source-tree head_tree mismatch")
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
            if gate_result.get("design_scope", {}).get("source_ref") != source_ref:
                raise SystemExit("handoff safety: Stage 5E design scope source_ref mismatch")
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
