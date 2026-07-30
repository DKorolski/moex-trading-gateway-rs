#!/usr/bin/env python3
"""Validate the Stage 5F protected-base authority boundary from Git trees.

The ``pull_request_target`` workflow invokes this file from the protected PR
base. Candidate files are never imported or executed.  Their Git tree is the
source of truth: every eligible entry is a regular blob with an explicit Git
mode and SHA-256.  This rejects checkout-shaped hidden gitlinks as well as
content-identical executable-mode authority drift.
"""

from __future__ import annotations

import argparse
import ast
from dataclasses import dataclass
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path


R3_AUTHORITY_FILES = (
    ".github/workflows/ci.yml",
    "scripts/make_handoff_archive.sh",
    "scripts/stage5f_atomic_hybrid_semantics_negative_harness.py",
    "scripts/stage5f_b3f_snapshot_provenance_gate.sh",
    "scripts/stage5f_ci_snapshot_inheritance_check.py",
    "scripts/stage5f_ci_snapshot_inheritance_negative_harness.py",
    "scripts/stage5f_descriptor.py",
)
BASE_AUTHORITY_FILES = (
    ".github/workflows/stage5f-base-authority.yml",
    "docs/stage-5/stage5f-a-atomic-hybrid-semantics-entry-inventory.json",
    "docs/stage-5/stage5f-authority-state.json",
    "scripts/handoff_safety_check.py",
    "scripts/stage5f_atomic_hybrid_semantics_entry_check.py",
    "scripts/stage5f_atomic_hybrid_semantics_gate.sh",
    "scripts/stage5f_base_authority_contract.py",
    "scripts/stage5f_base_authority_negative_harness.py",
)
AUTHORITY_FILES = tuple(sorted((*R3_AUTHORITY_FILES, *BASE_AUTHORITY_FILES)))
ROTATION_MANIFEST = "docs/stage-5/stage5f-authority-rotation.json"
AUTHORITY_STATE = "docs/stage-5/stage5f-authority-state.json"
AUTHORITY_WORKFLOW = ".github/workflows/stage5f-base-authority.yml"
WORKFLOW_PREFIX = ".github/workflows/"
GATE = "scripts/stage5f_atomic_hybrid_semantics_gate.sh"
CI_WORKFLOW = ".github/workflows/ci.yml"
INVENTORY = "docs/stage-5/stage5f-a-atomic-hybrid-semantics-entry-inventory.json"
ENTRY_CHECKER = "scripts/stage5f_atomic_hybrid_semantics_entry_check.py"
HANDOFF_CHECKER = "scripts/handoff_safety_check.py"
ALLOWED_GIT_MODES = {"100644", "100755"}
HEX64 = re.compile(r"[0-9a-f]{64}\Z")
PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE = "5F-a-r9-portable-forbidden-scanner"
PORTABLE_FORBIDDEN_SCANNER_REPAIR_PATHS = frozenset(
    {
        "docs/current-status.md",
        "docs/handoff.md",
        "docs/stage-5/5f-a-atomic-hybrid-semantics-entry.md",
        "docs/stage-5/5f-a-r8-bootstrap-repair-authority.md",
        "docs/stage-5/stage5f-a-atomic-hybrid-semantics-entry-inventory.json",
        "docs/stage-5/stage5f-authority-rotation-protocol.md",
        "docs/stage-5/stage5f-authority-rotation.json",
        "docs/stage-5/stage5f-authority-state.json",
        "scripts/forbidden_surface_negative_case_worker.sh",
        "scripts/forbidden_surface_negative_harness.py",
        "scripts/forbidden_surface_scan.sh",
        "scripts/handoff_safety_check.py",
        "scripts/stage5f_atomic_hybrid_semantics_entry_check.py",
        "scripts/stage5f_base_authority_negative_harness.py",
    }
)
PORTABLE_FORBIDDEN_SCANNER_PATH = "scripts/forbidden_surface_scan.sh"
SHA_LINE = re.compile(
    r'verify_sha256 "([0-9a-f]{64})" '
    r'"scripts/stage5f_atomic_hybrid_semantics_gate\.sh"'
)


