#!/usr/bin/env python3
"""Validate the immutable Stage 8B-P1-f O1 review handoff and nested bundle."""

from __future__ import annotations

import hashlib
import json
import struct
import sys
import zipfile
from pathlib import PurePosixPath

import stage8b_p1e_i1a_handoff_safety_check as common
import stage8b_p1f_o1_check as source_check


STAGE = "Stage 8B-P1-f O1 non-activating provisioning package"
PREFIX = "handoff-evidence/"
MARKER = "handoff-commit.txt"
MANIFEST = PREFIX + "source-tree-manifest.json"
COMMIT_RAW = PREFIX + "source-commit.raw"
EVIDENCE = PREFIX + "stage8b-p1f-o1-handoff-evidence.json"
GATE = PREFIX + "stage8b-p1f-o1-gate.txt"
BUILD = PREFIX + "stage8b-p1f-o1-binary-build.json"
BUILD_LOG = PREFIX + "stage8b-p1f-o1-binary-build.log"
SOURCE_ARCHIVE = PREFIX + "stage8b-p1f-o1-source-archive-check.txt"
REVIEW = PREFIX + "reviews/" + source_check.O0_REVIEW


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def strict_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def validate_name(name: str, *, nested: bool = False) -> None:
    path = PurePosixPath(name)
    require(bool(name) and not name.startswith("/") and "\\" not in name, f"unsafe member: {name!r}")
    require(all(part not in {"", ".", ".."} for part in path.parts), f"unsafe member: {name!r}")
    require(not any(part in {".git", "target", "tmp", "__pycache__", "__MACOSX"} for part in path.parts), f"forbidden member: {name}")
    basename = path.name
    require(basename != ".env" and not (basename.startswith(".env.") and basename != ".env.example"), f"env member: {name}")
    require(not basename.endswith((".pem", ".key", ".ed25519", ".sqlite", ".sqlite3", ".rdb")), f"secret/runtime member: {name}")
    if nested:
        require(not name.startswith("handoff-evidence/"), f"nested evidence recursion: {name}")


def parse_marker(raw: bytes) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in raw.decode().splitlines():
        require(bool(line) and "=" in line, "invalid marker line")
        key, value = line.split("=", 1)
        require(bool(key) and bool(value) and key not in result, "invalid marker entry")
        result[key] = value
    require(set(result) == {"stage", "source_short_ref", "source_ref", "source_parent", "source_tree", "branch", "archive_name", "bundle_name"}, "marker inventory drift")
    return result


def validate_elf_x86_64(raw: bytes) -> None:
    require(len(raw) > 64 and raw[:4] == b"\x7fELF", "binary is not ELF")
    require(raw[4] == 2 and raw[5] == 1, "binary is not little-endian ELF64")
    require(struct.unpack_from("<H", raw, 18)[0] == 62, "binary is not x86-64")


def validate_bundle(raw: bytes, source_files: dict[str, bytes], source_ref: str) -> dict[str, object]:
    import io

    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        require(len(names) == len(set(names)), "bundle duplicate members")
        for item in infos:
            validate_name(item.filename, nested=True)
            mode = (item.external_attr >> 16) & 0o177777
            require(mode & 0o170000 != 0o120000, f"bundle symlink: {item.filename}")
            require(mode & 0o170000 in {0, 0o100000}, f"bundle special member: {item.filename}")
        files = {name: archive.read(name) for name in names}
    expected = {
        "README.md",
        "o1-provisioning-manifest.json",
        "payload/stage8b-p1-paper-supervisor",
        source_check.INSTALLER,
        source_check.IDENTITY,
        *source_check.PAYLOADS,
    }
    require(set(files) == expected, "bundle member inventory drift")
    manifest = json.loads(files["o1-provisioning-manifest.json"], object_pairs_hook=strict_object)
    require(manifest["schema_version"] == 1 and manifest["domain"] == "moex.stage8b.p1f.o1.provisioning-package.v1", "bundle manifest identity drift")
    require(manifest["candidate_source_ref"] == source_ref, "bundle source drift")
    require(manifest["execution_authorized"] is False and manifest["remote_mutation_performed"] is False, "bundle execution opened")
    require(all(flag is False for flag in manifest["closed_surfaces"].values()), "bundle surface opened")
    artifacts = manifest["artifacts"]
    require(isinstance(artifacts, list) and len(artifacts) == 8, "bundle artifact inventory drift")
    by_bundle = {item["bundle_path"]: item for item in artifacts}
    require(len(by_bundle) == len(artifacts), "duplicate bundle artifact")
    for path, item in by_bundle.items():
        require(path in files and sha256(files[path]) == item["sha256"] and len(files[path]) == item["size"], f"artifact mismatch: {path}")
        if path != "payload/stage8b-p1-paper-supervisor":
            source_path = item["source_path"]
            require(source_path in source_files and files[path] == source_files[source_path], f"source artifact mismatch: {path}")
    binary = files["payload/stage8b-p1-paper-supervisor"]
    validate_elf_x86_64(binary)
    require(manifest["excluded"] == [
        "/etc/moex-finam-p1-paper/supervisor.json",
        "/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json",
        "/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key",
        "phase manifests and activation certificates",
    ], "excluded inventory drift")
    return {"members": len(files), "binary_sha256": sha256(binary), "manifest_sha256": sha256(files["o1-provisioning-manifest.json"])}


