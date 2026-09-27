#!/usr/bin/env python3
"""Build the non-activating O1 bundle and immutable full-project review handoff."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import struct
import subprocess
import tarfile
import tempfile
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1f_o1_check as source_check
import stage8b_p1f_o1_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
DOWNLOADS = Path("/Users/denisq/Downloads")
CARGO_HOME = ROOT / "tmp/stage8b-p1f-o1-cargo-home"


def git(*args: str) -> bytes:
    return subprocess.check_output(("git", *args), cwd=ROOT)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def artifact(path: str, raw: bytes, *, bundle_path: str | None = None, destination: str | None = None, mode: str = "0644") -> dict[str, object]:
    return {
        "source_path": path,
        "bundle_path": bundle_path or path,
        "destination": destination,
        "owner": "root",
        "group": "root",
        "mode": mode,
        "sha256": sha256(raw),
        "size": len(raw),
    }


def build_binary(work: Path) -> tuple[bytes, bytes, bytes]:
    source_archive = work / "accepted-ie-source.tar"
    source_dir = work / "source"
    target_dir = work / "target"
    source_dir.mkdir()
    target_dir.mkdir()
    CARGO_HOME.mkdir(parents=True, exist_ok=True)
    subprocess.run(("git", "archive", "--format=tar", f"--output={source_archive}", source_check.ACCEPTED_IE), cwd=ROOT, check=True)
    with tarfile.open(source_archive) as archive:
        for item in archive.getmembers():
            if item.issym() or item.islnk() or Path(item.name).is_absolute():
                raise SystemExit("stage8b-p1f-o1-handoff: FAIL unsafe source archive member")
            target = (source_dir / item.name).resolve()
            if source_dir.resolve() not in (target, *target.parents):
                raise SystemExit("stage8b-p1f-o1-handoff: FAIL unsafe source archive")
        archive.extractall(source_dir)
    common_docker = [
        "docker", "run", "--rm", "--platform", "linux/amd64",
        "-v", f"{source_dir}:/src:ro",
        "-v", f"{CARGO_HOME}:/cargo-home",
        "-v", f"{target_dir}:/target",
        "-w", "/src",
        "-e", "CARGO_HOME=/cargo-home",
        "-e", "CARGO_TARGET_DIR=/target",
        "-e", "CARGO_INCREMENTAL=0",
        "-e", "SOURCE_DATE_EPOCH=0",
        "-e", "RUSTFLAGS=-C strip=symbols --remap-path-prefix=/src=/usr/src/moex-trading-project",
        source_check.RUST_IMAGE,
    ]
    fetch = subprocess.run(
        [*common_docker, "cargo", "fetch", "--locked"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if fetch.returncode != 0:
        raise SystemExit(fetch.stdout.decode(errors="replace"))
    command = [
        *common_docker[:3], "--network", "none", *common_docker[3:],
        "cargo", "build", "--locked", "--release", "-p", "runtime-durable-service", "--bin", "stage8b-p1-paper-supervisor",
    ]
    process = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    binary_path = target_dir / "release/stage8b-p1-paper-supervisor"
    if process.returncode != 0 or not binary_path.is_file():
        raise SystemExit(process.stdout.decode(errors="replace"))
    binary = binary_path.read_bytes()
    if len(binary) <= 64 or binary[:4] != b"\x7fELF" or binary[4:6] != b"\x02\x01" or struct.unpack_from("<H", binary, 18)[0] != 62:
        raise SystemExit("stage8b-p1f-o1-handoff: FAIL built binary is not x86-64 ELF")
    source_tree = git("rev-parse", f"{source_check.ACCEPTED_IE}^{{tree}}").decode().strip()
    result = {
        "schema_version": 1,
        "source_ref": source_check.ACCEPTED_IE,
        "source_tree": source_tree,
        "source_archive_sha256": sha256(source_archive.read_bytes()),
        "rust_image": source_check.RUST_IMAGE,
        "dependency_fetch_exit_code": fetch.returncode,
        "dependency_fetch_network": "docker-default-before-build",
        "build_network": "none",
        "build_exit_code": process.returncode,
        "locked": True,
        "profile": "release",
        "package": "runtime-durable-service",
        "binary": "stage8b-p1-paper-supervisor",
        "target": "x86_64-unknown-linux-gnu",
        "binary_sha256": sha256(binary),
        "binary_size": len(binary),
    }
    archive_check = (
        f"source_ref={source_check.ACCEPTED_IE}\nsource_tree={source_tree}\n"
        f"source_archive_sha256={result['source_archive_sha256']}\nverification=PASS\n"
    ).encode()
    log = fetch.stdout + b"\nPASS stage8b-p1f-o1-locked-dependency-fetch\n" + process.stdout
    return binary, (json.dumps(result, indent=2, sort_keys=True) + "\n").encode(), log + b"\nPASS stage8b-p1f-o1-binary-build-network-none\n" + archive_check


def make_bundle(source_ref: str, short: str, binary: bytes) -> tuple[str, bytes, str]:
    spec = json.loads(git("show", f"{source_ref}:{source_check.SPEC}"))
    files: dict[str, tuple[bytes, str]] = {}
    installer = git("show", f"{source_ref}:{source_check.INSTALLER}")
    identity = git("show", f"{source_ref}:{source_check.IDENTITY}")
    files[source_check.INSTALLER] = (installer, "100755")
    files[source_check.IDENTITY] = (identity, "100644")
    for path in source_check.PAYLOADS:
        files[path] = (git("show", f"{source_ref}:{path}"), "100644")
    files["payload/stage8b-p1-paper-supervisor"] = (binary, "100755")
    destinations = {item["path"]: item["destination"] for item in spec["accepted_fixed_install"]["public_payloads"]}
    artifacts = [
        artifact(source_check.INSTALLER, installer, destination=None, mode="0755"),
        artifact(source_check.IDENTITY, identity, destination=None),
    ]
    artifacts.extend(artifact(path, files[path][0], destination=destinations[path]) for path in source_check.PAYLOADS)
    artifacts.append(artifact(source_check.ACCEPTED_IE, binary, bundle_path="payload/stage8b-p1-paper-supervisor", destination="/usr/local/libexec/moex/stage8b-p1-paper-supervisor", mode="0755"))
    manifest = {
        "schema_version": 1,
        "domain": "moex.stage8b.p1f.o1.provisioning-package.v1",
        "candidate_source_ref": source_ref,
        "accepted_o0_ref": source_check.ACCEPTED_O0,
        "accepted_o0_closure": source_check.BASE,
        "accepted_runtime_source_ref": source_check.ACCEPTED_IE,
        "accepted_fixed_install_ref": source_check.FIXED_INSTALL,
        "target": spec["target"],
        "artifacts": artifacts,
        "commands_after_separate_acceptance": spec["execution"],
        "execution_authorized": False,
        "remote_mutation_performed": False,
        "excluded": [
            "/etc/moex-finam-p1-paper/supervisor.json",
            "/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json",
            "/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key",
            "phase manifests and activation certificates",
        ],
        "closed_surfaces": spec["closed_surfaces"],
    }
    manifest_raw = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()
    readme = (
        "Stage 8B-P1-f O1 non-activating provisioning bundle\n\n"
        "EXECUTION IS NOT AUTHORIZED BY THIS PACKAGE.\n"
        "Independent acceptance and a fresh O0 preflight are mandatory before use.\n"
        "The bundle contains no operator configuration, first-boot source or credential.\n"
        "Do not run daemon-reload, enable or start during O1.\n"
    ).encode()
    files["o1-provisioning-manifest.json"] = (manifest_raw, "100644")
    files["README.md"] = (readme, "100644")
    import io
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, (raw, mode) in sorted(files.items()):
            archive.writestr(common.zip_info(name, mode), raw)
    name = f"stage8b-p1f-o1-provisioning-bundle-{short}.zip"
    return name, output.getvalue(), sha256(manifest_raw)


def main() -> None:
    status = git("status", "--porcelain", "--untracked-files=all").decode().strip()
    if status:
        raise SystemExit(f"stage8b-p1f-o1-handoff: FAIL dirty worktree\n{status}")
    branch = git("branch", "--show-current").decode().strip()
    if branch != source_check.BRANCH:
        raise SystemExit(f"stage8b-p1f-o1-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != source_check.BASE:
        raise SystemExit("stage8b-p1f-o1-handoff: FAIL source parent drift")
    changed = set(git("diff", "--name-only", source_check.BASE, source_ref, "--").decode().splitlines())
    if changed != source_check.ALLOWED_CHANGES:
        raise SystemExit(f"stage8b-p1f-o1-handoff: FAIL changed paths {sorted(changed ^ source_check.ALLOWED_CHANGES)}")
    review = (DOWNLOADS / source_check.O0_REVIEW).read_bytes()
    if sha256(review) != source_check.O0_REVIEW_SHA256:
        raise SystemExit("stage8b-p1f-o1-handoff: FAIL O0 review digest")
    gate_parts = []
    for command in (["python3", "scripts/stage8b_p1f_o1_check.py"], ["python3", "scripts/stage8b_p1f_o1_negative_harness.py"]):
        process = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
        gate_parts.append(process.stdout)
        if process.returncode != 0:
            raise SystemExit(process.stdout.decode(errors="replace"))
    gate = b"".join(gate_parts) + b"PASS stage8b-p1f-o1-gate execution=false remote_mutation=false\n"
    if git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1f-o1-handoff: FAIL source changed during gate")

    with tempfile.TemporaryDirectory(prefix="stage8b-p1f-o1-") as temporary:
        binary, build_result, build_combined = build_binary(Path(temporary))
    split = build_combined.rsplit(b"source_ref=", 1)
    build_log = split[0]
    source_archive_check = b"source_ref=" + split[1]
    short = source_ref[:7]
    bundle_name, bundle, bundle_manifest_sha256 = make_bundle(source_ref, short, binary)
    archive_name = f"moex-trading-project-{short}-stage8b-p1f-o1-review-package.zip"
    archive_path = OUTPUT / archive_name
    source_manifest, entries = common.source_manifest(source_ref)
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": "O1_PACKAGE_REVIEW_CANDIDATE_EXECUTION_NOT_AUTHORIZED",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "accepted_o0_ref": source_check.ACCEPTED_O0,
        "accepted_o0_closure": source_check.BASE,
        "accepted_ie_ref": source_check.ACCEPTED_IE,
        "accepted_fixed_install_ref": source_check.FIXED_INSTALL,
        "changed_paths": sorted(changed),
        "source_manifest_sha256": sha256(source_manifest),
        "gate_sha256": sha256(gate),
        "build_result_sha256": sha256(build_result),
        "bundle_name": bundle_name,
        "bundle_sha256": sha256(bundle),
        "bundle_manifest_sha256": bundle_manifest_sha256,
        "execution_authorized": False,
        "remote_mutation_performed": False,
        "ssh_connection_performed": False,
        "closed_surfaces": json.loads(git("show", f"{source_ref}:{source_check.SPEC}"))["closed_surfaces"],
    }
    marker = (
        f"stage={safety.STAGE}\nsource_short_ref={short}\nsource_ref={source_ref}\n"
        f"source_parent={source_parent}\nsource_tree={source_tree}\nbranch={branch}\n"
        f"archive_name={archive_name}\nbundle_name={bundle_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: source_manifest,
        safety.COMMIT_RAW: git("cat-file", "commit", source_ref),
        safety.EVIDENCE: (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode(),
        safety.GATE: gate,
        safety.BUILD: build_result,
        safety.BUILD_LOG: build_log,
        safety.SOURCE_ARCHIVE: source_archive_check,
        safety.REVIEW: review,
        safety.PREFIX + bundle_name: bundle,
    }
    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(common.zip_info(entry["path"], entry["mode"]), git("show", f"{source_ref}:{entry['path']}"))
        for name, raw in sorted(additions.items()):
            archive.writestr(common.zip_info(name), raw)
    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(f"archive={archive_path}\nsha256={digest}\nbundle_sha256={sha256(bundle)}\nbinary_sha256={json.loads(build_result)['binary_sha256']}\nsource_ref={source_ref}\nPASS stage8b-p1f-o1-handoff")


if __name__ == "__main__":
    main()
