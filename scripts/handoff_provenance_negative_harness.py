#!/usr/bin/env python3
"""Negative tests for handoff semantic provenance."""

from __future__ import annotations

import hashlib
import json
import argparse
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile
from dataclasses import dataclass
from pathlib import Path

sys.dont_write_bytecode = True

from copy_review_baseline import copy_review_baseline
from stage5e_descriptor import select_stage5e_descriptor


ROOT = Path(__file__).resolve().parents[1]
ARCHIVE_NAME = "moex-trading-project-0000000.zip"
SOURCE_COMMIT = "0000000"
SOURCE_REF = "0000000000000000000000000000000000000000"
EXCLUDED_PARTS = {".git", "target", "tmp", "reports", "__pycache__", "__MACOSX"}
FORBIDDEN_NAME_PATTERNS = (
    re.compile(r"^\.env$"),
    re.compile(r"^\.env\.(?!example$).+"),
    re.compile(r".*\.log$"),
    re.compile(r".*\.local\..*$"),
)


@dataclass(frozen=True)
class Case:
    name: str
    expected: str
    mutator: object
    stage: str = "5E-a-lifecycle-event-time-attachment-plan"
    checker_only: bool = False


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canonical_sha256(value: object) -> str:
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def git_blob_sha1(payload: bytes) -> bytes:
    return hashlib.sha1(b"blob " + str(len(payload)).encode() + b"\0" + payload).digest()


def git_tree_sha1(entries: dict[str, tuple[str, bytes]]) -> str:
    tree: dict[str, object] = {}
    for path, (mode, object_hash) in entries.items():
        parts = path.split("/")
        cursor = tree
        for part in parts[:-1]:
            cursor = cursor.setdefault(part, {})  # type: ignore[assignment]
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


def path_is_excluded(path: str) -> bool:
    parts = path.split("/")
    name = parts[-1]
    return (
        any(part in EXCLUDED_PARTS for part in parts)
        or any(pattern.fullmatch(name) for pattern in FORBIDDEN_NAME_PATTERNS)
        or name == ".DS_Store"
    )


def write_manifest(root: Path, mutate=None) -> None:
    freeze_manifest = json.loads(
        (root / "docs/stage-5/stage-5d-additive-freeze-manifest.json").read_text()
    )
    descriptor = select_stage5e_descriptor(root)
    stage5e_inventory = json.loads((root / descriptor["inventory"]).read_text())
    stage5c_checker_sha256 = sha256(root / "scripts/stage5c_api_freeze_check.py")
    stage5d_checker_sha256 = sha256(root / "scripts/stage5d_additive_freeze_check.py")
    stage5d_manifest_sha256 = sha256(root / "docs/stage-5/stage-5d-additive-freeze-manifest.json")
    stage5e_checker_sha256 = sha256(root / descriptor["checker"])
    stage5e_inventory_sha256 = sha256(root / descriptor["inventory"])
    stage5e_plan_sha256 = sha256(root / descriptor["plan"])
    stage5e_active_descriptor_sha256 = sha256(
        root / "docs/stage-5/stage5e-active-descriptor.json"
    )
    stage5e_descriptor_registry_sha256 = sha256(root / "scripts/stage5e_descriptor.py")
    index_lines = subprocess.check_output(["git", "ls-files", "-s"], cwd=ROOT, text=True).splitlines()
    source_members = []
    git_entries = {}
    for line in sorted(index_lines, key=lambda item: item.split("\t", 1)[1]):
        meta, rel = line.split("\t", 1)
        if path_is_excluded(rel):
            continue
        mode = meta.split()[0]
        payload = (root / rel).read_bytes()
        source_members.append({"git_mode": mode, "path": rel, "sha256": hashlib.sha256(payload).hexdigest()})
        git_entries[rel] = (mode, git_blob_sha1(payload))
    head_tree = git_tree_sha1(git_entries)
    design_scope = {
        "baseline_ref": stage5e_inventory["baseline_ref"],
        "changed_paths": stage5e_inventory["allowed_changed_paths"],
        "changed_paths_sha256": hashlib.sha256(
            json.dumps(
                stage5e_inventory["allowed_changed_paths"],
                separators=(",", ":"),
                sort_keys=True,
            ).encode()
        ).hexdigest(),
        "head_tree": head_tree,
        "source_ref": SOURCE_REF,
    }
    (root / "handoff-stage5e-gate-stdout.txt").write_text("stage5e-lifecycle-event-time-gate: ok\n")
    (root / "handoff-stage5e-gate-stderr.txt").write_text("")
    (root / "handoff-cargo-gate-stdout.txt").write_text("cargo gate: ok\n")
    (root / "handoff-cargo-gate-stderr.txt").write_text("")
    cargo_result_path = root / "handoff-cargo-gate-result.json"
    cargo_result_path.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "source_ref": SOURCE_REF,
                "cargo_version": "cargo test-fixture",
                "commands": [
                    ["cargo", "fmt", "--check"],
                    ["cargo", "test", "--workspace", "--all-targets"],
                    ["cargo", "test", "--workspace", "--doc"],
                    ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
                ],
                "exit_code": 0,
                "started_at_utc": "2026-01-01T00:00:00Z",
                "finished_at_utc": "2026-01-01T00:00:01Z",
                "stdout_member": "handoff-cargo-gate-stdout.txt",
                "stderr_member": "handoff-cargo-gate-stderr.txt",
                "stdout_sha256": sha256(root / "handoff-cargo-gate-stdout.txt"),
                "stderr_sha256": sha256(root / "handoff-cargo-gate-stderr.txt"),
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )
    provenance_stdout_path = root / "handoff-provenance-negative-stdout.txt"
    provenance_stderr_path = root / "handoff-provenance-negative-stderr.txt"
    provenance_result_path = root / "handoff-provenance-negative-result.json"
    provenance_stdout_path.write_text("PASS synthetic-fixture\n")
    provenance_stderr_path.write_text("")
    provenance_result_path.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "gate_id": "handoff_provenance_negative",
                "command": ["python3", "scripts/handoff_provenance_negative_harness.py"],
                "source_ref": SOURCE_REF,
                "started_at_utc": "2026-01-01T00:00:00Z",
                "finished_at_utc": "2026-01-01T00:00:01Z",
                "exit_code": 0,
                "passed_cases": 1,
                "stdout_member": "handoff-provenance-negative-stdout.txt",
                "stderr_member": "handoff-provenance-negative-stderr.txt",
                "stdout_sha256": sha256(provenance_stdout_path),
                "stderr_sha256": sha256(provenance_stderr_path),
                "source_tree_manifest_sha256": "0" * 64,
                "source_tree_member_count": 0,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )
    heavy_negative_results = []
    for prefix, gate_id, command, passed_cases in [
        (
            "stage5d",
            "stage5d_additive_freeze_negative",
            ["python3", "scripts/stage5d_additive_freeze_negative_harness.py"],
            303,
        ),
        (
            "forbidden",
            "forbidden_surface_negative",
            ["bash", "scripts/forbidden_surface_negative_harness.sh"],
            87,
        ),
    ]:
        stdout_path = root / f"handoff-{prefix}-negative-stdout.txt"
        stderr_path = root / f"handoff-{prefix}-negative-stderr.txt"
        result_path = root / f"handoff-{prefix}-negative-result.json"
        stdout_path.write_text(
            "".join(f"PASS synthetic-{prefix}-{index}\n" for index in range(passed_cases))
        )
        stderr_path.write_text("")
        result_path.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "gate_id": gate_id,
                    "command": command,
                    "source_ref": SOURCE_REF,
                    "started_at_utc": "2026-01-01T00:00:00Z",
                    "finished_at_utc": "2026-01-01T00:00:01Z",
                    "exit_code": 0,
                    "passed_cases": passed_cases,
                    "stdout_member": stdout_path.name,
                    "stderr_member": stderr_path.name,
                    "stdout_sha256": sha256(stdout_path),
                    "stderr_sha256": sha256(stderr_path),
                    "source_tree_manifest_sha256": "0" * 64,
                    "source_tree_member_count": 0,
                },
                indent=2,
                sort_keys=True,
            )
            + "\n"
        )
        heavy_negative_results.append((prefix, result_path))
    source_tree_manifest = {
        "schema_version": 1,
        "source_ref": SOURCE_REF,
        "head_tree": design_scope["head_tree"],
        "baseline_ref": design_scope["baseline_ref"],
        "changed_paths": design_scope["changed_paths"],
        "excluded_generated_members": [
            "handoff-commit.txt",
            "handoff-cargo-gate-result.json",
            "handoff-cargo-gate-stderr.txt",
            "handoff-cargo-gate-stdout.txt",
            "handoff-forbidden-negative-result.json",
            "handoff-forbidden-negative-stderr.txt",
            "handoff-forbidden-negative-stdout.txt",
            "handoff-manifest.json",
            "handoff-provenance-negative-result.json",
            "handoff-provenance-negative-stderr.txt",
            "handoff-provenance-negative-stdout.txt",
            "handoff-stage5d-negative-result.json",
            "handoff-stage5d-negative-stderr.txt",
            "handoff-stage5d-negative-stdout.txt",
            "handoff-stage5e-gate-result.json",
            "handoff-stage5e-gate-stderr.txt",
            "handoff-stage5e-gate-stdout.txt",
            "handoff-source-tree-manifest.json",
        ],
        "members": source_members,
    }
    source_tree_manifest_path = root / "handoff-source-tree-manifest.json"
    source_tree_manifest_path.write_text(
        json.dumps(source_tree_manifest, indent=2, sort_keys=True) + "\n"
    )
    source_tree_manifest_sha256 = sha256(source_tree_manifest_path)
    cargo_result = json.loads(cargo_result_path.read_text())
    cargo_result["source_tree_manifest_sha256"] = source_tree_manifest_sha256
    cargo_result["source_tree_member_count"] = len(source_members)
    cargo_result_path.write_text(json.dumps(cargo_result, indent=2, sort_keys=True) + "\n")
    provenance_result = json.loads(provenance_result_path.read_text())
    provenance_result["source_tree_manifest_sha256"] = source_tree_manifest_sha256
    provenance_result["source_tree_member_count"] = len(source_members)
    provenance_result_path.write_text(
        json.dumps(provenance_result, indent=2, sort_keys=True) + "\n"
    )
    for _prefix, result_path in heavy_negative_results:
        result = json.loads(result_path.read_text())
        result["source_tree_manifest_sha256"] = source_tree_manifest_sha256
        result["source_tree_member_count"] = len(source_members)
        result_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    gate_result_path = root / "handoff-stage5e-gate-result.json"
    gate_result_path.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "gate_id": "stage5e_lifecycle_event_time",
                "command": ["bash", "scripts/stage5e_lifecycle_event_time_gate.sh"],
                "source_ref": SOURCE_REF,
                "started_at_utc": "2026-01-01T00:00:00Z",
                "finished_at_utc": "2026-01-01T00:00:01Z",
                "exit_code": 0,
                "stdout_sha256": sha256(root / "handoff-stage5e-gate-stdout.txt"),
                "stderr_sha256": sha256(root / "handoff-stage5e-gate-stderr.txt"),
                "stdout_member": "handoff-stage5e-gate-stdout.txt",
                "stderr_member": "handoff-stage5e-gate-stderr.txt",
                "stdout_line_count": 1,
                "stderr_line_count": 0,
                "input_sha256": {
                    "stage5c_checker": stage5c_checker_sha256,
                    "stage5d_checker": stage5d_checker_sha256,
                    "stage5d_manifest": stage5d_manifest_sha256,
                    "stage5e_checker": stage5e_checker_sha256,
                    "stage5e_inventory": stage5e_inventory_sha256,
                    "stage5e_plan": stage5e_plan_sha256,
                    "stage5e_active_descriptor": stage5e_active_descriptor_sha256,
                    "stage5e_descriptor_registry": stage5e_descriptor_registry_sha256,
                },
                "design_scope": design_scope,
                "source_tree_manifest_sha256": source_tree_manifest_sha256,
                "source_tree_member_count": len(source_members),
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )
    manifest = {
        "schema_version": 1,
        "current_review_stage": stage5e_inventory["stage"],
        "review_stage": freeze_manifest["stage"],
        "source_commit": SOURCE_COMMIT,
        "source_ref": SOURCE_REF,
        "archive_name": ARCHIVE_NAME,
        "stage5c_checker_sha256": stage5c_checker_sha256,
        "stage5d_checker_sha256": stage5d_checker_sha256,
        "stage5d_manifest_sha256": stage5d_manifest_sha256,
        "stage5e_checker_sha256": stage5e_checker_sha256,
        "stage5e_inventory_sha256": stage5e_inventory_sha256,
        "stage5e_plan_sha256": stage5e_plan_sha256,
        "stage5e_gate_result_sha256": sha256(gate_result_path),
        "stage5e_design_scope_sha256": canonical_sha256(design_scope),
        "source_tree_manifest_sha256": source_tree_manifest_sha256,
        "cargo_gate_result_sha256": sha256(cargo_result_path),
        "provenance_negative_result_sha256": sha256(provenance_result_path),
        "stage5d_negative_result_sha256": sha256(
            root / "handoff-stage5d-negative-result.json"
        ),
        "forbidden_negative_result_sha256": sha256(
            root / "handoff-forbidden-negative-result.json"
        ),
    }
    marker = {
        "source_commit": manifest["source_commit"],
        "source_ref": manifest["source_ref"],
        "archive_name": manifest["archive_name"],
    }
    manifest_payload: object = manifest
    duplicate_member: str | None = None
    if mutate is not None:
        result = mutate(root, manifest, marker)
        if isinstance(result, dict):
            manifest_payload = result.get("manifest_payload", manifest_payload)
            duplicate_member = result.get("duplicate_member")
    if isinstance(manifest_payload, str):
        (root / "handoff-manifest.json").write_text(manifest_payload)
    else:
        (root / "handoff-manifest.json").write_text(
            json.dumps(manifest_payload, indent=2, sort_keys=True) + "\n"
        )
    (root / "handoff-commit.txt").write_text(
        "\n".join(
            [
                f"source_commit={marker.get('source_commit', SOURCE_COMMIT)}",
                f"source_ref={marker.get('source_ref', SOURCE_REF)}",
                f"archive_name={marker.get('archive_name', ARCHIVE_NAME)}",
            ]
        )
        + "\n"
    )
    if duplicate_member:
        (root / ".duplicate-handoff-member").write_text(duplicate_member)


