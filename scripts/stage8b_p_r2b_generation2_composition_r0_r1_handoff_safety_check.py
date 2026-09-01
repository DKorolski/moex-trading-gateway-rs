#!/usr/bin/env python3
"""Validate an immutable Generation-2 R0-R1 evidence-closure handoff."""

from __future__ import annotations

import hashlib
import json
import re
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

import stage8b_p_r2b_generation2_composition_r0_r1_check as stage_check


STAGE = stage_check.STAGE
BRANCH = "stage8b-p-r2b-generation2-composition-rebuild-r0-r1"
ACCEPTED_R0 = stage_check.ACCEPTED_R0_REF
SOURCE_FOUNDATION = stage_check.BUILD_SOURCE_REF
SOURCE_FOUNDATION_TREE = stage_check.BUILD_SOURCE_TREE
EVIDENCE_SOURCE_REF = "2765314ebf04ae72d0a7da3258e22bb74d7db7ad"
EVIDENCE_SOURCE_TREE = "770c44540ab7bfb646738a08949eb434dd2088c4"
EVIDENCE = "handoff-evidence/stage8b-p-r2b-generation2-composition-r0-r1-evidence.json"
GATE = "handoff-evidence/stage8b-p-r2b-generation2-composition-r0-r1-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
ARTIFACT_ROOT = "handoff-evidence/linux-amd64"
AUTHORITY = stage_check.AUTHORITY.as_posix()
BUILD = stage_check.r0.BUILD.as_posix()
REHEARSAL = stage_check.REHEARSAL.as_posix()
GENERATED_BASE = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
SECRET_MEMBER_NAMES = {
    "package-authorization.ed25519",
    "helper-acceptance.ed25519",
    "account-binding-generation-2.hex",
    "key.ed25519",
}
HEX40 = re.compile(r"[0-9a-f]{40}")
HEX64 = re.compile(r"[0-9a-f]{64}")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def mode(item: zipfile.ZipInfo) -> str:
    return f"{(item.external_attr >> 16) & 0o177777:06o}"


def binary_members(build: dict[str, object]) -> set[str]:
    records = build.get("binaries")
    if not isinstance(records, dict):
        raise ValueError("build binary inventory missing")
    return {
        f"{ARTIFACT_ROOT}/{build_name}/{name}"
        for build_name in ("build-a", "build-b")
        for name in records
    }


def require_safe_member(item: zipfile.ZipInfo) -> None:
    member = PurePosixPath(item.filename)
    item_mode = (item.external_attr >> 16) & 0o177777
    if member.is_absolute() or ".." in member.parts or "" in member.parts:
        raise ValueError(f"unsafe path: {item.filename}")
    if item_mode & 0o170000 == 0o120000:
        raise ValueError(f"symlink: {item.filename}")
    if item_mode & 0o170000 not in {0, 0o100000, 0o040000}:
        raise ValueError(f"special file: {item.filename}")
    if member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
        raise ValueError(f"forbidden root: {item.filename}")
    if member.name in SECRET_MEMBER_NAMES:
        raise ValueError(f"private ceremony member: {item.filename}")
    lower = member.name.lower()
    if lower.endswith((".age", ".agekey", ".pem", ".key", ".sqlite", ".sqlite3", ".log")):
        raise ValueError(f"secret/runtime artifact: {item.filename}")
    if ".env" in member.parts:
        raise ValueError(f"environment artifact: {item.filename}")


