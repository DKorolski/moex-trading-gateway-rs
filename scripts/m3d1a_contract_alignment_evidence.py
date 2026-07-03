#!/usr/bin/env python3
"""Generate M3d-1a contract-alignment evidence without order endpoint calls."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from zipfile import ZipFile


FIXTURE = Path(
    "crates/broker-finam/tests/fixtures/finam_spec/"
    "order_contract_enums_v2026_07_03.json"
)

CHECKS = [
    ["cargo", "fmt", "--all", "--check"],
    ["cargo", "test", "--all"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ["bash", "scripts/forbidden_surface_scan.sh"],
    ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"],
    ["bash", "scripts/redis_shadow_smoke.sh"],
    ["bash", "scripts/runtime_bridge_dry_smoke.sh"],
]


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_text(command: list[str], cwd: Path) -> dict[str, Any]:
    completed = subprocess.run(command, cwd=cwd, text=True, capture_output=True)
    return {
        "command": command,
        "exit_code": completed.returncode,
        "status": "Ok" if completed.returncode == 0 else "Failed",
        "stdout_tail": completed.stdout[-4000:],
        "stderr_tail": completed.stderr[-4000:],
    }


def read_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        payload = json.load(handle)
    if not isinstance(payload, dict):
        raise ValueError(f"expected object in {path}")
    return payload


def clean_handoff_summary(path: Path) -> dict[str, Any]:
    forbidden_markers = (
        ".env",
        ".git/",
        "target/",
        "tmp/",
        "reports/",
        "__MACOSX/",
        ".DS_Store",
    )
    forbidden_entries: list[str] = []
    handoff_marker_present = False
    with ZipFile(path) as archive:
        names = archive.namelist()
        handoff_marker_present = "handoff-commit.txt" in names
        for name in names:
            if name.endswith(".log") or any(marker in name for marker in forbidden_markers):
                forbidden_entries.append(name)
    return {
        "archive_name": path.name,
        "archive_sha256": sha256_file(path),
        "handoff_commit_marker_present": handoff_marker_present,
        "forbidden_entry_count": len(forbidden_entries),
        "forbidden_entries": forbidden_entries[:20],
        "clean": handoff_marker_present and not forbidden_entries,
    }


def fixture_policy_summary(fixture: dict[str, Any]) -> dict[str, Any]:
    order_status = fixture.get("order_status", [])
    order_status_policy = fixture.get("order_status_policy", {})
    time_in_force = fixture.get("time_in_force", [])
    tif_policy = fixture.get("time_in_force_plain_order_policy", {})
    valid_before = fixture.get("valid_before", [])
    valid_before_policy = fixture.get("valid_before_plain_order_policy", {})

    def missing(values: list[Any], policy: dict[str, Any]) -> list[str]:
        return [
            value
            for value in values
            if isinstance(value, str) and value not in policy
        ]

    return {
        "fixture_path": str(FIXTURE),
        "fixture_sha256": sha256_file(repo_root() / FIXTURE),
        "order_status_count": len(order_status),
        "order_status_policy_count": len(order_status_policy),
        "order_status_missing_policy": missing(order_status, order_status_policy),
        "order_status_only_unspecified_blocking_unknown": [
            key
            for key, value in order_status_policy.items()
            if value == "blocking_unknown" and key != "ORDER_STATUS_UNSPECIFIED"
        ]
        == [],
        "time_in_force_count": len(time_in_force),
        "time_in_force_policy_count": len(tif_policy),
        "time_in_force_missing_policy": missing(time_in_force, tif_policy),
        "valid_before_count": len(valid_before),
        "valid_before_policy_count": len(valid_before_policy),
        "valid_before_missing_policy": missing(valid_before, valid_before_policy),
        "valid_before_good_till_date_policy": valid_before_policy.get(
            "VALID_BEFORE_GOOD_TILL_DATE"
        ),
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate M3d-1a source-bound contract-alignment evidence."
    )
    parser.add_argument(
        "--source-archive",
        type=Path,
        help="Optional clean handoff archive to bind into evidence.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3d-contract-alignment/m3d1a-evidence.json"),
    )
    args = parser.parse_args()

    root = repo_root()
    git = run_text(["git", "rev-parse", "HEAD"], root)
    if git["exit_code"] != 0:
        print(git["stderr_tail"], file=sys.stderr)
        return git["exit_code"]
    source_commit = git["stdout_tail"].strip()

    fixture = read_json(root / FIXTURE)
    checks = [run_text(command, root) for command in CHECKS]
    archive_summary = None
    if args.source_archive:
        archive_path = (root / args.source_archive).resolve()
        if not archive_path.exists():
            print(f"source archive does not exist: {archive_path}", file=sys.stderr)
            return 2
        archive_summary = clean_handoff_summary(archive_path)

    policy_summary = fixture_policy_summary(fixture)
    all_checks_ok = all(check["exit_code"] == 0 for check in checks)
    policy_total = (
        not policy_summary["order_status_missing_policy"]
        and policy_summary["order_status_count"]
        == policy_summary["order_status_policy_count"]
        and policy_summary["order_status_only_unspecified_blocking_unknown"]
        and not policy_summary["time_in_force_missing_policy"]
        and policy_summary["time_in_force_count"]
        == policy_summary["time_in_force_policy_count"]
        and not policy_summary["valid_before_missing_policy"]
        and policy_summary["valid_before_count"]
        == policy_summary["valid_before_policy_count"]
        and policy_summary["valid_before_good_till_date_policy"] == "sltp_only"
    )
    archive_ok = archive_summary is None or archive_summary["clean"]
    evidence_ready = all_checks_ok and policy_total and archive_ok

    evidence = {
        "m3d_step": "M3d-1a",
        "contract_alignment_evidence": True,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": source_commit,
        "source_archive": archive_summary,
        "fixture_policy_summary": policy_summary,
        "checks": checks,
        "all_checks_ok": all_checks_ok,
        "policy_total_coverage_ok": policy_total,
        "trading_boundary": {
            "real_post_delete_added": False,
            "endpoint_calls_allowed": False,
            "real_order_endpoint_enabled": False,
            "command_consumer_enabled": False,
            "live_ready_allowed": False,
        },
        "evidence_ready_for_review": evidence_ready,
    }

    output = (root / args.output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
    output_sha256 = sha256_file(output)
    output.with_suffix(output.suffix + ".sha256").write_text(
        f"{output_sha256}  {output.name}\n"
    )
    print(json.dumps({"output": str(output), "sha256": output_sha256}, indent=2))
    return 0 if evidence_ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
