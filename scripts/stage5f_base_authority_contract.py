#!/usr/bin/env python3
"""Validate the Stage 5F base-controlled authority boundary without head code.

The GitHub ``pull_request_target`` workflow invokes this file from the
protected pull-request base.  Candidate files are treated solely as bytes and
JSON data.  A normal pull request must preserve every authority file.  The
only exception is a tightly described authority-rotation manifest, which is
reviewable data bound to the exact base revision and the exact next bytes.
"""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
import re
import stat
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
GATE = "scripts/stage5f_atomic_hybrid_semantics_gate.sh"
CI_WORKFLOW = ".github/workflows/ci.yml"
INVENTORY = "docs/stage-5/stage5f-a-atomic-hybrid-semantics-entry-inventory.json"
ENTRY_CHECKER = "scripts/stage5f_atomic_hybrid_semantics_entry_check.py"
HANDOFF_CHECKER = "scripts/handoff_safety_check.py"
HEX64 = re.compile(r"[0-9a-f]{64}\Z")
SHA_LINE = re.compile(
    r'verify_sha256 "([0-9a-f]{64})" '
    r'"scripts/stage5f_atomic_hybrid_semantics_gate\.sh"'
)


class ContractFailure(RuntimeError):
    pass


def fail(message: str) -> None:
    raise ContractFailure(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require_file(root: Path, relative: str) -> Path:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        fail(f"unsafe or missing authority file: {relative}")
    return path


def tree_hashes(root: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for path in root.rglob("*"):
        relative = path.relative_to(root)
        if relative.parts and relative.parts[0] == ".git":
            continue
        if path.is_symlink():
            fail(f"candidate tree contains symlink: {relative.as_posix()}")
        mode = path.lstat().st_mode
        if stat.S_ISREG(mode):
            result[relative.as_posix()] = sha256(path)
        elif stat.S_ISDIR(mode):
            continue
        else:
            fail(f"candidate tree contains special entry: {relative.as_posix()}")
    return result


def parse_json(path: Path, label: str) -> dict[str, object]:
    try:
        payload = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"invalid {label}: {exc}")
    if not isinstance(payload, dict):
        fail(f"invalid {label}: object required")
    return payload


def parse_static_dict(path: Path, variable: str) -> dict[str, object]:
    try:
        tree = ast.parse(path.read_text(), filename=str(path))
    except SyntaxError as exc:
        fail(f"invalid Python authority source: {path}: {exc}")
    for node in tree.body:
        if not isinstance(node, ast.Assign):
            continue
        if any(isinstance(target, ast.Name) and target.id == variable for target in node.targets):
            try:
                value = ast.literal_eval(node.value)
            except ValueError as exc:
                fail(f"non-literal authority declaration: {path}: {variable}: {exc}")
            if isinstance(value, dict):
                return value
            fail(f"invalid authority declaration: {path}: {variable}")
    fail(f"missing authority declaration: {path}: {variable}")


def gate_digest_from_ci(candidate: Path) -> str:
    matches = SHA_LINE.findall(require_file(candidate, CI_WORKFLOW).read_text())
    if len(matches) != 1:
        fail("canonical CI must contain exactly one Stage 5F gate digest")
    return matches[0]


def gate_digest_from_inventory(candidate: Path) -> str:
    authority = parse_json(require_file(candidate, INVENTORY), "Stage 5F inventory").get(
        "ci_snapshot_authority"
    )
    if not isinstance(authority, dict):
        fail("Stage 5F inventory authority is missing")
    value = authority.get("stage5f_atomic_hybrid_semantics_gate_sha256")
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        fail("Stage 5F inventory gate digest is invalid")
    return value


def gate_digest_from_static_authority(candidate: Path, path: str, variable: str) -> str:
    authority = parse_static_dict(
        require_file(candidate, path), variable
    )
    value = authority.get("stage5f_atomic_hybrid_semantics_gate_sha256")
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        fail(f"Stage 5F authority gate digest is invalid: {path}")
    return value


def validate_gate_digest_consistency(candidate: Path, expected: str | None = None) -> None:
    values = {
        "canonical CI": gate_digest_from_ci(candidate),
        "inventory": gate_digest_from_inventory(candidate),
        "entry checker": gate_digest_from_static_authority(
            candidate, ENTRY_CHECKER, "EXPECTED_CI_SNAPSHOT_AUTHORITY"
        ),
        "handoff checker": gate_digest_from_static_authority(
            candidate, HANDOFF_CHECKER, "STAGE5F_CI_SNAPSHOT_AUTHORITY"
        ),
        "actual gate": sha256(require_file(candidate, GATE)),
    }
    if len(set(values.values())) != 1:
        fail("Stage 5F gate authority digest split")
    if expected is not None and values["actual gate"] != expected:
        fail("rotation manifest Stage 5F gate digest mismatch")


def reject_drift(trusted: Path, candidate: Path, paths: tuple[str, ...]) -> None:
    for relative in paths:
        if sha256(require_file(trusted, relative)) != sha256(require_file(candidate, relative)):
            fail(f"Stage 5F external authority drift: {relative}")


def is_rotation_path_allowed(relative: str) -> bool:
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


def validate_rotation(authority: Path, base: Path, candidate: Path, base_sha: str) -> None:
    manifest = parse_json(require_file(candidate, ROTATION_MANIFEST), "authority rotation manifest")
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

    base_state = parse_json(require_file(base, AUTHORITY_STATE), "base authority state")
    candidate_state = parse_json(require_file(candidate, AUTHORITY_STATE), "candidate authority state")
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
    if manifest.get("previous_state_sha256") != sha256(require_file(base, AUTHORITY_STATE)):
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

    candidate_hashes = tree_hashes(candidate)
    base_hashes = tree_hashes(base)
    changed_paths = {
        relative: digest
        for relative, digest in candidate_hashes.items()
        if base_hashes.get(relative) != digest
    }
    deleted = set(base_hashes) - set(candidate_hashes)
    if deleted:
        fail("authority rotation deletes tracked source paths")
    declared_changes = manifest.get("changed_paths")
    changed_paths_without_manifest = {
        relative: digest
        for relative, digest in changed_paths.items()
        if relative != ROTATION_MANIFEST
    }
    if not isinstance(declared_changes, dict) or declared_changes != changed_paths_without_manifest:
        fail("authority rotation changed-path digest map mismatch")
    if not changed_paths or ROTATION_MANIFEST not in changed_paths or AUTHORITY_STATE not in changed_paths:
        fail("authority rotation lacks mandatory manifest/state transition")
    if any(not is_rotation_path_allowed(relative) for relative in changed_paths):
        fail("authority rotation contains an out-of-scope path")

    declared_authority = manifest.get("authority_files")
    if not isinstance(declared_authority, dict) or set(declared_authority) != set(AUTHORITY_FILES):
        fail("authority rotation authority-file map drift")
    for relative in AUTHORITY_FILES:
        digest = declared_authority.get(relative)
        if not isinstance(digest, str) or not HEX64.fullmatch(digest):
            fail(f"authority rotation invalid digest: {relative}")
        if candidate_hashes.get(relative) != digest:
            fail(f"authority rotation candidate authority digest mismatch: {relative}")
    if all(base_hashes.get(relative) == candidate_hashes.get(relative) for relative in AUTHORITY_FILES if relative != AUTHORITY_STATE):
        fail("authority rotation changes no authority file beyond state")

    expected_gate_digest = manifest.get("canonical_ci_gate_sha256")
    if not isinstance(expected_gate_digest, str) or not HEX64.fullmatch(expected_gate_digest):
        fail("authority rotation canonical CI gate digest is invalid")
    validate_gate_digest_consistency(candidate, expected_gate_digest)

    # R3 remains a separate accepted predecessor.  A rotation is permissible
    # only when every R3 byte is explicitly hash-bound in the manifest.
    for relative in R3_AUTHORITY_FILES:
        require_file(authority, relative)


def validate(authority: Path, base: Path, candidate: Path, base_sha: str) -> None:
    if not re.fullmatch(r"[0-9a-f]{40}", base_sha):
        fail("base SHA must be a full lowercase Git SHA-1")
    for root in (authority, base, candidate):
        if not root.is_dir() or root.is_symlink():
            fail("authority root is unsafe or missing")
    if (candidate / ROTATION_MANIFEST).is_file() and (
        not (base / ROTATION_MANIFEST).is_file()
        or sha256(candidate / ROTATION_MANIFEST) != sha256(base / ROTATION_MANIFEST)
    ):
        validate_rotation(authority, base, candidate, base_sha)
    else:
        reject_drift(authority, candidate, R3_AUTHORITY_FILES)
        reject_drift(base, candidate, BASE_AUTHORITY_FILES)
        validate_gate_digest_consistency(candidate)


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