def check(path: str) -> dict[str, object]:
    archive_path = Path(path)
    with zipfile.ZipFile(archive_path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        if len(names) != len(set(names)):
            raise ValueError("duplicate members")
        members = {item.filename: item for item in infos}
        for item in infos:
            require_safe_member(item)
        required = GENERATED_BASE | {AUTHORITY, BUILD, REHEARSAL}
        if missing := required - set(names):
            raise ValueError(f"missing members: {sorted(missing)}")

        marker = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        expected_marker_keys = {
            "source_short_ref",
            "source_ref",
            "source_tree",
            "branch",
            "accepted_r0_ref",
            "build_source_ref",
            "build_source_tree",
            "evidence_source_ref",
            "evidence_source_tree",
            "archive_name",
        }
        if set(marker) != expected_marker_keys:
            raise ValueError("handoff marker schema drift")
        evidence_bytes = archive.read(EVIDENCE)
        evidence = json.loads(evidence_bytes)
        authority_bytes = archive.read(AUTHORITY)
        authority = json.loads(authority_bytes)
        build_bytes = archive.read(BUILD)
        build = json.loads(build_bytes)
        rehearsal_bytes = archive.read(REHEARSAL)
        rehearsal = json.loads(rehearsal_bytes)
        manifest_bytes = archive.read(MANIFEST)
        manifest = json.loads(manifest_bytes)

        source_ref = marker.get("source_ref")
        source_tree = marker.get("source_tree")
        if not source_ref or HEX40.fullmatch(source_ref) is None:
            raise ValueError("source ref grammar")
        if not source_tree or HEX40.fullmatch(source_tree) is None:
            raise ValueError("source tree grammar")
        if marker.get("source_short_ref") != source_ref[:7]:
            raise ValueError("source short-ref mismatch")
        if evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if evidence.get("source_tree") != source_tree or marker.get("source_tree") != source_tree:
            raise ValueError("source-tree binding mismatch")
        if marker.get("archive_name") != archive_path.name or evidence.get("archive_name") != archive_path.name:
            raise ValueError("archive-name binding mismatch")
        if marker.get("branch") != BRANCH or evidence.get("branch") != BRANCH:
            raise ValueError("branch binding mismatch")
        expected_marker = {
            "accepted_r0_ref": ACCEPTED_R0,
            "build_source_ref": SOURCE_FOUNDATION,
            "build_source_tree": SOURCE_FOUNDATION_TREE,
            "evidence_source_ref": EVIDENCE_SOURCE_REF,
            "evidence_source_tree": EVIDENCE_SOURCE_TREE,
        }
        for key, value in expected_marker.items():
            if marker.get(key) != value or evidence.get(key) != value:
                raise ValueError(f"handoff lineage drift: {key}")

        expected_evidence_keys = {
            "schema_version",
            "stage",
            "source_ref",
            "source_tree",
            "source_short_ref",
            "archive_name",
            "branch",
            "accepted_r0_ref",
            "build_source_ref",
            "build_source_tree",
            "evidence_source_ref",
            "evidence_source_tree",
            "manifest_sha256",
            "gate_sha256",
            "authority_sha256",
            "build_evidence_sha256",
            "rehearsal_evidence_sha256",
            "binary_artifact_count",
            "production_binaries_rebuilt_in_r0_r1",
            "request_oracle",
            "request_ordinal",
            "request_method",
            "request_route_template",
            "request_error_category",
            "category_only_oracle",
            "generation",
            "generation_2_active",
            "production_credentials_installed",
            "controlled_installation",
            "authorization",
            "finam_endpoint_called",
            "container_residue_count",
            "private_ceremony_members",
            "review_status",
        }
        if set(evidence) != expected_evidence_keys:
            raise ValueError("handoff evidence schema drift")
        if evidence.get("schema_version") != 1 or evidence.get("stage") != STAGE:
            raise ValueError("handoff evidence stage drift")
        if evidence.get("authority_sha256") != sha256(authority_bytes):
            raise ValueError("aggregate authority digest mismatch")
        if evidence.get("build_evidence_sha256") != sha256(build_bytes):
            raise ValueError("build evidence digest mismatch")
        if evidence.get("rehearsal_evidence_sha256") != sha256(rehearsal_bytes):
            raise ValueError("rehearsal evidence digest mismatch")
        if authority.get("status") != "INDEPENDENT_REVIEW_REQUIRED" or evidence.get("review_status") != "INDEPENDENT_REVIEW_REQUIRED":
            raise ValueError("review status drift")
        if evidence.get("production_binaries_rebuilt_in_r0_r1") is not False:
            raise ValueError("production rebuild claimed")
        if evidence.get("generation") != 2 or evidence.get("generation_2_active") is not False:
            raise ValueError("generation activation drift")
        if evidence.get("production_credentials_installed") is not False or evidence.get("controlled_installation") is not False:
            raise ValueError("installation boundary opened")
        if evidence.get("authorization") != "NOT_ISSUED" or evidence.get("finam_endpoint_called") is not False:
            raise ValueError("authorization/FINAM boundary opened")
        if evidence.get("container_residue_count") != 0 or evidence.get("private_ceremony_members") != 0:
            raise ValueError("runtime/private residue evidence drift")

        proof = rehearsal.get("request_boundary_proof")
        if not isinstance(proof, dict):
            raise ValueError("typed request proof missing")
        error = stage_check.check_request_proof(proof)
        expected_request = {
            "request_oracle": stage_check.ORACLE_ID,
            "request_ordinal": 1,
            "request_method": "POST",
            "request_route_template": "/v1/sessions",
            "request_error_category": error,
            "category_only_oracle": False,
        }
        for key, value in expected_request.items():
            if evidence.get(key) != value:
                raise ValueError(f"request-boundary handoff drift: {key}")

        expected_binary_members = binary_members(build)
        if evidence.get("binary_artifact_count") != len(expected_binary_members):
            raise ValueError("binary artifact count drift")
        records = build.get("binaries")
        if not isinstance(records, dict):
            raise ValueError("build binary inventory shape drift")
        for name, record in records.items():
            if not isinstance(record, dict):
                raise ValueError(f"binary record shape: {name}")
            for build_name, hash_key in (("build-a", "build_a_sha256"), ("build-b", "build_b_sha256")):
                member_name = f"{ARTIFACT_ROOT}/{build_name}/{name}"
                if member_name not in members:
                    raise ValueError(f"binary member missing: {member_name}")
                data = archive.read(member_name)
                expected_hash = record.get(hash_key)
                if not isinstance(expected_hash, str) or HEX64.fullmatch(expected_hash) is None:
                    raise ValueError(f"binary evidence hash grammar: {member_name}")
                if sha256(data) != expected_hash or not data.startswith(b"\x7fELF"):
                    raise ValueError(f"binary artifact drift: {member_name}")
                if mode(members[member_name]) != "100755":
                    raise ValueError(f"binary mode drift: {member_name}")

        gate = archive.read(GATE)
        gate_marker = b"stage8b-generation2-composition-r0-r1-gate: PASS"
        if gate_marker not in gate or evidence.get("gate_sha256") != sha256(gate):
            raise ValueError("gate evidence mismatch")
        if evidence.get("manifest_sha256") != sha256(manifest_bytes):
            raise ValueError("manifest digest mismatch")
        tracked: set[str] = set()
        entries = manifest.get("entries")
        if set(manifest) != {"schema_version", "source_ref", "entry_count", "entries"}:
            raise ValueError("manifest schema drift")
        if manifest.get("schema_version") != 2:
            raise ValueError("manifest version drift")
        if not isinstance(entries, list) or manifest.get("entry_count") != len(entries):
            raise ValueError("manifest inventory drift")
        for entry in entries:
            if not isinstance(entry, dict) or set(entry) != {"mode", "path", "sha256", "size"}:
                raise ValueError("manifest entry schema drift")
            name = entry["path"]
            if name in tracked or name not in members:
                raise ValueError(f"source member mismatch: {name}")
            tracked.add(name)
            data = archive.read(name)
            if len(data) != entry["size"] or sha256(data) != entry["sha256"]:
                raise ValueError(f"source content mismatch: {name}")
            if mode(members[name]) != entry["mode"]:
                raise ValueError(f"source mode mismatch: {name}")
        expected_generated = GENERATED_BASE | expected_binary_members
        if set(names) - tracked != expected_generated:
            raise ValueError("generated member inventory mismatch")

        with tempfile.TemporaryDirectory(prefix="stage8b-g2-r0-r1-handoff-") as temporary:
            extracted = Path(temporary)
            archive.extractall(extracted)
            stage_check.check(extracted, extracted / ARTIFACT_ROOT)

        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "generated_members_verified": len(expected_generated),
            "binary_artifacts_verified": len(expected_binary_members),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "private_ceremony_members": 0,
            "source_ref": source_ref,
            "evidence_source_ref": EVIDENCE_SOURCE_REF,
            "build_source_ref": SOURCE_FOUNDATION,
            "request": f"POST:/v1/sessions:1:{error}",
            "category_only_oracle": False,
            "generation_2_active": False,
            "authorization": "NOT_ISSUED",
            "finam_endpoint_called": False,
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p_r2b_generation2_composition_r0_r1_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (AssertionError, KeyError, OSError, RuntimeError, ValueError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-generation2-composition-r0-r1-handoff-safety: FAIL {error}") from error
    print("stage8b-generation2-composition-r0-r1-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
