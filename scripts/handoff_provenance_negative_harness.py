#!/usr/bin/env python3
"""Negative tests for handoff semantic provenance."""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
import zipfile
from dataclasses import dataclass
from pathlib import Path

sys.dont_write_bytecode = True

from copy_review_baseline import copy_review_baseline


ROOT = Path(__file__).resolve().parents[1]
ARCHIVE_NAME = "moex-trading-project-0000000.zip"
SOURCE_COMMIT = "0000000"
SOURCE_REF = "0000000000000000000000000000000000000000"


@dataclass(frozen=True)
class Case:
    name: str
    expected: str
    mutator: object


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_manifest(root: Path, mutate=None) -> None:
    freeze_manifest = json.loads(
        (root / "docs/stage-5/stage-5d-additive-freeze-manifest.json").read_text()
    )
    manifest = {
        "schema_version": 1,
        "review_stage": freeze_manifest["stage"],
        "source_commit": SOURCE_COMMIT,
        "source_ref": SOURCE_REF,
        "archive_name": ARCHIVE_NAME,
        "stage5c_checker_sha256": sha256(root / "scripts/stage5c_api_freeze_check.py"),
        "stage5d_checker_sha256": sha256(root / "scripts/stage5d_additive_freeze_check.py"),
        "stage5d_manifest_sha256": sha256(
            root / "docs/stage-5/stage-5d-additive-freeze-manifest.json"
        ),
    }
    marker = {
        "source_commit": manifest["source_commit"],
        "source_ref": manifest["source_ref"],
        "archive_name": manifest["archive_name"],
    }
    manifest_payload: object = manifest
    duplicate_member: str | None = None
    if mutate is not None:
        result = mutate(root, manifest, marker)
        if isinstance(result, dict):
            manifest_payload = result.get("manifest_payload", manifest_payload)
            duplicate_member = result.get("duplicate_member")
    if isinstance(manifest_payload, str):
        (root / "handoff-manifest.json").write_text(manifest_payload)
    else:
        (root / "handoff-manifest.json").write_text(
            json.dumps(manifest_payload, indent=2, sort_keys=True) + "\n"
        )
    (root / "handoff-commit.txt").write_text(
        "\n".join(
            [
                f"source_commit={marker.get('source_commit', SOURCE_COMMIT)}",
                f"source_ref={marker.get('source_ref', SOURCE_REF)}",
                f"archive_name={marker.get('archive_name', ARCHIVE_NAME)}",
            ]
        )
        + "\n"
    )
    if duplicate_member:
        (root / ".duplicate-handoff-member").write_text(duplicate_member)


def build_archive(root: Path, archive_path: Path) -> None:
    duplicate_member_path = root / ".duplicate-handoff-member"
    duplicate_member = (
        duplicate_member_path.read_text().strip() if duplicate_member_path.exists() else None
    )
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(root.rglob("*")):
            if path.is_dir():
                continue
            rel = path.relative_to(root).as_posix()
            if rel == ".duplicate-handoff-member":
                continue
            archive.write(path, rel)
        if duplicate_member:
            archive.writestr(duplicate_member, b"duplicate")


