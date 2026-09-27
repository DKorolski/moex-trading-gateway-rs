#!/usr/bin/env python3
"""Build the non-activating O1 bundle and immutable full-project review handoff."""

from __future__ import annotations

import hashlib
import io
import json
import struct
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1f_o1_check as source_check
import stage8b_p1f_o1_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
DOWNLOADS = Path("/Users/denisq/Downloads")


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


def reuse_reviewed_binary() -> tuple[bytes, bytes, bytes, bytes]:
    previous_path = OUTPUT / source_check.PREVIOUS_ARCHIVE
    raw = previous_path.read_bytes()
    if sha256(raw) != source_check.PREVIOUS_ARCHIVE_SHA256:
        raise SystemExit("stage8b-p1f-o1-handoff: FAIL predecessor archive digest")
    previous_bundle_name = "handoff-evidence/stage8b-p1f-o1-provisioning-bundle-f2fe5a2.zip"
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        bundle = archive.read(previous_bundle_name)
        build_result = archive.read(safety.BUILD)
        build_log = archive.read(safety.BUILD_LOG)
        source_archive_check = archive.read(safety.SOURCE_ARCHIVE)
    if sha256(bundle) != source_check.PREVIOUS_BUNDLE_SHA256:
        raise SystemExit("stage8b-p1f-o1-handoff: FAIL predecessor bundle digest")
    with zipfile.ZipFile(io.BytesIO(bundle)) as archive:
        binary = archive.read("payload/stage8b-p1-paper-supervisor")
    build = json.loads(build_result)
    if (
        sha256(binary) != source_check.ACCEPTED_BINARY_SHA256
        or build.get("binary_sha256") != source_check.ACCEPTED_BINARY_SHA256
        or build.get("source_ref") != source_check.ACCEPTED_IE
        or len(binary) <= 64
        or binary[:4] != b"\x7fELF"
        or binary[4:6] != b"\x02\x01"
        or struct.unpack_from("<H", binary, 18)[0] != 62
    ):
        raise SystemExit("stage8b-p1f-o1-handoff: FAIL predecessor binary identity")
    return binary, build_result, build_log, source_archive_check


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
        "accepted_o0_closure": source_check.O0_CLOSURE,
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
        "Stage 8B-P1-f O1 R1 non-activating provisioning bundle\n\n"
        "EXECUTION IS NOT AUTHORIZED BY THIS PACKAGE.\n"
        "Independent acceptance and a fresh O0 preflight are mandatory before use.\n"
        "The bundle contains no operator configuration, first-boot source or credential.\n"
        "After separate acceptance, change to the root-owned extracted bundle directory and use:\n"
        "  bundle_dir=\"$(pwd -P)\"\n"
        "  python3 \"$bundle_dir/scripts/stage8b_p1e_i1_fixed_install.py\" install --root / --binary \"$bundle_dir/payload/stage8b-p1-paper-supervisor\"\n"
        "Require status result EXACT_INSTALLED. Do not run daemon-reload, enable or start during O1.\n"
    ).encode()
    files["o1-provisioning-manifest.json"] = (manifest_raw, "100644")
    files["README.md"] = (readme, "100644")
    import io
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, (raw, mode) in sorted(files.items()):
            archive.writestr(common.zip_info(name, mode), raw)
    name = f"stage8b-p1f-o1-r1-command-correction-bundle-{short}.zip"
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
    o0_review = (DOWNLOADS / source_check.O0_REVIEW).read_bytes()
    if sha256(o0_review) != source_check.O0_REVIEW_SHA256:
        raise SystemExit("stage8b-p1f-o1-handoff: FAIL O0 review digest")
    hold_review = (DOWNLOADS / source_check.O1_HOLD_REVIEW).read_bytes()
    if sha256(hold_review) != source_check.O1_HOLD_REVIEW_SHA256:
        raise SystemExit("stage8b-p1f-o1-handoff: FAIL O1 HOLD review digest")
    gate_parts = []
    for command in (
        ["python3", "scripts/stage8b_p1f_o1_check.py"],
        ["python3", "scripts/stage8b_p1f_o1_negative_harness.py"],
        ["python3", "scripts/stage8b_p1f_o1_command_behavioral_test.py"],
    ):
        process = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
        gate_parts.append(process.stdout)
        if process.returncode != 0:
            raise SystemExit(process.stdout.decode(errors="replace"))
    gate = b"".join(gate_parts) + b"PASS stage8b-p1f-o1-gate execution=false remote_mutation=false\n"
    if git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1f-o1-handoff: FAIL source changed during gate")

    binary, build_result, build_log, source_archive_check = reuse_reviewed_binary()
    short = source_ref[:7]
    bundle_name, bundle, bundle_manifest_sha256 = make_bundle(source_ref, short, binary)
    archive_name = f"moex-trading-project-{short}-stage8b-p1f-o1-r1-command-correction-review-package.zip"
    archive_path = OUTPUT / archive_name
    source_manifest, entries = common.source_manifest(source_ref)
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": "O1_R1_COMMAND_CORRECTION_REVIEW_CANDIDATE_EXECUTION_NOT_AUTHORIZED",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "accepted_o0_ref": source_check.ACCEPTED_O0,
        "accepted_o0_closure": source_check.O0_CLOSURE,
        "held_predecessor": {
            "source_ref": source_check.BASE,
            "archive_name": source_check.PREVIOUS_ARCHIVE,
            "archive_sha256": source_check.PREVIOUS_ARCHIVE_SHA256,
            "bundle_sha256": source_check.PREVIOUS_BUNDLE_SHA256,
            "binary_sha256": source_check.ACCEPTED_BINARY_SHA256,
            "review_sha256": source_check.O1_HOLD_REVIEW_SHA256,
            "finding": "P2-O101",
        },
        "accepted_ie_ref": source_check.ACCEPTED_IE,
        "accepted_fixed_install_ref": source_check.FIXED_INSTALL,
        "changed_paths": sorted(changed),
        "source_manifest_sha256": sha256(source_manifest),
        "gate_sha256": sha256(gate),
        "build_result_sha256": sha256(build_result),
        "bundle_name": bundle_name,
        "bundle_sha256": sha256(bundle),
        "bundle_manifest_sha256": bundle_manifest_sha256,
        "binary_rebuilt": False,
        "binary_reused_from_reviewed_predecessor": True,
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
        safety.O0_REVIEW: o0_review,
        safety.HOLD_REVIEW: hold_review,
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
