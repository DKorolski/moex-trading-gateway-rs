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
            "handoff-manifest.json",
            "handoff-provenance-negative-result.json",
            "handoff-provenance-negative-stderr.txt",
            "handoff-provenance-negative-stdout.txt",
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
