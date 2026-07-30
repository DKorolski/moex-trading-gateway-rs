#!/usr/bin/env python3
"""Exercise the protected-base Stage 5F authority protocol without head code.

Every validation is performed by a copy of the contract from the staged base.
The candidate tree is never imported or executed.  Besides ordinary rebinding
attacks, the matrix proves that a reviewer-visible, hash-bound next generation
can rotate the authority in-band for Stage 5F-b.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
R3_AUTHORITY_REF = "8ce0acd60c7cb5cc5d25a27f6553077240658b57"
BASE_SHA = "a" * 40
WORKFLOW = ROOT / ".github/workflows/stage5f-base-authority.yml"


def git_output(*args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=ROOT, check=True, capture_output=True, text=True
    ).stdout.strip()


def export_ref(ref: str, destination: Path) -> Path:
    archive = subprocess.run(
        ["git", "archive", "--format=tar", ref],
        cwd=ROOT,
        check=True,
        capture_output=True,
    ).stdout
    archive_path = destination / f"{ref[:12]}.tar"
    archive_path.write_bytes(archive)
    target = destination / ref[:12]
    with tarfile.open(archive_path) as tar:
        for member in tar.getmembers():
            member_path = Path(member.name)
            if (
                member_path.is_absolute()
                or ".." in member_path.parts
                or member.issym()
                or member.islnk()
                or member.isdev()
                or member.isfifo()
            ):
                raise RuntimeError(f"unsafe authority snapshot member: {member.name}")
            tar.extract(member, target)
    return target


def fresh_candidate(base: Path, root: Path) -> tuple[Path, Path]:
    candidate = Path(tempfile.mkdtemp(prefix="candidate-", dir=root))
    shutil.copytree(base, candidate, dirs_exist_ok=True)
    return candidate, candidate / "candidate-executed.txt"


def run_base_contract(authority: Path, base: Path, candidate: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            "python3",
            str(base / "scripts/stage5f_base_authority_contract.py"),
            "--authority",
            str(authority),
            "--base",
            str(base),
            "--candidate",
            str(candidate),
            "--base-sha",
            BASE_SHA,
        ],
        check=False,
        capture_output=True,
        text=True,
    )


def assert_accepted(authority: Path, base: Path, candidate: Path, sentinel: Path, case: str) -> None:
    completed = run_base_contract(authority, base, candidate)
    if completed.returncode != 0:
        raise RuntimeError(f"{case}: unexpected rejection: {completed.stderr.strip()}")
    if sentinel.exists():
        raise RuntimeError(f"{case}: candidate-owned code executed")
    print(f"PASS {case}")


def assert_rejected(authority: Path, base: Path, candidate: Path, sentinel: Path, case: str) -> None:
    completed = run_base_contract(authority, base, candidate)
    if completed.returncode == 0:
        raise RuntimeError(f"{case}: candidate was accepted")
    if sentinel.exists():
        raise RuntimeError(f"{case}: candidate-owned code executed")
    print(f"PASS {case}")


def mutate_coordinated_rebind(candidate: Path, sentinel: Path) -> None:
    mutations = {
        ".github/workflows/ci.yml": "# coordinated-rebind workflow authority\n",
        "scripts/stage5f_ci_snapshot_inheritance_check.py": (
            f"\nPath({str(sentinel)!r}).write_text('candidate executed')\n"
        ),
        "scripts/stage5f_atomic_hybrid_semantics_entry_check.py": "\n# entry authority\n",
        "docs/stage-5/stage5f-a-atomic-hybrid-semantics-entry-inventory.json": "\n",
        "scripts/handoff_safety_check.py": "\n# handoff authority\n",
    }
    for relative, suffix in mutations.items():
        path = candidate / relative
        path.write_text(path.read_text() + suffix)


def mutate_symlink(candidate: Path, relative: str) -> None:
    path = candidate / relative
    target = candidate / ".authority-symlink-target"
    target.write_text("not authority bytes\n")
    path.unlink()
    path.symlink_to(target)


def mutate_gitlink_shape(candidate: Path, relative: str) -> None:
    path = candidate / relative
    path.unlink()
    path.mkdir()


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def tree_hashes(root: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for path in root.rglob("*"):
        relative = path.relative_to(root)
        if relative.parts and relative.parts[0] == ".git":
            continue
        if path.is_file() and not path.is_symlink():
            result[relative.as_posix()] = sha256(path)
    return result


def build_valid_rotation(authority: Path, base: Path, candidate: Path) -> None:
    # A harmless descriptor change proves that an R3-rooted authority file can
    # advance only through the old base contract.  It remains data here.
    descriptor = candidate / "scripts/stage5f_descriptor.py"
    descriptor.write_text(descriptor.read_text() + "\n# staged authority generation two\n")
    state_path = candidate / "docs/stage-5/stage5f-authority-state.json"
    previous_state_sha = sha256(base / "docs/stage-5/stage5f-authority-state.json")
    state_path.write_text(
        json.dumps(
            {
                "authority_generation": 2,
                "previous_base_sha": BASE_SHA,
                "previous_state_sha256": previous_state_sha,
                "schema_version": 1,
                "stage": "5F-b-fixture-input-redacted-fingerprint-schema",
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )
    contract = __import__("stage5f_base_authority_contract")
    candidate_hashes = tree_hashes(candidate)
    base_hashes = tree_hashes(base)
    changed = {
        relative: digest
        for relative, digest in candidate_hashes.items()
        if base_hashes.get(relative) != digest
    }
    manifest = {
        "authority_files": {
            relative: candidate_hashes[relative] for relative in contract.AUTHORITY_FILES
        },
        "canonical_ci_gate_sha256": candidate_hashes[
            "scripts/stage5f_atomic_hybrid_semantics_gate.sh"
        ],
        "changed_paths": changed,
        "kind": "stage5f-authority-rotation",
        "next_generation": 2,
        "next_stage": "5F-b-fixture-input-redacted-fingerprint-schema",
        "previous_base_sha": BASE_SHA,
        "previous_generation": 1,
        "previous_state_sha256": previous_state_sha,
        "schema_version": 1,
    }
    (candidate / contract.ROTATION_MANIFEST).write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    )


def refresh_rotation_hashes(base: Path, candidate: Path) -> None:
    contract = __import__("stage5f_base_authority_contract")
    manifest_path = candidate / contract.ROTATION_MANIFEST
    manifest = json.loads(manifest_path.read_text())
    candidate_hashes = tree_hashes(candidate)
    base_hashes = tree_hashes(base)
    manifest["authority_files"] = {
        relative: candidate_hashes[relative] for relative in contract.AUTHORITY_FILES
    }
    manifest["changed_paths"] = {
        relative: digest
        for relative, digest in candidate_hashes.items()
        if relative != contract.ROTATION_MANIFEST and base_hashes.get(relative) != digest
    }
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")


def validate_workflow_contract() -> None:
    workflow = WORKFLOW.read_text()
    required = (
        "pull_request_target:",
        'ref: "8ce0acd60c7cb5cc5d25a27f6553077240658b57"',
        "path: authority",
        "path: base",
        "path: candidate",
        "persist-credentials: false",
        "Validate candidate authority data before execution",
        "base/scripts/stage5f_base_authority_contract.py",
        "--base-sha",
        "Execute only accepted R3 verifier and B3F provenance",
        "python3 scripts/stage5f_atomic_hybrid_semantics_entry_check.py",
        "python3 scripts/stage5f_ci_snapshot_inheritance_check.py --execute-verified-provenance",
    )
    for value in required:
        if value not in workflow:
            raise RuntimeError(f"base-authority workflow contract missing: {value}")
    if "candidate/scripts/" in workflow or "cd candidate" in workflow:
        raise RuntimeError("base-authority workflow executes candidate-owned code")


def current_staged_tree() -> str:
    if subprocess.run(["git", "diff", "--quiet"], cwd=ROOT).returncode != 0:
        raise RuntimeError("base-authority harness requires staged working files")
    return git_output("write-tree")


def main() -> int:
    try:
        validate_workflow_contract()
        contract = __import__("stage5f_base_authority_contract")
        with tempfile.TemporaryDirectory(prefix="stage5f-base-authority-") as directory:
            root = Path(directory)
            authority = export_ref(R3_AUTHORITY_REF, root)
            base = export_ref(current_staged_tree(), root)

            candidate, sentinel = fresh_candidate(base, root)
            assert_accepted(authority, base, candidate, sentinel, "clean-next-pr-from-current-base")

            candidate, sentinel = fresh_candidate(base, root)
            mutate_coordinated_rebind(candidate, sentinel)
            assert_rejected(authority, base, candidate, sentinel, "coordinated-r3-rebind")

            for relative in contract.BASE_AUTHORITY_FILES:
                candidate, sentinel = fresh_candidate(base, root)
                (candidate / relative).write_text(
                    (candidate / relative).read_text() + "# base-authority-drift\n"
                )
                assert_rejected(authority, base, candidate, sentinel, f"base-drift-{relative}")

            for relative in contract.AUTHORITY_FILES:
                candidate, sentinel = fresh_candidate(base, root)
                mutate_symlink(candidate, relative)
                assert_rejected(authority, base, candidate, sentinel, f"symlink-{relative}")

                candidate, sentinel = fresh_candidate(base, root)
                mutate_gitlink_shape(candidate, relative)
                assert_rejected(authority, base, candidate, sentinel, f"gitlink-{relative}")

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(authority, base, candidate)
            assert_accepted(authority, base, candidate, sentinel, "reviewable-in-band-authority-rotation")

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(authority, base, candidate)
            manifest_path = candidate / contract.ROTATION_MANIFEST
            manifest = json.loads(manifest_path.read_text())
            manifest["canonical_ci_gate_sha256"] = "0" * 64
            manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
            assert_rejected(authority, base, candidate, sentinel, "rotation-conflicting-gate-authority")

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(authority, base, candidate)
            inventory_path = candidate / "docs/stage-5/stage5f-a-atomic-hybrid-semantics-entry-inventory.json"
            inventory = json.loads(inventory_path.read_text())
            inventory["ci_snapshot_authority"]["stage5f_atomic_hybrid_semantics_gate_sha256"] = "0" * 64
            inventory_path.write_text(json.dumps(inventory, indent=2, sort_keys=False) + "\n")
            refresh_rotation_hashes(base, candidate)
            assert_rejected(authority, base, candidate, sentinel, "rotation-one-sided-inventory-gate-digest")

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(authority, base, candidate)
            (candidate / "crates/forbidden-stage5f-rotation.rs").write_text("// forbidden\n")
            assert_rejected(authority, base, candidate, sentinel, "rotation-out-of-scope-runtime-path")
    except (OSError, RuntimeError, subprocess.CalledProcessError, tarfile.TarError) as exc:
        print(f"stage5f-base-authority-negative: FAIL: {exc}", file=sys.stderr)
        return 1
    expected_cases = 1 + 1 + len(contract.BASE_AUTHORITY_FILES) + 2 * len(contract.AUTHORITY_FILES) + 4
    print(f"stage5f-base-authority-negative: ok cases={expected_cases}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
