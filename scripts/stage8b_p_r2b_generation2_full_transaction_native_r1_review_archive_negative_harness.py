#!/usr/bin/env python3
"""Post-package negative checks for actual ZIP binding and fresh extraction."""

from __future__ import annotations

import hashlib
import shutil
import sys
import tempfile
import zipfile
from pathlib import Path

import stage8b_p_r2b_generation2_full_transaction_native_r1_review_archive as checker


def rejected(name: str, operation) -> None:
    try:
        operation()
    except (OSError, RuntimeError, ValueError, zipfile.BadZipFile):
        print(f"PASS {name}")
        return
    raise SystemExit(f"stage8b-generation2-review-archive-negative: FAIL accepted={name}")


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: review_archive_negative_harness ARCHIVE")
    archive = Path(sys.argv[1]).resolve(strict=True)
    expected = hashlib.sha256(archive.read_bytes()).hexdigest()
    review = "a" * 64

    with tempfile.TemporaryDirectory(prefix="stage8b-review-archive-positive-") as temporary:
        root = Path(temporary) / "extracted"
        root.mkdir()
        result = checker.verify_and_extract(archive, expected, review, root)
        if result.get("result") != "PASS":
            raise SystemExit("stage8b-generation2-review-archive-negative: FAIL positive")
        print("PASS actual-archive-positive")

    with tempfile.TemporaryDirectory(prefix="stage8b-review-archive-wrong-sha-") as temporary:
        root = Path(temporary) / "extracted"
        root.mkdir()
        rejected("arbitrary-archive-sha-accepted", lambda: checker.verify_and_extract(archive, "0" * 64, review, root))

    with tempfile.TemporaryDirectory(prefix="stage8b-review-archive-review-binding-") as temporary:
        root = Path(temporary) / "extracted"
        root.mkdir()
        rejected("reviewer-binding-invalid", lambda: checker.verify_and_extract(archive, expected, "not-a-digest", root))

    with tempfile.TemporaryDirectory(prefix="stage8b-review-archive-not-fresh-") as temporary:
        root = Path(temporary) / "extracted"
        root.mkdir()
        (root / "residue").write_text("residue")
        rejected("nonempty-extraction-root", lambda: checker.verify_and_extract(archive, expected, review, root))

    with tempfile.TemporaryDirectory(prefix="stage8b-review-archive-tampered-") as temporary:
        tampered = Path(temporary) / archive.name
        shutil.copy2(archive, tampered)
        with zipfile.ZipFile(tampered, "a") as package:
            package.writestr("unexpected-member", b"unexpected")
        tampered_digest = hashlib.sha256(tampered.read_bytes()).hexdigest()
        root = Path(temporary) / "extracted"
        root.mkdir()
        rejected("additional-member", lambda: checker.verify_and_extract(tampered, tampered_digest, review, root))

    print("stage8b-generation2-review-archive-negative: PASS cases=5/5")


if __name__ == "__main__":
    main()
