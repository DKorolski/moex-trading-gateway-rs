#!/usr/bin/env python3
"""Exercise Stage 5F authority checks against real temporary Git trees.

The production job reads only Git entries in the protected base/candidate
checkouts. This harness therefore commits each synthetic candidate before it
is inspected, including artificial mode-160000 gitlinks that an ordinary
checkout represents as empty directories.
"""

from __future__ import annotations

import hashlib
import json
import os
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
ASSERTIONS_EXECUTED = 0


def git_output(*args: str, cwd: Path = ROOT) -> str:
    return subprocess.run(
        ["git", *args], cwd=cwd, check=True, capture_output=True, text=True
    ).stdout.strip()


def git_run(*args: str, cwd: Path) -> None:
    subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True)


def commit_worktree(repo: Path, message: str) -> None:
    git_run("add", "-A", cwd=repo)
    if subprocess.run(["git", "diff", "--cached", "--quiet"], cwd=repo).returncode == 0:
        return
    git_run(
        "-c",
        "user.name=stage5f-authority-test",
        "-c",
        "user.email=stage5f-authority-test@example.invalid",
        "commit",
        "--quiet",
        "-m",
        message,
        cwd=repo,
    )


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
    git_run("init", "--quiet", cwd=target)
    git_run("add", "-A", cwd=target)
    git_run(
        "-c",
        "user.name=stage5f-authority-test",
        "-c",
        "user.email=stage5f-authority-test@example.invalid",
        "commit",
        "--quiet",
        "-m",
        "authority snapshot",
        cwd=target,
    )
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
    global ASSERTIONS_EXECUTED
    completed = run_base_contract(authority, base, candidate)
    if completed.returncode != 0:
        raise RuntimeError(f"{case}: unexpected rejection: {completed.stderr.strip()}")
    if sentinel.exists():
        raise RuntimeError(f"{case}: candidate-owned code executed")
    ASSERTIONS_EXECUTED += 1
    print(f"PASS {case}")


def assert_rejected(authority: Path, base: Path, candidate: Path, sentinel: Path, case: str) -> None:
    global ASSERTIONS_EXECUTED
    completed = run_base_contract(authority, base, candidate)
    if completed.returncode == 0:
        raise RuntimeError(f"{case}: candidate was accepted")
    if sentinel.exists():
        raise RuntimeError(f"{case}: candidate-owned code executed")
    ASSERTIONS_EXECUTED += 1
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
    commit_worktree(candidate, "coordinated rebind")


def mutate_symlink(candidate: Path, relative: str) -> None:
    path = candidate / relative
    target = candidate / ".authority-symlink-target"
    target.write_text("not authority bytes\n")
    path.unlink()
    path.symlink_to(target)
    commit_worktree(candidate, "symlink substitution")


def mutate_directory_shape(candidate: Path, relative: str) -> None:
    path = candidate / relative
    path.unlink()
    path.mkdir()
    commit_worktree(candidate, "directory substitution")


