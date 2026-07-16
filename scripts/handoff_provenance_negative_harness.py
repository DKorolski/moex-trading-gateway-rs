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
    if mutate is not None:
        mutate(root, manifest)
    (root / "handoff-manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    )
    (root / "handoff-commit.txt").write_text(
        "\n".join(
            [
                f"source_commit={manifest.get('source_commit', SOURCE_COMMIT)}",
                f"source_ref={manifest.get('source_ref', SOURCE_REF)}",
                f"archive_name={manifest.get('archive_name', ARCHIVE_NAME)}",
            ]
        )
        + "\n"
    )


def build_archive(root: Path, archive_path: Path) -> None:
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(root.rglob("*")):
            if path.is_dir():
                continue
            rel = path.relative_to(root).as_posix()
            archive.write(path, rel)


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
    if case.expected not in combined:
        return False, f"expected marker {case.expected!r} missing\n{combined}"
    return True, ""


def main() -> int:
    cases = [
        Case(
            "missing-review-stage",
            "missing review_stage",
            lambda _root, manifest: manifest.pop("review_stage"),
        ),
        Case(
            "stale-review-stage",
            "review_stage/freeze-stage mismatch",
            lambda _root, manifest: manifest.__setitem__("review_stage", "5D-b2b-c1-r3"),
        ),
        Case(
            "freeze-stage-mismatch",
            "review_stage/freeze-stage mismatch",
            lambda root, _manifest: (
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
            "missing-checker-hash",
            "missing or invalid stage5d_checker_sha256",
            lambda _root, manifest: manifest.pop("stage5d_checker_sha256"),
        ),
        Case(
            "stale-checker-hash",
            "stage5d_checker_sha256 mismatch",
            lambda _root, manifest: manifest.__setitem__("stage5d_checker_sha256", "0" * 64),
        ),
        Case(
            "stale-stage5d-manifest-hash",
            "stage5d_manifest_sha256 mismatch",
            lambda _root, manifest: manifest.__setitem__("stage5d_manifest_sha256", "0" * 64),
        ),
        Case(
            "bad-short-full-relation",
            "source short/full commit mismatch",
            lambda _root, manifest: manifest.__setitem__("source_commit", "abcdef0"),
        ),
        Case(
            "archive-name-mismatch",
            "provenance marker/manifest mismatch",
            lambda _root, manifest: manifest.__setitem__("archive_name", "wrong.zip"),
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
