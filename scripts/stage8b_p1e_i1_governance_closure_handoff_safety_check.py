#!/usr/bin/env python3
"""Validate the immutable Stage 8B-P1-e I1 governance-closure handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

import stage8b_p1e_i1_governance_closure_check as closure
import stage8b_p1e_i1a_handoff_safety_check as common


PARENT = closure.BASE
BRANCH = "stage8b-paper-shadow-resumption"
STAGE = "Stage 8B-P1-e I1 governance closure"
MARKER = "handoff-commit.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
COMMIT_RAW = "handoff-evidence/source-commit.raw"
EVIDENCE = "handoff-evidence/stage8b-p1e-i1-governance-closure-evidence.json"
GATE_LOG = "handoff-evidence/stage8b-p1e-i1-governance-closure-gate.log"
REVIEW = "handoff-evidence/reviews/FINAM_I1_AGGREGATE_ACCEPTANCE_a9bcd94_2026-09-24.md"
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, GATE_LOG, REVIEW}
EVIDENCE_KEYS = {
    "schema_version",
    "stage",
    "status",
    "source_ref",
    "source_parent",
    "source_tree",
    "branch",
    "archive_name",
    "manifest_sha256",
    "changed_paths",
    "accepted_candidate",
    "review_sha256",
    "production_source_changed",
    "i1_closed",
    "p1f_design_authorized",
    "p1f_implementation_authorized",
    "p1f_operational_activation_authorized",
    "closed_surfaces",
    "gate_log_sha256",
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
    require(
        set(values)
        == {"stage", "source_short_ref", "source_ref", "source_parent", "source_tree", "branch", "archive_name"},
        "marker inventory drift",
    )
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
            require(
                not item.filename.endswith((".pem", ".key", ".ed25519", ".sqlite", ".sqlite3")),
                f"secret/runtime member: {item.filename}",
            )

        files = {name: archive.read(name) for name in names}
        marker = parse_marker(files[MARKER])
        require(marker["stage"] == STAGE, "stage mismatch")
        require(marker["source_parent"] == PARENT, "parent mismatch")
        require(marker["branch"] == BRANCH, "branch mismatch")
        require(marker["archive_name"] == Path(path).name, "archive name mismatch")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit hash mismatch")
        commit_lines = commit_raw.decode().splitlines()
        require(commit_lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {PARENT}" in commit_lines, "commit parent mismatch")

        manifest = json.loads(files[MANIFEST], object_pairs_hook=closure.strict_object)
        require(manifest["schema_version"] == 2, "manifest schema mismatch")
        require(manifest["source_ref"] == marker["source_ref"], "manifest source mismatch")
        entries = manifest["entries"]
        require(manifest["entry_count"] == len(entries), "manifest count mismatch")
        tracked: set[str] = set()
        payloads: dict[str, bytes] = {}
        for entry in entries:
            name = entry["path"]
            require(name not in tracked and name in files, f"tracked member mismatch: {name}")
            raw = files[name]
            mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
            require(entry["size"] == len(raw), f"size mismatch: {name}")
            require(entry["sha256"] == sha256(raw), f"digest mismatch: {name}")
            require(entry["mode"] == mode, f"mode mismatch: {name}")
            tracked.add(name)
            payloads[name] = raw
        require(set(names) - tracked == GENERATED, "generated inventory mismatch")
        require(common.build_tree_oid(entries, payloads) == marker["source_tree"], "tree reconstruction mismatch")

        evidence = json.loads(files[EVIDENCE], object_pairs_hook=closure.strict_object)
        require(set(evidence) == EVIDENCE_KEYS, "evidence key inventory drift")
        require(evidence["schema_version"] == 1 and evidence["stage"] == STAGE, "evidence identity drift")
        require(evidence["status"] == "CLOSED_ACCEPTED", "evidence status drift")
        require(evidence["source_ref"] == marker["source_ref"], "evidence source mismatch")
        require(evidence["source_parent"] == PARENT, "evidence parent mismatch")
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree mismatch")
        require(evidence["branch"] == BRANCH, "evidence branch mismatch")
        require(evidence["archive_name"] == marker["archive_name"], "evidence archive mismatch")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        require(set(evidence["changed_paths"]) == closure.ALLOWED_CHANGES, "changed paths drift")
        require(evidence["accepted_candidate"] == closure.ACCEPTED_CANDIDATE, "candidate binding drift")
        require(evidence["review_sha256"] == closure.REVIEW["sha256"], "review binding drift")
        require(sha256(files[REVIEW]) == closure.REVIEW["sha256"], "review bytes drift")
        require(evidence["production_source_changed"] is False, "production source opened")
        require(evidence["i1_closed"] is True, "I1 closure missing")
        require(evidence["p1f_design_authorized"] is True, "P1-f design not authorized")
        require(evidence["p1f_implementation_authorized"] is False, "P1-f implementation opened")
        require(evidence["p1f_operational_activation_authorized"] is False, "P1-f activation opened")
        require(set(evidence["closed_surfaces"]) == closure.CLOSED_SURFACES, "closed surface inventory drift")
        require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")

        gate = files[GATE_LOG]
        require(evidence["gate_log_sha256"] == sha256(gate), "gate digest mismatch")
        for token in (
            b"stage8b-p1e-i1-governance-closure-check: PASS rows=14 closed_surfaces=9 p1f_design=true activation=false",
            b"stage8b-p1e-i1-governance-closure-negative-harness: PASS 14/14",
            b"stage8b-p1e-i1-aggregate-acceptance-handoff-safety: PASS",
            b"PASS stage8b-p1e-i1-governance-closure-gate rows=14 negatives=14 production_change=false p1f_design=true activation=false",
            b"exit_code=0",
        ):
            require(token in gate, f"gate marker missing: {token!r}")

        with tempfile.TemporaryDirectory(prefix="stage8b-i1-governance-closure-") as directory:
            root = Path(directory) / "source"
            for name, raw in payloads.items():
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(raw)
            closure.validate(root, verify_lineage=False)

        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "review_verified": True,
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": marker["source_ref"],
            "source_parent": PARENT,
            "source_tree": marker["source_tree"],
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1e_i1_governance_closure_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (
        OSError,
        UnicodeDecodeError,
        ValueError,
        KeyError,
        TypeError,
        zipfile.BadZipFile,
        json.JSONDecodeError,
        closure.CheckFailure,
    ) as error:
        print(f"stage8b-p1e-i1-governance-closure-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1e-i1-governance-closure-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
