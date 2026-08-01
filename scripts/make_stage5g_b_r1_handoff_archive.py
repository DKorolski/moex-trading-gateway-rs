#!/usr/bin/env python3
"""Build a deterministic, self-attesting Stage 5G-b R1 review archive."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
import stage5g_b_r1_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
HANDOFF_DIR = ROOT / "reports/handoff"
CONTRACT_PATH = "docs/stage-5/stage5g-b-r1-contract.json"


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


def run_gate(temp: Path, label: str, command: list[str], source_ref: str) -> dict[str, Any]:
    evidence_dir = temp / safety.EVIDENCE_PREFIX / "gates"
    stdout_path = evidence_dir / f"{label}.stdout.txt"
    stderr_path = evidence_dir / f"{label}.stderr.txt"
    result_path = evidence_dir / f"{label}.result.json"
    completed = subprocess.run(
        command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False
    )
    write_bytes(stdout_path, completed.stdout)
    write_bytes(stderr_path, completed.stderr)
    result = {
        "schema_version": 1,
        "stage": safety.STAGE,
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
        raise SystemExit(f"Stage 5G-b R1 gate failed: {label}")
    print(f"GATE_OK {label} stdout_sha256={result['stdout_sha256']}")
    return result


def read_tracked_tree(source_ref: str) -> tuple[list[dict[str, str]], dict[str, bytes]]:
    raw = subprocess.check_output(["git", "ls-tree", "-r", "-z", source_ref], cwd=ROOT)
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
        body = subprocess.check_output(["git", "cat-file", "blob", object_id], cwd=ROOT)
        entries.append({"git_mode": mode, "path": relative, "sha256": sha256_bytes(body)})
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
    archive: Path, entries: list[dict[str, str]], payloads: dict[str, bytes]
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


def main() -> int:
    if run_text(["git", "status", "--porcelain", "--untracked-files=all"]):
        raise SystemExit("refusing Stage 5G-b R1 handoff: source tree is dirty")
    branch = run_text(["git", "branch", "--show-current"])
    if branch != safety.BRANCH:
        raise SystemExit(f"R1 handoff requires branch {safety.BRANCH}, got {branch}")
    if run_text(["git", "rev-parse", "--show-object-format"]) != "sha1":
        raise SystemExit("R1 handoff currently requires SHA-1 Git objects")
    source_ref = run_text(["git", "rev-parse", "HEAD"])
    source_commit = source_ref[:7]
    parent_ref = run_text(["git", "rev-parse", "HEAD^"])
    if parent_ref != safety.BASE_REF:
        raise SystemExit(f"R1 commit must directly follow immutable base {safety.BASE_REF}")
    if subprocess.run(
        ["git", "merge-base", "--is-ancestor", safety.STAGE5G_A_REF, source_ref], cwd=ROOT
    ).returncode != 0:
        raise SystemExit("accepted Stage 5G-a ref is not an ancestor of R1 source")
    head_tree = run_text(["git", "rev-parse", "HEAD^{tree}"])
    changed_paths = run_text(
        ["git", "diff", "--name-only", safety.BASE_REF, source_ref, "--"]
    ).splitlines()
    if changed_paths != safety.EXPECTED_CHANGED_PATHS:
        raise SystemExit(
            "Stage 5G-b R1 changed-path scope drift: "
            f"expected {safety.EXPECTED_CHANGED_PATHS!r}, got {changed_paths!r}"
        )

    HANDOFF_DIR.mkdir(parents=True, exist_ok=True)
    archive_name = f"moex-trading-project-{source_commit}.zip"
    archive = HANDOFF_DIR / archive_name
    sha_path = Path(str(archive) + ".sha256")
    archive.unlink(missing_ok=True)
    sha_path.unlink(missing_ok=True)

    with tempfile.TemporaryDirectory(prefix="stage5g-b-r1-handoff-") as raw_temp:
        temp = Path(raw_temp)
        results: dict[str, dict[str, Any]] = {}
        for label, command in safety.EXPECTED_COMMANDS.items():
            results[label] = run_gate(temp, label, command, source_ref)

        entries, payloads = read_tracked_tree(source_ref)
        write_json(
            temp / safety.SOURCE_MANIFEST,
            {
                "schema_version": 1,
                "stage": safety.STAGE,
                "source_ref": source_ref,
                "source_commit": source_commit,
                "source_branch": branch,
                "parent_ref": parent_ref,
                "immutable_base": safety.BASE_REF,
                "stage5g_a_accepted_ref": safety.STAGE5G_A_REF,
                "head_tree": head_tree,
                "members": entries,
            },
        )
        write_bytes(
            temp / safety.COMMIT_OBJECT,
            subprocess.check_output(["git", "cat-file", "commit", source_ref], cwd=ROOT),
        )
        write_bytes(
            temp / safety.COMMIT_MARKER,
            (
                f"stage={safety.STAGE}\n"
                f"source_ref={source_ref}\n"
                f"source_commit={source_commit}\n"
                f"source_branch={branch}\n"
                f"archive_name={archive_name}\n"
                f"immutable_base={safety.BASE_REF}\n"
                f"stage5g_a_accepted_ref={safety.STAGE5G_A_REF}\n"
            ).encode(),
        )

        repository_dir = temp / safety.EVIDENCE_PREFIX / "repository"
        status_path = repository_dir / "git-status.txt"
        changed_path = repository_dir / "changed-paths-since-r1-base.txt"
        write_bytes(
            status_path,
            subprocess.check_output(
                ["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT
            ),
        )
        write_bytes(changed_path, ("\n".join(changed_paths) + "\n").encode())

        gate_bindings: list[dict[str, str]] = []
        generated_members = [
            safety.COMMIT_MARKER,
            safety.COMMIT_OBJECT,
            safety.SOURCE_MANIFEST,
            safety.EVIDENCE_MANIFEST,
            str(status_path.relative_to(temp)),
            str(changed_path.relative_to(temp)),
        ]
        for label in safety.EXPECTED_COMMANDS:
            result = results[label]
            result_member = result["stdout_member"].replace(".stdout.txt", ".result.json")
            gate_bindings.append(
                {
                    "label": label,
                    "result_member": result_member,
                    "result_sha256": sha256_file(temp / result_member),
                }
            )
            generated_members.extend(
                [result["stdout_member"], result["stderr_member"], result_member]
            )

        contract = json.loads(
            (ROOT / CONTRACT_PATH).read_text(encoding="utf-8")
        )
        write_json(
            temp / safety.EVIDENCE_MANIFEST,
            {
                "schema_version": 1,
                "stage": safety.STAGE,
                "source_ref": source_ref,
                "source_branch": branch,
                "immutable_base": safety.BASE_REF,
                "stage5g_a_accepted_ref": safety.STAGE5G_A_REF,
                "gate_count": len(safety.EXPECTED_COMMANDS),
                "gates": gate_bindings,
                "repository_state": {
                    "git_status_member": str(status_path.relative_to(temp)),
                    "git_status_sha256": sha256_file(status_path),
                    "git_status_clean": True,
                    "changed_paths_base_ref": safety.BASE_REF,
                    "changed_paths_member": str(changed_path.relative_to(temp)),
                    "changed_paths_sha256": sha256_file(changed_path),
                },
                "closed_surfaces": contract["closed_surfaces"],
            },
        )

        create_source_archive(archive, entries, payloads)
        append_generated(archive, temp, generated_members)

        safety_result_path = temp / safety.SAFETY_RESULT
        preseal = subprocess.run(
            [
                "python3", "scripts/stage5g_b_r1_handoff_safety_check.py", str(archive),
                "--allow-missing-final-safety", "--result-out", str(safety_result_path),
            ],
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        write_bytes(temp / safety.SAFETY_STDOUT, preseal.stdout)
        write_bytes(temp / safety.SAFETY_STDERR, preseal.stderr)
        if preseal.returncode != 0:
            sys.stdout.buffer.write(preseal.stdout)
            sys.stderr.buffer.write(preseal.stderr)
            raise SystemExit("Stage 5G-b R1 preseal safety check failed")
        append_generated(
            archive, temp, [safety.SAFETY_RESULT, safety.SAFETY_STDOUT, safety.SAFETY_STDERR]
        )

    final = subprocess.run(
        ["python3", "scripts/stage5g_b_r1_handoff_safety_check.py", str(archive)],
        cwd=ROOT,
        check=False,
    )
    if final.returncode != 0:
        raise SystemExit("Stage 5G-b R1 final safety check failed")
    archive_sha = sha256_file(archive)
    sha_path.write_text(f"{archive_sha}  {archive.name}\n", encoding="utf-8")
    print(f"archive={archive}")
    print(f"sha256={archive_sha}")
    print(f"sha256_file={sha_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