def build_archive(root: Path, archive_path: Path) -> None:
    duplicate_member_path = root / ".duplicate-handoff-member"
    duplicate_member = (
        duplicate_member_path.read_text().strip() if duplicate_member_path.exists() else None
    )
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(root.rglob("*")):
            if path.is_dir():
                continue
            rel = path.relative_to(root).as_posix()
            if rel == ".duplicate-handoff-member":
                continue
            archive.write(path, rel)
        if duplicate_member:
            archive.writestr(duplicate_member, b"duplicate")


def run_case(base: Path, case: Case) -> tuple[bool, str]:
    root = base / case.name
    copy_review_baseline(ROOT, root)
    try:
        (root / "docs/stage-5/stage5e-active-descriptor.json").write_text(
            json.dumps({"schema_version": 1, "stage": case.stage}) + "\n"
        )
        if case.checker_only:
            case.mutator(root, {}, {})
            descriptor = select_stage5e_descriptor(root)
            result = subprocess.run(
                ["python3", str(root / descriptor["checker"])],
                cwd=root,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            combined = result.stdout + result.stderr
            if result.returncode == 0:
                return False, "checker mutation unexpectedly passed"
            if case.expected not in combined:
                return False, f"expected marker {case.expected!r} missing\n{combined}"
            return True, ""
        write_manifest(root, case.mutator)
        archive_path = base / ARCHIVE_NAME
        archive_path.unlink(missing_ok=True)
        build_archive(root, archive_path)
        result = subprocess.run(
            ["python3", str(ROOT / "scripts/handoff_safety_check.py"), "--archive", str(archive_path)],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        combined = result.stdout + result.stderr
        if result.returncode == 0:
            return False, "mutation unexpectedly passed"
        if "Traceback" in combined or "KeyError" in combined:
            return False, f"unexpected uncontrolled Python failure\n{combined}"
        if case.expected not in combined:
            return False, f"expected marker {case.expected!r} missing\n{combined}"
        return True, ""
    finally:
        shutil.rmtree(root, ignore_errors=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--case-start", type=int, default=0)
    parser.add_argument("--case-end", type=int)
    args = parser.parse_args()
    def pop_field(field: str):
        return lambda _root, manifest, _marker: manifest.pop(field)

    def set_field(field: str, value: object):
        return lambda _root, manifest, _marker: manifest.__setitem__(field, value)

    def alter_member(path: str):
        return lambda root, _manifest, _marker: (root / path).write_text("altered\n")

    def refresh_gate_result_hash(root: Path, manifest: dict[str, object]) -> None:
        manifest["stage5e_gate_result_sha256"] = sha256(root / "handoff-stage5e-gate-result.json")

    def refresh_gate_input_and_result_hashes(root: Path, manifest: dict[str, object]) -> None:
        descriptor = select_stage5e_descriptor(root)
        gate_path = root / "handoff-stage5e-gate-result.json"
        gate = json.loads(gate_path.read_text())
        gate["input_sha256"] = {
            "stage5c_checker": sha256(root / "scripts/stage5c_api_freeze_check.py"),
            "stage5d_checker": sha256(root / "scripts/stage5d_additive_freeze_check.py"),
            "stage5d_manifest": sha256(root / "docs/stage-5/stage-5d-additive-freeze-manifest.json"),
            "stage5e_checker": sha256(root / descriptor["checker"]),
            "stage5e_inventory": sha256(root / descriptor["inventory"]),
            "stage5e_plan": sha256(root / descriptor["plan"]),
            "stage5e_active_descriptor": sha256(
                root / "docs/stage-5/stage5e-active-descriptor.json"
            ),
            "stage5e_descriptor_registry": sha256(root / "scripts/stage5e_descriptor.py"),
        }
        gate_path.write_text(json.dumps(gate, indent=2, sort_keys=True) + "\n")
        refresh_gate_result_hash(root, manifest)

    def mutate_stage5e_inventory(mutator):
        def apply(root, _manifest, _marker):
            path = root / "docs/stage-5/stage5e-lifecycle-event-time-attachment-inventory.json"
            payload = json.loads(path.read_text())
            mutator(payload)
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")

        return apply

    def mutate_stage5e_inventory_rehash_manifest_only(mutator):
        def apply(root, manifest, _marker):
            path = root / "docs/stage-5/stage5e-lifecycle-event-time-attachment-inventory.json"
            payload = json.loads(path.read_text())
            mutator(payload)
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
            manifest["stage5e_inventory_sha256"] = sha256(path)

        return apply

    def mutate_stage5e_inventory_and_rehash(mutator):
        def apply(root, manifest, _marker):
            path = root / "docs/stage-5/stage5e-lifecycle-event-time-attachment-inventory.json"
            payload = json.loads(path.read_text())
            mutator(payload)
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
            manifest["stage5e_inventory_sha256"] = sha256(path)
            refresh_gate_input_and_result_hashes(root, manifest)

        return apply

    def mutate_stage5e_b_inventory_and_rehash(mutator):
        def apply(root, manifest, _marker):
            path = root / "docs/stage-5/stage5e-b-no-io-lifecycle-inventory.json"
            payload = json.loads(path.read_text())
            mutator(payload)
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
            manifest["stage5e_inventory_sha256"] = sha256(path)
            refresh_gate_input_and_result_hashes(root, manifest)

        return apply

    def mutate_stage5e_b_plan_and_rehash(mutator):
        def apply(root, manifest, _marker):
            path = root / "docs/stage-5/5e-b-no-io-lifecycle-capability-plan.md"
            path.write_text(mutator(path.read_text()))
            manifest["stage5e_plan_sha256"] = sha256(path)
            refresh_gate_input_and_result_hashes(root, manifest)

        return apply

    def mutate_stage5e_b_plan_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = root / "docs/stage-5/5e-b-no-io-lifecycle-capability-plan.md"
            path.write_text(mutator(path.read_text()))

        return apply

    def mutate_stage5e_b_contract_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = root / "docs/stage-5/stage5e-b-no-io-lifecycle-inventory.json"
            payload = json.loads(path.read_text())
            mutator(payload["contract_invariants"])
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")

        return apply

    def mutate_stage5e_b3_inventory_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = root / "docs/stage-5/stage5e-b3-schedule-window-evidence-inventory.json"
            payload = json.loads(path.read_text())
            mutator(payload)
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")

        return apply

    def mutate_stage5e_b3c_inventory_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = root / "docs/stage-5/stage5e-b3c-private-eligibility-seam-inventory.json"
            payload = json.loads(path.read_text())
            mutator(payload)
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")

        return apply

    def mutate_stage5e_b3c_plan_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = root / "docs/stage-5/5e-b3c-private-eligibility-seam-plan.md"
            path.write_text(mutator(path.read_text()))

        return apply

    def mutate_stage5e_b3c_module_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = root / "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs"
            path.write_text(mutator(path.read_text()))

        return apply

    def mutate_stage5e_authority_inventory_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = root / "docs/stage-5/stage5e-b3c-source-authority-freeze-extension-inventory.json"
            payload = json.loads(path.read_text())
            mutator(payload)
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")

        return apply

    def mutate_stage5e_authority_plan_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = root / "docs/stage-5/5e-b3c-source-authority-freeze-extension-plan.md"
            path.write_text(mutator(path.read_text()))

        return apply

    def mutate_stage5e_authority_source_for_checker(path_value, mutator):
        def apply(root, _manifest, _marker):
            path = root / path_value
            mutated = mutator(path.read_text())
            path.write_text(mutated)

        return apply

    def mutate_stage5e_b3d_inventory_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = (
                root
                / "docs/stage-5/stage5e-b3d-callback-authority-design-inventory.json"
            )
            payload = json.loads(path.read_text())
            mutator(payload)
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")

        return apply

    def mutate_stage5e_b3d_source_for_checker(path_value, mutator):
        def apply(root, _manifest, _marker):
            path = root / path_value
            path.write_text(mutator(path.read_text()))

        return apply

    def mutate_stage5e_b3e_inventory_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = (
                root
                / "docs/stage-5/stage5e-b3e-callback-invocation-design-inventory.json"
            )
            payload = json.loads(path.read_text())
            mutator(payload)
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")

        return apply

    def mutate_stage5e_b3e_plan_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = root / "docs/stage-5/5e-b3e-callback-invocation-design.md"
            path.write_text(mutator(path.read_text()))

        return apply

    def mutate_stage5e_b3e_source_for_checker(path_value, mutator):
        def apply(root, _manifest, _marker):
            path = root / path_value
            path.write_text(mutator(path.read_text()))

        return apply

    def mutate_stage5e_b3f_inventory_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = (
                root
                / "docs/stage-5/stage5e-b3f-callback-settlement-escrow-design-inventory.json"
            )
            payload = json.loads(path.read_text())
            mutator(payload)
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")

        return apply

    def mutate_stage5e_b3f_plan_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = root / "docs/stage-5/5e-b3f-callback-settlement-escrow-design.md"
            path.write_text(mutator(path.read_text()))

        return apply

    def mutate_stage5e_b3f_source_for_checker(path_value, mutator):
        def apply(root, _manifest, _marker):
            path = root / path_value
            path.write_text(mutator(path.read_text()))

        return apply

    def mutate_stage5e_b3_module_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = root / "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs"
            path.write_text(mutator(path.read_text()))

        return apply

    def mutate_stage5e_b_module_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = root / "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs"
            path.write_text(mutator(path.read_text()))

        return apply

    def mutate_stage5e_b_host_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = root / "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
            path.write_text(mutator(path.read_text()))

        return apply

    def mutate_stage5e_b_builder_for_checker(mutator):
        def apply(root, _manifest, _marker):
            path = root / "scripts/make_handoff_archive.sh"
            path.write_text(mutator(path.read_text()))

        return apply

    def mutate_stage5e_plan_rehash_manifest_only(mutator):
        def apply(root, manifest, _marker):
            path = root / "docs/stage-5/5e-a-lifecycle-event-time-attachment-plan.md"
            path.write_text(mutator(path.read_text()))
            manifest["stage5e_plan_sha256"] = sha256(path)

        return apply

    def mutate_stage5e_checker_rehash_manifest_only(mutator):
        def apply(root, manifest, _marker):
            path = root / "scripts/stage5e_lifecycle_event_time_freeze_check.py"
            path.write_text(mutator(path.read_text()))
            manifest["stage5e_checker_sha256"] = sha256(path)

        return apply

    def mutate_stage5e_gate_result(mutator):
        def apply(root, _manifest, _marker):
            path = root / "handoff-stage5e-gate-result.json"
            payload = json.loads(path.read_text())
            mutator(payload)
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")

        return apply

    def mutate_stage5e_gate_result_and_rehash(mutator):
        def apply(root, manifest, _marker):
            path = root / "handoff-stage5e-gate-result.json"
            payload = json.loads(path.read_text())
            mutator(payload)
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
            refresh_gate_result_hash(root, manifest)

        return apply

    def mutate_design_scope_and_rehash_gate(mutator):
        def apply(root, manifest, _marker):
            path = root / "handoff-stage5e-gate-result.json"
            payload = json.loads(path.read_text())
            mutator(payload["design_scope"])
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
            refresh_gate_result_hash(root, manifest)

        return apply

    def mutate_design_scope_and_rehash_all(mutator):
        def apply(root, manifest, _marker):
            path = root / "handoff-stage5e-gate-result.json"
            payload = json.loads(path.read_text())
            mutator(payload["design_scope"])
            manifest["stage5e_design_scope_sha256"] = canonical_sha256(payload["design_scope"])
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
            refresh_gate_result_hash(root, manifest)

        return apply

    def mutate_source_tree_manifest_and_rehash(mutator):
        def apply(root, manifest, _marker):
            path = root / "handoff-source-tree-manifest.json"
            payload = json.loads(path.read_text())
            mutator(payload)
            path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
            manifest["source_tree_manifest_sha256"] = sha256(path)
            provenance_path = root / "handoff-provenance-negative-result.json"
            provenance = json.loads(provenance_path.read_text())
            provenance["source_tree_manifest_sha256"] = manifest["source_tree_manifest_sha256"]
            provenance["source_tree_member_count"] = len(payload["members"])
            provenance_path.write_text(
                json.dumps(provenance, indent=2, sort_keys=True) + "\n"
            )
            manifest["provenance_negative_result_sha256"] = sha256(provenance_path)
            gate_path = root / "handoff-stage5e-gate-result.json"
            gate = json.loads(gate_path.read_text())
            gate["source_tree_manifest_sha256"] = manifest["source_tree_manifest_sha256"]
            gate_path.write_text(json.dumps(gate, indent=2, sort_keys=True) + "\n")
            refresh_gate_result_hash(root, manifest)

        return apply

    def mutate_file_after_source_manifest(path: str, payload: str):
        return lambda root, _manifest, _marker: (root / path).write_text(payload)

    def marker_set(field: str, value: str):
        return lambda _root, _manifest, marker: marker.__setitem__(field, value)

    cases = [
        Case(
            "malformed-json",
            "malformed handoff manifest JSON",
            lambda _root, _manifest, _marker: {"manifest_payload": "{not-json"},
        ),
        Case(
            "non-object-manifest",
            "handoff manifest must be a JSON object",
            lambda _root, _manifest, _marker: {"manifest_payload": ["not", "object"]},
        ),
        Case(
            "unsupported-schema",
            "unsupported handoff manifest schema_version",
            set_field("schema_version", 2),
        ),
        Case(
            "missing-schema",
            "unsupported handoff manifest schema_version",
            pop_field("schema_version"),
        ),
        Case(
            "missing-review-stage",
            "missing review_stage",
            pop_field("review_stage"),
        ),
        Case(
            "empty-review-stage",
            "missing review_stage",
            set_field("review_stage", ""),
        ),
        Case(
            "stale-review-stage",
            "review_stage/freeze-stage mismatch",
            set_field("review_stage", "5D-b2b-c1-r3"),
        ),
        Case(
            "freeze-stage-mismatch",
            "review_stage/freeze-stage mismatch",
            lambda root, _manifest, _marker: (
                (root / "docs/stage-5/stage-5d-additive-freeze-manifest.json").write_text(
                    json.dumps(
                        {
                            **json.loads(
                                (
                                    root
                                    / "docs/stage-5/stage-5d-additive-freeze-manifest.json"
                                ).read_text()
                            ),
                            "stage": "5D-b2b-c1-r3",
                        },
                        indent=2,
                        sort_keys=True,
                    )
                    + "\n"
                )
            ),
        ),
        Case(
            "missing-stage5c-checker-hash",
            "missing or invalid stage5c_checker_sha256",
            pop_field("stage5c_checker_sha256"),
        ),
        Case(
            "stale-stage5c-checker-hash",
            "stage5c_checker_sha256 mismatch",
            set_field("stage5c_checker_sha256", "0" * 64),
        ),
        Case(
            "altered-stage5c-checker-hash",
            "stage5c_checker_sha256 mismatch",
            alter_member("scripts/stage5c_api_freeze_check.py"),
        ),
        Case(
            "missing-stage5d-checker-hash",
            "missing or invalid stage5d_checker_sha256",
            pop_field("stage5d_checker_sha256"),
        ),
        Case(
            "stale-stage5d-checker-hash",
            "stage5d_checker_sha256 mismatch",
            set_field("stage5d_checker_sha256", "0" * 64),
        ),
        Case(
            "altered-stage5d-checker-hash",
            "stage5d_checker_sha256 mismatch",
            alter_member("scripts/stage5d_additive_freeze_check.py"),
        ),
        Case(
            "missing-current-review-stage",
            "current_review_stage/Stage 5E inventory mismatch",
            pop_field("current_review_stage"),
        ),
        Case(
            "stage5e-current-stage-mismatch",
            "current_review_stage/Stage 5E inventory mismatch",
            set_field("current_review_stage", "5E-b-wrong-stage"),
        ),
        Case(
            "missing-stage5e-checker-hash",
            "missing or invalid stage5e_checker_sha256",
            pop_field("stage5e_checker_sha256"),
        ),
        Case(
            "altered-stage5e-checker-hash",
            "stage5e_checker_sha256 mismatch",
            alter_member("scripts/stage5e_lifecycle_event_time_freeze_check.py"),
        ),
        Case(
            "stage5e-checker-rehashed-stale-gate-input",
            "Stage 5E gate input/manifest mismatch: stage5e_checker",
            mutate_stage5e_checker_rehash_manifest_only(lambda text: text + "\n# mutated\n"),
        ),
        Case(
            "missing-stage5e-inventory-hash",
            "missing or invalid stage5e_inventory_sha256",
            pop_field("stage5e_inventory_sha256"),
        ),
        Case(
            "altered-stage5e-inventory-hash",
            "stage5e_inventory_sha256 mismatch",
            mutate_stage5e_inventory(lambda payload: payload["closed_surfaces"].__setitem__("redis", True)),
        ),
        Case(
            "stage5e-inventory-rehashed-stale-gate-input",
            "Stage 5E gate input/manifest mismatch: stage5e_inventory",
            mutate_stage5e_inventory_rehash_manifest_only(
                lambda payload: payload["stage5e_a_claims"].__setitem__(
                    "design_inventory_only", False
                )
            ),
        ),
        Case(
            "missing-stage5e-plan-hash",
            "missing or invalid stage5e_plan_sha256",
            pop_field("stage5e_plan_sha256"),
        ),
        Case(
            "altered-stage5e-plan-hash",
            "stage5e_plan_sha256 mismatch",
            alter_member("docs/stage-5/5e-a-lifecycle-event-time-attachment-plan.md"),
        ),
        Case(
            "stage5e-plan-rehashed-stale-gate-input",
            "Stage 5E gate input/manifest mismatch: stage5e_plan",
            mutate_stage5e_plan_rehash_manifest_only(lambda text: text + "\npost-gate mutation\n"),
        ),
        Case(
            "missing-stage5e-gate-result-hash",
            "missing or invalid stage5e_gate_result_sha256",
            pop_field("stage5e_gate_result_sha256"),
        ),
        Case(
            "altered-stage5e-gate-result-hash",
            "stage5e_gate_result_sha256 mismatch",
            mutate_stage5e_gate_result(lambda payload: payload.__setitem__("exit_code", 1)),
        ),
        Case(
            "missing-stage5e-gate-input-key",
            "Stage 5E gate input hash key set drift",
            mutate_stage5e_gate_result_and_rehash(
                lambda payload: payload["input_sha256"].pop("stage5e_plan")
            ),
        ),
        Case(
            "extra-stage5e-gate-input-key",
            "Stage 5E gate input hash key set drift",
            mutate_stage5e_gate_result_and_rehash(
                lambda payload: payload["input_sha256"].__setitem__("extra", "0" * 64)
            ),
        ),
        Case(
            "stage5e-gate-schema-drift",
            "unsupported Stage 5E gate result schema_version",
            mutate_stage5e_gate_result_and_rehash(
                lambda payload: payload.__setitem__("schema_version", 999)
            ),
        ),
        Case(
            "stage5e-gate-command-drift",
            "Stage 5E gate command mismatch",
            mutate_stage5e_gate_result_and_rehash(
                lambda payload: payload.__setitem__("command", ["false"])
            ),
        ),
        Case(
            "stage5e-gate-invalid-started-at",
            "invalid Stage 5E gate timestamp: started_at_utc",
            mutate_stage5e_gate_result_and_rehash(
                lambda payload: payload.__setitem__("started_at_utc", "not-a-time")
            ),
        ),
        Case(
            "stage5e-gate-reversed-timestamps",
            "Stage 5E gate timestamp order invalid",
            mutate_stage5e_gate_result_and_rehash(
                lambda payload: (
                    payload.__setitem__("started_at_utc", "2026-01-01T00:00:02Z"),
                    payload.__setitem__("finished_at_utc", "2026-01-01T00:00:01Z"),
                )
            ),
        ),
        Case(
            "stage5e-gate-invalid-stdout-hash",
            "missing or invalid Stage 5E gate stdout_sha256",
            mutate_stage5e_gate_result_and_rehash(
                lambda payload: payload.__setitem__("stdout_sha256", "not-a-sha")
            ),
        ),
        Case(
            "stage5e-gate-negative-line-count",
            "invalid Stage 5E gate stdout_line_count",
            mutate_stage5e_gate_result_and_rehash(
                lambda payload: payload.__setitem__("stdout_line_count", -1)
            ),
        ),
        Case(
            "stage5e-design-scope-manifest-hash-mismatch",
            "Stage 5E design scope hash mismatch",
            mutate_design_scope_and_rehash_gate(
                lambda scope: scope.__setitem__("changed_paths_sha256", "1" * 64)
            ),
        ),
        Case(
            "stage5e-design-scope-extra-key",
            "Stage 5E design scope key set drift",
            mutate_design_scope_and_rehash_gate(lambda scope: scope.__setitem__("extra", True)),
        ),
        Case(
            "source-tree-rust-member-changed-after-manifest",
            "source-tree member hash mismatch: crates/broker-core/src/lib.rs",
            mutate_file_after_source_manifest("crates/broker-core/src/lib.rs", "// post-gate mutation\n"),
        ),
        Case(
            "source-tree-extra-archive-member",
            "source-tree/archive member set mismatch",
            mutate_file_after_source_manifest("unexpected-extra.txt", "extra\n"),
        ),
        Case(
            "source-tree-member-removed-after-manifest",
            "source-tree/archive member set mismatch",
            lambda root, _manifest, _marker: (root / "README.md").unlink(),
        ),
        Case(
            "source-tree-forged-head-tree",
            "cargo gate/source-tree manifest mismatch",
            lambda root, manifest, _marker: (
                mutate_design_scope_and_rehash_all(
                    lambda scope: scope.__setitem__("head_tree", "0" * 40)
                )(root, manifest, _marker),
                mutate_source_tree_manifest_and_rehash(
                    lambda payload: payload.__setitem__("head_tree", "0" * 40)
                )(root, manifest, _marker),
            ),
        ),
        Case(
            "source-tree-stale-manifest-hash",
            "source_tree_manifest_sha256 mismatch",
            lambda root, _manifest, _marker: (
                root / "handoff-source-tree-manifest.json"
            ).write_text("{}\n"),
        ),
        Case(
            "source-tree-omitted-changed-path",
            "source-tree manifest changed_paths mismatch",
            mutate_source_tree_manifest_and_rehash(
                lambda payload: payload.__setitem__(
                    "changed_paths",
                    [path for path in payload["changed_paths"] if path != "README.md"],
                )
            ),
        ),
        Case(
            "stage5e-source-baseline-mismatch",
            "Stage 5E source baseline ref mismatch",
            mutate_stage5e_inventory_and_rehash(
                lambda payload: payload.__setitem__(
                    "source_stage5d_aggregate_closure_r2_ref",
                    "1" * 40,
                )
            ),
        ),
        Case(
            "stage5e-baseline-ref-mismatch",
            "Stage 5E baseline_ref mismatch",
            mutate_stage5e_inventory_and_rehash(
                lambda payload: payload.__setitem__("baseline_ref", "1" * 40)
            ),
        ),
        Case(
            "stage5e-gate-id-mismatch",
            "Stage 5E gate result id mismatch",
            mutate_stage5e_gate_result_and_rehash(
                lambda payload: payload.__setitem__("gate_id", "wrong")
            ),
        ),
        Case(
            "stage5e-gate-source-ref-mismatch",
            "source-tree manifest source_ref mismatch",
            mutate_stage5e_gate_result_and_rehash(
                lambda payload: payload.__setitem__("source_ref", "1" * 40)
            ),
        ),
        Case(
            "missing-stage5d-manifest-hash",
            "missing or invalid stage5d_manifest_sha256",
            pop_field("stage5d_manifest_sha256"),
        ),
        Case(
            "stale-stage5d-manifest-hash",
            "stage5d_manifest_sha256 mismatch",
            set_field("stage5d_manifest_sha256", "0" * 64),
        ),
        Case(
            "malformed-stage5d-manifest-hash",
            "missing or invalid stage5d_manifest_sha256",
            set_field("stage5d_manifest_sha256", "not-a-sha"),
        ),
        Case(
            "missing-source-commit",
            "missing or invalid source_commit",
            pop_field("source_commit"),
        ),
        Case(
            "malformed-source-commit",
            "missing or invalid source_commit",
            set_field("source_commit", "nothex"),
        ),
        Case(
            "missing-source-ref",
            "missing or invalid source_ref",
            pop_field("source_ref"),
        ),
        Case(
            "malformed-source-ref",
            "missing or invalid source_ref",
            set_field("source_ref", "0" * 39),
        ),
        Case(
            "bad-short-full-relation",
            "source short/full commit mismatch",
            set_field("source_commit", "abcdef0"),
        ),
        Case(
            "missing-archive-name",
            "missing archive_name",
            pop_field("archive_name"),
        ),
        Case(
            "archive-name-mismatch",
            "provenance marker/manifest mismatch",
            set_field("archive_name", "wrong.zip"),
        ),
        Case(
            "marker-source-commit-mismatch",
            "provenance marker/manifest mismatch",
            marker_set("source_commit", "1111111"),
        ),
        Case(
            "marker-source-ref-mismatch",
            "provenance marker/manifest mismatch",
            marker_set("source_ref", "1" * 40),
        ),
        Case(
            "marker-archive-name-mismatch",
            "provenance marker/manifest mismatch",
            marker_set("archive_name", "wrong.zip"),
        ),
        Case(
            "duplicate-handoff-manifest-member",
            "duplicate ZIP entries",
            lambda _root, _manifest, _marker: {
                "duplicate_member": "handoff-manifest.json"
            },
        ),
        Case(
            "stage5e-b-extra-inventory-key",
            "Stage 5E-b inventory key set drift",
            mutate_stage5e_b_inventory_and_rehash(lambda payload: payload.__setitem__("extra", True)),
            "5E-b-no-io-lifecycle-capability",
        ),
        Case(
            "stage5e-b-production-path-self-authorized",
            "Stage 5E-b allowed_changed_paths drift",
            mutate_stage5e_b_inventory_and_rehash(
                lambda payload: payload["allowed_changed_paths"].append("crates/broker-core/src/lib.rs")
            ),
            "5E-b-no-io-lifecycle-capability",
        ),
        Case(
            "stage5e-b-duplicate-allowed-path",
            "Stage 5E-b allowed_changed_paths drift",
            mutate_stage5e_b_inventory_and_rehash(
                lambda payload: payload["allowed_changed_paths"].append(payload["allowed_changed_paths"][0])
            ),
            "5E-b-no-io-lifecycle-capability",
        ),
        Case(
            "stage5e-b-wrong-baseline",
            "Stage 5E baseline_ref mismatch",
            mutate_stage5e_b_inventory_and_rehash(lambda payload: payload.__setitem__("baseline_ref", "0" * 40)),
            "5E-b-no-io-lifecycle-capability",
        ),
        Case(
            "stage5e-b-freshness-weakened",
            "market freshness inequality weakened",
            mutate_stage5e_b_plan_for_checker(
                lambda text: text.replace(
                    "last_history_bar_close < observed_live_bar_close",
                    "last_history_bar_close <= observed_live_bar_close",
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-instrument-check-removed",
            "Stage 5E-b1 contextual condition missing",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "if bar_instrument != target_instrument {",
                    "if false && bar_instrument != target_instrument {",
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-tick-check-removed",
            "Stage 5E-b1 contextual condition missing",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "if bar_tick_size.to_bits() != admission_tick_size.to_bits() {",
                    "if false && bar_tick_size.to_bits() != admission_tick_size.to_bits() {",
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-future-check-removed",
            "Stage 5E-b1 contextual condition missing",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "if bar_close > lifecycle_now.timestamp() {",
                    "if false && bar_close > lifecycle_now.timestamp() {",
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-expiry-check-removed",
            "Stage 5E-b1 contextual condition missing",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "if lifecycle_now > admission_expires_at {",
                    "if false && lifecycle_now > admission_expires_at {",
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-test-clock-made-crate-wide",
            "deterministic clock seam must be test-only",
            mutate_stage5e_b_host_for_checker(
                lambda text: text.replace("#[cfg(test)]\npub(crate) fn stage5e_try_observe_live_bar_after_history_at", "pub(crate) fn stage5e_try_observe_live_bar_after_history_at")
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-validator-early-success",
            "Stage 5E-b1 validator region hash mismatch",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "    if origin != broker_core::HybridRuntimeBarOrigin::Live {",
                    "    if true {\n        return Ok(());\n    }\n    if origin != broker_core::HybridRuntimeBarOrigin::Live {",
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-proof-callback-count-one",
            "Stage 5E-b1 capability proof region hash mismatch",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace("pub(crate) fn callback_count(&self) -> usize {\n        0", "pub(crate) fn callback_count(&self) -> usize {\n        1")
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-proof-intent-count-one",
            "Stage 5E-b1 capability proof region hash mismatch",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace("pub(crate) fn intent_count(&self) -> usize {\n        0", "pub(crate) fn intent_count(&self) -> usize {\n        1")
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-proof-strategy-called-true",
            "Stage 5E-b1 capability proof region hash mismatch",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace("pub(crate) fn strategy_was_called(&self) -> bool {\n        false", "pub(crate) fn strategy_was_called(&self) -> bool {\n        true")
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-proof-executable-intent-true",
            "Stage 5E-b1 capability proof region hash mismatch",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace("pub(crate) fn executable_intent_created(&self) -> bool {\n        false", "pub(crate) fn executable_intent_created(&self) -> bool {\n        true")
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-callback-surface-introduced",
            "forbidden Stage 5E-b1 bridge surface: on_broker_bar",
            mutate_stage5e_b_host_for_checker(
                lambda text: text.replace(
                    "// STAGE5E-NO-IO-BRIDGE-END: contextual-observation-v1",
                    "// on_broker_bar\n// STAGE5E-NO-IO-BRIDGE-END: contextual-observation-v1",
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-cargo-runner-fail-open",
            "cargo runner must fail closed per command",
            mutate_stage5e_b_builder_for_checker(
                lambda text: text.replace("  set -euo pipefail\n  cd \"$repo_root\"", "  cd \"$repo_root\"")
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-contract-callback-count",
            "contract invariants drift",
            mutate_stage5e_b_contract_for_checker(lambda contract: contract.__setitem__("callback_count", 1)),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-contract-intent-count",
            "contract invariants drift",
            mutate_stage5e_b_contract_for_checker(lambda contract: contract.__setitem__("intent_count", 1)),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-contract-calls-strategy",
            "contract invariants drift",
            mutate_stage5e_b_contract_for_checker(lambda contract: contract.__setitem__("calls_strategy", True)),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-contract-executable-intent",
            "contract invariants drift",
            mutate_stage5e_b_contract_for_checker(lambda contract: contract.__setitem__("creates_executable_intent", True)),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-session-open-check-removed",
            "Stage 5E-b2 session condition missing",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "if session_state != broker_core::BrokerMarketSessionState::Open {",
                    "if false && session_state != broker_core::BrokerMarketSessionState::Open {",
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-session-freshness-check-removed",
            "Stage 5E-b2 session condition missing",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "if !schedule_freshness.available",
                    "if false && !schedule_freshness.available",
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-session-window-check-removed",
            "Stage 5E-b2 session condition missing",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "if bar_close_ts < observed_open_from_bar_close\n            || bar_close_ts > observed_open_until_bar_close\n        {",
                    "if false && (bar_close_ts < observed_open_from_bar_close\n            || bar_close_ts > observed_open_until_bar_close)\n        {",
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-session-callback-surface-introduced",
            "forbidden Stage 5E-b2 session surface: on_broker_bar",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "// STAGE5E-NO-IO-SESSION-ELIGIBILITY-BEGIN: observed-open-session-v1",
                    "// STAGE5E-NO-IO-SESSION-ELIGIBILITY-BEGIN: observed-open-session-v1\n// on_broker_bar",
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-contract-session-observation-mode",
            "contract invariants drift",
            mutate_stage5e_b_contract_for_checker(
                lambda contract: contract.__setitem__("session_observation_mode", "inferred_calendar")
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-session-receipt-made-copyable",
            "forbidden Stage 5E-b2 receipt derivation or constructor surface: Clone",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "#[derive(Debug, PartialEq, Eq)]\n    pub(super) struct Stage5eObservedOpenSession",
                    "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n    pub(super) struct Stage5eObservedOpenSession",
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-session-receipt-field-made-crate-visible",
            "Stage 5E-b2 session eligibility region hash mismatch",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace("    bar_close_ts: i64,", "    pub(crate) bar_close_ts: i64,")
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-session-receipt-default-forge",
            "forbidden Stage 5E-b2 alternate receipt construction or export: impl Default for Stage5eObservedOpenSession",
            mutate_stage5e_b_module_for_checker(
                lambda text: text + "\nimpl Default for Stage5eObservedOpenSession {\n    fn default() -> Self { Self { bar_close_ts: 0 } }\n}\n"
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-qualified-second-receipt-impl",
            "receipt type leaked outside sealed session region",
            mutate_stage5e_b_module_for_checker(
                lambda text: text + "\nimpl session_eligibility::Stage5eObservedOpenSession { fn forged(&self) {} }\n"
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-qualified-free-receipt-forge",
            "forbidden Stage 5E-b2 free receipt forge function",
            mutate_stage5e_b_module_for_checker(
                lambda text: text + "\nfn forge_open_session() -> session_eligibility::Stage5eObservedOpenSession { unsafe { core::mem::zeroed() } }\n"
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b-qualified-manual-copy",
            "receipt type leaked outside sealed session region",
            mutate_stage5e_b_module_for_checker(
                lambda text: text + "\nimpl core::marker::Copy for session_eligibility::Stage5eObservedOpenSession {}\n"
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b3-validator-early-success",
            "b3 schedule evidence region hash mismatch",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "if snapshot.sessions.is_empty() {",
                    "if false && snapshot.sessions.is_empty() {",
                    1,
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3-mapping-expiry-removed",
            "b3 schedule evidence region hash mismatch",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "if lifecycle_now.0 > stage4.expires_at.0 {",
                    "if false && lifecycle_now.0 > stage4.expires_at.0 {",
                    1,
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3-constant-fingerprint",
            "b3 schedule evidence region hash mismatch",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "ScheduleFingerprint(encoder.finish())",
                    "ScheduleFingerprint([0; 32])",
                    1,
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3-closed-surface-key-removed",
            "closed surface key set drift",
            mutate_stage5e_b3_inventory_for_checker(
                lambda payload: payload["closed_surfaces"].pop("strategy_callback")
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3-extra-inventory-key",
            "inventory key set drift",
            mutate_stage5e_b3_inventory_for_checker(
                lambda payload: payload.__setitem__("unexpected", True)
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3-canonical-broker-symbol-relaxed",
            "b3 schedule evidence region hash mismatch",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "if ticker != snapshot.instrument.symbol || mic != snapshot.venue_mic {",
                    "if false && (ticker != snapshot.instrument.symbol || mic != snapshot.venue_mic) {",
                    1,
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3-inclusive-endpoint-overlap-relaxed",
            "b3 schedule evidence region hash mismatch",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "session.start.0 <= end",
                    "session.start.0 < end",
                    1,
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3-stage4-report-future-check-relaxed",
            "b3 schedule evidence region hash mismatch",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "if report_checked_ts > lifecycle_now.0 {",
                    "if false && report_checked_ts > lifecycle_now.0 {",
                    1,
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3-independent-evidence-relation-drift",
            "contract invariant drift",
            mutate_stage5e_b3_inventory_for_checker(
                lambda payload: payload["contract_invariants"].__setitem__(
                    "stage4_normalized_relation", "stage4_only"
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3b-instrument-binding-relaxed",
            "b3 schedule evidence region hash mismatch",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "if schedule_window.instrument != *observed_bar_instrument {",
                    "if false && schedule_window.instrument != *observed_bar_instrument {",
                    1,
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3b-window-expiry-relaxed",
            "b3 schedule evidence region hash mismatch",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "if lifecycle_now.0 > schedule_window.expires_at.0 {",
                    "if false && lifecycle_now.0 > schedule_window.expires_at.0 {",
                    1,
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3b-future-observed-bar-relaxed",
            "b3 schedule evidence region hash mismatch",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "if observed_bar_close_ts.0 > lifecycle_now.0.timestamp() {",
                    "if false && observed_bar_close_ts.0 > lifecycle_now.0.timestamp() {",
                    1,
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3b-inclusive-upper-bound-relaxed",
            "b3 schedule evidence region hash mismatch",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "observed_bar_close_ts.0 > schedule_window.open_until.0",
                    "observed_bar_close_ts.0 >= schedule_window.open_until.0",
                    1,
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3b-linear-retry-contract-drift",
            "contract invariant drift",
            mutate_stage5e_b3_inventory_for_checker(
                lambda payload: payload["contract_invariants"].__setitem__(
                    "returns_linear_inputs_on_binding_block", False
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3b-production-clock-relaxed",
            "b3 schedule evidence region hash mismatch",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "LifecycleInstant(Utc::now())",
                    "LifecycleInstant(chrono::DateTime::<Utc>::UNIX_EPOCH)",
                    1,
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3b-clock-rewind-check-relaxed",
            "b3 schedule evidence region hash mismatch",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "if lifecycle_now.0 < schedule_window.effective_observed_at.0 {",
                    "if false && lifecycle_now.0 < schedule_window.effective_observed_at.0 {",
                    1,
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3b-successful-unbinding-reintroduced",
            "b3 schedule evidence region hash mismatch",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "impl Stage5eBoundScheduleWindowForObservedLiveBar {\n        fn callback_count(&self) -> usize {",
                    "impl Stage5eBoundScheduleWindowForObservedLiveBar {\n        fn into_inputs(self) {}\n\n        fn callback_count(&self) -> usize {",
                    1,
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3b-binding-fingerprint-constant",
            "b3 schedule evidence region hash mismatch",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "ScheduleObservedBarBindingFingerprint(encoder.finish())",
                    "ScheduleObservedBarBindingFingerprint([0; 32])",
                    1,
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3b-monotonic-contract-drift",
            "contract invariant drift",
            mutate_stage5e_b3_inventory_for_checker(
                lambda payload: payload["contract_invariants"].__setitem__(
                    "successful_binding_is_monotonic", False
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3b-strategy-ownership-made-optional",
            "Stage 5E-b3b observed receipt must retain mandatory Stage 5C ownership",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "strategy: HybridIntradayRuntimeStrategy,",
                    "strategy: Option<HybridIntradayRuntimeStrategy>,",
                    1,
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b3b-recovery-ownership-made-optional",
            "Stage 5E-b3b observed receipt must retain mandatory Stage 5C ownership",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "recovery_receipt: Stage5cPendingRecoveryReceipt,",
                    "recovery_receipt: Option<Stage5cPendingRecoveryReceipt>,",
                    1,
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b3b-empty-state-test-constructor",
            "forbidden Stage 5E-b3b empty-state or forge surface: test_only_for_schedule_binding",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "impl Stage5eObservedLiveBarAfterHistory {\n    pub(crate) fn bar_close_ts",
                    "impl Stage5eObservedLiveBarAfterHistory {\n    #[cfg(test)]\n    fn test_only_for_schedule_binding() {}\n\n    pub(crate) fn bar_close_ts",
                    1,
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b3b-free-forged-observed-bar",
            "forbidden Stage 5E-b3b empty-state or forge surface: forge_observed_live_bar_without_stage5c",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "// STAGE5E-NO-IO-SESSION-ELIGIBILITY-BEGIN",
                    "fn forge_observed_live_bar_without_stage5c() -> Stage5eObservedLiveBarAfterHistory { unreachable!() }\n\n// STAGE5E-NO-IO-SESSION-ELIGIBILITY-BEGIN",
                    1,
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b3b-second-associated-constructor",
            "Stage 5E-b3b alternate receipt constructor detected",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "impl Stage5eObservedLiveBarAfterHistory {\n    pub(crate) fn bar_close_ts",
                    "impl Stage5eObservedLiveBarAfterHistory {\n    fn forged() -> Self { unreachable!() }\n\n    pub(crate) fn bar_close_ts",
                    1,
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b3b-direct-observed-bar-literal",
            "Stage 5E-b3b receipt struct literal escaped its sealed constructor",
            mutate_stage5e_b_module_for_checker(
                lambda text: text.replace(
                    "// STAGE5E-NO-IO-SESSION-ELIGIBILITY-BEGIN",
                    "fn direct_observed_bar_literal() { let _ = Stage5eObservedLiveBarAfterHistory { strategy: unreachable!(), recovery_receipt: unreachable!(), bar: unreachable!(), tick_size: 0.5 }; }\n\n// STAGE5E-NO-IO-SESSION-ELIGIBILITY-BEGIN",
                    1,
                )
            ),
            "5E-b-no-io-lifecycle-capability",
            True,
        ),
        Case(
            "stage5e-b3b-actual-ownership-retention-test-removed",
            "b3 schedule evidence region hash mismatch",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "bound.observed_live_bar.ownership_fingerprint(),\n                expected_ownership",
                    "bound.observed_live_bar.callback_count(),\n                0",
                    1,
                )
            ),
            "5E-b3-schedule-window-evidence",
            True,
        ),
        Case(
            "stage5e-b3c-session-contract-removed",
            "contract invariant drift",
            mutate_stage5e_b3c_inventory_for_checker(
                lambda payload: payload["contract_invariants"].__setitem__(
                    "requires_separate_session_calendar_sequence_receipts", False
                )
            ),
            "5E-b3c-private-eligibility-seam",
            True,
        ),
        Case(
            "stage5e-b3c-callback-authority-opened",
            "contract invariant drift",
            mutate_stage5e_b3c_inventory_for_checker(
                lambda payload: payload["contract_invariants"].__setitem__("callback_ready", True)
            ),
            "5E-b3c-private-eligibility-seam",
            True,
        ),
        Case(
            "stage5e-b3c-calendar-inference-opened",
            "contract invariant drift",
            mutate_stage5e_b3c_inventory_for_checker(
                lambda payload: payload["contract_invariants"].__setitem__("calendar_inference_allowed", True)
            ),
            "5E-b3c-private-eligibility-seam",
            True,
        ),
        Case(
            "stage5e-b3c-b2-repurpose-contradiction",
            "forbidden plan contradiction",
            mutate_stage5e_b3c_plan_for_checker(
                lambda text: text + "\nb2 session receipt may be repurposed\n"
            ),
            "5E-b3c-private-eligibility-seam",
            True,
        ),
        Case(
            "stage5e-b3c-evidence-field-schema-drift",
            "exact evidence_contracts contract drift",
            mutate_stage5e_b3c_inventory_for_checker(
                lambda payload: payload["evidence_contracts"]["fresh_open_session"].__setitem__("required_fields", ["full_instrument_id"])
            ), "5E-b3c-private-eligibility-seam", True,
        ),
        Case(
            "stage5e-b3c-source-authority-drift",
            "exact source_authorities contract drift",
            mutate_stage5e_b3c_inventory_for_checker(
                lambda payload: payload["source_authorities"].__setitem__("calendar", "caller bool")
            ), "5E-b3c-private-eligibility-seam", True,
        ),
        Case(
            "stage5e-b3c-transition-check-drift",
            "exact transition_contract contract drift",
            mutate_stage5e_b3c_inventory_for_checker(
                lambda payload: payload["transition_contract"].__setitem__("checks", ["always_true"])
            ), "5E-b3c-private-eligibility-seam", True,
        ),
        Case(
            "stage5e-b3c-blocker-taxonomy-drift",
            "exact block_reasons contract drift",
            mutate_stage5e_b3c_inventory_for_checker(
                lambda payload: payload["block_reasons"].__setitem__("retryable", [f"X{i}" for i in range(25)])
            ), "5E-b3c-private-eligibility-seam", True,
        ),
        Case(
            "stage5e-b3c-evidence-receipt-made-parent-visible",
            "b3c evidence region hash mismatch",
            mutate_stage5e_b3c_module_for_checker(
                lambda text: text.replace(
                    "struct Stage5eFreshOpenSessionEvidence {",
                    "pub(super) struct Stage5eFreshOpenSessionEvidence {",
                    1,
                )
            ),
            "5E-b3c-private-eligibility-seam",
            True,
        ),
        Case(
            "stage5e-b3c-evidence-gap-check-relaxed",
            "b3c evidence region hash mismatch",
            mutate_stage5e_b3c_module_for_checker(
                lambda text: text.replace(
                    "|| !source.gap_free",
                    "|| false && !source.gap_free",
                    1,
                )
            ),
            "5E-b3c-private-eligibility-seam",
            True,
        ),
        Case(
            "stage5e-b3c-evidence-freshness-check-relaxed",
            "b3c evidence region hash mismatch",
            mutate_stage5e_b3c_module_for_checker(
                lambda text: text.replace(
                    "fresh(source.observed_at.0, source.expires_at.0, now)",
                    "Ok(())",
                    1,
                )
            ),
            "5E-b3c-private-eligibility-seam",
            True,
        ),
        Case(
            "stage5e-b3c-evidence-intent-surface-injected",
            "b3c evidence region hash mismatch",
            mutate_stage5e_b3c_module_for_checker(
                lambda text: text.replace(
                    "mod legacy_b3c_evidence {",
                    "mod legacy_b3c_evidence {\n    fn on_broker_bar() {}",
                    1,
                )
            ),
            "5E-b3c-private-eligibility-seam",
            True,
        ),
        Case(
            "stage5e-b3c-b3b-core-mutated-through-nested-enclave",
            "b3b predecessor freeze failed",
            mutate_stage5e_b3_module_for_checker(
                lambda text: text.replace(
                    "if lifecycle_now.0 > schedule_window.expires_at.0 {",
                    "if false && lifecycle_now.0 > schedule_window.expires_at.0 {",
                    1,
                )
            ),
            "5E-b3c-private-eligibility-seam",
            True,
        ),
        Case(
            "stage5e-b3c-combined-receipt-made-parent-visible",
            "b3c evidence region hash mismatch",
            mutate_stage5e_b3c_module_for_checker(
                lambda text: text.replace(
                    "\n        struct Stage5eBoundSessionCalendarSequenceForObservedLiveBar {",
                    "\n        pub(super) struct Stage5eBoundSessionCalendarSequenceForObservedLiveBar {",
                    1,
                )
            ),
            "5E-b3c-private-eligibility-seam",
            True,
        ),
        Case(
            "stage5e-b3c-event-key-conjunction-relaxed",
            "b3c evidence region hash mismatch",
            mutate_stage5e_b3c_module_for_checker(
                lambda text: text.replace(
                    "if session.event_key_fingerprint != b3b.binding_fingerprint.0",
                    "if false && session.event_key_fingerprint != b3b.binding_fingerprint.0",
                    1,
                )
            ),
            "5E-b3c-private-eligibility-seam",
            True,
        ),
        Case(
            "stage5e-b3c-continuation-epoch-conjunction-relaxed",
            "b3c evidence region hash mismatch",
            mutate_stage5e_b3c_module_for_checker(
                lambda text: text.replace(
                    "if session.continuation_epoch != calendar.continuation_epoch",
                    "if false && session.continuation_epoch != calendar.continuation_epoch",
                    1,
                )
            ),
            "5E-b3c-private-eligibility-seam",
            True,
        ),
        Case(
            "stage5e-b3c-blocked-transition-drops-sequence",
            "b3c evidence region hash mismatch",
            mutate_stage5e_b3c_module_for_checker(
                lambda text: text.replace(
                    "(self.b3b, self.session, self.calendar, self.sequence)",
                    "(self.b3b, self.session, self.calendar, unreachable!())",
                    1,
                )
            ),
            "5E-b3c-private-eligibility-seam",
            True,
        ),
        Case(
            "stage5e-authority-stage4-open-field-removed",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["boundary_authority_contract"]["projection_fields"].pop()
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-stage4-owner-replaced",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["boundary_authority_contract"].__setitem__(
                    "owner", "caller_supplied_schedule"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-continuation-constant",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["identity_derivation_contract"]["continuation_binding"].__setitem__(
                    "constant_epoch_allowed", True
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-gap-boundary-relaxed",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["linear_transition_contract"].__setitem__(
                    "boundary_rule", "caller_supplied_boolean"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-required-test-removed",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["required_implementation_tests"].pop()
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-production-path-widened",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["future_production_implementation_paths"].append(
                    "crates/broker-finam/src/lib.rs"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-stage4-baseline-removed",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["production_source_baselines"].pop(
                    "crates/broker-core/src/stage4_bootstrap.rs"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-plan-contradicts-no-io",
            "authority freeze plan drift",
            mutate_stage5e_authority_plan_for_checker(
                lambda text: text + "\nThis package authorizes a callback.\n"
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r2-boundary-owner-replaced",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["boundary_authority_contract"].__setitem__(
                    "owner", "caller_supplied_schedule"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r2-boundary-boolean-opened",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["boundary_authority_contract"].__setitem__(
                    "caller_supplied_boundary_boolean_allowed", True
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r2-stage4-open-retention-removed",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["boundary_authority_contract"].pop(
                    "stage4_dynamic_open_source"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r2-parallel-issuer-reintroduced",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["linear_transition_contract"]["replaced_parallel_issuers"].pop()
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r2-combined-output-drops-recovery",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["linear_transition_contract"]["output_owns"].remove(
                    "recovery_receipt"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r2-sequence-identity-constant",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["identity_derivation_contract"]["sequence_identity"].__setitem__(
                    "algorithm", "constant"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r2-schedule-identity-field-removed",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["identity_derivation_contract"]["schedule_snapshot_identity"][
                    "fields_in_order"
                ].pop()
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r2-restart-reuses-receipt",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["identity_derivation_contract"].__setitem__(
                    "restart_model", "persist_and_reuse_receipts"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r2-required-freeze-path-omitted",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["required_governance_update_paths"].remove(
                    "docs/stage-5/stage-5c-api-freeze-manifest.json"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r2-production-path-widened",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["future_production_implementation_paths"].append(
                    "crates/broker-finam/src/lib.rs"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r3-output-bypasses-b3b",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["b3b_b3c_topology_contract"].pop("b3b_transition")
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r3-sequence-dropped-before-b3c",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["b3b_b3c_topology_contract"]["b3b_transition"][
                    "output_owns"
                ].remove("sequence_identity_fingerprint")
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r3-projection-reused-twice",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["cross_module_bridge_contract"].__setitem__(
                    "sole_consumer", "multiple_consumers_allowed"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r3-projection-raw-visible",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["cross_module_bridge_contract"].__setitem__(
                    "visibility", "pub(crate)_fields_visible"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r3-cross-module-alternate-constructor",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["cross_module_bridge_contract"].__setitem__(
                    "sole_constructor", "stage5c_paper_host::new_projection"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r3-expected-close-grid-relaxed",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["expected_close_boundary_algorithm"].__setitem__(
                    "candidate_membership", "any_non_open_candidate"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r3-cross-day-gap-accepted",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["expected_close_boundary_algorithm"].__setitem__(
                    "cross_trading_day", "accept"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r3-stage3-fingerprint-constant",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["identity_derivation_contract"]["stage3_provenance_identity"].__setitem__(
                    "algorithm", "constant"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r3-recovery-identity-field-removed",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["identity_derivation_contract"]["recovery_receipt_identity"][
                    "fields_in_order"
                ].pop()
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r3-b3-checker-update-omitted",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["required_predecessor_governance_update_paths"].remove(
                    "scripts/stage5e_b3_schedule_window_evidence_check.py"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r3-b-checker-update-omitted",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["required_predecessor_governance_update_paths"].remove(
                    "scripts/stage5e_b_no_io_lifecycle_check.py"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r4-boundary-precomputed-without-candidate",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["boundary_authority_contract"].__setitem__(
                    "candidate_specific_boundary_precomputed", True
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r4-caller-timestamps-enter-owner",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["boundary_authority_contract"]["owner_source_types"].append(
                    "caller_previous_current_timeframe"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r4-sealed-classifier-bypassed",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["sealed_candidate_classifier_contract"].__setitem__(
                    "classifier", "stage5c_caller_boolean"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r4-b3c-clock-removed",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["b3b_b3c_topology_contract"]["b3c_transition"].pop(
                    "production_clock"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r4-b3c-expiry-check-removed",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["b3b_b3c_topology_contract"]["b3c_transition"][
                    "continuation_checks"
                ].remove("clock_not_after_effective_expires_at")
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r4-b3c-test-clock-production-visible",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["b3b_b3c_topology_contract"]["b3c_transition"].__setitem__(
                    "test_clock_seam", "pub(crate)_production_visible"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r4-stage4-schedule-identity-constant",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["identity_derivation_contract"]["stage4_schedule_source_identity"].__setitem__(
                    "algorithm", "constant"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r4-stage4-schedule-identity-field-removed",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["identity_derivation_contract"]["stage4_schedule_source_identity"][
                    "fields_in_order"
                ].pop()
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r4-continuous-coverage-reintroduced",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["expected_close_boundary_algorithm"].__setitem__(
                    "coverage_requirement", "continuous_closed_range"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r5-candidate-owner-changed",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["sealed_candidate_classifier_contract"].__setitem__(
                    "candidate_defined_in", "strategy_runtime_core::stage5e_no_io_lifecycle"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r5-second-candidate-constructor",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["sealed_candidate_classifier_contract"].__setitem__(
                    "candidate_sole_constructor", "alternate_free_candidate_constructor"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r5-candidate-raw-scalar-escape",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["sealed_candidate_classifier_contract"].__setitem__(
                    "candidate_getters_or_raw_scalar_escape_allowed", True
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r5-classifier-alternate-constructor",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["sealed_candidate_classifier_contract"].__setitem__(
                    "classifier_sole_constructor", "alternate_projection_classifier_constructor"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r5-classifier-call-site-duplicated",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["sealed_candidate_classifier_contract"].__setitem__(
                    "classifier_sole_call_site", "two_or_more_stage5c_call_sites"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r5-precomputed-boundary-prose-restored",
            "authority freeze plan drift",
            mutate_stage5e_authority_plan_for_checker(
                lambda text: text.replace(
                    "There is no optional precomputed boundary, no raw schedule-session export and",
                    "The projection is the sole producer of an optional precomputed boundary and",
                    1,
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r5-sequence-freshness-removed",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["b3b_b3c_topology_contract"]["b3c_transition"][
                    "continuation_checks"
                ].remove("sequence_fresh")
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r5-sequence-expiry-policy-removed",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["b3b_b3c_topology_contract"]["b3c_transition"].pop(
                    "sequence_expiry_policy"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r5-effective-expiry-max",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["b3b_b3c_topology_contract"]["b3c_transition"].__setitem__(
                    "effective_expires_at_formula", "max(projection_expires_at, sequence_expires_at)"
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case(
            "stage5e-authority-r5-observed-bar-raw-constructible",
            "authority freeze contract drift",
            mutate_stage5e_authority_inventory_for_checker(
                lambda payload: payload["observed_live_bar_with_sequence_construction_seal"].__setitem__(
                    "alternate_or_raw_constructor_allowed", True
                )
            ),
            "5E-b3c-source-authority-freeze-extension",
            True,
        ),
        Case("stage5e-authority-r6-candidate-final-identity", "authority freeze contract drift", mutate_stage5e_authority_inventory_for_checker(lambda p: p["sealed_candidate_classifier_contract"].__setitem__("candidate_final_sequence_identity_allowed", True)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r6-classified-drops-boundary", "authority freeze contract drift", mutate_stage5e_authority_inventory_for_checker(lambda p: p["sealed_candidate_classifier_contract"]["classified_owned_fields"].remove("optional_boundary_fingerprint")), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r6-final-identity-preclassification", "authority freeze contract drift", mutate_stage5e_authority_inventory_for_checker(lambda p: p["sealed_candidate_classifier_contract"].__setitem__("final_sequence_identity_compute_point", "before_classifier")), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r6-observed-generic-parts", "authority freeze contract drift", mutate_stage5e_authority_inventory_for_checker(lambda p: p["observed_live_bar_with_sequence_construction_seal"].__setitem__("generic_into_parts_or_getters_allowed", True)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r6-b3b-seal-second-constructor", "authority freeze contract drift", mutate_stage5e_authority_inventory_for_checker(lambda p: p["b3b_consume_seal_contract"].__setitem__("sole_constructor", "alternate_b3b_consume_seal_constructor")), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r6-b3b-second-consumer", "authority freeze contract drift", mutate_stage5e_authority_inventory_for_checker(lambda p: p["observed_live_bar_with_sequence_construction_seal"].__setitem__("b3b_consume_sole_call_site", "two_b3b_consumers")), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r6-source-for-final-identity", "authority freeze contract drift", mutate_stage5e_authority_inventory_for_checker(lambda p: p["b3b_b3c_topology_contract"]["b3b_transition"]["event_key_fields_in_order"].__setitem__(3, "sequence_source_fingerprint")), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r6-sequence-created-expired", "authority freeze contract drift", mutate_stage5e_authority_inventory_for_checker(lambda p: p["b3b_b3c_topology_contract"]["b3c_transition"].pop("sequence_creation_freshness_requirement")), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r6-accepted-stage3-digest-dropped", "authority freeze contract drift", mutate_stage5e_authority_inventory_for_checker(lambda p: p["sealed_candidate_classifier_contract"]["candidate_owned_preclassification_fields"].remove("stage3_provenance_identity")), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r6-plan-consume-bridge-removed", "authority freeze plan drift", mutate_stage5e_authority_plan_for_checker(lambda t: t.replace("consume_for_b3b(self, Stage5eB3bConsumeSeal)", "into_parts(self)", 1)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r6-implementation-stage4-open-stripped", "protected implementation source changed", mutate_stage5e_authority_source_for_checker("crates/broker-core/src/stage4_bootstrap.rs", lambda t: t.replace("schedule_state: validated.schedule_state,", "schedule_state: BrokerMarketSessionState::Unknown,", 1)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r6-implementation-candidate-final-id-injected", "protected implementation source changed", mutate_stage5e_authority_source_for_checker("crates/strategy-runtime-core/src/stage5c_paper_host.rs", lambda t: t.replace("pub(crate) struct Stage5cSequenceCandidateSeal {\n", "pub(crate) struct Stage5cSequenceCandidateSeal {\n    sequence_identity_fingerprint: [u8; 32],\n", 1)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r6-implementation-sequence-domain-drift", "protected implementation source changed", mutate_stage5e_authority_source_for_checker("crates/strategy-runtime-core/src/stage5c_paper_host.rs", lambda t: t.replace("stage5e-b3c-market-sequence-v2", "stage5e-b3c-market-sequence-v1", 1)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r6-implementation-effective-expiry-max", "protected implementation source changed", mutate_stage5e_authority_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("schedule.expires_at.0.min(b3b.payload.sequence_expires_at)", "schedule.expires_at.0.max(b3b.payload.sequence_expires_at)", 1)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r6-implementation-consume-seal-drift", "protected implementation source changed", mutate_stage5e_authority_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("Stage5eB3bConsumeSeal(())", "Stage5eB3bConsumeSeal(()) /* alternate issuer */", 1)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r6-implementation-unverified-source-injected", "protected implementation source changed", mutate_stage5e_authority_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("// STAGE5E-B3C-PRODUCTION-BRIDGE-BEGIN: trusted-no-io-v1", "// STAGE5E-B3C-PRODUCTION-BRIDGE-BEGIN: trusted-no-io-v1\\n    struct UnverifiedMarketSequenceSource;", 1)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r1-b3b-blocked-output-type-drift", "protected implementation source changed", mutate_stage5e_authority_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("observed: crate::stage5c_paper_host::Stage5eObservedLiveBarWithSequenceEvidence,", "payload: Stage5eB3bObservedLiveBarBridgePayload,", 1)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r1-b3b-retry-entry-removed", "protected implementation source changed", mutate_stage5e_authority_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("pub(crate) fn into_retry(", "fn removed_into_retry(", 1)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r1-b3b-retry-loses-owned-state", "protected implementation source changed", mutate_stage5e_authority_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("self.observed\n        }", "panic!(\"drop owned state\")\n        }", 1)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r1-expired-projection-marked-retryable", "protected implementation source changed", mutate_stage5e_authority_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("Self::EvidenceExpired | Self::BarOutsideSelectedOpenWindow => {", "Self::BarOutsideSelectedOpenWindow => {", 1)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r1-integrity-blocker-marked-retryable", "protected implementation source changed", mutate_stage5e_authority_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("Self::ClockBeforeEffectiveObservation | Self::BarObservedInFuture => {", "Self::ClockBeforeEffectiveObservation | Self::BarObservedInFuture | Self::SequenceIdentityMissing => {", 1)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r1-full-stage4-integration-test-removed", "protected implementation source changed", mutate_stage5e_authority_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("fn canonical_stage4_to_b3c_chain_uses_real_accepted_evidence_without_io()", "fn removed_canonical_stage4_to_b3c_chain()", 1)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-authority-r1-stage4-projection-bypassed-in-test", "protected implementation source changed", mutate_stage5e_authority_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("project_accepted_stage4_schedule(&stage4_evidence, LifecycleInstant(now))", "stage4(now, instrument())", 1)), "5E-b3c-source-authority-freeze-extension", True),
        Case("stage5e-b3d-callback-invocation-opened", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["callback_authority_receipt_contract"]["authority_vector"].__setitem__("callback_invoked", True)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-executable-intent-opened", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["callback_authority_receipt_contract"]["authority_vector"].__setitem__("creates_executable_intent", True)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-successful-unbinding-opened", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["callback_authority_receipt_contract"].__setitem__("successful_unbinding_allowed", True)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-terminal-retry-opened", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["issue_block_contract"].__setitem__("terminal_retry_refresh_or_unbinding_allowed", True)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-autonomous-retry-opened", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["issue_block_contract"].__setitem__("autonomous_retry_authorized", True)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-provider-calendar-inference-opened", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["provider_and_calendar_contract"].__setitem__("utc_civil_date_inference_allowed", True)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-production-authority-type-injected", "B3D implementation source drift", mutate_stage5e_b3d_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("// STAGE5E-B3C-PRODUCTION-BRIDGE-END: trusted-no-io-v1", "// duplicate Stage5eCallbackAuthorityReadyPaperStrategy\\n// STAGE5E-B3C-PRODUCTION-BRIDGE-END: trusted-no-io-v1", 1)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-r1-legacy-stage5c-runtime-attachment-opened", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["route_exclusivity_contract"].__setitem__("legacy_route_stage5e_runtime_attachment_allowed", True)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-r1-sole-authority-input-bypassed", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["route_exclusivity_contract"].__setitem__("sole_new_stage5e_callback_input", "Stage5cPendingRecoveredPaperStrategy")), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-r1-authority-expiry-extended", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["callback_authority_issue_transition"].__setitem__("authority_expires_at_formula", "b3c_effective_expires_at_plus_grace")), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-r1-callback-time-expiry-check-removed", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["callback_authority_invocation_contract"]["callback_time_checks"].remove("now_not_after_authority_expires_at")), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-r1-authority-id-field-removed", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["callback_authority_id_contract"]["fields_in_order"].remove("sequence_identity_fingerprint")), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-r1-issuance-ledger-opened", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["callback_authority_id_contract"].__setitem__("issuance_ledger_required", True)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-r1-runtime-ownership-id-opened", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["ownership_contract"].__setitem__("production_ownership_binding_id_present", True)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-r1-refresh-path-opened", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["issue_block_contract"].__setitem__("refresh_type_present", True)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-r1-second-callback-consumer-opened", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["callback_authority_consume_seal"].__setitem__("sole_issuer", "two_callback_invocation_routes")), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-r1-authority-clone-protection-removed", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["callback_authority_receipt_contract"]["forbidden_traits_and_surfaces"].remove("Clone")), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-r1-callback-intent-coupling-denied", "R1 inventory drift", mutate_stage5e_b3d_inventory_for_checker(lambda p: p["callback_authority_invocation_contract"].__setitem__("callback_invocation_implies_in_memory_intent_construction", False)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-impl-authority-marker-removed", "B3D implementation source drift", mutate_stage5e_b3d_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("// STAGE5E-B3D-CALLBACK-AUTHORITY-BEGIN: private-no-io-issue-v1", "// removed authority marker", 1)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-impl-receipt-clone-opened", "B3D implementation source drift", mutate_stage5e_b3d_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("    pub(crate) struct Stage5eCallbackAuthorityReadyPaperStrategy {", "    #[derive(Clone)]\\n    pub(crate) struct Stage5eCallbackAuthorityReadyPaperStrategy {", 1)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-impl-receipt-serialize-opened", "B3D implementation source drift", mutate_stage5e_b3d_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("    pub(crate) struct Stage5eCallbackAuthorityReadyPaperStrategy {", "    #[derive(serde::Serialize)]\\n    pub(crate) struct Stage5eCallbackAuthorityReadyPaperStrategy {", 1)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-impl-raw-strategy-getter-opened", "B3D implementation source drift", mutate_stage5e_b3d_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("        fn from_approved(", "        fn raw_strategy(&self) {}\\n\\n        fn from_approved(", 1)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-impl-second-issue-seal-constructor", "B3D implementation source drift", mutate_stage5e_b3d_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("        Stage5eCallbackAuthorityIssueSeal(())", "        let _duplicate = Stage5eCallbackAuthorityIssueSeal(());\\n        Stage5eCallbackAuthorityIssueSeal(())", 1)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-impl-production-clock-externalized", "B3D implementation source drift", mutate_stage5e_b3d_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("issue_stage5e_callback_authority_with_now(b3c_receipt, Utc::now())", "issue_stage5e_callback_authority_with_now(b3c_receipt, DateTime::<Utc>::default())", 1)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-impl-expiry-extended", "B3D implementation source drift", mutate_stage5e_b3d_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("let authority_expires_at = preflight.effective_expires_at;", "let authority_expires_at = preflight.effective_expires_at + chrono::Duration::seconds(1);", 1)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-impl-authority-id-sequence-field-removed", "B3D implementation source drift", mutate_stage5e_b3d_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("        encoder.field(5, &sequence_identity_fingerprint);\n", "", 1)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-impl-retry-drops-receipt", "B3D implementation source drift", mutate_stage5e_b3d_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("                        b3c_receipt,\n                    },", "                        b3c_receipt: panic!(\"drop receipt\"),\n                    },", 1)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-impl-terminal-unbinding-opened", "B3D implementation source drift", mutate_stage5e_b3d_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("    pub(crate) struct Stage5eCallbackAuthorityTerminalBlock {\n        reason:", "    pub(crate) struct Stage5eCallbackAuthorityTerminalBlock {\n        b3c_receipt: Option<Stage5eBoundSessionCalendarSequenceForObservedLiveBar>,\n        reason:", 1)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-impl-actual-callback-opened", "B3D implementation source drift", mutate_stage5e_b3d_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("// STAGE5E-B3D-CALLBACK-AUTHORITY-END: private-no-io-issue-v1", "fn invoke_stage5e_authorized_paper_callback() { on_broker_bar(); }\\n// STAGE5E-B3D-CALLBACK-AUTHORITY-END: private-no-io-issue-v1", 1)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-impl-legacy-stage5c-route-called", "B3D implementation source drift", mutate_stage5e_b3d_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("// STAGE5E-B3D-CALLBACK-AUTHORITY-END: private-no-io-issue-v1", "fn bypass_authority() { apply_stage5c_semantic_bar(); }\\n// STAGE5E-B3D-CALLBACK-AUTHORITY-END: private-no-io-issue-v1", 1)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-impl-test-clock-cfg-removed", "B3D implementation source drift", mutate_stage5e_b3d_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("    #[cfg(test)]\n    pub(crate) fn issue_stage5e_callback_authority_at(", "    pub(crate) fn issue_stage5e_callback_authority_at(", 1)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3d-impl-second-borrowed-bridge-opened", "B3D implementation source drift", mutate_stage5e_b3d_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("            pub(crate) fn borrow_callback_authority_preflight(", "            pub(crate) fn duplicate_borrow_callback_authority_preflight() {}\\n\\n            pub(crate) fn borrow_callback_authority_preflight(", 1)), "5E-b3d-callback-authority-design", True),
        Case("stage5e-b3e-sole-authority-input-bypassed", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["invocation_transition_contract"].__setitem__("only_input", "Stage5eBoundSessionCalendarSequenceForObservedLiveBar")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-production-clock-externalized", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["invocation_transition_contract"].__setitem__("caller_supplied_production_clock_allowed", True)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-callback-expiry-check-removed", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["callback_time_preflight_contract"]["checks_in_order"].remove("now_not_after_authority_expires_at")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-terminal-retry-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["terminal_block_contract"].__setitem__("retry_allowed", True)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-escrow-intent-getter-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["result_escrow_contract"]["forbidden_traits_and_surfaces"].remove("intent_getter")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-runtime-implementation-source-drift", "protected implementation source changed", mutate_stage5e_b3e_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("fn invoke_stage5e_authorized_paper_callback_with_now(", "fn invoke_stage5e_authorized_paper_callback_with_drifted_now(", 1)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-stage5c-implementation-source-drift", "protected implementation source changed", mutate_stage5e_b3e_source_for_checker("crates/strategy-runtime-core/src/stage5c_paper_host.rs", lambda t: t.replace("BrokerNeutralHybridStrategy::on_broker_bar", "BrokerNeutralHybridStrategy::on_broker_bar_drifted", 1)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-legacy-stage5c-route-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["invocation_transition_contract"].__setitem__("legacy_stage5c_route_allowed", True)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-callback-cardinality-expanded", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["invocation_transition_contract"].__setitem__("callback_count_on_success_path", 2)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-runtime-live-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["closed_surfaces"].__setitem__("runtime_live", True)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-separate-review-marker-removed", "design plan drift", mutate_stage5e_b3e_plan_for_checker(lambda t: t.replace("Any settlement or external side effect requires a separate accepted assignment", "Settlement may proceed immediately", 1)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r1-callback-context-field-drift", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["canonical_callback_input_contract"]["context_fields"][4].__setitem__("source", "constant_HybridRuntimeTradeMode_Live")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r1-allow-live-orders-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["canonical_callback_input_contract"]["context_fields"][6].__setitem__("source", "constant_true")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r1-callback-payload-reconstructed", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["canonical_callback_input_contract"].__setitem__("payload_source", "reconstructed_from_scalars")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r1-pre-callback-attribution-removed", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["stage5c_callback_materialization_contract"]["material_fields"].remove("attribution_snapshot")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r1-post-callback-attribution-substituted", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["pre_callback_attribution_snapshot_contract"].__setitem__("source_state", "post_callback_strategy_state")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r1-consume-payload-raw-getter-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["module_and_consume_topology"]["payload_forbidden_surfaces"].remove("raw_strategy_getter")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r1-second-consume-bridge-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["module_and_consume_topology"].__setitem__("authority_consume_call_site_count", 2)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r1-escrow-intents-duplicated", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["callback_outcome_contract"].__setitem__("intent_vector_owner_count", 2)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r1-effective-observation-equality-removed", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["callback_time_preflight_contract"]["checks_in_order"].remove("authority_effective_observed_at_equals_owned_b3c_effective_observed_at")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r1-callback-chronology-weakened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["callback_time_preflight_contract"]["checks_in_order"].remove("accepted_bar_close_not_after_issued_at")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r2-raw-admission-getter-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["stage5c_callback_materialization_contract"]["forbidden_surfaces"].remove("raw_admission_getter")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r2-raw-accepted-bar-getter-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["stage5c_callback_materialization_contract"]["forbidden_surfaces"].remove("raw_semantic_bar_getter")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r2-stage5c-context-algorithm-duplicated", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["canonical_callback_input_contract"].__setitem__("builder", "sibling_authority_owner_builder")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r2-b3d-issue-seal-reused", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["nested_b3c_invocation_preflight_contract"].__setitem__("seal", "Stage5eCallbackAuthorityIssueSeal")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r2-b3c-consume-before-validation", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["nested_b3c_invocation_preflight_contract"].__setitem__("consume_before_validation_allowed", True)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r2-recovery-receipt-dropped", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["result_escrow_contract"]["owns"].remove("recovery_receipt")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r2-accepted-bar-metadata-dropped", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["result_escrow_contract"]["owns"].remove("accepted_semantic_bar_identity")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r2-callback-authority-id-dropped", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["result_escrow_contract"]["owns"].remove("callback_authority_id")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r2-second-stage5c-material-constructor", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["stage5c_callback_materialization_contract"].__setitem__("material_constructor_count", 2)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r2-attribution-snapshot-substituted", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["payload_to_escrow_transfer_matrix"][3].__setitem__("escrow_destination", "post_callback_attribution_snapshot")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r3-stage5c-issuer-made-sibling-unreachable", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["stage5c_callback_materialization_contract"].__setitem__("seal_issuer_visibility", "private")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r3-second-material-seal-issuer", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["stage5c_callback_materialization_contract"].__setitem__("issuer_call_site_count", 2)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r3-material-fields-made-pub-crate", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["stage5c_callback_materialization_contract"].__setitem__("material_visibility", "pub_crate_fields")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r3-material-into-parts-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["stage5c_callback_materialization_contract"]["forbidden_surfaces"].remove("generic_into_parts")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r3-material-callback-consumer-omitted", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["stage5c_material_callback_execution_contract"].__setitem__("method", "missing")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r3-second-material-callback-consumer", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["stage5c_material_callback_execution_contract"].__setitem__("method_call_site_count", 2)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r3-legacy-apply-used-in-material-consumer", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["stage5c_material_callback_execution_contract"].__setitem__("legacy_stage5c_apply_or_loop_allowed", True)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r3-post-callback-recovery-receipt-dropped", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["stage5c_post_callback_material_contract"]["fields"].remove("recovery_receipt")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r3-second-escrow-constructor", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["escrow_construction_contract"].__setitem__("constructor_definition_count", 2)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r3-escrow-constructed-before-callback", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["escrow_construction_contract"].__setitem__("pre_callback_construction_allowed", True)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r3-escrow-constructor-without-seal", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["escrow_construction_contract"].__setitem__("construction_without_seal_allowed", True)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r4-callback-now-dropped-before-stage5c", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["invocation_consume_context_contract"]["clock_flow"].remove("callback_now_to_stage5c_materialization")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r4-issued-at-substituted-for-callback-now", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["invocation_consume_context_contract"]["clock_flow"].__setitem__(0, "issued_at_to_stage5c_materialization")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r4-audit-authority-id-dropped", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["audit_lineage_contract"]["outer_authority_sources"].pop("callback_authority_id")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r4-audit-expiry-dropped", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["audit_lineage_contract"]["outer_authority_sources"].pop("authority_expires_at")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r4-invocation-context-second-constructor", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["invocation_consume_context_contract"].__setitem__("constructor_count", 2)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r4-authorized-payload-owner-changed", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["authorized_payload_contract"].__setitem__("owner", "b3c_evidence")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r4-authorized-payload-raw-getters-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["authorized_payload_contract"]["forbidden_surfaces"].remove("raw_getters")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r4-authorized-payload-second-consumer", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["authorized_payload_contract"].__setitem__("consumer_call_site_count", 2)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r4-payload-constructor-without-nested-capability", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["authorized_payload_contract"].__setitem__("constructor_capability", "none")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r4-callback-outcome-alternate-constructor", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["callback_outcome_contract"].__setitem__("move_constructor_count", 2)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r5-context-read-without-seal", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["invocation_consume_context_contract"].__setitem__("access_capability", "none")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r5-context-raw-getter-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["invocation_consume_context_contract"]["forbidden_surfaces"].remove("raw_getters")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r5-context-copy-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["invocation_consume_context_contract"]["forbidden_surfaces"].remove("Copy")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r5-context-from-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["invocation_consume_context_contract"]["forbidden_surfaces"].remove("From")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r5-outcome-exposed-pub-crate-enum", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["callback_outcome_contract"].__setitem__("representation", "pub_crate_enum")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r5-outcome-variant-constructed-externally", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["callback_outcome_contract"].__setitem__("external_variant_construction_allowed", True)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r5-outcome-inspected-outside-settlement", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["callback_outcome_contract"].__setitem__("external_variant_inspection_allowed", True)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r5-second-audit-lineage-constructor", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["audit_lineage_contract"].__setitem__("constructor_definition_count", 2)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r5-audit-lineage-owner-changed", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["audit_lineage_contract"].__setitem__("owner", "b3c_evidence")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r5-post-callback-sibling-bridge-duplicated", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["escrow_construction_contract"].__setitem__("stage5c_sibling_bridge_call_site_count", 2)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r6-audit-constructor-reads-opaque-material", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["audit_lineage_contract"].__setitem__("source_authority_material", "&Stage5eB3eNestedInvocationMaterial")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r6-authority-scalar-removed", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["audit_lineage_contract"]["constructor_scalar_arguments"].remove("authority_expires_at")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r6-authority-field-substituted", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["audit_lineage_contract"]["field_transfer_matrix"][2].__setitem__("destination", "audit_lineage_effective_observed_at")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r6-audit-raw-getter-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["audit_lineage_contract"].__setitem__("nested_to_audit_bridge_raw_getters_allowed", True)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r6-second-nested-audit-bridge", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["audit_lineage_contract"].__setitem__("nested_to_audit_bridge_count", 2)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r6-audit-constructor-without-capability", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["audit_lineage_contract"].__setitem__("constructor_capability", "none")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r6-outcome-debug-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["callback_outcome_contract"]["forbidden_traits"].remove("Debug")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r6-post-callback-debug-opened", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["stage5c_post_callback_material_contract"]["forbidden_surfaces"].remove("Debug")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r6-materialization-panic-ambiguity", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["stage5c_callback_materialization_contract"]["integrity_failure_policy"].__setitem__("panic_allowed", True)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r7-materialization-error-swallowed", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["terminal_block_contract"]["propagation_chain"].remove("nested_consume_returns_Err_top_level_terminal")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r7-materialization-error-mapped-success", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["terminal_block_contract"]["materialization_mapping"].__setitem__("destination", "Stage5eAuthorizedPaperCallbackPayload")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r7-b3c-consume-left-infallible", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["module_and_consume_topology"].__setitem__("nested_consume_output", "Stage5eAuthorizedPaperCallbackPayload")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r7-authority-consume-drops-terminal", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["module_and_consume_topology"].__setitem__("authority_consume_output", "Stage5eAuthorizedPaperCallbackPayload")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r7-top-level-reason-omitted", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["terminal_block_contract"]["reasons"].remove("MaterializationIntegrityMismatch")), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r7-mapping-made-retryable", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["terminal_block_contract"]["materialization_mapping"].__setitem__("retryable_mapping_allowed", True)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r7-materialization-panic-reintroduced", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["stage5c_callback_materialization_contract"]["integrity_failure_policy"].__setitem__("panic_allowed", True)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r7-strategy-returned-on-mismatch", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["terminal_block_contract"].__setitem__("materialization_terminal_returns_strategy", True)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-r7-second-materialization-conversion", "design inventory drift", mutate_stage5e_b3e_inventory_for_checker(lambda p: p["terminal_block_contract"]["materialization_mapping"].__setitem__("mapper_definition_count", 2)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-impl-r1-nested-schedule-field-removed", "protected implementation source changed", mutate_stage5e_b3e_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("schedule_window_identity_fingerprint: [u8; 32],", "schedule_window_identity_fingerprint_removed: [u8; 32],", 1)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-impl-r1-b3c-bound-field-removed", "protected implementation source changed", mutate_stage5e_b3e_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("b3c_bound_at: DateTime<Utc>,", "b3c_bound_at_removed: DateTime<Utc>,", 1)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-impl-r1-event-key-recomputation-removed", "protected implementation source changed", mutate_stage5e_b3e_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("let recomputed_event_key =", "let recomputed_event_key_removed =", 1)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-impl-r1-zero-identity-guard-removed", "protected implementation source changed", mutate_stage5e_b3e_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace(".contains(&[0; 32])", ".contains(&[1; 32])", 1)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-impl-r1-canonical-stage4-callback-test-removed", "protected implementation source changed", mutate_stage5e_b3e_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("fn canonical_stage4_to_b3c_chain_uses_real_accepted_evidence_without_io(", "fn removed_canonical_stage4_to_b3c_chain_uses_real_accepted_evidence_without_io(", 1)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-impl-r1-nonopen-stage4-bypass-reintroduced", "protected implementation source changed", mutate_stage5e_b3e_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace(".expect(\"canonical non-open Stage 4 evidence must still be constructible\")", ".unwrap_or_else(|_| panic!(\"synthetic bypass\"))", 1)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-impl-r1-attribution-binding-removed", "protected implementation source changed", mutate_stage5e_b3e_source_for_checker("crates/strategy-runtime-core/src/stage5c_paper_host.rs", lambda t: t.replace("target_instrument: InstrumentId,", "target_instrument_removed: InstrumentId,", 1)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-impl-r1-nonempty-intent-proof-removed", "protected implementation source changed", mutate_stage5e_b3e_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("fn b3e_actual_authorized_callback_retains_nonempty_intents_only_in_opaque_escrow(", "fn removed_b3e_actual_authorized_callback_retains_nonempty_intents_only_in_opaque_escrow(", 1)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3e-impl-r1-second-intent-owner-introduced", "protected implementation source changed", mutate_stage5e_b3e_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("callback_outcome: Stage5ePaperCallbackOutcome,", "callback_outcome: Stage5ePaperCallbackOutcome, duplicated_callback_outcome: Stage5ePaperCallbackOutcome,", 1)), "5E-b3e-callback-invocation-design", True),
        Case("stage5e-b3f-raw-escrow-input-replaced", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["transition_contract"].__setitem__("only_input", "Vec<BrokerNeutralHybridIntent>")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-settlement-implemented-in-design", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["transition_contract"].__setitem__("implementation_status", "implemented")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-preflight-after-consume", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["transition_contract"].__setitem__("borrowed_preflight_before_consume", False)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-second-escrow-consumer", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["transition_contract"].__setitem__("consume_count", 2)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-seal-conversion-opened", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["seal_contract"].__setitem__("conversion_between_seals_allowed", True)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-raw-intent-export-opened", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["preflight_contract"].__setitem__("raw_intent_export_allowed", True)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-account-identity-check-removed", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["preflight_contract"]["checks"].remove("account_id_exact_equality")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-intent-limit-raised", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["capacity_contract"].__setitem__("maximum_intents", 256)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-validation-error-made-empty-success", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["callback_validation_error_policy"].__setitem__("empty_success_batch_allowed", True)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-callback-retry-opened", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["callback_validation_error_policy"].__setitem__("callback_retry_allowed", True)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-stage5c-oracle-replaced", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["stage5c_bridge_contract"].__setitem__("canonical_batch_builder", "stage5e_build_paper_intent_batch")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-parallel-oracle-opened", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["stage5c_bridge_contract"].__setitem__("stage5e_reimplementation_allowed", True)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-client-order-id-substitution", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["stage5c_bridge_contract"].__setitem__("client_order_id_may_replace_strategy_request_id", True)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-terminal-made-retryable", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["terminal_receipt_contract"].__setitem__("retryable", True)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-intent-getter-opened", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["success_receipt_contract"].__setitem__("intent_getter_allowed", True)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-durable-persistence-opened", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["closed_surfaces"].__setitem__("durable_persistence", True)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-plan-side-effect-ban-removed", "design plan drift", mutate_stage5e_b3f_plan_for_checker(lambda t: t.replace("FINAM I/O", "broker I/O", 1)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-protected-b3e-source-mutated", "protected B3E implementation source changed", mutate_stage5e_b3f_source_for_checker("crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs", lambda t: t.replace("Stage5ePaperCallbackResultEscrow", "Stage5ePaperCallbackResultEscrowMutated", 1)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-terminal-decision-not-consumed", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["transition_contract"].__setitem__("consume_after_every_decision", False)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-terminal-decision-removed", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["transition_contract"]["preflight_decisions"].remove("Terminal")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-preflight-terminal-keeps-escrow", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["preflight_contract"].__setitem__("terminal_decision_still_consumes_escrow", False)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-second-borrow-bridge", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["escrow_bridge_contract"].__setitem__("borrow_call_site_count", 2)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-second-consume-bridge", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["escrow_bridge_contract"].__setitem__("consume_call_site_count", 2)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-payload-drops-recovery", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["escrow_bridge_contract"]["payload_fields"].remove("recovery_receipt")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-outcome-raw-access-opened", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["escrow_bridge_contract"].__setitem__("outcome_preflight_access", "raw_intent_slice")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-stage5c-bridge-renamed", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["stage5c_bridge_contract"].__setitem__("function", "ad_hoc_settle")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-attribution-builder-replaced", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["stage5c_bridge_contract"].__setitem__("canonical_attribution_builder", "stage5e_attribution")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-fallback-attribution-opened", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["stage5c_bridge_contract"].__setitem__("stage5e_fallback_map_allowed", True)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-terminal-drops-strategy", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["stage5c_bridge_contract"]["terminal_survives"].remove("mutated_strategy")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-intent-vector-recovered-after-error", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["stage5c_bridge_contract"].__setitem__("intent_vector_after_builder_error", "returned_for_retry")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-error-mapping-incomplete", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["stage5c_error_mapping"].pop("UnsupportedIntentAction")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-replay-error-mapped-generic", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["stage5c_error_mapping"].__setitem__("ReplayIntentNotExecutable", "Stage5cIntentValidationFailed")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-wildcard-error-mapping-opened", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["stage5c_error_mapping_policy"].__setitem__("wildcard_mapping_allowed", True)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-audit-domain-unversioned", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["audit_commitment_contract"].__setitem__("domain", "audit")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-audit-authority-id-omitted", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["audit_commitment_contract"]["ordered_fields"].remove("callback_authority_id")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-settlement-domain-unversioned", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["settlement_identity_contract"].__setitem__("domain", "settlement")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-settlement-audit-binding-omitted", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["settlement_identity_contract"]["ordered_fields"].remove("audit_commitment")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-native-endian-identity-opened", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["canonical_encoding_contract"].__setitem__("native_endian_allowed", True)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-second-success-constructor", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["success_receipt_contract"].__setitem__("constructor_count", 2)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-public-settled-getter-opened", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["success_receipt_contract"]["forbidden_surfaces"].remove("settled")), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-second-terminal-constructor", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["terminal_receipt_contract"].__setitem__("constructor_count", 2)), "5E-b3f-callback-settlement-escrow-design", True),
        Case("stage5e-b3f-r1-stage5c-error-metadata-dropped", "design inventory drift", mutate_stage5e_b3f_inventory_for_checker(lambda p: p["terminal_receipt_contract"].__setitem__("records_optional_exact_stage5c_error", False)), "5E-b3f-callback-settlement-escrow-design", True),
    ]
    if args.case_start < 0 or args.case_start > len(cases):
        print("handoff-provenance-negative-harness: invalid --case-start", file=sys.stderr)
        return 2
    case_end = len(cases) if args.case_end is None else args.case_end
    if case_end < args.case_start or case_end > len(cases):
        print("handoff-provenance-negative-harness: invalid --case-end", file=sys.stderr)
        return 2
    selected_cases = cases[args.case_start:case_end]
    with tempfile.TemporaryDirectory(prefix="handoff-provenance-negative-") as tmp:
        base = Path(tmp)
        failures = []
        for case in selected_cases:
            ok, diagnostics = run_case(base, case)
            print(f"{'PASS' if ok else 'FAIL'} {case.name}")
            if not ok:
                failures.append((case.name, diagnostics))
                print(diagnostics, file=sys.stderr)
        if failures:
            return 1
    print(f"handoff-provenance-negative-harness: ok cases={len(selected_cases)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
