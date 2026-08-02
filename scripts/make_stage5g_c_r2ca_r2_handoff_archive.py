#!/usr/bin/env python3
"""Build the push-bound self-attesting R2 terminal-fill review archive."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import make_stage5g_c_r2ca_r1_handoff_archive as common
import stage5g_c_r2ca_r2_handoff_safety_check as safety

ROOT = Path(__file__).resolve().parents[1]
HANDOFF_DIR = ROOT / "reports/handoff"
CONTRACT_PATH = "docs/stage-5/stage5g-c-r2ca-r2-deterministic-terminal-fill-boundary.json"


def main() -> int:
    safety.configure_validation_callback()
    common.safety = safety
    if common.run_text(["git", "status", "--porcelain", "--untracked-files=all"]):
        raise SystemExit("refusing R2 handoff: source tree is dirty")
    branch = common.run_text(["git", "branch", "--show-current"])
    if branch != safety.BRANCH:
        raise SystemExit(f"R2 handoff requires {safety.BRANCH}, got {branch}")
    if common.run_text(["git", "rev-parse", "--show-object-format"]) != "sha1":
        raise SystemExit("R2 handoff requires SHA-1 Git objects")
    source_ref = common.run_text(["git", "rev-parse", "HEAD"])
    parent_ref = common.run_text(["git", "rev-parse", "HEAD^"])
    origin_ref = common.run_text(["git", "rev-parse", f"origin/{branch}"])
    if parent_ref != safety.BASE_REF:
        raise SystemExit(f"R2 must directly follow {safety.BASE_REF}")
    if origin_ref != source_ref:
        raise SystemExit("origin/stage5g-lifecycle must equal HEAD before handoff")
    source_commit = source_ref[:7]
    head_tree = common.run_text(["git", "rev-parse", "HEAD^{tree}"])
    changed_paths = common.run_text(
        ["git", "diff", "--name-only", safety.BASE_REF, source_ref, "--"]
    ).splitlines()
    if changed_paths != safety.EXPECTED_CHANGED_PATHS:
        raise SystemExit(
            f"R2 changed-path scope drift: expected {safety.EXPECTED_CHANGED_PATHS!r}, "
            f"got {changed_paths!r}"
        )

    HANDOFF_DIR.mkdir(parents=True, exist_ok=True)
    archive_name = f"moex-trading-project-{source_commit}.zip"
    archive = HANDOFF_DIR / archive_name
    sha_path = Path(str(archive) + ".sha256")
    archive.unlink(missing_ok=True)
    sha_path.unlink(missing_ok=True)

    with tempfile.TemporaryDirectory(prefix="stage5g-r2ca-r2-handoff-") as raw_temp:
        temp = Path(raw_temp)
        results = {
            label: common.run_gate(temp, label, command, source_ref)
            for label, command in safety.EXPECTED_COMMANDS.items()
        }
        entries, payloads = common.read_tracked_tree(source_ref)
        common.write_json(
            temp / safety.SOURCE_MANIFEST,
            {
                "schema_version": 1,
                "stage": safety.STAGE,
                "source_ref": source_ref,
                "source_commit": source_commit,
                "source_branch": branch,
                "parent_ref": parent_ref,
                "origin_ref": origin_ref,
                "head_tree": head_tree,
                "members": entries,
            },
        )
        common.write_bytes(
            temp / safety.COMMIT_OBJECT,
            subprocess.check_output(["git", "cat-file", "commit", source_ref], cwd=ROOT),
        )
        common.write_bytes(
            temp / safety.COMMIT_MARKER,
            (
                f"stage={safety.STAGE}\n"
                f"source_ref={source_ref}\n"
                f"source_commit={source_commit}\n"
                f"source_branch={branch}\n"
                f"archive_name={archive_name}\n"
                f"parent_ref={parent_ref}\n"
                f"origin_ref={origin_ref}\n"
            ).encode(),
        )

        repository_dir = temp / safety.EVIDENCE_PREFIX / "repository"
        status_path = repository_dir / "git-status.txt"
        changed_path = repository_dir / "changed-paths-since-predecessor.txt"
        common.write_bytes(
            status_path,
            subprocess.check_output(
                ["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT
            ),
        )
        common.write_bytes(changed_path, ("\n".join(changed_paths) + "\n").encode())

        generated_members = [
            safety.COMMIT_MARKER,
            safety.COMMIT_OBJECT,
            safety.SOURCE_MANIFEST,
            safety.EVIDENCE_MANIFEST,
            str(status_path.relative_to(temp)),
            str(changed_path.relative_to(temp)),
        ]
        gate_bindings: list[dict[str, str]] = []
        for label in safety.EXPECTED_COMMANDS:
            result = results[label]
            result_member = result["stdout_member"].replace(".stdout.txt", ".result.json")
            gate_bindings.append(
                {
                    "label": label,
                    "result_member": result_member,
                    "result_sha256": common.sha256_file(temp / result_member),
                }
            )
            generated_members.extend(
                [result["stdout_member"], result["stderr_member"], result_member]
            )

        contract = json.loads((ROOT / CONTRACT_PATH).read_text())
        common.write_json(
            temp / safety.EVIDENCE_MANIFEST,
            {
                "schema_version": 1,
                "stage": safety.STAGE,
                "source_ref": source_ref,
                "source_branch": branch,
                "parent_ref": parent_ref,
                "origin_ref": origin_ref,
                "gate_count": len(safety.EXPECTED_COMMANDS),
                "gates": gate_bindings,
                "repository_state": {
                    "git_status_member": str(status_path.relative_to(temp)),
                    "git_status_sha256": common.sha256_file(status_path),
                    "git_status_clean": True,
                    "changed_paths_base_ref": safety.BASE_REF,
                    "changed_paths_member": str(changed_path.relative_to(temp)),
                    "changed_paths_sha256": common.sha256_file(changed_path),
                },
                "closed_surfaces": contract["closed_surfaces"],
            },
        )

        common.create_source_archive(archive, entries, payloads)
        common.append_generated(archive, temp, generated_members)

        safety_result_path = temp / safety.SAFETY_RESULT
        preseal = subprocess.run(
            [
                "python3",
                "scripts/stage5g_c_r2ca_r2_handoff_safety_check.py",
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
        common.write_bytes(temp / safety.SAFETY_STDOUT, preseal.stdout)
        common.write_bytes(temp / safety.SAFETY_STDERR, preseal.stderr)
        if preseal.returncode != 0:
            sys.stdout.buffer.write(preseal.stdout)
            sys.stderr.buffer.write(preseal.stderr)
            raise SystemExit("R2 preseal safety check failed")
        common.append_generated(
            archive,
            temp,
            [safety.SAFETY_RESULT, safety.SAFETY_STDOUT, safety.SAFETY_STDERR],
        )

    final = subprocess.run(
        ["python3", "scripts/stage5g_c_r2ca_r2_handoff_safety_check.py", str(archive)],
        cwd=ROOT,
        check=False,
    )
    if final.returncode != 0:
        raise SystemExit("R2 final safety check failed")
    archive_sha = common.sha256_file(archive)
    sha_path.write_text(f"{archive_sha}  {archive.name}\n")
    print(f"archive={archive}")
    print(f"sha256={archive_sha}")
    print(f"sha256_file={sha_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
