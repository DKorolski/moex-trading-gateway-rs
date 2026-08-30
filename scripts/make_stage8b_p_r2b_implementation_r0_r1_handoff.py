#!/usr/bin/env python3
"""Create immutable Stage 8B-P R2B Implementation R0-R1 handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p_r2b_implementation_r0_r1_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-p-r2b-implementation-r0-r1"
PREDECESSOR = "da83f5922d9e2a9a5a1db3e581d2d9f55d810d81"
ARTIFACT_ROOT = ROOT / "reports/stage8b-p-r2b-r0-r1/linux-amd64"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p-r2b-implementation-r0-r1-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p-r2b-implementation-r0-r1-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if run("git", "merge-base", source_ref, PREDECESSOR).decode().strip() != PREDECESSOR:
        raise SystemExit("stage8b-p-r2b-implementation-r0-r1-handoff: FAIL predecessor lineage drift")

    build = json.loads(
        (ROOT / "docs/stage-8/stage8b-p-r2b-implementation-r0-r1-linux-build-evidence.json").read_text()
    )
    binary_additions: dict[str, bytes] = {}
    for binary, record in build["binaries"].items():
        for build_name in ("build-a", "build-b"):
            source = ARTIFACT_ROOT / build_name / binary
            data = source.read_bytes()
            expected = record[f"{build_name.replace('-', '_')}_sha256"]
            if sha(data) != expected:
                raise SystemExit(f"stage8b-p-r2b-implementation-r0-r1-handoff: FAIL ELF drift {source}")
            binary_additions[f"handoff-evidence/linux-amd64/{build_name}/{binary}"] = data

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p_r2b_implementation_r0_r1_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    marker = (
        b"stage8b-p-r2b-implementation-r0-r1-gate: PASS predecessor="
        + PREDECESSOR.encode()
    )
    if gate.returncode != 0 or marker not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    generated_evidence = (
        json.dumps(
            {
                "schema_version": 1,
                "stage": "Stage 8B-P R2B Implementation Package R0-R1",
                "source_ref": source_ref,
                "source_tree": source_tree,
                "source_short_ref": short_ref,
                "archive_name": archive_name,
                "branch": branch,
                "accepted_predecessor": PREDECESSOR,
                "gate_sha256": sha(gate.stdout),
                "manifest_sha256": sha(manifest),
                "phase_count": 6,
                "service_invocations": 31,
                "dynamic_failure_cases": 5,
                "negative_mutations": 20,
                "linux_elf_members": 4,
                "linux_build_reproducible": True,
                "installed": False,
                "enabled": False,
                "started": False,
                "operator_selected": False,
                "real_credentials_materialized": False,
                "authorization": "NOT_ISSUED",
                "finam_open": False,
                "runtime_live": False,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    ).encode()
    additions = {
        "handoff-commit.txt": (
            f"source_short_ref={short_ref}\nsource_ref={source_ref}\n"
            f"source_tree={source_tree}\narchive_name={archive_name}\n"
        ).encode(),
        safety.EVIDENCE: generated_evidence,
        safety.GATE: gate.stdout,
        safety.MANIFEST: manifest,
        **binary_additions,
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(
        archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for entry in entries:
            archive.writestr(
                common.zip_info(entry["path"], entry["mode"]),
                run("git", "show", f"{source_ref}:{entry['path']}"),
            )
        for name, data in sorted(additions.items()):
            generated_mode = "100755" if name in safety.BINARIES else "100644"
            archive.writestr(common.zip_info(name, generated_mode), data)

    result = safety.check(str(archive_path))
    digest = sha(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(
        f"{digest}  {archive_name}\n", encoding="utf-8"
    )
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\n"
        "stage8b-p-r2b-implementation-r0-r1-handoff: PASS"
    )


if __name__ == "__main__":
    main()
