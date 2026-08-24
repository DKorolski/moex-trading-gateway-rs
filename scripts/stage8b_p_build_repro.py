#!/usr/bin/env python3
"""Reproduce the exact accepted TLS-qualified Stage 8B-P candidate binary."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import tempfile
import zipfile
from pathlib import Path

import stage8b_tls_handoff_safety_check as tls_safety

ROOT = Path(__file__).resolve().parents[1]
ARCHIVE = ROOT / "reports/handoff/moex-trading-project-6cb1795.zip"
IDENTITY = ROOT / "docs/stage-8/stage8b-p-build-identity-2026-08-23.json"
REPORT = ROOT / "reports/stage8b-p-build-repro.json"
BINARY = ROOT / "reports/stage8b-p-broker-cli-aarch64-apple-darwin"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def verify_source(root: Path) -> None:
    manifest = json.loads((root / "handoff-evidence/source-tree-manifest.json").read_text())
    for entry in manifest["entries"]:
        path = root / entry["path"]
        if not path.is_file() or len(path.read_bytes()) != entry["size"] or sha(path.read_bytes()) != entry["sha256"]:
            raise SystemExit(f"stage8b-p-build-repro: FAIL source drift: {entry['path']}")


def build_once(parent: Path, name: str, identity: dict[str, object]) -> tuple[str, int, bytes]:
    root = parent / name
    root.mkdir()
    with zipfile.ZipFile(ARCHIVE) as archive:
        archive.extractall(root)
    verify_source(root)
    env = os.environ.copy()
    env.update(
        {
            "CARGO_NET_OFFLINE": "true",
            "CARGO_INCREMENTAL": "0",
            "SOURCE_DATE_EPOCH": str(identity["build"]["source_date_epoch"]),
            "RUSTFLAGS": (
                f"--remap-path-prefix={root.resolve()}=/stage8b-source "
                f"--remap-path-prefix={root}=/stage8b-source"
            ),
        }
    )
    subprocess.run(
        ["cargo", "build", "--release", "--locked", "-p", "broker-cli"],
        cwd=root,
        env=env,
        check=True,
    )
    verify_source(root)
    executable = root / "target/release/broker-cli"
    data = executable.read_bytes()
    return sha(data), len(data), data


def main() -> None:
    identity = json.loads(IDENTITY.read_text())
    if sha(ARCHIVE.read_bytes()) != identity["source"]["archive_sha256"]:
        raise SystemExit("stage8b-p-build-repro: FAIL accepted archive drift")
    tls_safety.check(str(ARCHIVE))
    # `/tmp` is part of the accepted macOS build contract. Selecting it
    # explicitly prevents tempfile's per-user `/var/folders/...` root from
    # becoming an unbound compiler input before path remapping is applied.
    with tempfile.TemporaryDirectory(prefix="stage8b-p-repro-", dir="/tmp") as tmp:
        parent = Path(tmp)
        results = [build_once(parent, "clean-a", identity), build_once(parent, "clean-b", identity)]
    expected = (identity["build"]["executable_sha256"], identity["build"]["executable_size"])
    observed = [(item[0], item[1]) for item in results]
    if observed != [expected, expected] or results[0][2] != results[1][2]:
        raise SystemExit(f"stage8b-p-build-repro: FAIL non-reproducible {observed}")
    report = {
        "schema_version": 1,
        "stage": "8B-P-PRECONDITIONS",
        "revision": "R1",
        "source_ref": identity["source"]["commit"],
        "archive_sha256": identity["source"]["archive_sha256"],
        "build_count": 2,
        "executable_sha256": expected[0],
        "executable_size": expected[1],
        "all_hashes_identical": True,
        "source_unchanged_after_each_build": True,
        "network_dependency_fetch": False,
        "executable_invoked": False,
        "stage8b_p": False,
        "broker_effect": False,
    }
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    BINARY.write_bytes(results[0][2])
    print(f"stage8b-p-build-repro: PASS builds=2 executable={expected[0]} source_unchanged=true stage8b_p=false")


if __name__ == "__main__":
    main()
