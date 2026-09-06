#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P1-d4 R7 design handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p1d4-r7-design-evidence.json"
GATE = "handoff-evidence/stage8b-p1d4-r7-design-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
R6_REF = "cb6e6ddf863f314cc96b5f8ac0a75809e8c6824a"
HASHES = {
    "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv": "e2a376fda504a691b3dffa5160542c5f184bf59ff9f224ae4159b5bbaf8c06de",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv": "f4125113d4f7f3ceafd849c1d32f1097c313e15eccbb353c8bd89a19434edac6",
    "docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v3.csv": "99b30f93c2c7b3c281f6f96c7eacab677e45b32bd89ec81d1847259cb39d514e",
    "docs/stage-8/stage8b-p1d4-r7-acceptance-amendment.csv": "6cd2525bd748b913e7a90e1d0448a4f9f86141c3438031832edbfd66e6d44ebf",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r7.md": "43f0abd00dc3a7b8f0805b4d06c4f32b23e45c6ac749696a86f92a1612152d6d",
    "docs/stage-8/stage8b-p1d4-source-discovery-r7.md": "1c924ee40394de568daf439ee1e77286e81fad9279193ec4b30414c9064a4371",
    "docs/stage-8/stage8b-p1d4-source-shape-r7.json": "9ba83b6d677e12520ab1f783cc2f557ff1c9aaaee636540651345045f432ba01",
    "docs/stage-8/fixtures/stage8b-p1d4-command-publication-binding-v1.json": "5f3f684b6d752199d8c82778c6b56ab9e282b22ad8f130d88dcd6d445923159f",
}
SOURCE_HASHES = {
    "crates/runtime-durable-service/src/recovery.rs": "70cf38671834d81e84b55b90903d1fca26ce7f79f598e92462bd1a62ec0d5284",
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs": "17edbe4c41315aef97faa8574eedd884e75ad90cb70e9aa01812bc6e197a7295",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs": "ef113596c8f9835669987853ea1ce4bdb976c73d507cf8014dabcff23a923ef8",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs": "619a6f7e4aff4d2d6ba2e0c86036da7df96383e3de020afbb6eaa3f907cb8478",
    "crates/strategy-runtime-core/src/stage8b_p1d1_paper_provider.rs": "2e9630533aa0470ad73294fb0afff6e155bf0a738eba2dae51eca9d0f7387a2b",
    "crates/strategy-runtime-core/src/stage8b_p1d2_market_feedback.rs": "376b0af099cdb95a31c48a4ab1b23272aa34dc436b19dd61898ddc078bde6480",
}
REQUIRED = GENERATED | set(HASHES) | set(SOURCE_HASHES) | {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r6.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-evidence-r7.json",
    "scripts/make_stage8b_p1d4_r7_design_handoff.py",
    "scripts/stage8b_p1d4_r7_design_check.py",
    "scripts/stage8b_p1d4_r7_design_gate.sh",
    "scripts/stage8b_p1d4_r7_design_handoff_safety_check.py",
    "scripts/stage8b_p1d4_r7_design_negative_harness.py",
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def archive_mode(info: zipfile.ZipInfo) -> str:
    return f"{(info.external_attr >> 16) & 0o177777:06o}"


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [info.filename for info in infos]
        by_name = {info.filename: info for info in infos}
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
            if info.filename.endswith((".log", ".sqlite", ".sqlite3", ".pem", ".key", ".ed25519")):
                raise ValueError(f"runtime/key artifact: {info.filename}")

        marker = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        manifest = json.loads(archive.read(MANIFEST))
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("parent_ref") != R6_REF or evidence.get("parent_ref") != R6_REF:
            raise ValueError("R6 parent mismatch")
        if marker.get("source_tree") != evidence.get("source_tree"):
            raise ValueError("source tree mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if marker.get("branch") != "stage8b-paper-shadow-resumption":
            raise ValueError("source branch mismatch")

        if evidence.get("stage") != "Stage 8B-P1-d4 crash/replay design R7 correction":
            raise ValueError("stage mismatch")
        if evidence.get("status") != "DESIGN_R7_REVIEW_CANDIDATE":
            raise ValueError("candidate status mismatch")
        if evidence.get("active_positive_cells") != 105 or evidence.get("acceptance_amendment_rows") != 40:
            raise ValueError("proof inventory mismatch")
        if evidence.get("targeted_negative_cases") != 60 or evidence.get("total_contract_negative_cases") != 188:
            raise ValueError("negative inventory mismatch")
        if evidence.get("design_only") is not True or evidence.get("implementation_authorized") is not False:
            raise ValueError("design boundary opened")
        if evidence.get("source_wip_included") is not False:
            raise ValueError("source WIP entered package")
        if any(value is not False for value in evidence.get("closed_surfaces", {}).values()):
            raise ValueError("closed surface opened")
        reservation = evidence.get("reservation_contract", {})
        if reservation != {
            "dynamic_repick_allowed": False,
            "explicit_xadd_id_required": True,
            "predecessor_source": "XINFO_STREAM_last-generated-id",
            "reserved_id_rule": "checked_immediate_successor",
            "single_writer_required": True,
            "xadd_star_allowed": False,
        }:
            raise ValueError("reservation contract drifted")
        routing = evidence.get("routing_contract", {})
        if routing.get("present_invalid") != "hard_conflict_no_fallback":
            raise ValueError("invalid composite fallback opened")
        if routing.get("present_valid") != "p1d4_v1_before_ordinary_p1d2":
            raise ValueError("package-aware precedence drifted")
        if routing.get("absent") != "ordinary_p1d2_then_generic_p1":
            raise ValueError("standalone P1-d2 route drifted")
        if evidence.get("generation_contract") != {
            "prepublication": "W0_G0",
            "s_ack": "W0_plus_1_G0_plus_1",
            "s_truth": "W0_plus_2_G0_plus_2",
            "successor_seal_substitution_allowed": False,
        }:
            raise ValueError("generation contract drifted")

        for name, expected in HASHES.items():
            if sha256(archive.read(name)) != expected:
                raise ValueError(f"content hash mismatch: {name}")
        for name, expected in SOURCE_HASHES.items():
            if sha256(archive.read(name)) != expected:
                raise ValueError(f"committed source-shape mismatch: {name}")
        verification = evidence.get("verification", {})
        if not verification or any(value != "PASS" for value in verification.values()):
            raise ValueError("verification incomplete")
        gate = archive.read(GATE)
        for expected in (
            b"PASS stage8b-p1d4-r7-design-scope",
            b"PASS stage8b-p1d4-r7-design-negative-harness 60/60",
            b"PASS stage8b-p1d4-r7-matrices",
            b"PASS stage8b-p1d4-r7-design-gate",
        ):
            if expected not in gate:
                raise ValueError(f"gate marker missing: {expected!r}")
        if sha256(gate) != evidence.get("gate_sha256"):
            raise ValueError("gate digest mismatch")
        if sha256(archive.read(MANIFEST)) != evidence.get("manifest_sha256"):
            raise ValueError("manifest digest mismatch")

        entries = manifest.get("entries", [])
        if manifest.get("entry_count") != len(entries):
            raise ValueError("manifest count mismatch")
        tracked: set[str] = set()
        for entry in entries:
            name = entry["path"]
            if name in tracked or name not in by_name:
                raise ValueError(f"manifest member mismatch: {name}")
            tracked.add(name)
            data = archive.read(name)
            if len(data) != entry["size"] or sha256(data) != entry["sha256"] or archive_mode(by_name[name]) != entry["mode"]:
                raise ValueError(f"manifest content mismatch: {name}")
        if set(names) - tracked != GENERATED:
            raise ValueError("generated member inventory mismatch")
        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": source_ref,
            "stage": "Stage 8B-P1-d4 R7 design correction",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1d4_r7_design_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1d4-r7-design-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-p1d4-r7-design-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