def run_case(base: Path, case: Case) -> tuple[bool, str]:
    root = base / case.name
    copy_review_baseline(ROOT, root)
    write_manifest(root, case.mutator)
    archive_path = base / f"{case.name}.zip"
    build_archive(root, archive_path)
    result = subprocess.run(
        ["python3", str(ROOT / "scripts/handoff_safety_check.py"), "--archive", str(archive_path)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    combined = result.stdout + result.stderr
    if result.returncode == 0:
        return False, "mutation unexpectedly passed"
    if "Traceback" in combined or "KeyError" in combined:
        return False, f"unexpected uncontrolled Python failure\n{combined}"
    if case.expected not in combined:
        return False, f"expected marker {case.expected!r} missing\n{combined}"
    return True, ""


def main() -> int:
    def pop_field(field: str):
        return lambda _root, manifest, _marker: manifest.pop(field)

    def set_field(field: str, value: object):
        return lambda _root, manifest, _marker: manifest.__setitem__(field, value)

    def alter_member(path: str):
        return lambda root, _manifest, _marker: (root / path).write_text("altered\n")

    def marker_set(field: str, value: str):
        return lambda _root, _manifest, marker: marker.__setitem__(field, value)

    cases = [
        Case(
            "malformed-json",
            "malformed handoff manifest JSON",
            lambda _root, _manifest, _marker: {"manifest_payload": "{not-json"},
        ),
        Case(
            "non-object-manifest",
            "handoff manifest must be a JSON object",
            lambda _root, _manifest, _marker: {"manifest_payload": ["not", "object"]},
        ),
        Case(
            "unsupported-schema",
            "unsupported handoff manifest schema_version",
            set_field("schema_version", 2),
        ),
        Case(
            "missing-schema",
            "unsupported handoff manifest schema_version",
            pop_field("schema_version"),
        ),
        Case(
            "missing-review-stage",
            "missing review_stage",
            pop_field("review_stage"),
        ),
        Case(
            "empty-review-stage",
            "missing review_stage",
            set_field("review_stage", ""),
        ),
        Case(
            "stale-review-stage",
            "review_stage/freeze-stage mismatch",
            set_field("review_stage", "5D-b2b-c1-r3"),
        ),
        Case(
            "freeze-stage-mismatch",
            "review_stage/freeze-stage mismatch",
            lambda root, _manifest, _marker: (
                (root / "docs/stage-5/stage-5d-additive-freeze-manifest.json").write_text(
                    json.dumps(
                        {
                            **json.loads(
                                (
                                    root
                                    / "docs/stage-5/stage-5d-additive-freeze-manifest.json"
                                ).read_text()
                            ),
                            "stage": "5D-b2b-c1-r3",
                        },
                        indent=2,
                        sort_keys=True,
                    )
                    + "\n"
                )
            ),
        ),
        Case(
            "missing-stage5c-checker-hash",
            "missing or invalid stage5c_checker_sha256",
            pop_field("stage5c_checker_sha256"),
        ),
        Case(
            "stale-stage5c-checker-hash",
            "stage5c_checker_sha256 mismatch",
            set_field("stage5c_checker_sha256", "0" * 64),
        ),
        Case(
            "altered-stage5c-checker-hash",
            "stage5c_checker_sha256 mismatch",
            alter_member("scripts/stage5c_api_freeze_check.py"),
        ),
        Case(
            "missing-stage5d-checker-hash",
            "missing or invalid stage5d_checker_sha256",
            pop_field("stage5d_checker_sha256"),
        ),
        Case(
            "stale-stage5d-checker-hash",
            "stage5d_checker_sha256 mismatch",
            set_field("stage5d_checker_sha256", "0" * 64),
        ),
        Case(
            "altered-stage5d-checker-hash",
            "stage5d_checker_sha256 mismatch",
            alter_member("scripts/stage5d_additive_freeze_check.py"),
        ),
        Case(
            "missing-stage5d-manifest-hash",
            "missing or invalid stage5d_manifest_sha256",
            pop_field("stage5d_manifest_sha256"),
        ),
        Case(
            "stale-stage5d-manifest-hash",
            "stage5d_manifest_sha256 mismatch",
            set_field("stage5d_manifest_sha256", "0" * 64),
        ),
        Case(
            "malformed-stage5d-manifest-hash",
            "missing or invalid stage5d_manifest_sha256",
            set_field("stage5d_manifest_sha256", "not-a-sha"),
        ),
        Case(
            "missing-source-commit",
            "missing or invalid source_commit",
            pop_field("source_commit"),
        ),
        Case(
            "malformed-source-commit",
            "missing or invalid source_commit",
            set_field("source_commit", "nothex"),
        ),
        Case(
            "missing-source-ref",
            "missing or invalid source_ref",
            pop_field("source_ref"),
        ),
        Case(
            "malformed-source-ref",
            "missing or invalid source_ref",
            set_field("source_ref", "0" * 39),
        ),
        Case(
            "bad-short-full-relation",
            "source short/full commit mismatch",
            set_field("source_commit", "abcdef0"),
        ),
        Case(
            "missing-archive-name",
            "missing archive_name",
            pop_field("archive_name"),
        ),
        Case(
            "archive-name-mismatch",
            "provenance marker/manifest mismatch",
            set_field("archive_name", "wrong.zip"),
        ),
        Case(
            "marker-source-commit-mismatch",
            "provenance marker/manifest mismatch",
            marker_set("source_commit", "1111111"),
        ),
        Case(
            "marker-source-ref-mismatch",
            "provenance marker/manifest mismatch",
            marker_set("source_ref", "1" * 40),
        ),
        Case(
            "marker-archive-name-mismatch",
            "provenance marker/manifest mismatch",
            marker_set("archive_name", "wrong.zip"),
        ),
        Case(
            "duplicate-handoff-manifest-member",
            "duplicate ZIP entries",
            lambda _root, _manifest, _marker: {
                "duplicate_member": "handoff-manifest.json"
            },
        ),
    ]
    with tempfile.TemporaryDirectory(prefix="handoff-provenance-negative-") as tmp:
        base = Path(tmp)
        failures = []
        for case in cases:
            ok, diagnostics = run_case(base, case)
            print(f"{'PASS' if ok else 'FAIL'} {case.name}")
            if not ok:
                failures.append((case.name, diagnostics))
                print(diagnostics, file=sys.stderr)
        if failures:
            return 1
    print(f"handoff-provenance-negative-harness: ok cases={len(cases)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
