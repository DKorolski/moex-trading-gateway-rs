#!/usr/bin/env python3
"""Exercise the exact artifact safety checker on a good ZIP and isolated mutations."""
from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import zipfile

import stage8b_p1f_o2_artifact_handoff_safety_check as check


def run(path: Path) -> dict:
    check.check(str(path))  # Positive control before any negative mutation.
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        original = {info.filename: archive.read(info.filename) for info in infos}

    cases = (
        ("supervisor-bytes", "payload drift"),
        ("build-log", "build log drift"),
        ("build-commit", "build commit object mismatch"),
        ("build-manifest", "build source manifest ref drift"),
        ("build-original", "build source blob drift"),
        ("smoke-coverage", "smoke coverage marker missing"),
        ("duplicate", "duplicate archive members"),
        ("symlink", "symlink member"),
        ("execution", "evidence opened execution"),
    )
    results = []
    with tempfile.TemporaryDirectory(prefix="o2-archive-negative-") as directory:
        candidate = Path(directory) / path.name  # Preserve immutable filename binding.
        for name, expected_error in cases:
            members = dict(original)
            changed_infos = copy.deepcopy(infos)
            if name == "supervisor-bytes":
                members[check.SUPERVISOR] = members[check.SUPERVISOR][:-1] + bytes([members[check.SUPERVISOR][-1] ^ 1])
            elif name == "build-log":
                members[check.BUILD_LOG] += b"tampered\n"
            elif name == "build-commit":
                members[check.BUILD_COMMIT] += b"tampered\n"
                value = json.loads(members[check.BUILD])
                value["source_commit_raw_sha256"] = hashlib.sha256(members[check.BUILD_COMMIT]).hexdigest()
                members[check.BUILD] = json.dumps(value).encode()
                evidence = json.loads(members[check.EVIDENCE])
                evidence["build_sha256"] = hashlib.sha256(members[check.BUILD]).hexdigest()
                members[check.EVIDENCE] = json.dumps(evidence).encode()
            elif name == "build-manifest":
                value = json.loads(members[check.BUILD_MANIFEST])
                value["source_ref"] = "0" * 40
                members[check.BUILD_MANIFEST] = json.dumps(value).encode()
            elif name == "build-original":
                value = json.loads(members[check.BUILD_ORIGINALS])
                value[next(iter(value))] = "dGFtcGVyZWQ="
                members[check.BUILD_ORIGINALS] = json.dumps(value).encode()
            elif name == "smoke-coverage":
                members[check.ELF_SMOKE] = members[check.ELF_SMOKE].replace(b"PASS exact-elf bootstrap-supervisor-baseline07-config", b"REMOVED")
                evidence = json.loads(members[check.EVIDENCE])
                evidence["elf_smoke_sha256"] = hashlib.sha256(members[check.ELF_SMOKE]).hexdigest()
                members[check.EVIDENCE] = json.dumps(evidence).encode()
            elif name == "duplicate":
                changed_infos.append(copy.copy(changed_infos[0]))
            elif name == "symlink":
                next(info for info in changed_infos if info.filename == check.SUPERVISOR).external_attr = 0o120777 << 16
            elif name == "execution":
                evidence = json.loads(members[check.EVIDENCE])
                evidence["execution_authorized"] = True
                members[check.EVIDENCE] = json.dumps(evidence).encode()
            with zipfile.ZipFile(candidate, "w", compression=zipfile.ZIP_DEFLATED) as archive:
                for info in changed_infos:
                    archive.writestr(info, members[info.filename])
            try:
                check.check(str(candidate))
            except ValueError as error:
                if expected_error not in str(error):
                    raise AssertionError(f"{name}: unexpected failure: {error}") from error
            else:
                raise AssertionError(f"{name}: mutation accepted")
            results.append({"case": name, "result": "PASS", "expected_error": expected_error})
    return {"archive_sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "positive_control": "PASS", "negative_cases": results}


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: test_stage8b_p1f_o2_artifact_archive.py ARCHIVE")
    print(json.dumps(run(Path(sys.argv[1])), indent=2))
