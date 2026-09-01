#!/usr/bin/env python3
"""Create immutable review package for the Generation-2 native runner."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p_r2b_generation2_full_transaction_native_r0_check as checker
import stage8b_p_r2b_generation2_full_transaction_native_r0_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-p-r2b-generation2-full-transaction-native-r0"
UPSTREAM_ROOT = ROOT / "tmp/stage8b-r2b-r4r2-production-a/release"
GENERATION2_ROOT = ROOT / "reports/stage8b-p-r2b-generation2-composition-r0/linux-amd64/build-a"
PROOF_TOOL_SOURCES = {
    "stage8b-r2a5-controlled-layout": ROOT / "tmp/stage8b-g2-r0-r1-controlled.tB20fg/x86_64-unknown-linux-musl/release/stage8b-r2a5-controlled-layout",
    "stage8b-r2b-creator-chain-seeder": ROOT / "tmp/stage8b-r2b-r4-controlled-a/release/stage8b-r2b-creator-chain-seeder",
}


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def binary_source(name: str) -> Path:
    if name in checker.UPSTREAM_NAMES:
        return UPSTREAM_ROOT / name
    source_name = "stage8b-readonly-preflight" if name == "accepted-stage8b-readonly-preflight" else name
    return GENERATION2_ROOT / source_name


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-generation2-native-r0-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-generation2-native-r0-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    gate = subprocess.run(
        ["bash", "scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if gate.returncode != 0 or b"stage8b-generation2-full-transaction-native-r0-gate: PASS" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    contract = json.loads((ROOT / checker.CONTRACT).read_text())
    binaries: dict[str, bytes] = {}
    for name, expected in contract["production_linux_amd64_sha256"].items():
        source = binary_source(name)
        data = source.read_bytes()
        if digest(data) != expected:
            raise SystemExit(f"stage8b-generation2-native-r0-handoff: FAIL binary drift {name}")
        binaries[f"{safety.BIN_ROOT}/{name}"] = data
    for name, expected in contract["proof_tool_linux_amd64_sha256"].items():
        data = PROOF_TOOL_SOURCES[name].read_bytes()
        if digest(data) != expected:
            raise SystemExit(f"stage8b-generation2-native-r0-handoff: FAIL proof tool drift {name}")
        binaries[f"{safety.TOOL_ROOT}/{name}"] = data

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = (
        json.dumps(
            {
                "schema_version": 1,
                "stage": "Stage 8B-P R2B Generation-2 native runner review package",
                "source_ref": source_ref,
                "source_tree": source_tree,
                "source_short_ref": short_ref,
                "archive_name": archive_name,
                "branch": branch,
                "accepted_predecessor_ref": checker.ACCEPTED_COMPOSITION_REF,
                "accepted_predecessor_archive_sha256": checker.ACCEPTED_COMPOSITION_ARCHIVE,
                "manifest_sha256": digest(manifest),
                "gate_sha256": digest(gate.stdout),
                "production_binary_count": 12,
                "proof_tool_binary_count": 2,
                "phase_count": 6,
                "service_invocation_count": 31,
                "contract_negative_cases": 43,
                "host_negative_cases": 11,
                "eligible_disposable_host_identified": True,
                "container_created": False,
                "native_execution": False,
                "generation_2_active": False,
                "authorization": "NOT_ISSUED",
                "external_finam_network": False,
                "broker_dispatch": False,
                "real_orders": False,
                "next_step": "INDEPENDENT_REVIEW_THEN_NATIVE_EXECUTION",
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
        safety.EVIDENCE: evidence,
        safety.GATE: gate.stdout,
        safety.MANIFEST: manifest,
        **binaries,
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(
                common.zip_info(entry["path"], entry["mode"]),
                run("git", "show", f"{source_ref}:{entry['path']}"),
            )
        for name, data in sorted(additions.items()):
            file_mode = "100755" if name in binaries else "100644"
            archive.writestr(common.zip_info(name, file_mode), data)

    result = safety.check(str(archive_path))
    with tempfile.TemporaryDirectory(prefix="stage8b-g2-native-r0-handoff-") as temporary:
        extracted = Path(temporary)
        with zipfile.ZipFile(archive_path) as archive:
            archive.extractall(extracted)
        os.chmod(extracted / "scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_runner.sh", 0o755)
        artifact_root = extracted / safety.BIN_ROOT
        completed = subprocess.run(
            [
                sys.executable,
                "scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_check.py",
                "--root",
                str(extracted),
                "--artifact-root",
                str(artifact_root),
            ],
            cwd=extracted,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        if completed.returncode != 0:
            raise SystemExit(completed.stdout.decode(errors="replace"))

    archive_digest = digest(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{archive_digest}  {archive_name}\n")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(
        f"archive={archive_path}\nsha256={archive_digest}\nsource_ref={source_ref}\n"
        "stage8b-generation2-native-r0-handoff: PASS"
    )


if __name__ == "__main__":
    main()