class ContractFailure(RuntimeError):
    pass


@dataclass(frozen=True)
class GitEntry:
    git_mode: str
    object_id: str
    sha256: str

    def binding(self) -> dict[str, str]:
        return {"git_mode": self.git_mode, "sha256": self.sha256}


def fail(message: str) -> None:
    raise ContractFailure(message)


def git(root: Path, *args: str) -> bytes:
    try:
        return subprocess.run(
            ["git", "-C", str(root), *args],
            check=True,
            capture_output=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError) as exc:
        fail(f"cannot inspect authority Git tree: {root}: {exc}")


def safe_relative_path(raw: bytes) -> str:
    try:
        relative = raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        fail(f"non-UTF-8 Git tree path: {exc}")
    path = Path(relative)
    if not relative or path.is_absolute() or ".." in path.parts or relative.startswith(".git/"):
        fail(f"unsafe Git tree path: {relative!r}")
    return relative


def git_blob_sha256s(root: Path, object_ids: list[str]) -> dict[str, str]:
    """Return content hashes for Git blobs with one `cat-file --batch` process."""
    if not object_ids:
        return {}
    try:
        completed = subprocess.run(
            ["git", "-C", str(root), "cat-file", "--batch"],
            input=("\n".join(object_ids) + "\n").encode("ascii"),
            check=True,
            capture_output=True,
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        fail(f"cannot read authority Git blobs: {root}: {exc}")

    hashes: dict[str, str] = {}
    offset = 0
    output = completed.stdout
    for requested in object_ids:
        header_end = output.find(b"\n", offset)
        if header_end < 0:
            fail("truncated Git blob batch header")
        header = output[offset:header_end].split()
        offset = header_end + 1
        if len(header) != 3 or header[1] != b"blob":
            fail(f"invalid Git blob batch header for {requested}")
        try:
            returned = header[0].decode("ascii")
            size = int(header[2])
        except (UnicodeDecodeError, ValueError) as exc:
            fail(f"invalid Git blob batch metadata: {exc}")
        if returned != requested or size < 0 or offset + size >= len(output):
            fail(f"invalid Git blob batch payload for {requested}")
        blob = output[offset : offset + size]
        offset += size
        if output[offset : offset + 1] != b"\n":
            fail(f"missing Git blob batch terminator for {requested}")
        offset += 1
        hashes[requested] = hashlib.sha256(blob).hexdigest()
    if offset != len(output):
        fail("unexpected trailing Git blob batch data")
    return hashes


def git_tree_entries(root: Path) -> dict[str, GitEntry]:
    raw_entries = git(root, "ls-tree", "-r", "-z", "--full-tree", "HEAD")
    parsed: list[tuple[str, str, str]] = []
    object_ids: list[str] = []
    seen_object_ids: set[str] = set()
    paths: set[str] = set()
    for record in raw_entries.split(b"\0"):
        if not record:
            continue
        try:
            header, raw_path = record.split(b"\t", 1)
            raw_mode, raw_type, raw_object_id = header.split(b" ", 2)
            mode = raw_mode.decode("ascii")
            object_type = raw_type.decode("ascii")
            object_id = raw_object_id.decode("ascii")
        except (UnicodeDecodeError, ValueError) as exc:
            fail(f"malformed Git tree entry: {exc}")
        relative = safe_relative_path(raw_path)
        if relative in paths:
            fail(f"duplicate Git tree path: {relative}")
        paths.add(relative)
        if object_type != "blob" or mode not in ALLOWED_GIT_MODES:
            fail(
                f"forbidden Git tree entry: {relative} "
                f"mode={mode} type={object_type}"
            )
        parsed.append((relative, mode, object_id))
        if object_id not in seen_object_ids:
            seen_object_ids.add(object_id)
            object_ids.append(object_id)

    blob_hashes = git_blob_sha256s(root, object_ids)
    entries: dict[str, GitEntry] = {}
    for relative, mode, object_id in parsed:
        entries[relative] = GitEntry(
            git_mode=mode,
            object_id=object_id,
            sha256=blob_hashes[object_id],
        )
    return entries


def require_entry(entries: dict[str, GitEntry], relative: str) -> GitEntry:
    entry = entries.get(relative)
    if entry is None:
        fail(f"missing authority Git tree entry: {relative}")
    return entry


def blob_text(root: Path, entries: dict[str, GitEntry], relative: str) -> str:
    entry = require_entry(entries, relative)
    try:
        return git(root, "cat-file", "blob", entry.object_id).decode("utf-8")
    except UnicodeDecodeError as exc:
        fail(f"authority source is not UTF-8: {relative}: {exc}")


def parse_json(root: Path, entries: dict[str, GitEntry], relative: str, label: str) -> dict[str, object]:
    try:
        payload = json.loads(blob_text(root, entries, relative))
    except json.JSONDecodeError as exc:
        fail(f"invalid {label}: {exc}")
    if not isinstance(payload, dict):
        fail(f"invalid {label}: object required")
    return payload


def parse_static_dict(
    root: Path, entries: dict[str, GitEntry], relative: str, variable: str
) -> dict[str, object]:
    try:
        tree = ast.parse(blob_text(root, entries, relative), filename=relative)
    except SyntaxError as exc:
        fail(f"invalid Python authority source: {relative}: {exc}")
    for node in tree.body:
        if not isinstance(node, ast.Assign):
            continue
        if any(isinstance(target, ast.Name) and target.id == variable for target in node.targets):
            try:
                value = ast.literal_eval(node.value)
            except ValueError as exc:
                fail(f"non-literal authority declaration: {relative}: {variable}: {exc}")
            if isinstance(value, dict):
                return value
            fail(f"invalid authority declaration: {relative}: {variable}")
    fail(f"missing authority declaration: {relative}: {variable}")


def gate_digest_from_ci(candidate: Path, entries: dict[str, GitEntry]) -> str:
    matches = SHA_LINE.findall(blob_text(candidate, entries, CI_WORKFLOW))
    if len(matches) != 1:
        fail("canonical CI must contain exactly one Stage 5F gate digest")
    return matches[0]


def gate_digest_from_inventory(candidate: Path, entries: dict[str, GitEntry]) -> str:
    authority = parse_json(candidate, entries, INVENTORY, "Stage 5F inventory").get(
        "ci_snapshot_authority"
    )
    if not isinstance(authority, dict):
        fail("Stage 5F inventory authority is missing")
    value = authority.get("stage5f_atomic_hybrid_semantics_gate_sha256")
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        fail("Stage 5F inventory gate digest is invalid")
    return value


def gate_digest_from_static_authority(
    candidate: Path, entries: dict[str, GitEntry], relative: str, variable: str
) -> str:
    authority = parse_static_dict(candidate, entries, relative, variable)
    value = authority.get("stage5f_atomic_hybrid_semantics_gate_sha256")
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        fail(f"Stage 5F authority gate digest is invalid: {relative}")
    return value


def validate_gate_digest_consistency(
    candidate: Path, entries: dict[str, GitEntry], expected: str | None = None
) -> None:
    values = {
        "canonical CI": gate_digest_from_ci(candidate, entries),
        "inventory": gate_digest_from_inventory(candidate, entries),
        "entry checker": gate_digest_from_static_authority(
            candidate, entries, ENTRY_CHECKER, "EXPECTED_CI_SNAPSHOT_AUTHORITY"
        ),
        "handoff checker": gate_digest_from_static_authority(
            candidate, entries, HANDOFF_CHECKER, "STAGE5F_CI_SNAPSHOT_AUTHORITY"
        ),
        "actual gate": require_entry(entries, GATE).sha256,
    }
    if len(set(values.values())) != 1:
        fail("Stage 5F gate authority digest split")
    if expected is not None and values["actual gate"] != expected:
        fail("rotation manifest Stage 5F gate digest mismatch")


def reject_drift(
    trusted_entries: dict[str, GitEntry], candidate_entries: dict[str, GitEntry], paths: tuple[str, ...]
) -> None:
    for relative in paths:
        if require_entry(trusted_entries, relative) != require_entry(candidate_entries, relative):
            fail(f"Stage 5F external authority drift: {relative}")


def changed_entries(
    base_entries: dict[str, GitEntry], candidate_entries: dict[str, GitEntry]
) -> dict[str, dict[str, str]]:
    return {
        relative: entry.binding()
        for relative, entry in candidate_entries.items()
        if base_entries.get(relative) != entry
    }


def is_rotation_path_allowed(relative: str, next_stage: str) -> bool:
    if next_stage == PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE:
        return relative in PORTABLE_FORBIDDEN_SCANNER_REPAIR_PATHS
    if relative.startswith(WORKFLOW_PREFIX):
        return relative == AUTHORITY_WORKFLOW
    if relative in AUTHORITY_FILES or relative == ROTATION_MANIFEST:
        return True
    if relative in {"README.md", "docs/current-status.md", "docs/handoff.md"}:
        return True
    return relative.startswith((
        "docs/stage-5/",
        "fixtures/stage5f/",
        "tests/fixtures/stage5f/",
        "scripts/stage5f_",
    ))


def validate_rotation(
    authority: Path,
    authority_entries: dict[str, GitEntry],
    base: Path,
    base_entries: dict[str, GitEntry],
    candidate: Path,
    candidate_entries: dict[str, GitEntry],
    base_sha: str,
) -> None:
    manifest = parse_json(candidate, candidate_entries, ROTATION_MANIFEST, "authority rotation manifest")
    required_keys = {
        "authority_files",
        "canonical_ci_gate_sha256",
        "changed_paths",
        "kind",
        "next_generation",
        "next_stage",
        "previous_base_sha",
        "previous_generation",
        "previous_state_sha256",
        "schema_version",
    }
    if set(manifest) != required_keys:
        fail("authority rotation manifest key set drift")
    if manifest.get("schema_version") != 1 or manifest.get("kind") != "stage5f-authority-rotation":
        fail("authority rotation manifest identity drift")
    if manifest.get("previous_base_sha") != base_sha:
        fail("authority rotation manifest base SHA mismatch")
    if not isinstance(manifest.get("next_stage"), str) or not manifest["next_stage"].startswith("5F-"):
        fail("authority rotation manifest next stage is invalid")

    base_state = parse_json(base, base_entries, AUTHORITY_STATE, "base authority state")
    candidate_state = parse_json(candidate, candidate_entries, AUTHORITY_STATE, "candidate authority state")
    if set(base_state) != {
        "authority_generation",
        "previous_base_sha",
        "previous_state_sha256",
        "schema_version",
        "stage",
    } or base_state.get("schema_version") != 1:
        fail("base authority state schema drift")
    previous_generation = base_state.get("authority_generation")
    if not isinstance(previous_generation, int) or previous_generation < 1:
        fail("base authority generation is invalid")
    if manifest.get("previous_generation") != previous_generation:
        fail("authority rotation previous generation mismatch")
    if manifest.get("next_generation") != previous_generation + 1:
        fail("authority rotation next generation mismatch")
    if manifest.get("previous_state_sha256") != require_entry(base_entries, AUTHORITY_STATE).sha256:
        fail("authority rotation previous state digest mismatch")
    expected_state = {
        "authority_generation": previous_generation + 1,
        "previous_base_sha": base_sha,
        "previous_state_sha256": manifest["previous_state_sha256"],
        "schema_version": 1,
        "stage": manifest["next_stage"],
    }
    if candidate_state != expected_state:
        fail("candidate authority state is not an exact one-generation transition")

    deleted = set(base_entries) - set(candidate_entries)
    if deleted:
        fail("authority rotation deletes tracked source paths")
    changes = changed_entries(base_entries, candidate_entries)
    declared_changes = manifest.get("changed_paths")
    changes_without_manifest = {
        relative: binding for relative, binding in changes.items() if relative != ROTATION_MANIFEST
    }
    if not isinstance(declared_changes, dict) or declared_changes != changes_without_manifest:
        fail("authority rotation changed-path binding map mismatch")
    if not changes or ROTATION_MANIFEST not in changes or AUTHORITY_STATE not in changes:
        fail("authority rotation lacks mandatory manifest/state transition")
    if any(
        not is_rotation_path_allowed(relative, manifest["next_stage"])
        for relative in changes
    ):
        fail("authority rotation contains an out-of-scope path")

    if manifest["next_stage"] == PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE:
        scanner_entry = changes.get(PORTABLE_FORBIDDEN_SCANNER_PATH)
        if scanner_entry is None:
            fail("portable forbidden-scanner repair does not change the scanner")
        if scanner_entry.get("git_mode") != "100755":
            fail("portable forbidden-scanner repair changes scanner mode")

    declared_authority = manifest.get("authority_files")
    if not isinstance(declared_authority, dict) or set(declared_authority) != set(AUTHORITY_FILES):
        fail("authority rotation authority-file map drift")
    for relative in AUTHORITY_FILES:
        if declared_authority.get(relative) != require_entry(candidate_entries, relative).binding():
            fail(f"authority rotation candidate authority binding mismatch: {relative}")
    if all(
        base_entries.get(relative) == candidate_entries.get(relative)
        for relative in AUTHORITY_FILES
        if relative != AUTHORITY_STATE
    ):
        fail("authority rotation changes no authority file beyond state")

    # The canonical CI workflow owns required-check names. It stays immutable
    # under this rotation protocol; only the exact authority workflow itself
    # may evolve to validate the next generation.
    if require_entry(authority_entries, CI_WORKFLOW) != require_entry(candidate_entries, CI_WORKFLOW):
        fail("authority rotation attempts to change canonical CI workflow")
    expected_gate_digest = manifest.get("canonical_ci_gate_sha256")
    if not isinstance(expected_gate_digest, str) or not HEX64.fullmatch(expected_gate_digest):
        fail("authority rotation canonical CI gate digest is invalid")
    validate_gate_digest_consistency(candidate, candidate_entries, expected_gate_digest)

    for relative in R3_AUTHORITY_FILES:
        require_entry(authority_entries, relative)


def validate(authority: Path, base: Path, candidate: Path, base_sha: str) -> None:
    if not re.fullmatch(r"[0-9a-f]{40}", base_sha):
        fail("base SHA must be a full lowercase Git SHA-1")
    for root in (authority, base, candidate):
        if not root.is_dir() or root.is_symlink():
            fail("authority root is unsafe or missing")
    authority_entries = git_tree_entries(authority)
    base_entries = git_tree_entries(base)
    candidate_entries = git_tree_entries(candidate)
    candidate_rotation = candidate_entries.get(ROTATION_MANIFEST)
    base_rotation = base_entries.get(ROTATION_MANIFEST)
    if candidate_rotation is not None and candidate_rotation != base_rotation:
        validate_rotation(
            authority,
            authority_entries,
            base,
            base_entries,
            candidate,
            candidate_entries,
            base_sha,
        )
    else:
        reject_drift(authority_entries, candidate_entries, R3_AUTHORITY_FILES)
        reject_drift(base_entries, candidate_entries, BASE_AUTHORITY_FILES)
        if any(relative.startswith(WORKFLOW_PREFIX) for relative in changed_entries(base_entries, candidate_entries)):
            fail("ordinary candidate changes GitHub Actions workflow namespace")
        validate_gate_digest_consistency(candidate, candidate_entries)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--authority", type=Path, required=True)
    parser.add_argument("--base", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--base-sha", required=True)
    args = parser.parse_args()
    try:
        validate(args.authority, args.base, args.candidate, args.base_sha)
    except ContractFailure as exc:
        print(f"stage5f-base-authority-contract: FAIL: {exc}", file=sys.stderr)
        return 1
    print("stage5f-base-authority-contract: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
