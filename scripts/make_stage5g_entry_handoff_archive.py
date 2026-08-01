#!/usr/bin/env python3
"""Build a complete commit-bound Stage 5G-a design review handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
STAGE = "5G-a-lifecycle-entry"
BRANCH = "stage5g-lifecycle"
ACCEPTED_STAGE5F = "fb8245e2f91cfc1678548a1228e8558d9adc2181"
CLOSURE_COMMIT = "cac83da38725aeadd6d029a3078157c2ab7fa004"
HANDOFF_DIR = ROOT / "reports/handoff"
SOURCE_MANIFEST = "stage5g-a-source-tree-manifest.json"
EVIDENCE_MANIFEST = "stage5g-a-evidence-manifest.json"
COMMIT_OBJECT = "stage5g-a-commit-object.txt"
COMMIT_MARKER = "handoff-commit.txt"
SAFETY_RESULT = "stage5g-a-archive-safety-result.json"
SAFETY_STDOUT = "stage5g-a-archive-safety.stdout.txt"
SAFETY_STDERR = "stage5g-a-archive-safety.stderr.txt"

GATES: list[tuple[str, list[str]]] = [
    ("entry-checker", ["python3", "scripts/stage5g_entry_plan_check.py"]),
    (
        "entry-negative",
        ["python3", "scripts/stage5g_entry_plan_negative_harness.py"],
    ),
    ("fmt", ["cargo", "fmt", "--all", "--", "--check"]),
    ("workspace-tests", ["cargo", "test", "--workspace", "--all-targets"]),
    ("doctests", ["cargo", "test", "--workspace", "--doc"]),
    (
        "clippy",
        [
            "cargo",
            "clippy",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
    ),
    ("forbidden-no-rg", ["bash", "scripts/stage5f_forbidden_no_rg_gate.sh"]),
]


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def run_text(command: list[str]) -> str:
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def write_bytes(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(payload)


def run_gate(
    temp: Path,
    label: str,
    command: list[str],
    source_ref: str,
) -> dict[str, Any]:
    evidence_dir = temp / "stage5g-a-evidence/gates"
    evidence_dir.mkdir(parents=True, exist_ok=True)
    stdout_path = evidence_dir / f"{label}.stdout.txt"
    stderr_path = evidence_dir / f"{label}.stderr.txt"
    result_path = evidence_dir / f"{label}.result.json"
    completed = subprocess.run(
        command,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    write_bytes(stdout_path, completed.stdout)
    write_bytes(stderr_path, completed.stderr)
    result = {
        "schema_version": 1,
        "stage": STAGE,
        "label": label,
        "command": command,
        "source_ref": source_ref,
        "exit_code": completed.returncode,
        "stdout_member": str(stdout_path.relative_to(temp)),
        "stdout_sha256": sha256_file(stdout_path),
        "stderr_member": str(stderr_path.relative_to(temp)),
        "stderr_sha256": sha256_file(stderr_path),
    }
    write_json(result_path, result)
    if completed.returncode != 0:
        sys.stdout.buffer.write(completed.stdout)
        sys.stderr.buffer.write(completed.stderr)
        raise SystemExit(f"Stage 5G-a gate failed: {label}")
    print(f"GATE_OK {label} stdout_sha256={result['stdout_sha256']}")
    return result


def read_tracked_tree(source_ref: str) -> tuple[list[dict[str, str]], dict[str, bytes]]:
    raw = subprocess.check_output(
        ["git", "ls-tree", "-r", "-z", source_ref], cwd=ROOT
    )
    entries: list[dict[str, str]] = []
    payloads: dict[str, bytes] = {}
    for record in raw.split(b"\0"):
        if not record:
            continue
        metadata, path_raw = record.split(b"\t", 1)
        mode, kind, object_id = metadata.decode("ascii").split()
        relative = path_raw.decode("utf-8")
        if kind != "blob" or mode not in {"100644", "100755"}:
            raise SystemExit(f"unsupported tracked entry: {mode} {kind} {relative}")
        body = subprocess.check_output(
            ["git", "cat-file", "blob", object_id], cwd=ROOT
        )
        entries.append(
            {
                "git_mode": mode,
                "path": relative,
                "sha256": sha256_bytes(body),
            }
        )
        payloads[relative] = body
    entries.sort(key=lambda item: item["path"])
    return entries, payloads


def zip_info(name: str, mode: int = 0o644) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.create_system = 3
    info.external_attr = (0o100000 | mode) << 16
    info.compress_type = zipfile.ZIP_DEFLATED
    return info


def create_source_archive(
    archive: Path,
    entries: list[dict[str, str]],
    payloads: dict[str, bytes],
) -> None:
    archive.unlink(missing_ok=True)
    with zipfile.ZipFile(archive, "w") as handle:
        for entry in entries:
            mode = 0o755 if entry["git_mode"] == "100755" else 0o644
            handle.writestr(zip_info(entry["path"], mode), payloads[entry["path"]])


def append_generated(archive: Path, temp: Path, members: list[str]) -> None:
    with zipfile.ZipFile(archive, "a") as handle:
        existing = set(handle.namelist())
        for member in members:
            if member in existing:
                raise SystemExit(f"refusing duplicate archive member: {member}")
            handle.writestr(zip_info(member), (temp / member).read_bytes())
            existing.add(member)


def validate_gate_markers(results: dict[str, dict[str, Any]], temp: Path) -> None:
    checker_output = (temp / results["entry-checker"]["stdout_member"]).read_text(
        errors="replace"
    )
    if "stage5g-entry-plan-check: ok cases=54 design_only=true" not in checker_output:
        raise SystemExit("Stage 5G-a checker marker missing")
    negative_output = (temp / results["entry-negative"]["stdout_member"]).read_text(
        errors="replace"
    )
    pass_count = sum(line.startswith("PASS ") for line in negative_output.splitlines())
    if pass_count != 30:
        raise SystemExit(
            f"Stage 5G-a negative PASS count mismatch: expected 30, got {pass_count}"
        )
    if "stage5g-entry-plan-negative-harness: ok cases=30" not in negative_output:
        raise SystemExit("Stage 5G-a negative completion marker missing")


def main() -> int:
    if run_text(["git", "status", "--porcelain", "--untracked-files=all"]):
        raise SystemExit("refusing Stage 5G-a handoff: source tree is dirty")
    branch = run_text(["git", "branch", "--show-current"])
    if branch != BRANCH:
        raise SystemExit(f"Stage 5G-a handoff requires branch {BRANCH}, got {branch}")
    if run_text(["git", "rev-parse", "--show-object-format"]) != "sha1":
        raise SystemExit("Stage 5G-a handoff currently requires SHA-1 Git objects")
    source_ref = run_text(["git", "rev-parse", "HEAD"])
    source_commit = source_ref[:7]
    parent_ref = run_text(["git", "rev-parse", "HEAD^"])
    head_tree = run_text(["git", "rev-parse", "HEAD^{tree}"])
    subprocess.run(
        ["git", "merge-base", "--is-ancestor", CLOSURE_COMMIT, source_ref],
        cwd=ROOT,
        check=True,
    )

    HANDOFF_DIR.mkdir(parents=True, exist_ok=True)
    archive_name = f"moex-trading-project-{source_commit}.zip"
    archive = HANDOFF_DIR / archive_name
    sha_path = Path(str(archive) + ".sha256")
    archive.unlink(missing_ok=True)
    sha_path.unlink(missing_ok=True)

    with tempfile.TemporaryDirectory(prefix="stage5g-a-handoff-") as raw_temp:
        temp = Path(raw_temp)
        results: dict[str, dict[str, Any]] = {}
        for label, command in GATES:
            results[label] = run_gate(temp, label, command, source_ref)
        validate_gate_markers(results, temp)

        entries, payloads = read_tracked_tree(source_ref)
        write_json(
            temp / SOURCE_MANIFEST,
            {
                "schema_version": 1,
                "stage": STAGE,
                "source_ref": source_ref,
                "source_commit": source_commit,
                "source_branch": branch,
                "parent_ref": parent_ref,
                "head_tree": head_tree,
                "members": entries,
            },
        )
        write_bytes(
            temp / COMMIT_OBJECT,
            subprocess.check_output(["git", "cat-file", "commit", source_ref], cwd=ROOT),
        )
        write_bytes(
            temp / COMMIT_MARKER,
            (
                f"stage={STAGE}\n"
                f"source_ref={source_ref}\n"
                f"source_commit={source_commit}\n"
                f"source_branch={branch}\n"
                f"archive_name={archive_name}\n"
                f"stage5f_predecessor={ACCEPTED_STAGE5F}\n"
                f"stage5f_closure_commit={CLOSURE_COMMIT}\n"
            ).encode(),
        )

        repository_dir = temp / "stage5g-a-evidence/repository"
        git_status_path = repository_dir / "git-status.txt"
        changed_paths_path = repository_dir / "changed-paths-since-closure.txt"
        write_bytes(
            git_status_path,
            subprocess.check_output(
                ["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT
            ),
        )
        changed_paths = subprocess.check_output(
            ["git", "diff", "--name-only", CLOSURE_COMMIT, source_ref, "--"],
            cwd=ROOT,
        )
        write_bytes(changed_paths_path, changed_paths)

        inventory = json.loads(
            (ROOT / "docs/stage-5/stage5g-lifecycle-entry-inventory.json").read_text(
                encoding="utf-8"
            )
        )
        gate_bindings = []
        for label, _ in GATES:
            result_member = results[label]["stdout_member"].replace(
                ".stdout.txt", ".result.json"
            )
            gate_bindings.append(
                {
                    "label": label,
                    "result_member": result_member,
                    "result_sha256": sha256_file(temp / result_member),
                }
            )
        write_json(
            temp / EVIDENCE_MANIFEST,
            {
                "schema_version": 1,
                "stage": STAGE,
                "source_ref": source_ref,
                "source_branch": branch,
                "stage5f_predecessor": ACCEPTED_STAGE5F,
                "stage5f_closure_commit": CLOSURE_COMMIT,
                "gate_count": len(GATES),
                "gates": gate_bindings,
                "repository_state": {
                    "git_status_member": str(git_status_path.relative_to(temp)),
                    "git_status_sha256": sha256_file(git_status_path),
                    "git_status_clean": True,
                    "changed_paths_base_ref": CLOSURE_COMMIT,
                    "changed_paths_member": str(changed_paths_path.relative_to(temp)),
                    "changed_paths_sha256": sha256_file(changed_paths_path),
                },
                "closed_surfaces": inventory["closed_surfaces"],
            },
        )

        create_source_archive(archive, entries, payloads)
        generated_members = [
            COMMIT_MARKER,
            COMMIT_OBJECT,
            SOURCE_MANIFEST,
            EVIDENCE_MANIFEST,
            str(git_status_path.relative_to(temp)),
            str(changed_paths_path.relative_to(temp)),
        ]
        for label, _ in GATES:
            result = results[label]
            generated_members.extend(
                [
                    result["stdout_member"],
                    result["stderr_member"],
                    result["stdout_member"].replace(".stdout.txt", ".result.json"),
                ]
            )
        append_generated(archive, temp, generated_members)

        safety_result_path = temp / SAFETY_RESULT
        preseal = subprocess.run(
            [
                "python3",
                "scripts/stage5g_entry_handoff_safety_check.py",
                str(archive),
                "--allow-missing-final-safety",
                "--result-out",
                str(safety_result_path),
            ],
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        write_bytes(temp / SAFETY_STDOUT, preseal.stdout)
        write_bytes(temp / SAFETY_STDERR, preseal.stderr)
        if preseal.returncode != 0:
            sys.stdout.buffer.write(preseal.stdout)
            sys.stderr.buffer.write(preseal.stderr)
            raise SystemExit("Stage 5G-a preseal safety check failed")
        append_generated(
            archive,
            temp,
            [SAFETY_RESULT, SAFETY_STDOUT, SAFETY_STDERR],
        )

    final_safety = subprocess.run(
        ["python3", "scripts/stage5g_entry_handoff_safety_check.py", str(archive)],
        cwd=ROOT,
        check=False,
    )
    if final_safety.returncode != 0:
        raise SystemExit("Stage 5G-a final safety check failed")
    archive_sha = sha256_file(archive)
    sha_path.write_text(f"{archive_sha}  {archive.name}\n", encoding="utf-8")
    print(f"archive={archive}")
    print(f"sha256={archive_sha}")
    print(f"sha256_file={sha_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
