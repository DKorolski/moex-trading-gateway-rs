#!/usr/bin/env python3
"""Mutation proof for the Stage 8A-5 closed-surface scanner."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = Path("crates/finam-gateway/src/stage8a4_reconciliation.rs")
AUTHORITY = Path("docs/stage-8/stage8a5-aggregate-acceptance-authority.json")


def main() -> None:
    cases = (
        ("reqwest-transport", SOURCE, "\n// reqwest transport\n"),
        ("http-post", SOURCE, "\n// client.post(\"/orders\")\n"),
        ("http-delete", SOURCE, "\n// client.delete(\"/orders/1\")\n"),
        ("method-post", SOURCE, "\n// Method::POST\n"),
        ("method-delete", SOURCE, "\n// Method::DELETE\n"),
        ("redis-xadd", SOURCE, "\n// XADD command\n"),
        ("redis-xack", SOURCE, "\n// XACK command\n"),
    )
    with tempfile.TemporaryDirectory(prefix="stage8a5-forbidden-negative-") as temp:
        work = Path(temp) / "repo"
        shutil.copytree(
            ROOT,
            work,
            ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports", "*.log", "__pycache__"),
        )
        source = work / SOURCE
        original_source = source.read_text(encoding="utf-8")
        passed = 0
        for name, _, injection in cases:
            source.write_text(original_source + injection, encoding="utf-8")
            result = subprocess.run(
                ["python3", "scripts/stage8a5_forbidden_surface_check.py", "--root", str(work), "--no-git"],
                cwd=work,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            source.write_text(original_source, encoding="utf-8")
            if result.returncode == 0:
                raise SystemExit(f"stage8a5-forbidden-negative: FAIL survived={name}")
            passed += 1
            print(f"PASS {name}")

        authority_path = work / AUTHORITY
        original_authority = authority_path.read_text(encoding="utf-8")
        for name, key in (
            ("redis-consumer-authority", "redis_live_consumer_enabled"),
            ("broker-dispatch-authority", "broker_dispatch_enabled"),
            ("runtime-live-authority", "runtime_live_enabled"),
        ):
            authority = json.loads(original_authority)
            authority[key] = True
            authority_path.write_text(json.dumps(authority, indent=2) + "\n", encoding="utf-8")
            result = subprocess.run(
                ["python3", "scripts/stage8a5_forbidden_surface_check.py", "--root", str(work), "--no-git"],
                cwd=work,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            authority_path.write_text(original_authority, encoding="utf-8")
            if result.returncode == 0:
                raise SystemExit(f"stage8a5-forbidden-negative: FAIL survived={name}")
            passed += 1
            print(f"PASS {name}")
    print(f"stage8a5-forbidden-negative: PASS cases={passed}/10")


if __name__ == "__main__":
    main()
