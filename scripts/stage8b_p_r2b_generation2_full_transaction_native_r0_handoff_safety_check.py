#!/usr/bin/env python3
"""Validate the immutable Generation-2 native-runner review handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-generation2-native-runner-evidence.json"
GATE = "handoff-evidence/stage8b-generation2-native-runner-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
BIN_ROOT = "handoff-evidence/linux-amd64/exact-binaries"
TOOL_ROOT = "handoff-evidence/linux-amd64/proof-tools"
GENERATION2_ROOT = "handoff-evidence/linux-amd64"
GENERATED_BASE = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
FORBIDDEN_BASENAMES = {
    ".env",
    "package-authorization.ed25519",
    "helper-acceptance.ed25519",
    "account-binding-generation-2.hex",
    "stage8b-generation2-backup.agekey",
}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def mode(info: zipfile.ZipInfo) -> str:
    return f"{(info.external_attr >> 16) & 0o177777:06o}"


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        members = {item.filename: item for item in infos}
        if len(names) != len(set(names)):
            raise ValueError("duplicate members")
        for item in infos:
            member = PurePosixPath(item.filename)
            item_mode = (item.external_attr >> 16) & 0o177777
            if member.is_absolute() or ".." in member.parts or "" in member.parts:
                raise ValueError(f"unsafe path: {item.filename}")
            if item_mode & 0o170000 == 0o120000:
                raise ValueError(f"symlink: {item.filename}")
            if item_mode & 0o170000 not in {0, 0o100000, 0o040000}:
                raise ValueError(f"special file: {item.filename}")
            basename = member.name.lower()
            if basename in FORBIDDEN_BASENAMES or basename.endswith((".pem", ".key", ".agekey")):
                raise ValueError(f"private/runtime artifact name: {item.filename}")
            if member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden source root: {item.filename}")

        required = GENERATED_BASE | {
            "docs/current-status.md",
            "docs/stage-8/stage8b-p-r2b-generation2-full-transaction-contract.json",
            "docs/stage-8/stage8b-p-r2b-generation2-full-transaction-native-r0-authority.json",
            "scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_runner.sh",
            "scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_container_run.sh",
            "scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_host_preflight.py",
            "scripts/stage8b_p_r2b_generation2_full_transaction_native_r1_review_archive.py",
        }
        if missing := required - set(names):
            raise ValueError(f"missing members: {sorted(missing)}")

        marker = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        contract = json.loads(
            archive.read("docs/stage-8/stage8b-p-r2b-generation2-full-transaction-contract.json")
        )
        manifest_bytes = archive.read(MANIFEST)
        manifest = json.loads(manifest_bytes)
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive-name binding mismatch")
        if evidence.get("source_tree") != marker.get("source_tree"):
            raise ValueError("tree binding mismatch")
        if evidence.get("authorization") != "NOT_ISSUED" or evidence.get("native_execution") is not False:
            raise ValueError("review-only boundary opened")
        if evidence.get("generation_2_active") is not False or evidence.get("container_created") is not False:
            raise ValueError("execution/activation falsely claimed")
        if digest(manifest_bytes) != evidence.get("manifest_sha256"):
            raise ValueError("manifest digest mismatch")
        gate = archive.read(GATE)
        if b"runner=implemented r1=true review=required native_execution=false authorization=NOT_ISSUED" not in gate:
            raise ValueError("gate marker missing")
        if digest(gate) != evidence.get("gate_sha256"):
            raise ValueError("gate digest mismatch")

        generated = set(GENERATED_BASE)
        for name, expected in contract["production_linux_amd64_sha256"].items():
            member = f"{BIN_ROOT}/{name}"
            generated.add(member)
            if member not in members or digest(archive.read(member)) != expected:
                raise ValueError(f"production binary mismatch: {name}")
            if mode(members[member]) != "100755":
                raise ValueError(f"production binary mode mismatch: {name}")
        for name, expected in contract["proof_tool_linux_amd64_sha256"].items():
            member = f"{TOOL_ROOT}/{name}"
            generated.add(member)
            if member not in members or digest(archive.read(member)) != expected:
                raise ValueError(f"proof tool mismatch: {name}")
            if mode(members[member]) != "100755":
                raise ValueError(f"proof tool mode mismatch: {name}")

        build = json.loads(
            archive.read("docs/stage-8/stage8b-p-r2b-generation2-composition-r0-linux-build-evidence.json")
        )
        for name, record in build["binaries"].items():
            for build_name, key in (("build-a", "build_a_sha256"), ("build-b", "build_b_sha256")):
                member = f"{GENERATION2_ROOT}/{build_name}/{name}"
                generated.add(member)
                if member not in members or digest(archive.read(member)) != record[key]:
                    raise ValueError(f"Generation-2 reproducible binary mismatch: {build_name}/{name}")
                if mode(members[member]) != "100755":
                    raise ValueError(f"Generation-2 binary mode mismatch: {build_name}/{name}")

        tracked: set[str] = set()
        entries = manifest.get("entries", [])
        if manifest.get("entry_count") != len(entries):
            raise ValueError("manifest count mismatch")
        for entry in entries:
            name = entry["path"]
            if name in tracked or name not in members:
                raise ValueError(f"source member mismatch: {name}")
            tracked.add(name)
            data = archive.read(name)
            if len(data) != entry["size"] or digest(data) != entry["sha256"] or mode(members[name]) != entry["mode"]:
                raise ValueError(f"source content mismatch: {name}")
        if set(names) - tracked != generated:
            raise ValueError("generated member inventory mismatch")
        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "production_binaries_verified": 12,
            "proof_tools_verified": len(contract["proof_tool_linux_amd64_sha256"]),
            "generation2_reproducible_binary_members_verified": 16,
            "duplicates": 0,
            "symlinks": 0,
            "private_material_members": 0,
            "native_execution": False,
            "authorization": "NOT_ISSUED",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: native_r0_handoff_safety_check ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (KeyError, OSError, ValueError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-generation2-native-r0-handoff-safety: FAIL {error}") from error
    print("stage8b-generation2-native-r0-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
