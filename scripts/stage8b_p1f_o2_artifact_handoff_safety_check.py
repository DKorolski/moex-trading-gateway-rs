#!/usr/bin/env python3
"""Validate the commit-bound Stage 8B-P1F O2 execution-artifact handoff."""

from __future__ import annotations

import hashlib
import base64
import json
import sys
import zipfile
from pathlib import PurePosixPath
from typing import Any

import stage8b_p1e_i1a_handoff_safety_check as common


STAGE = "Stage 8B-P1F O2 immutable execution artifact"
STATUS = "REVIEW_CANDIDATE_EXECUTION_NOT_AUTHORIZED"
BRANCH = "stage8b-o2-artifact-resumption"
CONTRACT_REF = "a9f8fe30a45752c943f9e399775322d83fcd8a36"
IMPLEMENTATION_REF = "9d9bd1192467532d0ee48350d3c531d9e156dee3"
IMPLEMENTATION_TREE = "0e1ee00d73e23460dc0c4af5a13296d6e2ad12e3"
ARTIFACT_REF = IMPLEMENTATION_REF
REVIEW_NAME = "FINAM_P1F_O2_R1_CONTRACT_ACCEPT_a9f8fe3_2026-09-27.md"
REVIEW_SHA256 = "3342409133760dbb8ea909460310881eebdd91c797ed97f3879be4cfbb12ce1b"

PREFIX = "handoff-evidence/"
MARKER = "handoff-commit.txt"
MANIFEST = PREFIX + "source-tree-manifest.json"
COMMIT_RAW = PREFIX + "source-commit.raw"
EVIDENCE = PREFIX + "stage8b-p1f-o2-artifact-handoff-evidence.json"
GATE = PREFIX + "stage8b-p1f-o2-artifact-gate.txt"
BUILD = PREFIX + "stage8b-p1f-o2-linux-build.json"
BUILD_LOG = PREFIX + "linux-build.log"
BUILD_COMMIT = PREFIX + "build-source-commit.raw"
BUILD_MANIFEST = PREFIX + "build-source-tree-manifest.json"
BUILD_ORIGINALS = PREFIX + "build-source-original-blobs.json"
ELF_SMOKE = PREFIX + "linux-elf-smoke.log"
SOURCE_REVIEW_NAME = "FINAM_5b8f878_O2_RECOVERY_SOURCE_REVIEW_2026-09-28.md"
SOURCE_REVIEW = PREFIX + "reviews/" + SOURCE_REVIEW_NAME
SOURCE_REVIEW_SHA256 = "4759f46e8b878bd9a86ee13de46d968ca52e4c4e827501524ef081563b59d177"
REVIEW = PREFIX + "reviews/" + REVIEW_NAME
MATERIALIZER = "payload/stage8b-p1f-o2-materializer"
OPERATOR = "payload/stage8b-p1f-o2-operator"
SUPERVISOR = "payload/stage8b-p1-paper-supervisor"
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, GATE, BUILD, REVIEW, MATERIALIZER, OPERATOR,
             BUILD_LOG, BUILD_COMMIT, BUILD_MANIFEST, BUILD_ORIGINALS, ELF_SMOKE, SOURCE_REVIEW, SUPERVISOR}