def check(path: str) -> dict[str, object]:
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
    marker = parse_marker(files[MARKER])
    require(marker["stage"] == STAGE and marker["archive_name"] == PurePosixPath(path).name, "marker drift")
    require(marker["source_parent"] == source_check.BASE and marker["branch"] == source_check.BRANCH, "lineage drift")
    require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref drift")
    bundle_path = PREFIX + marker["bundle_name"]
    generated = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, GATE, BUILD, BUILD_LOG, SOURCE_ARCHIVE, REVIEW, bundle_path}
    require(generated <= set(files), "generated evidence missing")

    commit_raw = files[COMMIT_RAW]
    require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit object mismatch")
    lines = commit_raw.decode().splitlines()
    require(lines[0] == f"tree {marker['source_tree']}" and f"parent {source_check.BASE}" in lines, "commit lineage mismatch")
    manifest = json.loads(files[MANIFEST], object_pairs_hook=strict_object)
    require(manifest["source_ref"] == marker["source_ref"] and manifest["entry_count"] == len(manifest["entries"]), "source manifest drift")
    tracked: set[str] = set()
    payloads: dict[str, bytes] = {}
    for entry in manifest["entries"]:
        name = entry["path"]
        require(name not in tracked and name in files, f"tracked member missing: {name}")
        raw = files[name]
        mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
        require(len(raw) == entry["size"] and sha256(raw) == entry["sha256"] and mode == entry["mode"], f"tracked member mismatch: {name}")
        tracked.add(name)
        payloads[name] = raw
    require(set(files) - tracked == generated, "generated member inventory drift")
    require(common.build_tree_oid(manifest["entries"], payloads) == marker["source_tree"], "source tree reconstruction mismatch")

    evidence = json.loads(files[EVIDENCE], object_pairs_hook=strict_object)
    build = json.loads(files[BUILD], object_pairs_hook=strict_object)
    require(evidence["source_ref"] == marker["source_ref"] and evidence["source_parent"] == source_check.BASE, "evidence lineage drift")
    require(evidence["execution_authorized"] is False and evidence["remote_mutation_performed"] is False, "handoff execution opened")
    require(all(flag is False for flag in evidence["closed_surfaces"].values()), "handoff surface opened")
    require(build["source_ref"] == source_check.ACCEPTED_IE and build["source_tree"] == source_check.ACCEPTED_IE_TREE, "build source drift")
    require(build["dependency_fetch_exit_code"] == 0 and build["dependency_fetch_network"] == "docker-default-before-build", "dependency fetch drift")
    require(build["build_exit_code"] == 0 and build["build_network"] == "none" and build["rust_image"] == source_check.RUST_IMAGE, "build result drift")
    bundle = validate_bundle(files[bundle_path], payloads, marker["source_ref"])
    require(bundle["binary_sha256"] == build["binary_sha256"], "binary build/bundle mismatch")
    require(evidence["bundle_sha256"] == sha256(files[bundle_path]), "bundle digest drift")
    require(evidence["bundle_manifest_sha256"] == bundle["manifest_sha256"], "bundle manifest digest drift")
    require(evidence["build_result_sha256"] == sha256(files[BUILD]), "build evidence digest drift")
    require(evidence["gate_sha256"] == sha256(files[GATE]), "gate digest drift")
    require(evidence["source_manifest_sha256"] == sha256(files[MANIFEST]), "source manifest digest drift")
    require(sha256(files[REVIEW]) == source_check.O0_REVIEW_SHA256, "O0 review digest drift")
    require(b"PASS stage8b-p1f-o1-check" in files[GATE] and b"PASS stage8b-p1f-o1-negative-harness 12/12" in files[GATE], "gate marker missing")
    return {
        "archive_members": len(files),
        "tracked_members_verified": len(tracked),
        "bundle_members": bundle["members"],
        "duplicates": 0,
        "symlinks": 0,
        "unsafe_paths": 0,
        "source_ref": marker["source_ref"],
        "source_tree": marker["source_tree"],
        "bundle_sha256": sha256(files[bundle_path]),
        "binary_sha256": build["binary_sha256"],
        "execution_authorized": False,
        "remote_mutation_performed": False,
        "result": "PASS",
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1f_o1_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, UnicodeDecodeError, ValueError, KeyError, TypeError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1f-o1-handoff-safety: FAIL {error}")
        raise SystemExit(1) from error
    print("PASS stage8b-p1f-o1-handoff-safety " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
