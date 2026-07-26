#!/usr/bin/env python3
"""Fail-closed governance gate for the Stage 5E-b3c authority freeze r3."""

import hashlib
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "docs/stage-5/stage5e-b3c-source-authority-freeze-extension-inventory.json"
PLAN = ROOT / "docs/stage-5/5e-b3c-source-authority-freeze-extension-plan.md"
ACTIVE = ROOT / "docs/stage-5/stage5e-active-descriptor.json"
BASELINE_REF = "80331a45cd2f6f4fe308a0a396ca6b1b74d01237"

EXPECTED_INVENTORY_SHA256 = "f9d27b0fde1b52429e40ad69703903e9a174ca23e58ba920a8ed8a406e980e9b"
EXPECTED_PLAN_SHA256 = "a133165923df73c6e9b2c7aa2108aaa5df1008348fc1bbd76d9c00156bc5e92c"
EXPECTED_SOURCE_BASELINES = {
    "crates/broker-core/src/lib.rs": "5d8758624f53a6b46d8903dd3f2339d5bd04f64c9c6490448167f08ac68ec8a2",
    "crates/broker-core/src/operational_config.rs": "492905c6e404ee67f62ad456128ff659cd4a32c8e638936b94b5ea14ff3ba2f8",
    "crates/broker-core/src/stage4_bootstrap.rs": "cb4362acce9624ec62c0007731b779fe96d2222c0ec8c0b1a921ed30a0aecf07",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": "14a723bd2adf98f50c2443166b7fb838edd8df6c5cf46968d13eb9e8d901b4c9",
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": "76bade52f3ebb309475812b617823825a3b7e4838bf89f9eb297ca2bbffbf821",
}
EXPECTED_ALLOWED_CHANGED_PATHS = [
    "docs/stage-5/5e-b3c-source-authority-freeze-extension-plan.md",
    "docs/stage-5/stage5e-b3c-source-authority-freeze-extension-inventory.json",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/handoff_safety_check.py",
    "scripts/stage5e_b3c_source_authority_freeze_extension_check.py",
]


def fail(message: str) -> None:
    print(
        f"stage5e-b3c-source-authority-freeze-extension-check: FAIL: {message}",
        file=sys.stderr,
    )
    raise SystemExit(1)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canonical_sha256(value: object) -> str:
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def main() -> int:
    payload = json.loads(INVENTORY.read_text())
    if canonical_sha256(payload) != EXPECTED_INVENTORY_SHA256:
        fail("authority freeze contract drift")
    if sha256(PLAN) != EXPECTED_PLAN_SHA256:
        fail("authority freeze plan drift")
    if payload.get("schema_version") != 4:
        fail("authority freeze schema drift")
    if payload.get("stage") != "5E-b3c-source-authority-freeze-extension":
        fail("authority freeze identity drift")
    if payload.get("status") != "authority_freeze_r3_pending_review":
        fail("authority freeze status drift")
    if payload.get("baseline_ref") != BASELINE_REF:
        fail("authority freeze baseline drift")
    if payload.get("expected_provenance_case_count") != 165:
        fail("authority freeze negative-matrix count drift")
    if payload.get("production_source_baselines") != EXPECTED_SOURCE_BASELINES:
        fail("authority freeze source baseline drift")
    if payload.get("allowed_changed_paths") != EXPECTED_ALLOWED_CHANGED_PATHS:
        fail("authority freeze changed-path contract drift")
    if payload.get("implementation_authorization") != {
        "authority_freeze_r3_reviewed": False,
        "production_source_changes_allowed": False,
        "trusted_combined_eligibility": False,
        "unverified_sequence_production_authoritative": False,
    }:
        fail("authority freeze implementation authorization drift")
    for rel, expected in EXPECTED_SOURCE_BASELINES.items():
        if sha256(ROOT / rel) != expected:
            fail(f"authority freeze protected source changed: {rel}")
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", BASELINE_REF, "--"], cwd=ROOT, text=True
    ).splitlines()
    if sorted(changed) != sorted(EXPECTED_ALLOWED_CHANGED_PATHS):
        fail("authority freeze review diff drift")
    if json.loads(ACTIVE.read_text()) != {
        "schema_version": 1,
        "stage": "5E-b3c-source-authority-freeze-extension",
    }:
        fail("active descriptor drift")
    predecessor = subprocess.run(
        [sys.executable, "scripts/stage5e_b3c_private_eligibility_seam_check.py"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if predecessor.returncode != 0:
        fail("B3C predecessor contract failed")
    print("stage5e-b3c-source-authority-freeze-extension-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