def file_binding(path: Path) -> dict[str, str]:
    mode = "100755" if path.stat().st_mode & 0o111 else "100644"
    return {"git_mode": mode, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def worktree_bindings(root: Path) -> dict[str, dict[str, str]]:
    result: dict[str, dict[str, str]] = {}
    for path in root.rglob("*"):
        relative = path.relative_to(root)
        if relative.parts and relative.parts[0] == ".git":
            continue
        if path.is_file() and not path.is_symlink():
            result[relative.as_posix()] = file_binding(path)
    return result


def base_authority_state(base: Path, contract: object) -> dict[str, object]:
    state = json.loads((base / contract.AUTHORITY_STATE).read_text())
    if not isinstance(state, dict):
        raise RuntimeError("base authority state is not an object")
    generation = state.get("authority_generation")
    if type(generation) is not int:
        raise RuntimeError("base authority generation is invalid")
    return state


def build_valid_rotation(
    authority: Path,
    base: Path,
    candidate: Path,
    *,
    next_stage: str = "5F-b-fixture-input-redacted-fingerprint-schema",
    mutate_scanner: bool = False,
    mutate_stage5d_freeze_rebind_paths: bool = False,
    add_out_of_scope_readme_change: bool = False,
    add_out_of_scope_stage5d_change: bool = False,
) -> None:
    del authority
    contract = __import__("stage5f_base_authority_contract")
    base_entries = contract.git_tree_entries(base)
    base_state = base_authority_state(base, contract)
    previous_generation = base_state["authority_generation"]
    previous_state_sha = base_entries[contract.AUTHORITY_STATE].sha256
    if next_stage == contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE:
        entry_checker = candidate / "scripts/stage5f_atomic_hybrid_semantics_entry_check.py"
        entry_checker.write_text(entry_checker.read_text() + "\n# staged portable scanner authority\n")
        if add_out_of_scope_readme_change:
            readme = candidate / "README.md"
            readme.write_text(readme.read_text() + "\n<!-- staged scanner scope drift -->\n")
        if mutate_stage5d_freeze_rebind_paths:
            for relative in (
                "docs/stage-5/stage-5d-additive-freeze-manifest.json",
                "scripts/stage5d_additive_freeze_check.py",
            ):
                path = candidate / relative
                path.write_text(path.read_text() + "\n# staged r9 freeze rebind\n")
        if add_out_of_scope_stage5d_change:
            path = candidate / "scripts/stage5d_additive_freeze_negative_harness.py"
            path.write_text(path.read_text() + "\n# staged r9 scope drift\n")
    else:
        descriptor = candidate / "scripts/stage5f_descriptor.py"
        descriptor.write_text(descriptor.read_text() + "\n# staged authority generation successor\n")
    if mutate_scanner:
        scanner = candidate / contract.PORTABLE_FORBIDDEN_SCANNER_PATH
        scanner.write_text(scanner.read_text() + "\n# staged portable scanner repair\n")
    state_path = candidate / contract.AUTHORITY_STATE
    state_path.write_text(
        json.dumps(
            {
                "authority_generation": previous_generation + 1,
                "previous_base_sha": BASE_SHA,
                "previous_state_sha256": previous_state_sha,
                "schema_version": 1,
                "stage": next_stage,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )
    candidate_bindings = worktree_bindings(candidate)
    base_bindings = {
        relative: entry.binding() for relative, entry in base_entries.items()
    }
    changed = {
        relative: binding
        for relative, binding in candidate_bindings.items()
        if base_bindings.get(relative) != binding
    }
    manifest = {
        "authority_files": {
            relative: candidate_bindings[relative] for relative in contract.AUTHORITY_FILES
        },
        "canonical_ci_gate_sha256": candidate_bindings[contract.GATE]["sha256"],
        "changed_paths": changed,
        "kind": "stage5f-authority-rotation",
        "next_generation": previous_generation + 1,
        "next_stage": next_stage,
        "previous_base_sha": BASE_SHA,
        "previous_generation": previous_generation,
        "previous_state_sha256": previous_state_sha,
        "schema_version": 1,
    }
    (candidate / contract.ROTATION_MANIFEST).write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    )
    commit_worktree(candidate, "valid rotation")


def refresh_rotation_bindings(base: Path, candidate: Path) -> None:
    contract = __import__("stage5f_base_authority_contract")
    manifest_path = candidate / contract.ROTATION_MANIFEST
    manifest = json.loads(manifest_path.read_text())
    candidate_bindings = worktree_bindings(candidate)
    base_entries = contract.git_tree_entries(base)
    base_bindings = {
        relative: entry.binding() for relative, entry in base_entries.items()
    }
    manifest["authority_files"] = {
        relative: candidate_bindings[relative] for relative in contract.AUTHORITY_FILES
    }
    manifest["changed_paths"] = {
        relative: binding
        for relative, binding in candidate_bindings.items()
        if relative != contract.ROTATION_MANIFEST and base_bindings.get(relative) != binding
    }
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    commit_worktree(candidate, "refresh rotation bindings")


def rewrite_authority_state(root: Path, **updates: object) -> None:
    contract = __import__("stage5f_base_authority_contract")
    state_path = root / contract.AUTHORITY_STATE
    state = json.loads(state_path.read_text())
    if not isinstance(state, dict):
        raise RuntimeError("authority state is not an object")
    state.update(updates)
    state_path.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n")
    commit_worktree(root, "rewrite authority state")


def rewrite_rotation_manifest(root: Path, **updates: object) -> None:
    contract = __import__("stage5f_base_authority_contract")
    manifest_path = root / contract.ROTATION_MANIFEST
    manifest = json.loads(manifest_path.read_text())
    if not isinstance(manifest, dict):
        raise RuntimeError("authority rotation manifest is not an object")
    manifest.update(updates)
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    commit_worktree(root, "rewrite authority rotation manifest")


def add_gitlink(candidate: Path, relative: str) -> None:
    # Git accepts the opaque target as a submodule object id. The contract must
    # reject mode 160000 before it has any chance to dereference this value.
    path = candidate / relative
    path.mkdir(parents=True, exist_ok=True)
    git_run(
        "update-index",
        "--add",
        "--cacheinfo",
        f"160000,{('1' * 40)},{relative}",
        cwd=candidate,
    )
    git_run(
        "-c",
        "user.name=stage5f-authority-test",
        "-c",
        "user.email=stage5f-authority-test@example.invalid",
        "commit",
        "--quiet",
        "-m",
        "hidden gitlink",
        cwd=candidate,
    )


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
    global ASSERTIONS_EXECUTED
    ASSERTIONS_EXECUTED = 0
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
                path = candidate / relative
                path.write_text(path.read_text() + "# base-authority-drift\n")
                commit_worktree(candidate, "base authority drift")
                assert_rejected(authority, base, candidate, sentinel, f"base-drift-{relative}")

            for relative in contract.AUTHORITY_FILES:
                candidate, sentinel = fresh_candidate(base, root)
                mutate_symlink(candidate, relative)
                assert_rejected(authority, base, candidate, sentinel, f"symlink-{relative}")

                candidate, sentinel = fresh_candidate(base, root)
                mutate_directory_shape(candidate, relative)
                assert_rejected(authority, base, candidate, sentinel, f"directory-shape-{relative}")

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(authority, base, candidate)
            assert_accepted(authority, base, candidate, sentinel, "reviewable-in-band-authority-rotation")

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(
                authority,
                base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
            )
            assert_accepted(
                authority,
                base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-rotation",
            )

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(
                authority,
                base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
                mutate_stage5d_freeze_rebind_paths=True,
            )
            assert_accepted(
                authority,
                base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-allows-exact-stage5d-freeze-rebind",
            )

            r9_base, sentinel = fresh_candidate(base, root)
            build_valid_rotation(
                authority,
                base,
                r9_base,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
            )
            assert_accepted(
                authority,
                base,
                r9_base,
                sentinel,
                "portable-forbidden-scanner-repair-first-r9",
            )
            candidate, sentinel = fresh_candidate(r9_base, root)
            build_valid_rotation(
                authority,
                r9_base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
            )
            assert_rejected(
                authority,
                r9_base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-replay-from-generation-three",
            )

            later_base, sentinel = fresh_candidate(base, root)
            build_valid_rotation(authority, base, later_base)
            assert_accepted(
                authority,
                base,
                later_base,
                sentinel,
                "portable-forbidden-scanner-repair-valid-later-generation-base",
            )
            candidate, sentinel = fresh_candidate(later_base, root)
            build_valid_rotation(
                authority,
                later_base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
            )
            assert_rejected(
                authority,
                later_base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-rejects-arbitrary-later-generation",
            )

            wrong_stage_base, sentinel = fresh_candidate(base, root)
            rewrite_authority_state(
                wrong_stage_base,
                stage="5F-a-unrelated-generation-two-authority",
            )
            candidate, sentinel = fresh_candidate(wrong_stage_base, root)
            build_valid_rotation(
                authority,
                wrong_stage_base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
            )
            assert_rejected(
                authority,
                wrong_stage_base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-rejects-wrong-generation-two-stage",
            )

            rollback_base, sentinel = fresh_candidate(base, root)
            build_valid_rotation(authority, base, rollback_base)
            rewrite_authority_state(
                rollback_base,
                stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_PREDECESSOR_STAGE,
            )
            candidate, sentinel = fresh_candidate(rollback_base, root)
            build_valid_rotation(
                authority,
                rollback_base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
            )
            assert_rejected(
                authority,
                rollback_base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-rejects-rolled-back-stage-spoof",
            )

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(
                authority,
                base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
            )
            rewrite_authority_state(candidate, authority_generation=3.0)
            refresh_rotation_bindings(base, candidate)
            assert_rejected(
                authority,
                base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-rejects-float-candidate-generation",
            )

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(
                authority,
                base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
            )
            rewrite_authority_state(candidate, schema_version=1.0)
            refresh_rotation_bindings(base, candidate)
            assert_rejected(
                authority,
                base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-rejects-float-candidate-schema",
            )

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(
                authority,
                base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
            )
            rewrite_authority_state(candidate, schema_version=True)
            refresh_rotation_bindings(base, candidate)
            assert_rejected(
                authority,
                base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-rejects-boolean-candidate-schema",
            )

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(
                authority,
                base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
            )
            rewrite_rotation_manifest(candidate, previous_generation=2.0)
            assert_rejected(
                authority,
                base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-rejects-float-manifest-predecessor",
            )

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(
                authority,
                base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
            )
            rewrite_rotation_manifest(candidate, next_generation=3.0)
            assert_rejected(
                authority,
                base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-rejects-float-manifest-successor",
            )

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(
                authority,
                base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
            )
            rewrite_rotation_manifest(candidate, schema_version=True)
            assert_rejected(
                authority,
                base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-rejects-boolean-manifest-schema",
            )

            float_base, sentinel = fresh_candidate(base, root)
            build_valid_rotation(
                authority,
                base,
                float_base,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
            )
            assert_accepted(
                authority,
                base,
                float_base,
                sentinel,
                "portable-forbidden-scanner-repair-valid-base-before-float-state",
            )
            future, sentinel = fresh_candidate(float_base, root)
            build_valid_rotation(authority, float_base, future)
            rewrite_authority_state(float_base, authority_generation=3.0)
            float_state_sha256 = hashlib.sha256(
                (float_base / contract.AUTHORITY_STATE).read_bytes()
            ).hexdigest()
            rewrite_authority_state(
                future,
                previous_state_sha256=float_state_sha256,
            )
            rewrite_rotation_manifest(
                future,
                previous_state_sha256=float_state_sha256,
            )
            refresh_rotation_bindings(float_base, future)
            assert_rejected(
                authority,
                float_base,
                future,
                sentinel,
                "float-authority-state-cannot-seed-future-rotation",
            )

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(
                authority,
                base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
            )
            scanner = candidate / contract.PORTABLE_FORBIDDEN_SCANNER_PATH
            scanner.chmod(scanner.stat().st_mode & ~0o111)
            refresh_rotation_bindings(base, candidate)
            assert_rejected(
                authority,
                base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-rejects-scanner-mode-drift",
            )

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(
                authority,
                base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
            )
            assert_rejected(
                authority,
                base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-requires-scanner-change",
            )

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(authority, base, candidate, mutate_scanner=True)
            assert_rejected(
                authority,
                base,
                candidate,
                sentinel,
                "generic-rotation-cannot-change-forbidden-scanner",
            )

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(
                authority,
                base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
                add_out_of_scope_readme_change=True,
            )
            assert_rejected(
                authority,
                base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-rejects-scope-creep",
            )

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(
                authority,
                base,
                candidate,
                next_stage=contract.PORTABLE_FORBIDDEN_SCANNER_REPAIR_STAGE,
                mutate_scanner=True,
                add_out_of_scope_stage5d_change=True,
            )
            assert_rejected(
                authority,
                base,
                candidate,
                sentinel,
                "portable-forbidden-scanner-repair-rejects-other-stage5d-freeze-path",
            )

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(authority, base, candidate)
            manifest_path = candidate / contract.ROTATION_MANIFEST
            manifest = json.loads(manifest_path.read_text())
            manifest["canonical_ci_gate_sha256"] = "0" * 64
            manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
            commit_worktree(candidate, "conflicting digest")
            assert_rejected(authority, base, candidate, sentinel, "rotation-conflicting-gate-authority")

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(authority, base, candidate)
            inventory_path = candidate / contract.INVENTORY
            inventory = json.loads(inventory_path.read_text())
            inventory["ci_snapshot_authority"]["stage5f_atomic_hybrid_semantics_gate_sha256"] = "0" * 64
            inventory_path.write_text(json.dumps(inventory, indent=2, sort_keys=False) + "\n")
            refresh_rotation_bindings(base, candidate)
            assert_rejected(authority, base, candidate, sentinel, "rotation-one-sided-inventory-gate-digest")

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(authority, base, candidate)
            (candidate / "crates/forbidden-stage5f-rotation.rs").write_text("// forbidden\n")
            refresh_rotation_bindings(base, candidate)
            assert_rejected(authority, base, candidate, sentinel, "rotation-out-of-scope-runtime-path")

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(authority, base, candidate)
            add_gitlink(candidate, "crates/stage5f-hidden-gitlink")
            assert_rejected(authority, base, candidate, sentinel, "rotation-hidden-crates-gitlink")

            candidate, sentinel = fresh_candidate(base, root)
            build_valid_rotation(authority, base, candidate)
            add_gitlink(candidate, "docs/stage-5/stage5f-hidden-gitlink")
            assert_rejected(authority, base, candidate, sentinel, "rotation-hidden-allowed-prefix-gitlink")

            candidate, sentinel = fresh_candidate(base, root)
            contract_path = candidate / "scripts/stage5f_base_authority_contract.py"
            contract_path.chmod(contract_path.stat().st_mode | 0o111)
            commit_worktree(candidate, "authority executable mode drift")
            assert_rejected(authority, base, candidate, sentinel, "authority-mode-100644-to-100755")

            candidate, sentinel = fresh_candidate(base, root)
            duplicate = candidate / ".github/workflows/duplicate-base-authority.yml"
            duplicate.write_text("name: Stage 5F Base Authority\n")
            commit_worktree(candidate, "duplicate workflow namespace")
            assert_rejected(authority, base, candidate, sentinel, "ordinary-duplicate-workflow-namespace")
    except (OSError, RuntimeError, subprocess.CalledProcessError, tarfile.TarError) as exc:
        print(f"stage5f-base-authority-negative: FAIL: {exc}", file=sys.stderr)
        return 1
    expected_cases = 69
    if ASSERTIONS_EXECUTED != expected_cases:
        print(
            "stage5f-base-authority-negative: FAIL: assertion inventory drift "
            f"executed={ASSERTIONS_EXECUTED} expected={expected_cases}",
            file=sys.stderr,
        )
        return 1
    print(f"stage5f-base-authority-negative: ok cases={ASSERTIONS_EXECUTED}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
