#!/usr/bin/env python3
"""Validate the immutable Stage 8B-P1-e I1 aggregate handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

import stage8b_p1e_i1_aggregate_acceptance_check as aggregate
import stage8b_p1e_i1_aggregate_readiness_handoff_safety_check as readiness_safety
import stage8b_p1e_i1_fixed_install_handoff_safety_check as fixed_safety
import stage8b_p1e_i1a_handoff_safety_check as common


PARENT = aggregate.BASE
BRANCH = "stage8b-paper-shadow-resumption"
STAGE = "Stage 8B-P1-e I1 aggregate acceptance candidate"
MARKER = "handoff-commit.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
COMMIT_RAW = "handoff-evidence/source-commit.raw"
EVIDENCE = "handoff-evidence/stage8b-p1e-i1-aggregate-acceptance-evidence.json"
GATE_LOG = "handoff-evidence/stage8b-p1e-i1-aggregate-acceptance-gate.log"
REPORT_PREFIX = "handoff-evidence/fixed-install-linux/"
REPORT_FILES = fixed_safety.REPORT_FILES
REVIEWS = {
    **readiness_safety.REVIEWS,
    "handoff-evidence/reviews/FINAM_I1_TELEMETRY_SOURCE_ACCEPT_b6f6d5b_2026-09-24.md":
        "9157abb28e34df6afb16551a60cdf94a42f5b9c53172b99ba974ab576e037bd8",
    "handoff-evidence/reviews/FINAM_I1_FIXED_INSTALL_SOURCE_ACCEPT_7f2e876_2026-09-24.md":
        "1ba875ce95187c984c36a76d9ce70973bfea47ec620028406febdca01fbe085c",
}
GENERATED = {
    MARKER,
    MANIFEST,
    COMMIT_RAW,
    EVIDENCE,
    GATE_LOG,
    *REVIEWS,
    *(REPORT_PREFIX + name for name in REPORT_FILES),
}
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
    "production_source_changed",
    "i1_closed",
    "p1f_authorized",
    "closed_surfaces",
    "review_sha256",
    "target_evidence_sha256",
    "gate_log_sha256",
    "next_after_acceptance",
}
EVIDENCE_CLOSED_SURFACES = {
    "operational_redis_db0",
    "operational_redis_db15",
    "vps_installation_or_service_start",
    "paper_provider_operational_activation",
    "finam_post_delete_send",
    "broker_dispatch",
    "runtime_live",
    "real_orders",
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
        lines = commit_raw.decode().splitlines()
        require(lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {PARENT}" in lines, "commit parent mismatch")

        manifest = json.loads(files[MANIFEST], object_pairs_hook=aggregate.strict_object)
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

        evidence = json.loads(files[EVIDENCE], object_pairs_hook=aggregate.strict_object)
        require(set(evidence) == EVIDENCE_KEYS, "evidence key inventory drift")
        require(evidence["schema_version"] == 1, "evidence schema drift")
        require(evidence["stage"] == STAGE, "evidence stage drift")
        require(evidence["status"] == "REVIEW_CANDIDATE_I1_NOT_CLOSED", "status drift")
        require(evidence["source_ref"] == marker["source_ref"], "evidence source mismatch")
        require(evidence["source_parent"] == PARENT, "evidence parent mismatch")
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree mismatch")
        require(evidence["branch"] == BRANCH, "evidence branch mismatch")
        require(evidence["archive_name"] == marker["archive_name"], "evidence archive mismatch")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        require(set(evidence["changed_paths"]) == aggregate.ALLOWED_CHANGES, "changed paths drift")
        require(evidence["production_source_changed"] is False, "production source opened")
        require(evidence["i1_closed"] is False, "I1 self-closed")
        require(evidence["p1f_authorized"] is False, "P1-f self-authorized")
        require(evidence["review_sha256"] == REVIEWS, "review inventory drift")
        require(set(evidence["closed_surfaces"]) == EVIDENCE_CLOSED_SURFACES, "closed surface inventory drift")
        require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")
        require(
            evidence["next_after_acceptance"] == "separate P1-f isolated operational acceptance design",
            "next boundary drift",
        )
        for name, digest in REVIEWS.items():
            require(sha256(files[name]) == digest, f"review digest mismatch: {name}")
        report_hashes = evidence["target_evidence_sha256"]
        require(set(report_hashes) == REPORT_FILES, "target evidence inventory drift")
        for name, digest in report_hashes.items():
            require(digest == sha256(files[REPORT_PREFIX + name]), f"target evidence digest drift: {name}")

        gate = files[GATE_LOG]
        require(evidence["gate_log_sha256"] == sha256(gate), "gate digest mismatch")
        for token in (
            b"stage8b-p1e-i1-aggregate-acceptance-check: PASS milestones=16 rows=20 closed_surfaces=9",
            b"stage8b-p1e-i1-aggregate-acceptance-negative-harness: PASS 18/18",
            b"stage8b-p1e-i1-fixed-install-negative-harness: PASS 48/48",
            b"stage8b-p1e-i1-telemetry-composition-negative-harness 79/79",
            b"stage8b-p1e-i1-process-supervision-negative-harness 77/77",
            b"PASS stage8b-p1e-i1-aggregate-acceptance-gate aggregate_negative=18/18 isolated_redis=true target_evidence=21 operational_activation=false",
            b"exit_code=0",
        ):
            require(token in gate, f"gate marker missing: {token!r}")

        with tempfile.TemporaryDirectory(prefix="stage8b-i1-aggregate-accept-") as directory:
            root = Path(directory) / "source"
            report = Path(directory) / "target-evidence"
            for name, raw in payloads.items():
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(raw)
            report.mkdir()
            for name in REPORT_FILES:
                (report / name).write_bytes(files[REPORT_PREFIX + name])
            aggregate.validate(root, verify_lineage=False, evidence_dir=report)

        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "reviews_verified": len(REVIEWS),
            "target_evidence_members_verified": len(REPORT_FILES),
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
        raise SystemExit("usage: stage8b_p1e_i1_aggregate_acceptance_handoff_safety_check.py ARCHIVE")
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
        aggregate.CheckFailure,
    ) as error:
        print(f"stage8b-p1e-i1-aggregate-acceptance-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1e-i1-aggregate-acceptance-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
