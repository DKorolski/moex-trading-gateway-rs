#!/usr/bin/env python3
"""Generate M3j-1 live gate / operator-risk-controls design evidence."""

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


GATEWAY_LIB = Path("crates/finam-gateway/src/lib.rs")
DOC = Path("docs/m3j1-live-gate-risk-controls-design.md")

CHECKS = [
    ["cargo", "fmt", "--all", "--check"],
    ["cargo", "test", "-p", "finam-gateway", "m3j", "--", "--nocapture"],
    ["cargo", "test", "--all"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ["bash", "scripts/forbidden_surface_scan.sh"],
    ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"],
    ["python3", "-m", "py_compile", "scripts/m3j1_live_gate_risk_controls_evidence.py"],
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
        "stdout_tail": completed.stdout[-5000:],
        "stderr_tail": completed.stderr[-5000:],
    }


def clean_handoff_summary(path: Path, source_commit: str) -> dict[str, Any]:
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
    handoff_marker_candidates: list[str] = []
    handoff_commit = None
    with ZipFile(path) as archive:
        names = archive.namelist()
        handoff_marker_candidates = [
            name for name in names if name.endswith("handoff-commit.txt")
        ]
        if handoff_marker_candidates:
            handoff_commit = (
                archive.read(handoff_marker_candidates[0]).decode("utf-8").strip()
            )
        for name in names:
            if name.endswith(".log") or any(marker in name for marker in forbidden_markers):
                forbidden_entries.append(name)
    return {
        "archive_name": path.name,
        "archive_sha256": sha256_file(path),
        "handoff_commit_marker_present": bool(handoff_marker_candidates),
        "handoff_commit_marker": handoff_marker_candidates[0]
        if handoff_marker_candidates
        else None,
        "handoff_commit_matches_source": bool(
            handoff_commit and source_commit in handoff_commit
        ),
        "forbidden_entry_count": len(forbidden_entries),
        "forbidden_entries": forbidden_entries[:20],
        "clean": bool(handoff_marker_candidates)
        and bool(handoff_commit and source_commit in handoff_commit)
        and not forbidden_entries,
    }


def contains_all(source: str, patterns: list[str]) -> dict[str, bool]:
    return {pattern: pattern in source for pattern in patterns}


def evidence_summary(root: Path) -> dict[str, Any]:
    source = (root / GATEWAY_LIB).read_text(encoding="utf-8")
    doc = (root / DOC).read_text(encoding="utf-8")
    required_patterns = [
        "pub enum M3j1OperatorDisarmReason",
        "pub struct M3j1OperatorArmDesign",
        "pub struct M3j1KillSwitchDesign",
        "pub struct M3j1RiskLimitsDesign",
        "pub struct M3j1ScopeControlsDesign",
        "pub struct M3j1LiveGateDesignInput",
        "pub struct M3j1LiveGateDesignReport",
        "pub fn m3j1_live_gate_design_report",
        "m3j_step: \"M3j-1\"",
        "pre_live_design_only: true",
        "live_micro_go: false",
        "operator_arm_design_ready",
        "kill_switch_design_ready",
        "max_orders_qty_loss_limits_ready",
        "scope_controls_ready",
        "max_unknown_pending_count == 0",
        "ri_rts_allowed_initially",
        "external_finam_post_delete_allowed: false",
        "command_consumer_to_real_finam_transport_allowed: false",
        "non_loopback_order_endpoint_allowed: false",
        "m3j1_live_gate_design_is_ready_but_still_no_go",
        "m3j1_missing_controls_or_live_boundary_cannot_pass_design_gate",
    ]
    forbidden_patterns = [
        "M3j1LiveGateDesignReport::reqwest",
        "M3j1LiveGateDesignReport::POST",
        "M3j1LiveGateDesignReport::DELETE",
        "api.finam.ru/order",
    ]
    operator_arm_ready = all(
        pattern in source
        for pattern in [
            "one_shot",
            "ttl_required",
            "expected_account_digest_required",
            "expected_symbol_digest_required",
            "expected_config_digest_required",
            "expected_endpoint_session_digest_required",
            "no_auto_rearm_after_restart",
            "TtlExpired",
            "OneShotConsumed",
            "EndpointSessionDigestMismatch",
            "ManualDisarm",
        ]
    )
    kill_switch_ready = all(
        pattern in source
        for pattern in [
            "hard_global_order_emission_block",
            "blocks_runtime",
            "blocks_command_consumer",
            "blocks_endpoint_path",
            "persists_across_restart",
            "redacted_operator_report",
        ]
    )
    risk_limits_ready = all(
        pattern in source
        for pattern in [
            "max_orders_per_day_required",
            "max_orders_per_session_required",
            "max_qty_required",
            "max_notional_placeholder",
            "max_loss_stop_out_placeholder",
            "max_unknown_pending_count_zero",
            "no_ri_rts_initially",
        ]
    )
    scope_controls_ready = all(
        pattern in source
        for pattern in [
            "account_allowlist_count == 1",
            "symbol_allowlist_count == 1",
            "timeframe_scope_count == 1",
            "strategy_scope_count == 1",
            "market_limit_only",
            "stop_orders_allowed",
            "sltp_allowed",
            "bracket_allowed",
            "replace_allowed",
            "multi_leg_allowed",
        ]
    )
    return {
        "gateway_lib_sha256": sha256_file(root / GATEWAY_LIB),
        "doc_sha256": sha256_file(root / DOC),
        "required_patterns_present": contains_all(source, required_patterns),
        "forbidden_patterns_absent": {
            pattern: pattern not in source for pattern in forbidden_patterns
        },
        "m3j1_live_gate_design_ok": "m3j1_live_gate_design_ok" in source,
        "operator_arm_design_ready": operator_arm_ready,
        "kill_switch_design_ready": kill_switch_ready,
        "max_orders_qty_loss_limits_ready": risk_limits_ready,
        "scope_controls_ready": scope_controls_ready,
        "live_micro_go_false": "live_micro_go: false" in source,
        "no_live_boundary": (
            "live_ready_allowed: false" in source
            and "runtime_live_attachment_allowed: false" in source
            and "external_finam_post_delete_allowed: false" in source
            and "command_consumer_to_real_finam_transport_allowed: false" in source
            and "non_loopback_order_endpoint_allowed: false" in source
        ),
        "no_stop_sltp_bracket": "stop_sltp_bracket_replace_multileg_allowed: false"
        in source,
        "no_real_finam_order_endpoint": "real FINAM POST/DELETE" in doc,
        "closure_documented": "M3j-1 closes the design part" in doc,
        "real_finam_order_endpoint_used": False,
        "external_order_endpoint_allowed": False,
        "runtime_live_attachment_allowed": False,
        "live_ready_allowed": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate M3j-1 live gate / operator-risk-controls evidence."
    )
    parser.add_argument(
        "--source-archive",
        type=Path,
        help="Optional clean handoff archive to bind into evidence.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3j-pre-live/m3j1-live-gate-risk-controls-evidence.json"),
    )
    args = parser.parse_args()

    root = repo_root()
    git = run_text(["git", "rev-parse", "HEAD"], root)
    if git["exit_code"] != 0:
        print(git["stderr_tail"], file=sys.stderr)
        return git["exit_code"]

    source_commit = git["stdout_tail"].strip()
    checks = [run_text(command, root) for command in CHECKS]
    archive_summary = None
    if args.source_archive:
        archive_path = (root / args.source_archive).resolve()
        if not archive_path.exists():
            print(f"source archive does not exist: {archive_path}", file=sys.stderr)
            return 2
        archive_summary = clean_handoff_summary(archive_path, source_commit)

    summary = evidence_summary(root)
    all_checks_ok = all(check["exit_code"] == 0 for check in checks)
    archive_ok = archive_summary is None or archive_summary["clean"]
    policy_ok = (
        all(summary["required_patterns_present"].values())
        and all(summary["forbidden_patterns_absent"].values())
        and summary["m3j1_live_gate_design_ok"]
        and summary["operator_arm_design_ready"]
        and summary["kill_switch_design_ready"]
        and summary["max_orders_qty_loss_limits_ready"]
        and summary["scope_controls_ready"]
        and summary["live_micro_go_false"]
        and summary["no_live_boundary"]
        and summary["no_stop_sltp_bracket"]
        and summary["closure_documented"]
    )
    evidence_ready = all_checks_ok and archive_ok and policy_ok
    evidence = {
        "m3j_step": "M3j-1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": source_commit,
        "source_archive": archive_summary,
        "source_archive_name": archive_summary["archive_name"]
        if archive_summary
        else None,
        "source_archive_sha256": archive_summary["archive_sha256"]
        if archive_summary
        else None,
        "source_archive_content_binding_verified": archive_summary["clean"]
        if archive_summary
        else False,
        "summary": summary,
        "checks": checks,
        "all_checks_ok": all_checks_ok,
        "m3j1_live_gate_design_ok": summary["m3j1_live_gate_design_ok"],
        "operator_arm_design_ready": summary["operator_arm_design_ready"],
        "kill_switch_design_ready": summary["kill_switch_design_ready"],
        "max_orders_qty_loss_limits_ready": summary["max_orders_qty_loss_limits_ready"],
        "scope_controls_ready": summary["scope_controls_ready"],
        "live_micro_go": False,
        "no_live_boundary": summary["no_live_boundary"],
        "no_stop_sltp_bracket": summary["no_stop_sltp_bracket"],
        "real_finam_order_endpoint_used": False,
        "external_order_endpoint_allowed": False,
        "runtime_live_attachment_allowed": False,
        "live_ready_allowed": False,
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
