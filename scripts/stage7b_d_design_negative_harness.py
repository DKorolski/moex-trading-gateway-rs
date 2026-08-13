#!/usr/bin/env python3
"""Mutation checks for the Stage 7B-d design authority."""
from __future__ import annotations

import json
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts/stage7b_d_design_check.py"

CASES = (
    ("allow-mutable-extractor", "descriptor", "mutable_recovered_extractor_allowed", True),
    ("drop-seal-barrier", "descriptor", "seal_before_ack_xack_required", False),
    ("drop-disk-seal-revalidation", "descriptor", "on_disk_seal_revalidation_required", False),
    ("drop-atomic-ack", "descriptor", "atomic_ack_xack_required", False),
    ("drop-atomic-dlq", "descriptor", "atomic_dlq_xack_required", False),
    ("execution-identity-marker", "descriptor", "settlement_marker_transport_only", False),
    ("memory-ack-authority", "descriptor", "process_memory_ack_restart_authority", True),
    ("merge-freshness", "descriptor", "source_claim_freshness_independent", False),
    ("abort-stale-ready", "descriptor", "explicit_task_abort_clears_readiness", False),
    ("premature-redis", "descriptor", "redis_consumer_attached", True),
    ("premature-xack", "descriptor", "xack_enabled", True),
    ("exactly-once-overclaim", "descriptor", "cross_process_exactly_once_claimed", True),
    ("remove-lua-primitive", "design", "one reviewed Lua primitive", ""),
    ("remove-response-loss", "design", "response loss", ""),
    ("remove-legacy-isolation", "design", "Legacy SQLite/M3", ""),
)


def main() -> None:
    for name, kind, key, replacement in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage7b-d-design-negative-{name}-") as tmp:
            clone = Path(tmp) / "repo"
            subprocess.run(
                ["git", "clone", "--quiet", "--no-hardlinks", str(ROOT), str(clone)],
                check=True,
            )
            subprocess.run(
                ["git", "checkout", "--quiet", ROOT.resolve().as_posix()],
                cwd=clone,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            ) if False else None
            # Copy the current design worktree because the harness also runs before commit.
            for relative in (
                "docs/stage-7/stage7b-d-entry-descriptor.json",
                "docs/stage-7/stage7b-entry-descriptor.json",
                "docs/stage-7/stage7b-c-entry-descriptor.json",
                "docs/stage-7/stage7b-d-design.md",
                "docs/stage-7/stage7b-acceptance-proof-map.json",
                "scripts/stage7b_proof_map.py",
                "scripts/stage7b_d_design_check.py",
            ):
                source = ROOT / relative
                target = clone / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
            if kind == "descriptor":
                path = clone / "docs/stage-7/stage7b-d-entry-descriptor.json"
                value = json.loads(path.read_text())
                value[key] = replacement
                path.write_text(json.dumps(value, indent=2) + "\n")
            else:
                path = clone / "docs/stage-7/stage7b-d-design.md"
                text = path.read_text()
                if key not in text:
                    raise SystemExit(f"stage7b-d-design-negative: fixture token absent: {key}")
                path.write_text(re.sub(re.escape(key), replacement, text, flags=re.IGNORECASE))
            result = subprocess.run(
                ["python3", str(clone / CHECK.relative_to(ROOT))],
                cwd=clone,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage7b-d-design-negative: FAIL mutation survived: {name}")
            print(f"PASS {name}")
    print(f"stage7b-d-design-negative: PASS cases={len(CASES)}")


if __name__ == "__main__":
    main()
