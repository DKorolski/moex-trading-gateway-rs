#!/usr/bin/env python3
"""Validate the immutable Stage 8B-P1-f R4 design-correction handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

import stage8b_p1e_i1a_handoff_safety_check as common
import stage8b_p1f_r4_design_check as design


PARENT = design.BASE
BRANCH = "stage8b-paper-shadow-resumption"
STAGE = "Stage 8B-P1-f R4 isolated operational acceptance design correction"
MARKER = "handoff-commit.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
COMMIT_RAW = "handoff-evidence/source-commit.raw"
EVIDENCE = "handoff-evidence/stage8b-p1f-r4-design-evidence.json"
GATE_LOG = "handoff-evidence/stage8b-p1f-r4-design-gate.log"
REVIEW = "handoff-evidence/reviews/FINAM_P1F_R3_DESIGN_REVIEW_811ebe8_2026-09-25.md"
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, GATE_LOG, REVIEW}
EVIDENCE_KEYS = {
    "schema_version", "stage", "status", "source_ref", "source_parent", "source_tree",
    "branch", "archive_name", "manifest_sha256", "changed_paths", "correction_parent",
    "r3_review_sha256", "accepted_i1_predecessor", "target_baseline_sha256",
    "matrix_sha256", "models_sha256", "finding_closed", "findings_preserved",
    "rollback_model", "production_source_changed", "remote_mutation_performed",
    "p1f_source_implementation_authorized", "operational_activation_authorized",
    "closed_surfaces", "gate_log_sha256", "next_after_acceptance",
    "redis_script_hashes", "proof_counts",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def parse_marker(raw: bytes) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in raw.decode().splitlines():
        require("=" in line, "invalid marker line")
        key, value = line.split("=", 1)
        require(bool(key) and bool(value) and key not in values, "invalid marker field")
        values[key] = value
    require(set(values) == {"stage", "source_short_ref", "source_ref", "source_parent",
                            "source_tree", "branch", "archive_name"},
            "marker inventory drift")
    return values


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        by_name = {item.filename: item for item in infos}
        require(len(names) == len(set(names)), "duplicate archive member")
        require(not (GENERATED - set(names)), "generated member missing")
        for item in infos:
            common.validate_member_name(item.filename)
            mode = (item.external_attr >> 16) & 0o177777
            require(mode & 0o170000 != 0o120000, f"symlink member: {item.filename}")
            require(mode & 0o170000 in {0, 0o100000}, f"special member: {item.filename}")
            parts = PurePosixPath(item.filename).parts
            require(not any(part == ".env" for part in parts), f"secret member: {item.filename}")
            require(not item.filename.endswith((".pem", ".key", ".ed25519", ".sqlite", ".sqlite3")),
                    f"secret/runtime member: {item.filename}")

        files = {name: archive.read(name) for name in names}
        marker = parse_marker(files[MARKER])
        require(marker["stage"] == STAGE and marker["source_parent"] == PARENT,
                "stage or parent mismatch")
        require(marker["branch"] == BRANCH and marker["archive_name"] == Path(path).name,
                "branch or archive mismatch")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")
        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"],
                "commit hash mismatch")
        lines = commit_raw.decode().splitlines()
        require(lines[0] == f"tree {marker['source_tree']}" and f"parent {PARENT}" in lines,
                "commit lineage mismatch")

        manifest = json.loads(files[MANIFEST], object_pairs_hook=design.r3.r2.strict_object)
        require(manifest["schema_version"] == 2
                and manifest["source_ref"] == marker["source_ref"], "manifest identity mismatch")
        entries = manifest["entries"]
        require(manifest["entry_count"] == len(entries), "manifest count mismatch")
        tracked: set[str] = set()
        payloads: dict[str, bytes] = {}
        for entry in entries:
            name = entry["path"]
            require(name not in tracked and name in files, f"tracked member mismatch: {name}")
            raw = files[name]
            mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
            require(entry["size"] == len(raw) and entry["sha256"] == sha256(raw)
                    and entry["mode"] == mode, f"tracked identity mismatch: {name}")
            tracked.add(name)
            payloads[name] = raw
        require(set(names) - tracked == GENERATED, "generated inventory mismatch")
        require(common.build_tree_oid(entries, payloads) == marker["source_tree"],
                "tree reconstruction mismatch")

        evidence = json.loads(files[EVIDENCE], object_pairs_hook=design.r3.r2.strict_object)
        require(set(evidence) == EVIDENCE_KEYS, "evidence key inventory drift")
        require(evidence["schema_version"] == 1 and evidence["stage"] == STAGE
                and evidence["status"] == "DESIGN_CORRECTION_REVIEW_CANDIDATE_NO_ACTIVATION",
                "evidence identity drift")
        require(evidence["source_ref"] == marker["source_ref"]
                and evidence["source_parent"] == PARENT
                and evidence["source_tree"] == marker["source_tree"], "evidence source mismatch")
        require(evidence["branch"] == BRANCH and evidence["archive_name"] == marker["archive_name"],
                "evidence archive binding drift")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        require(set(evidence["changed_paths"]) == design.ALLOWED_CHANGES, "changed paths drift")
        require(evidence["correction_parent"] == PARENT
                and evidence["r3_review_sha256"] == design.REVIEW_SHA256
                and sha256(files[REVIEW]) == design.REVIEW_SHA256, "R3 review binding drift")
        require(evidence["accepted_i1_predecessor"]
                == "3f171d997de5616cb9a07311d7776e446456c0c1", "I1 predecessor drift")
        require(evidence["target_baseline_sha256"] == design.r3.r2.r1.r0.TARGET_SHA256,
                "target baseline drift")
        require(evidence["matrix_sha256"] == design.MATRIX_SHA256
                and evidence["models_sha256"] == design.MODELS_SHA256, "design digest drift")
        require(evidence["finding_closed"] == "P1-F07 consumed-history rollback"
                and evidence["findings_preserved"] == ["P1-F02", "P1-F05", "P1-F06"],
                "finding closure inventory drift")
        require(evidence["rollback_model"] == "trusted-control-root with explicit coherent-rollback limit and generation rebind",
                "rollback model evidence drift")
        require(evidence["production_source_changed"] is False
                and evidence["remote_mutation_performed"] is False, "design performed effects")
        require(evidence["p1f_source_implementation_authorized"] is False
                and evidence["operational_activation_authorized"] is False,
                "design self-authorized")
        require(set(evidence["closed_surfaces"]) == design.r3.r2.r1.CLOSED_SURFACES
                and all(value is False for value in evidence["closed_surfaces"].values()),
                "closed surface drift")
        require(evidence["next_after_acceptance"] == "P1F-I source implementation only",
                "next boundary drift")
        require(evidence["redis_script_hashes"] == design.r3.r2.SCRIPT_HASHES,
                "Redis script evidence drift")
        require(evidence["proof_counts"] == {
            "matrix_rows": 78, "model_cases": 45, "artifacts": 9, "redis_scripts": 8,
            "source_operations": 10, "public_operation_traces": 2,
            "schedule_routes": 6, "redis_roles": 8, "negative_mutations": 43,
        }, "proof count drift")

        gate = files[GATE_LOG]
        require(evidence["gate_log_sha256"] == sha256(gate), "gate digest mismatch")
        for token in (
            b"stage8b-p1f-r4-design-check: PASS rows=78 models=45 artifacts=9 scripts=8 operations=10 traces=2 routes=6 roles=8 phases=7 activation=false",
            b"stage8b-p1f-r4-design-negative-harness: PASS 43/43 no_op=0",
            b"stage8b-p1f-r3-design-handoff-safety: PASS",
            b"PASS stage8b-p1f-r4-design-gate rows=78 models=45 artifacts=9 scripts=8 operations=10 traces=2 routes=6 roles=8 negatives=43 remote_mutation=false activation=false",
            b"exit_code=0",
        ):
            require(token in gate, f"gate marker missing: {token!r}")

        with tempfile.TemporaryDirectory(prefix="stage8b-p1f-r4-design-") as directory:
            root = Path(directory) / "source"
            for name, raw in payloads.items():
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(raw)
            design.validate(root, verify_lineage=False)

        return {
            "archive_members": len(names), "tracked_members_verified": len(tracked),
            "review_verified": True, "duplicates": 0, "symlinks": 0, "unsafe_paths": 0,
            "source_ref": marker["source_ref"], "source_parent": PARENT,
            "source_tree": marker["source_tree"], "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1f_r4_design_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, UnicodeDecodeError, ValueError, KeyError, TypeError,
            zipfile.BadZipFile, json.JSONDecodeError, design.CheckFailure,
            design.r3.CheckFailure, design.r3.r2.CheckFailure,
            design.r3.r2.r1.CheckFailure, design.r3.r2.r1.r0.CheckFailure) as error:
        print(f"stage8b-p1f-r4-design-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1f-r4-design-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
