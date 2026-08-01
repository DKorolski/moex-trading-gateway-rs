#!/usr/bin/env python3
"""Pin Stage 5G-c R2-b to the independently accepted R2-a authority snapshot."""

from __future__ import annotations

import hashlib
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ACCEPTED = "c6ae2bdaea2575dd41e6da00acad5c231f3c7572"
PINNED = (
    "scripts/stage5g_c_r2a_authority_check.py",
    "scripts/stage5g_c_r2a_authority_negative_harness.py",
    "docs/stage-5/stage5g-c-source-projection-extension.json",
    "docs/stage-5/stage5g-lifecycle-entry-inventory.json",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs",
)


def git(*args: str, text: bool = False):
    return subprocess.run(
        ["git", *args], cwd=ROOT, check=True, capture_output=True, text=text
    ).stdout


def main() -> int:
    try:
        resolved = git("rev-parse", f"{ACCEPTED}^{{commit}}", text=True).strip()
        if resolved != ACCEPTED:
            raise ValueError("accepted R2-a commit does not resolve exactly")
        for relative in PINNED:
            accepted = git("show", f"{ACCEPTED}:{relative}")
            current_path = ROOT / relative
            if not current_path.is_file() or current_path.read_bytes() != accepted:
                raise ValueError(f"accepted R2-a authority drift: {relative}")

        # Execute the checker bytes from the accepted Git object, not the mutable
        # working-tree copy. The current source tree is only its inspection root.
        checker = git("show", f"{ACCEPTED}:scripts/stage5g_c_r2a_authority_check.py")
        with tempfile.NamedTemporaryFile(suffix=".py") as handle:
            handle.write(checker)
            handle.flush()
            result = subprocess.run(
                ["python3", handle.name, "--root", str(ROOT)],
                cwd=ROOT,
                check=False,
                capture_output=True,
                text=True,
            )
        if result.returncode != 0:
            raise ValueError(f"detached accepted checker rejected tree: {result.stderr.strip()}")
    except (OSError, subprocess.CalledProcessError, ValueError) as error:
        print(f"stage5g-c-r2b-snapshot-gate: FAIL: {error}", file=sys.stderr)
        return 1

    print("stage5g-c-r2b-snapshot-gate: PASS")
    print(f"accepted_commit: {ACCEPTED}")
    for relative in PINNED:
        digest = hashlib.sha256((ROOT / relative).read_bytes()).hexdigest()
        print(f"pinned: {relative} {digest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
