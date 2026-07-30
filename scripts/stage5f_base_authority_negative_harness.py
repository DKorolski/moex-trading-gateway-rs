#!/usr/bin/env python3
"""Exercise the base-controlled Stage 5F authority boundary without candidate code.

The production pull_request_target workflow executes only the accepted R3
verifier and B3F runner. Pull-request files are compared as bytes. This local
harness proves the bootstrap invariant with the staged current tree: a clean
next PR is admitted, while coordinated R3 rebinding, R4 authority drift and
symlink/gitlink substitutions are rejected without execution.
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
    "scripts/handoff_safety_check.py",
    "scripts/stage5f_atomic_hybrid_semantics_entry_check.py",
    "scripts/stage5f_atomic_hybrid_semantics_gate.sh",
    "scripts/stage5f_base_authority_negative_harness.py",
)
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


def validate_candidate(authority: Path, base: Path, candidate: Path) -> None:
    reject_drift(authority, candidate, R3_AUTHORITY_FILES)
    reject_drift(base, candidate, BASE_AUTHORITY_FILES)


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


def fresh_candidate(base: Path, root: Path) -> tuple[Path, Path]:
    candidate = Path(tempfile.mkdtemp(prefix="candidate-", dir=root))
    shutil.copytree(base, candidate, dirs_exist_ok=True)
    return candidate, candidate / "candidate-executed.txt"


def assert_rejected(
    authority: Path,
    base: Path,
    candidate: Path,
    sentinel: Path,
    expected_relative: str,
    case_name: str,
) -> None:
    try:
        validate_candidate(authority, base, candidate)
    except RuntimeError as exc:
        if str(exc) != f"Stage 5F external authority drift: {expected_relative}":
            raise RuntimeError(f"{case_name}: unexpected rejection: {exc}") from exc
    else:
        raise RuntimeError(f"{case_name}: candidate was accepted")
    if sentinel.exists():
        raise RuntimeError(f"{case_name}: candidate-owned code executed")
    print(f"PASS {case_name}")


def mutate_coordinated_rebind(candidate: Path, sentinel: Path) -> None:
    # The exact five roots from R3 P0 span the immutable R3 and R4 base sets.
    # The verifier would create this sentinel if any candidate code were run.
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


def current_staged_tree() -> str:
    if subprocess.run(["git", "diff", "--quiet"], cwd=ROOT).returncode != 0:
        raise RuntimeError("base-authority harness requires staged working files")
    return git_output("write-tree")


def main() -> int:
    try:
        validate_workflow_contract()
        with tempfile.TemporaryDirectory(prefix="stage5f-base-authority-") as directory:
            root = Path(directory)
            authority = export_ref(R3_AUTHORITY_REF, root)
            base = export_ref(current_staged_tree(), root)

            candidate, sentinel = fresh_candidate(base, root)
            validate_candidate(authority, base, candidate)
            if sentinel.exists():
                raise RuntimeError("clean-next-pr: candidate-owned code executed")
            print("PASS clean-next-pr-from-current-base")

            candidate, sentinel = fresh_candidate(base, root)
            mutate_coordinated_rebind(candidate, sentinel)
            assert_rejected(
                authority,
                base,
                candidate,
                sentinel,
                ".github/workflows/ci.yml",
                "coordinated-r3-rebind-rejected-before-candidate-execution",
            )

            for relative in BASE_AUTHORITY_FILES:
                candidate, sentinel = fresh_candidate(base, root)
                (candidate / relative).write_text(
                    (candidate / relative).read_text() + "# r4-base-authority-drift\n"
                )
                assert_rejected(
                    authority,
                    base,
                    candidate,
                    sentinel,
                    relative,
                    f"r4-base-authority-drift-{relative}",
                )

            for relative in (*R3_AUTHORITY_FILES, *BASE_AUTHORITY_FILES):
                candidate, sentinel = fresh_candidate(base, root)
                mutate_symlink(candidate, relative)
                assert_rejected(
                    authority,
                    base,
                    candidate,
                    sentinel,
                    relative,
                    f"symlink-replacement-{relative}",
                )

                candidate, sentinel = fresh_candidate(base, root)
                mutate_gitlink_shape(candidate, relative)
                assert_rejected(
                    authority,
                    base,
                    candidate,
                    sentinel,
                    relative,
                    f"gitlink-replacement-{relative}",
                )
    except (OSError, RuntimeError, subprocess.CalledProcessError, tarfile.TarError) as exc:
        print(f"stage5f-base-authority-negative: FAIL: {exc}", file=sys.stderr)
        return 1
    expected_cases = 1 + 1 + len(BASE_AUTHORITY_FILES) + 2 * len(
        R3_AUTHORITY_FILES + BASE_AUTHORITY_FILES
    )
    print(f"stage5f-base-authority-negative: ok cases={expected_cases}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
