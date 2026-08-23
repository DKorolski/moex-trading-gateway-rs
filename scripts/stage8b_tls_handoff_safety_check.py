#!/usr/bin/env python3
"""Validate Stage 8B controlled TLS qualification handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

EVIDENCE = "handoff-evidence/stage8b-tls-evidence.json"
GATE = "handoff-evidence/stage8b-tls-gate.txt"
GRAPH = "handoff-evidence/stage8b-tls-qualification-graph.json"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
REQUIRED = {
    "handoff-commit.txt",
    EVIDENCE,
    GATE,
    GRAPH,
    MANIFEST,
    "crates/finam-gateway/src/stage8b_no_send/stage8b_adapter.rs",
    "crates/finam-gateway/src/stage8b_no_send/stage8b_tls_qualification.rs",
    "docs/stage-8/STAGE8B_TLS_QUALIFICATION_2026-08-23.md",
    "docs/stage-8/STAGE8B_TLS_ACCEPTANCE_MATRIX_2026-08-23.csv",
    "docs/stage-8/STAGE8B_TLS_NEGATIVE_INVENTORY_2026-08-23.md",
    "docs/stage-8/stage8b-tls-qualification-authority.json",
    "scripts/stage8b_tls_qualification_check.py",
    "scripts/stage8b_tls_negative_harness.py",
    "scripts/stage8b_tls_graph_evidence.py",
    "scripts/stage8b_tls_predecessor_replay.sh",
    "scripts/stage8b_tls_gate.sh",
    "scripts/make_stage8b_tls_handoff.py",
    "scripts/stage8b_tls_handoff_safety_check.py",
}
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, GRAPH, MANIFEST}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def archive_mode(info: zipfile.ZipInfo) -> str:
    return f"{(info.external_attr >> 16) & 0o177777:06o}"


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [info.filename for info in infos]
        info_by_name = {info.filename: info for info in infos}
        if len(names) != len(set(names)):
            raise ValueError("duplicate members")
        missing = REQUIRED - set(names)
        if missing:
            raise ValueError(f"missing members: {sorted(missing)}")
        for info in infos:
            member = PurePosixPath(info.filename)
            mode = (info.external_attr >> 16) & 0o177777
            if member.is_absolute() or ".." in member.parts or "" in member.parts:
                raise ValueError(f"unsafe path: {info.filename}")
            if mode & 0o170000 == 0o120000:
                raise ValueError(f"symlink: {info.filename}")
            if mode & 0o170000 not in {0, 0o100000, 0o040000}:
                raise ValueError(f"special file: {info.filename}")
            if member.parts and member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden root: {info.filename}")
            if any(part == ".env" for part in member.parts):
                raise ValueError(f"secret path: {info.filename}")
            if info.filename.endswith((".log", ".sqlite", ".sqlite3", ".pem", ".key")):
                raise ValueError(f"runtime or key artifact: {info.filename}")

        marker = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        graph = json.loads(archive.read(GRAPH))
        manifest = json.loads(archive.read(MANIFEST))
        if evidence.get("stage") != "8B-IT-TLS" or evidence.get("revision") != "R1":
            raise ValueError("stage/revision mismatch")
        source_ref = marker.get("source_ref")
        if not source_ref or any(
            item.get("source_ref") != source_ref for item in (evidence, graph, manifest)
        ):
            raise ValueError("source mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive mismatch")
        if evidence.get("acceptance_rows") != 50 or evidence.get("negative_mutations") != 40:
            raise ValueError("matrix count mismatch")
        for key in (
            "accepted_it_r3_replayed",
            "controlled_tls_qualified",
            "valid_ca_hostname",
            "http2_alpn_negotiated",
            "wrong_ca_rejected",
            "wrong_hostname_rejected",
            "expired_rejected",
            "not_yet_valid_rejected",
            "timeout_classified",
            "response_loss_classified",
            "automatic_retry_disabled",
            "native_tls_absent",
            "canonical_full_regression",
        ):
            if evidence.get(key) is not True:
                raise ValueError(f"qualification evidence missing: {key}")
        for key in (
            "production_endpoint_authority",
            "finam_credentials",
            "real_finam_effect",
            "redis_execution",
            "broker_dispatch",
            "runtime_live",
            "stage8b_p",
            "stage8b_xe",
            "stage12",
        ):
            if evidence.get(key) is not False:
                raise ValueError(f"closed surface opened: {key}")
        gate = archive.read(GATE)
        if b"stage8b-tls-gate: PASS revision=R1 rows=50 negatives=40" not in gate:
            raise ValueError("TLS gate marker missing")
        if f"current-tree-ci-gate: PASS source_ref={source_ref} ".encode() not in gate:
            raise ValueError("gate is not exact-commit bound")
        if b"stage8b-tls-predecessor-replay: PASS accepted_ref=14e01a9f" not in gate:
            raise ValueError("accepted IT-R3 replay missing")
        if b"stage8b-i-full-regression: PASS canonical_ci=true" not in gate:
            raise ValueError("canonical full regression missing")
        if graph.get("native_tls_present") is not False or graph.get("crypto_provider") != "ring":
            raise ValueError("TLS graph identity mismatch")
        if (
            sha256(gate) != evidence.get("gate_sha256")
            or sha256(archive.read(GRAPH)) != evidence.get("graph_evidence_sha256")
            or sha256(archive.read(MANIFEST)) != evidence.get("manifest_sha256")
        ):
            raise ValueError("generated evidence hash mismatch")

        entries = manifest.get("entries", [])
        if manifest.get("entry_count") != len(entries):
            raise ValueError("manifest count mismatch")
        tracked: set[str] = set()
        for entry in entries:
            name = entry["path"]
            if name in tracked:
                raise ValueError(f"duplicate manifest path: {name}")
            tracked.add(name)
            info = info_by_name.get(name)
            if info is None:
                raise ValueError(f"missing tracked member: {name}")
            data = archive.read(name)
            if (
                len(data) != entry["size"]
                or sha256(data) != entry["sha256"]
                or archive_mode(info) != entry["mode"]
            ):
                raise ValueError(f"manifest mismatch: {name}")
        if set(names) - tracked != GENERATED:
            raise ValueError("unexpected generated member inventory")
        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": source_ref,
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_tls_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-tls-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-tls-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
