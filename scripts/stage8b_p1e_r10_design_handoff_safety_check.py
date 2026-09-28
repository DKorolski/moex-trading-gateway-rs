#!/usr/bin/env python3
"""Validate immutable Stage 8B-P1-e R10 design handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p1e-r10-design-evidence.json"
GATE = "handoff-evidence/stage8b-p1e-r10-design-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
R9_REVIEW = "handoff-evidence/FINAM_P1e_R9_REVIEW_4051a7b_2026-09-09.md"
R9_REVIEW_SHA256 = "6ae8cf12280e5dfcd15928fd6812c5bef991755aa6163c742a0a4e6c7706546b"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST, R9_REVIEW}
REQUIRED = GENERATED | {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-design-r10.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r10-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1e-active-acceptance-contract-v10.json",
    "docs/stage-8/stage8b-p1e-semantic-authority-registry-v10.json",
    "docs/stage-8/stage8b-p1e-route-outcome-counter-contract-v2.json",
    "docs/stage-8/stage8b-p1e-source-effect-counter-oracle-v1.json",
    "docs/stage-8/stage8b-p1e-i0-regression-gate-v3.json",
    "scripts/stage8b_p1e_i0_scope_check.py",
    "scripts/stage8b_p1e_i0_retained_evidence.py",
    "scripts/stage8b_p1e_i0_regression_gate.sh",
    "scripts/stage8b_p1e_r10_design_check.py",
    "scripts/stage8b_p1e_r10_design_negative_harness.py",
    "scripts/stage8b_p1e_r10_design_gate.sh",
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

        marker = dict(line.split("=", 1) for line in archive.read("handoff-commit.txt").decode().splitlines() if "=" in line)
        evidence = json.loads(archive.read(EVIDENCE))
        manifest = json.loads(archive.read(MANIFEST))
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("source_tree") != evidence.get("source_tree"):
            raise ValueError("source tree mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if evidence.get("status") != "R10_DESIGN_REVIEW_CANDIDATE" or evidence.get("parent") != "4051a7b4d2c810100aaf983bddede62dbd03d96f":
            raise ValueError("candidate lineage mismatch")
        expected_counts = (317, 36, 46, 10, 7, 7, 105, 2, 6, 53, 8, 45)
        actual_counts = tuple(evidence.get(key) for key in (
            "active_rows", "semantic_authority_keys", "route_outcome_fixtures",
            "corrected_fixture_bindings", "counter_fields", "source_effect_paths",
            "p1d4_positive_sigkill_cells", "p1d4_clean_runs",
            "p1d2_p1d3_exact_tests", "negative_cases",
            "integrity_negative_cases", "semantic_negative_cases",
        ))
        if actual_counts != expected_counts:
            raise ValueError("contract inventory mismatch")
        if evidence.get("design_checker_only") is not True or evidence.get("source_modified") is not False:
            raise ValueError("design boundary opened")
        if evidence.get("i0_source_seam_authorized_now") is not False or evidence.get("supervisor_source_implementation_authorized") is not False:
            raise ValueError("source boundary opened")
        if any(value is not False for value in evidence.get("closed_surfaces", {}).values()):
            raise ValueError("closed surface opened")
        if sha256(archive.read(R9_REVIEW)) != R9_REVIEW_SHA256 or evidence.get("bundled_r9_review_sha256") != R9_REVIEW_SHA256:
            raise ValueError("R9 review binding")

        gate = archive.read(GATE)
        for expected in (
            b"PASS stage8b-p1e-r10-design-scope files=18",
            b"PASS stage8b-p1e-r10-design-negative-harness integrity=8/8 semantic-redigested=45/45 total=53/53",
            b"PASS stage8b-p1e-r10-active-contract rows=317",
            b"PASS stage8b-p1e-r10-route-outcomes fixtures=46 corrected=10",
            b"PASS stage8b-p1e-r10-source-effects paths=7",
            b"PASS stage8b-p1e-r10-i0-regression clean_immutable=true retained_pass_fail=true exact_tests=6",
            b"PASS stage8b-p1e-r10-normalized-semantic-oracle",
            b"PASS stage8b-p1e-r10-retained-evidence-smoke",
            b"PASS stage8b-p1e-r10-design-gate",
        ):
            if expected not in gate:
                raise ValueError(f"gate marker missing: {expected!r}")
        if sha256(gate) != evidence.get("gate_sha256"):
            raise ValueError("gate digest mismatch")
        if sha256(archive.read(MANIFEST)) != evidence.get("manifest_sha256"):
            raise ValueError("manifest digest mismatch")
        verification = evidence.get("verification", {})
        if not verification or any(value != "PASS" for value in verification.values()):
            raise ValueError("verification incomplete")

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
            "stage": "Stage 8B-P1-e R10 outcome/source and I0 evidence closure",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1e_r10_design_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-r10-design-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1e-r10-design-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
