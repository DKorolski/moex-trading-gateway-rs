#!/usr/bin/env python3
"""Build a commit-bound Stage 8A-4 I3 review archive."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import tempfile
import zipfile
from pathlib import Path

import stage8a4_durable_composition_i3_handoff_safety_check as safety

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports" / "handoff"


def run(*args: str, cwd: Path = ROOT) -> bytes:
    return subprocess.check_output(args, cwd=cwd)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def zip_info(name: str) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = 0o100644 << 16
    return info


def main() -> None:
    status = run("git", "status", "--porcelain", "--untracked-files=all").decode().strip()
    if status:
        raise SystemExit("stage8a4-durable-composition-i3-handoff: FAIL worktree must be clean")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != "stage8a4-durable-composition-i3-r5":
        raise SystemExit(f"stage8a4-durable-composition-i3-handoff: FAIL branch={branch}")
    full_ref = run("git", "rev-parse", "HEAD").decode().strip()
    short_ref = run("git", "rev-parse", "--short=7", "HEAD").decode().strip()
    archive_name = f"moex-trading-project-{short_ref}.zip"
    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path = OUTPUT / archive_name

    with tempfile.TemporaryDirectory(prefix="stage8a4-i3-handoff-") as raw:
        temporary = Path(raw)
        gate_dir = temporary / "gate"
        environment = os.environ.copy()
        environment["STAGE8A4_I3_ARTIFACT_DIR"] = str(gate_dir)
        gate = subprocess.run(
            ["bash", "scripts/stage8a4_durable_composition_i3_gate.sh"],
            cwd=ROOT,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        if gate.returncode != 0:
            raise SystemExit(gate.stdout.decode(errors="replace"))
        marker = b"stage8a4-durable-composition-i3-gate: PASS rows=77 negatives=88 sealed=private pending=true fresh_process=true rearm=false broker_neutral=true recovery=true ack=false execution=false"
        if marker not in gate.stdout:
            raise SystemExit("stage8a4-durable-composition-i3-handoff: FAIL gate marker missing")

        tracked = run("git", "ls-tree", "-r", "--name-only", "HEAD").decode().splitlines()
        entries = []
        for name in tracked:
            data = run("git", "show", f"HEAD:{name}")
            entries.append({"path": name, "sha256": sha256(data), "size": len(data)})
        manifest = (json.dumps({
            "schema_version": 1,
            "source_ref": full_ref,
            "entry_count": len(entries),
            "entries": entries,
        }, indent=2, sort_keys=True) + "\n").encode()
        gate_files = {
            f"handoff-evidence/gate-artifacts/{path.name}": path.read_bytes()
            for path in sorted(gate_dir.iterdir()) if path.is_file()
        }
        evidence = (json.dumps({
            "schema_version": 1,
            "stage": "8A-4-durable-composition-I3-R5",
            "source_ref": full_ref,
            "source_short_ref": short_ref,
            "archive_name": archive_name,
            "branch": branch,
            "accepted_i2_r3_ref": "90f46052cc31cea012437eddb59fb7c3ca5c2320",
            "accepted_i2_r3_review_sha256": "196c2b69161081f9034eb9399f41245f11ccd7eca229fadc3f8ec842cd1231f0",
            "rejected_i3_r1_ref": "a490bbe700c51f0e9c6debd2a007cb9b5061c3d8",
            "rejected_i3_r1_review_sha256": "c0ecc723ab98ba67560cb857e2761d0913f47c8ff78355bc04e74c8e03b585fe",
            "rejected_i3_r2_ref": "62e5e0509adb9cceb1d9947b5b3f92120e2f19ea",
            "rejected_i3_r2_review_sha256": "606ce34c3369fe732dfced14c283fe2bf1020e5c64db638109daa6b26f55d1cc",
            "rejected_i3_r3_ref": "3aa267029d512ba21f91dd95eb118b8d51810b56",
            "rejected_i3_r3_review_sha256": "aeae8245d421510301672a3885eb2396efdee0071c1dbd1af8313a9aa3d29cb3",
            "i3_r4_correction_spec_sha256": "5f0bfb0fd65ce5723b883638735c610220c51d279b8b7e7085fad9e544ed79a5",
            "rejected_i3_r4_ref": "44030688053c41a2179bb0f7bc59458c408348fd",
            "rejected_i3_r4_review_sha256": "cd171953a5c72ea49a63e2249124c76b9e0711bbe27bde66961d6ecd13337762",
            "i3_r5_correction_spec_sha256": "2ccd2c663bb1e577898771d8c13720f92f8b80d84e01cbf32ba311b9a276553a",
            "acceptance_rows": 77,
            "negative_cases": 88,
            "source_tree_manifest_sha256": sha256(manifest),
            "full_gate_sha256": sha256(gate.stdout),
            "v2_durable_append_enabled": True,
            "four_field_cas_enabled": True,
            "covering_seal_writer_enabled": True,
            "sealed_linear_writer_authority": True,
            "exact_request_truth_control_binding": True,
            "post_write_sticky_fail_stop": True,
            "stage8a1_r3_authority_restored": True,
            "broker_neutral_runtime_dependency": True,
            "broker_core_sqlite_baseline_unchanged": True,
            "production_normal_composition_path": True,
            "production_restart_without_i2_candidate": True,
            "writer_entry_ed25519_attested": True,
            "writer_issuer_public_key_pinned_by_operational_identity": True,
            "production_normal_and_three_recovery_paths_directly_tested": True,
            "complete_uncovered_restart_remains_pending": True,
            "external_raw_mutator_compile_fail": True,
            "fresh_process_sigkill_recovery_directly_tested": True,
            "arm_registration_ed25519_attested": True,
            "arm_registration_issuer_key_pinned": True,
            "arm_registration_exact_binding_verified": True,
            "recovery_recreates_operator_arm": False,
            "recovery_mints_stage8_execution_capability": False,
            "recovery_requires_stage8_execution_capability": False,
            "recovery_requires_precrash_issuer_object": False,
            "recovery_reads_existing_arm_registration": True,
            "missing_or_mismatched_arm_registration_fails_closed": True,
            "ack_readiness_enabled": False,
            "redis_live_enabled": False,
            "finam_post_delete_enabled": False,
            "runtime_live_enabled": False,
            "real_orders_enabled": False,
        }, indent=2, sort_keys=True) + "\n").encode()
        source_zip = temporary / "source.zip"
        subprocess.run(["git", "archive", "--format=zip", f"--output={source_zip}", "HEAD"], cwd=ROOT, check=True)
        additions = {
            "handoff-commit.txt": f"source_short_ref={short_ref}\nsource_ref={full_ref}\narchive_name={archive_name}\n".encode(),
            "handoff-evidence/stage8a4-durable-composition-i3-full-gate.txt": gate.stdout,
            "handoff-evidence/stage8a4-durable-composition-i3-evidence.json": evidence,
            "handoff-evidence/source-tree-manifest.json": manifest,
            **gate_files,
        }
        with zipfile.ZipFile(source_zip) as source, zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as target:
            for info in source.infolist():
                target.writestr(info, source.read(info.filename))
            for name, data in sorted(additions.items()):
                target.writestr(zip_info(name), data)

    result = safety.check(str(archive_path))
    archive_hash = sha256(archive_path.read_bytes())
    archive_path.with_suffix(archive_path.suffix + ".sha256").write_text(
        f"{archive_hash}  {archive_name}\n", encoding="utf-8"
    )
    archive_path.with_suffix(archive_path.suffix + ".safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"archive={archive_path}")
    print(f"sha256={archive_hash}")
    print(f"source_ref={full_ref}")
    print("stage8a4-durable-composition-i3-handoff: PASS")


if __name__ == "__main__":
    main()
