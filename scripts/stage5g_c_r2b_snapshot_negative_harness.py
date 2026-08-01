#!/usr/bin/env python3
"""Rehash-aware adversarial proof for the detached R2-a authority gate."""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ACCEPTED = "c6ae2bdaea2575dd41e6da00acad5c231f3c7572"
STAGE5C = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
DESCRIPTOR = "docs/stage-5/stage5g-c-source-projection-extension.json"
LOCAL_CHECKER = "scripts/stage5g_c_r2a_authority_check.py"


def git_show(path: str) -> bytes:
    return subprocess.run(
        ["git", "show", f"{ACCEPTED}:{path}"],
        cwd=ROOT,
        check=True,
        capture_output=True,
    ).stdout


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage5g-c-r2b-rehash-") as raw:
        root = Path(raw) / "repo"
        shutil.copytree(
            ROOT,
            root,
            ignore=shutil.ignore_patterns(".git", "target", "reports", "tmp", "*.log"),
        )
        stage5c = root / STAGE5C
        source = stage5c.read_text()
        anchor = "pub request_id: StrategyRequestId,"
        if anchor not in source:
            raise RuntimeError("Stage 5C mutation anchor missing")
        source = source.replace(anchor, anchor + "\n    pub forged_r2b: bool,", 1)
        stage5c.write_text(source)

        begin = "// STAGE5G-C-SOURCE-PROJECTION-BEGIN: source-projection-types\n"
        end = "// STAGE5G-C-SOURCE-PROJECTION-END: source-projection-types\n"
        body = source.split(begin, 1)[1].split(end, 1)[0]
        region_digest = hashlib.sha256(body.encode()).hexdigest()
        current_digest = hashlib.sha256(stage5c.read_bytes()).hexdigest()

        descriptor_path = root / DESCRIPTOR
        descriptor = json.loads(descriptor_path.read_text())
        descriptor["stage5c_extension"]["current_sha256"] = current_digest
        for row in descriptor["stage5c_extension"]["regions"]:
            if row["name"] == "source-projection-types":
                row["sha256"] = region_digest
        descriptor_path.write_text(json.dumps(descriptor, indent=2) + "\n")

        checker_path = root / LOCAL_CHECKER
        checker = checker_path.read_text()
        old_digest = "b01a3731afa1da385628425f5d58c9529277f3605c1f996ebe4a184eb322135f"
        if old_digest not in checker:
            raise RuntimeError("local checker digest anchor missing")
        checker_path.write_text(checker.replace(old_digest, region_digest, 1))

        # The mutable local checker is deliberately made to accept the forged
        # source/descriptor/checker tuple.
        local = subprocess.run(
            ["python3", LOCAL_CHECKER, "--root", str(root)], cwd=root, check=False
        )
        if local.returncode != 0:
            raise RuntimeError("rehash mutation did not establish the adversarial premise")

        accepted_checker = root / "accepted-r2a-checker.py"
        accepted_checker.write_bytes(git_show(LOCAL_CHECKER))
        detached = subprocess.run(
            ["python3", str(accepted_checker), "--root", str(root)],
            cwd=root,
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        if detached.returncode == 0:
            print("FAIL detached accepted checker accepted rehash-aware mutation")
            return 1

    print("PASS stage5c-region-descriptor-local-checker-rehash")
    print("stage5g-c-r2b-snapshot-negative-harness: PASS 1/1")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