REQUIRED_TRACKED = {
    "docs/stage-8/stage8b-p1f-o2-execution-artifact.json",
    "docs/stage-8/stage8b-p1f-o2-execution-artifact.md",
    "docs/stage-8/stage8b-p1f-o2-artifact-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1f-o2-authority-public-key.hex",
    "docs/stage-8/stage8b-p1f-o2-materialization-policy.json",
    "docs/stage-8/stage8b-p1f-o2-source-template.json",
    "docs/stage-8/stage8b-p1f-o2-supervisor-template.json",
    "deploy/stage8b-p1e/moex-finam-p1f-o2-materializer.service",
    "deploy/stage8b-p1e/moex-finam-p1-paper-o2-bootstrap-runner.service",
    "scripts/stage8b_p1f_o2_artifact_check.py",
    "scripts/stage8b_p1f_o2_artifact_negative_harness.py",
    "scripts/stage8b_p1f_o2_artifact_witness.sh",
    "scripts/stage8b_p1f_o2_artifact_gate.sh",
    "scripts/make_stage8b_p1f_o2_artifact_handoff.py",
    "scripts/stage8b_p1f_o2_artifact_handoff_safety_check.py",
    "scripts/stage8b_p1f_o2_build_linux.py",
    "scripts/stage8b_p1f_o2_elf_smoke.sh",
    "deploy/stage8b-p1e/moex-finam-p1-paper-bootstrap.service",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        require(key not in value, f"duplicate JSON key: {key}")
        value[key] = item
    return value


def validate_name(name: str) -> None:
    path = PurePosixPath(name)
    require(
        bool(name)
        and not name.startswith("/")
        and "\\" not in name
        and all(part not in {"", ".", ".."} for part in path.parts),
        f"unsafe archive member: {name!r}",
    )
    require(
        not any(part in {".git", "target", "tmp", "__pycache__", "__MACOSX"} for part in path.parts),
        f"forbidden archive path: {name}",
    )
    basename = path.name
    require(
        basename != ".env" and not (basename.startswith(".env.") and basename != ".env.example"),
        f"secret env member: {name}",
    )
    require(
        not basename.endswith((".pem", ".key", ".ed25519", ".sqlite", ".sqlite3", ".rdb")),
        f"secret/runtime artifact: {name}",
    )


def parse_marker(raw: bytes) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in raw.decode().splitlines():
        require(bool(line) and "=" in line, "invalid marker line")
        key, value = line.split("=", 1)
        require(bool(key) and bool(value) and key not in result, "invalid marker item")
        result[key] = value
    require(
        set(result)
        == {
            "stage",
            "status",
            "source_short_ref",
            "source_ref",
            "source_parent",
            "source_tree",
            "branch",
            "contract_ref",
            "implementation_ref",
            "artifact_ref",
            "archive_name",
        },
        "marker inventory drift",
    )
    return result


def check(path: str) -> dict[str, Any]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        by_name = {item.filename: item for item in infos}
        require(len(names) == len(set(names)), "duplicate archive members")
        for item in infos:
            validate_name(item.filename)
            mode = (item.external_attr >> 16) & 0o177777
            require(mode & 0o170000 != 0o120000, f"symlink member: {item.filename}")
            require(mode & 0o170000 in {0, 0o100000, 0o040000}, f"special member: {item.filename}")
        files = {name: archive.read(name) for name in names}
        require(not (GENERATED - set(names)), f"generated members missing: {sorted(GENERATED - set(names))}")
        require(not (REQUIRED_TRACKED - set(names)), f"tracked members missing: {sorted(REQUIRED_TRACKED - set(names))}")

        marker = parse_marker(files[MARKER])
        require(marker["stage"] == STAGE and marker["status"] == STATUS, "marker status drift")
        require(marker["archive_name"] == PurePosixPath(path).name, "archive name drift")
        require(marker["branch"] == BRANCH, "branch drift")
        require(marker["contract_ref"] == CONTRACT_REF, "contract ref drift")
        require(marker["implementation_ref"] == IMPLEMENTATION_REF, "implementation ref drift")
        require(marker["artifact_ref"] == ARTIFACT_REF, "artifact ref drift")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref drift")

        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit object mismatch")
        commit_lines = commit_raw.decode().splitlines()
        require(commit_lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {marker['source_parent']}" in commit_lines, "commit parent mismatch")

        manifest = json.loads(files[MANIFEST], object_pairs_hook=strict_object)
        require(manifest["schema_version"] == 2, "manifest schema drift")
        require(manifest["source_ref"] == marker["source_ref"], "manifest source drift")
        entries = manifest["entries"]
        require(manifest["entry_count"] == len(entries), "manifest count drift")
        tracked: set[str] = set()
        payloads: dict[str, bytes] = {}
        for entry in entries:
            name = entry["path"]
            require(name not in tracked and name in files, f"manifest member mismatch: {name}")
            raw = files[name]
            mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
            require(len(raw) == entry["size"], f"manifest size mismatch: {name}")
            require(sha256(raw) == entry["sha256"], f"manifest hash mismatch: {name}")
            require(mode == entry["mode"], f"manifest mode mismatch: {name}")
            tracked.add(name)
            payloads[name] = raw
        require(set(names) - tracked == GENERATED, "generated member inventory drift")
        require(common.build_tree_oid(entries, payloads) == marker["source_tree"], "source tree reconstruction failed")

        artifact = json.loads(
            files["docs/stage-8/stage8b-p1f-o2-execution-artifact.json"],
            object_pairs_hook=strict_object,
        )
        require(artifact["status"] == STATUS, "artifact status drift")
        require(artifact["accepted_contract_ref"] == CONTRACT_REF, "artifact contract drift")
        require(artifact["implementation_ref"] == IMPLEMENTATION_REF, "artifact implementation drift")
        require(artifact["implementation_tree"] == IMPLEMENTATION_TREE, "artifact implementation tree drift")
        require(artifact["execution_authorized"] is False, "execution opened")
        require(artifact["target_mutation_performed"] is False, "target mutation claimed")
        binary_by_name = {item["name"]: item for item in artifact["build"]["binaries"]}
        require(sha256(files[MATERIALIZER]) == binary_by_name["stage8b-p1f-o2-materializer"]["sha256"], "materializer payload drift")
        require(len(files[MATERIALIZER]) == binary_by_name["stage8b-p1f-o2-materializer"]["size"], "materializer size drift")
        require(sha256(files[OPERATOR]) == binary_by_name["stage8b-p1f-o2-operator"]["sha256"], "operator payload drift")
        require(len(files[OPERATOR]) == binary_by_name["stage8b-p1f-o2-operator"]["size"], "operator size drift")
        require(files[MATERIALIZER][:4] == b"\x7fELF" and files[OPERATOR][:4] == b"\x7fELF", "payload is not ELF")
        for name in (MATERIALIZER, OPERATOR, SUPERVISOR):
            require(files[name][:6] == b"\x7fELF\x02\x01" and int.from_bytes(files[name][18:20], "little") == 62, "payload is not ELF64 x86-64")
            require((by_name[name].external_attr >> 16) == 0o100755, "payload executable mode drift")

        evidence = json.loads(files[EVIDENCE], object_pairs_hook=strict_object)
        require(evidence["stage"] == STAGE and evidence["status"] == STATUS, "evidence status drift")
        require(evidence["source_ref"] == marker["source_ref"], "evidence source drift")
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree drift")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest evidence drift")
        require(evidence["gate_sha256"] == sha256(files[GATE]), "gate evidence drift")
        require(evidence["build_sha256"] == sha256(files[BUILD]), "build evidence drift")
        require(evidence["review_sha256"] == REVIEW_SHA256 == sha256(files[REVIEW]), "review evidence drift")
        require(sha256(files[SOURCE_REVIEW]) == SOURCE_REVIEW_SHA256, "source review drift")
        require(evidence["elf_smoke_sha256"] == sha256(files[ELF_SMOKE]), "ELF smoke evidence drift")
        require(b"PASS stage8b-p1f-o2-elf-smoke network=none authority=absent systemd_runtime_tested=false execution=false" in files[ELF_SMOKE], "ELF smoke marker missing")
        require(evidence["systemd_runtime_tested"] is False, "ELF smoke overstated as systemd proof")
        for binary, payload, field in (
            ("stage8b-p1f-o2-materializer", MATERIALIZER, "materializer_sha256"),
            ("stage8b-p1f-o2-operator", OPERATOR, "operator_sha256"),
            ("stage8b-p1-paper-supervisor", SUPERVISOR, "bootstrap_supervisor_sha256"),
        ):
            require(evidence[field] == sha256(files[payload]), "evidence payload drift")
            require(f"{sha256(files[payload])}  /usr/local/libexec/moex/{binary}".encode() in files[ELF_SMOKE], "smoke payload identity drift")
        for smoke_marker in (
            b"PASS exact-elf bootstrap-supervisor-baseline07-config",
            b"PASS exact-elf bootstrap-supervisor-stale-profile exit=64",
            b"PASS exact-elf runner-no-selector exit=70",
            b"PASS exact-elf precreated-control-root-no-capabilities",
        ):
            require(smoke_marker in files[ELF_SMOKE], "smoke coverage marker missing")
        require(evidence["private_authority_key_in_handoff"] is False, "private authority key included")
        require(evidence["finam_credentials_in_handoff"] is False, "FINAM credential included")
        require(evidence["execution_authorized"] is False, "evidence opened execution")
        require(evidence["target_mutation_performed"] is False, "evidence claimed target mutation")
        require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")

        build = json.loads(files[BUILD], object_pairs_hook=strict_object)
        require(build["platform"] == "linux/amd64", "build platform drift")
        require(build["implementation_ref"] == IMPLEMENTATION_REF, "build implementation drift")
        require(build["source_tree"] == IMPLEMENTATION_TREE, "build tree drift")
        require(build["rust_image"] == artifact["build"]["rust_image"] and build["cargo_args"] == artifact["build"]["cargo_args"], "build recipe drift")
        require(build["network"] == "none" and build["cargo_offline"] is True, "build network drift")
        require(build["build_log_sha256"] == sha256(files[BUILD_LOG]), "build log drift")
        require(build["source_commit_raw_sha256"] == sha256(files[BUILD_COMMIT]), "build commit evidence drift")
        require(common.git_object_id("commit", files[BUILD_COMMIT]) == IMPLEMENTATION_REF, "build commit object mismatch")
        require(files[BUILD_COMMIT].splitlines()[0] == f"tree {IMPLEMENTATION_TREE}".encode(), "build commit tree mismatch")
        built = {item["name"]: item for item in build["binaries"]}
        require(len(build["binaries"]) == 3 and set(built) == set(binary_by_name), "build binary inventory drift")
        for name, payload in (("stage8b-p1f-o2-materializer", MATERIALIZER), ("stage8b-p1f-o2-operator", OPERATOR), ("stage8b-p1-paper-supervisor", SUPERVISOR)):
            require(built[name]["sha256"] == sha256(files[payload]) and built[name]["size"] == len(files[payload]), "build payload drift")
            require(binary_by_name[name]["sha256"] == sha256(files[payload]) and binary_by_name[name]["size"] == len(files[payload]), "manifest payload drift")
        prerequisite = artifact["bootstrap_runtime_prerequisite"]
        require(prerequisite["sha256"] == sha256(files[SUPERVISOR]) and prerequisite["source_ref"] == IMPLEMENTATION_REF, "bootstrap prerequisite identity drift")
        require(prerequisite["replacement_required"] is True and prerequisite["replacement_authorized"] is False, "bootstrap replacement boundary drift")
        # Reconstruct the compiled merge tree independently from the packaging
        # tree. Only changed original public blobs need duplication in the ZIP.
        source_manifest = json.loads(files[BUILD_MANIFEST], object_pairs_hook=strict_object)
        original_blobs = json.loads(files[BUILD_ORIGINALS], object_pairs_hook=strict_object)
        source_entries = source_manifest["entries"]
        require(source_manifest["source_ref"] == IMPLEMENTATION_REF, "build source manifest ref drift")
        require(source_manifest["entry_count"] == len(source_entries), "build source manifest count drift")
        source_payloads = {}
        for entry in source_entries:
            name = entry["path"]
            require(name not in source_payloads, "duplicate build-source entry")
            raw = base64.b64decode(original_blobs[name], validate=True) if name in original_blobs else files[name]
            require(sha256(raw) == entry["sha256"] and len(raw) == entry["size"], "build source blob drift")
            source_payloads[name] = raw
        require(set(original_blobs) <= set(source_payloads), "unreferenced build-source original")
        require(common.build_tree_oid(source_entries, source_payloads) == IMPLEMENTATION_TREE, "compiled tree reconstruction failed")
        for marker_text in (
            b"PASS stage8b-p1f-o2-artifact-check rows=30 execution=false",
            b"PASS stage8b-p1f-o2-artifact-negative-harness 36/36",
            b"PASS stage8b-p1f-o2-artifact-witness execution=false",
            b"PASS stage8b-p1f-o2-artifact-gate execution=false",
        ):
            require(marker_text in files[GATE], f"gate marker missing: {marker_text!r}")

        return {
            "result": "PASS",
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": marker["source_ref"],
            "source_tree": marker["source_tree"],
            "build_source_ref": IMPLEMENTATION_REF,
            "build_source_tree": IMPLEMENTATION_TREE,
            "build_tree_binding": True,
            "systemd_runtime_tested": False,
            "materializer_sha256": sha256(files[MATERIALIZER]),
            "operator_sha256": sha256(files[OPERATOR]),
            "supervisor_sha256": sha256(files[SUPERVISOR]),
            "execution_authorized": False,
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1f_o2_artifact_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, UnicodeDecodeError, ValueError, KeyError, TypeError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1f-o2-artifact-handoff-safety: FAIL {error}")
        raise SystemExit(1) from error
    print("PASS stage8b-p1f-o2-artifact-handoff-safety " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
