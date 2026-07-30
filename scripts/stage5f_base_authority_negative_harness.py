#!/usr/bin/env python3
"""Prove that the base-controlled Stage 5F workflow rejects a coordinated rebind.

This is local evidence for the workflow bootstrap. The production workflow is
the trust root: on pull_request_target it runs from the protected base and uses
the candidate checkout strictly as byte data. This harness never executes a
candidate file.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
R3_AUTHORITY_REF = "8ce0acd60c7cb5cc5d25a27f6553077240658b57"
R3_AUTHORITY_FILES = (
    ".github/workflows/ci.yml",
    "docs/stage-5/stage5f-a-atomic-hybrid-semantics-entry-inventory.json",
    "scripts/handoff_safety_check.py",
    "scripts/make_handoff_archive.sh",
    "scripts/stage5f_atomic_hybrid_semantics_entry_check.py",
    "scripts/stage5f_atomic_hybrid_semantics_gate.sh",
    "scripts/stage5f_atomic_hybrid_semantics_negative_harness.py",
    "scripts/stage5f_b3f_snapshot_provenance_gate.sh",
    "scripts/stage5f_ci_snapshot_inheritance_check.py",
    "scripts/stage5f_ci_snapshot_inheritance_negative_harness.py",
    "scripts/stage5f_descriptor.py",
)
BASE_AUTHORITY_FILES = (
    ".github/CODEOWNERS",
    ".github/workflows/stage5f-base-authority.yml",
)
WORKFLOW = ROOT / ".github/workflows/stage5f-base-authority.yml"


def export_snapshot(destination: Path) -> None:
    archive = subprocess.run(
        ["git", "archive", "--format=tar", R3_AUTHORITY_REF],
        cwd=ROOT,
        check=True,
        capture_output=True,
    ).stdout
    archive_path = destination / "authority.tar"
    archive_path.write_bytes(archive)
    with tarfile.open(archive_path) as tar:
        target = destination / "tree"
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


def reject_drift(trusted: Path, candidate: Path, paths: tuple[str, ...]) -> None:
    for relative in paths:
        trusted_path = trusted / relative
        candidate_path = candidate / relative
        if (
            not trusted_path.is_file()
            or not candidate_path.is_file()
            or trusted_path.is_symlink()
            or candidate_path.is_symlink()
            or trusted_path.read_bytes() != candidate_path.read_bytes()
        ):
            raise RuntimeError(f"Stage 5F external authority drift: {relative}")


def validate_workflow_contract() -> None:
    workflow = WORKFLOW.read_text()
    required = (
        "pull_request_target:",
        'ref: "8ce0acd60c7cb5cc5d25a27f6553077240658b57"',
        "path: authority",
        "path: base",
        "path: candidate",
        "persist-credentials: false",
        "Reject candidate authority rebinding before execution",
        "Execute only accepted R3 verifier and B3F provenance",
        "python3 scripts/stage5f_atomic_hybrid_semantics_entry_check.py",
        "python3 scripts/stage5f_ci_snapshot_inheritance_check.py --execute-verified-provenance",
    )
    for value in required:
        if value not in workflow:
            raise RuntimeError(f"base-authority workflow contract missing: {value}")
    if "candidate/scripts/" in workflow or "cd candidate" in workflow:
        raise RuntimeError("base-authority workflow executes candidate-owned code")
    for relative in (*R3_AUTHORITY_FILES, *BASE_AUTHORITY_FILES):
        if relative not in workflow:
            raise RuntimeError(f"base-authority workflow does not freeze: {relative}")


def mutate_coordinated_rebind(candidate: Path, sentinel: Path) -> None:
    # These are the five mutually-rebound roots from the R3 P0 proof. The
    # verifier is intentionally armed to create a sentinel if candidate code is
    # ever executed; this harness only compares bytes and must leave it absent.
    mutations = {
        ".github/workflows/ci.yml": "# coordinated-rebind workflow authority\n",
        "scripts/stage5f_ci_snapshot_inheritance_check.py": (
            f"\nPath({str(sentinel)!r}).write_text('candidate executed')\n"
        ),
        "scripts/stage5f_atomic_hybrid_semantics_entry_check.py": (
            "\n# coordinated-rebind entry authority\n"
        ),
        "docs/stage-5/stage5f-a-atomic-hybrid-semantics-entry-inventory.json": "\n",
        "scripts/handoff_safety_check.py": "\n# coordinated-rebind handoff authority\n",
    }
    for relative, suffix in mutations.items():
        path = candidate / relative
        path.write_text(path.read_text() + suffix)


def main() -> int:
    try:
        validate_workflow_contract()
        with tempfile.TemporaryDirectory(prefix="stage5f-base-authority-") as directory:
            root = Path(directory)
            export_snapshot(root)
            authority = root / "tree"
            candidate = root / "candidate"
            shutil.copytree(authority, candidate)
            reject_drift(authority, candidate, R3_AUTHORITY_FILES)
            sentinel = root / "candidate-executed.txt"
            mutate_coordinated_rebind(candidate, sentinel)
            try:
                reject_drift(authority, candidate, R3_AUTHORITY_FILES)
            except RuntimeError as exc:
                if str(exc) != "Stage 5F external authority drift: .github/workflows/ci.yml":
                    raise
            else:
                raise RuntimeError("coordinated rebind was accepted")
            if sentinel.exists():
                raise RuntimeError("candidate-owned verifier executed during authority check")
    except (OSError, RuntimeError, subprocess.CalledProcessError, tarfile.TarError) as exc:
        print(f"stage5f-base-authority-negative: FAIL: {exc}", file=sys.stderr)
        return 1
    print("PASS base-workflow-contract")
    print("PASS coordinated-rebind-rejected-before-candidate-execution")
    print("stage5f-base-authority-negative: ok cases=2")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
