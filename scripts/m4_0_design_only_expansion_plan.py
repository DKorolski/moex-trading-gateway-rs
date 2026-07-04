#!/usr/bin/env python3
"""Generate M4-0 design-only expansion plan evidence.

No broker calls are performed. The evidence binds the M4-0 document to the
closed M3j/M3j-20a artifacts and verifies that live runtime/consumer/bracket
boundaries remain disabled.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


DOC = Path("docs/m4-0-design-only-expansion-plan.md")
M3J20A = Path("reports/m3j-pre-live/m3j20a-operator-signoff-freeze.json")
M3J20 = Path("reports/m3j-pre-live/m3j20-working-limitcancel-evidence.json")
M3J19A = Path("reports/m3j-pre-live/m3j19a-actual-boundary-failure-matrix-evidence-rebound.json")

COMMANDS = {
    "forbidden_surface_scan": ["bash", "scripts/forbidden_surface_scan.sh"],
    "forbidden_surface_negative_harness": ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    "order_endpoint_scanner_transition_spec": [
        "bash",
        "scripts/order_endpoint_scanner_transition_spec.sh",
    ],
    "script_py_compile": ["python3", "-m", "py_compile", "scripts/m4_0_design_only_expansion_plan.py"],
}


REQUIRED_DOC_MARKERS = [
    "M4-0 is design-only",
    "does not perform broker calls",
    "continuous runtime-live remains disabled",
    "command-consumer-to-real-FINAM remains disabled",
    "Stop/SLTP/bracket/replace/multi-leg remain blocked",
    "M4-1 tiny position lifecycle design",
    "M4-2 economics / fees / EOD reconciliation",
    "M4-3 persistent live order audit",
    "M4-4 command-consumer-to-real-FINAM gate design",
    "M4-5 strategy runtime live-attachment policy",
    "M4-6 Stop/SLTP/bracket/replace/multi-leg research",
    "M4-1a tiny position lifecycle preflight design / no-send runbook",
]


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def artifact(path: Path) -> dict[str, Any]:
    result: dict[str, Any] = {"path": str(path), "exists": path.exists()}
    if path.exists():
        data = path.read_bytes()
        result.update({"sha256": sha256_bytes(data), "bytes": len(data)})
    return result


def load(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def run_command(cmd: list[str]) -> dict[str, Any]:
    completed = subprocess.run(cmd, text=True, capture_output=True)
    return {
        "cmd": cmd,
        "exit_code": completed.returncode,
        "ok": completed.returncode == 0,
        "stdout_tail": completed.stdout[-4000:],
        "stderr_tail": completed.stderr[-4000:],
    }


def git_head() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-0-design-only-expansion-plan-evidence.json"),
    )
    parser.add_argument("--skip-command-checks", action="store_true")
    args = parser.parse_args()

    doc_text = DOC.read_text() if DOC.exists() else ""
    marker_results = {marker: marker in doc_text for marker in REQUIRED_DOC_MARKERS}
    m3j20a = load(M3J20A) if M3J20A.exists() else {}
    m3j20 = load(M3J20) if M3J20.exists() else {}
    m3j19a = load(M3J19A) if M3J19A.exists() else {}

    command_results = {}
    if not args.skip_command_checks:
        command_results = {name: run_command(cmd) for name, cmd in COMMANDS.items()}

    m3j20a_closed = (
        m3j20a.get("operator_signoff_status") == "SignedOff"
        and m3j20a.get("operator_confirmations_all_true") is True
        and m3j20a.get("closure_ready_for_review") is True
        and m3j20a.get("stage_status", {}).get("m3j20a_operator_signoff_freeze") == "Closed"
    )
    m3j20_closed = (
        m3j20.get("evidence_ready_for_review") is True
        and m3j20.get("checks", {}).get("working_observation_ok") is True
        and m3j20.get("checks", {}).get("post_run_clean") is True
    )
    m3j19a_closed = (
        m3j19a.get("evidence_ready_for_review") is True
        and m3j19a.get("matrix_checks", {}).get("no_live_order_send") is True
    )
    no_live_expansion = {
        "m4_0_performs_broker_calls": False,
        "position_opened": False,
        "continuous_runtime_live_enabled": False,
        "command_consumer_to_real_finam_enabled": False,
        "stop_sltp_bracket_replace_multileg_enabled": False,
        "portfolio_live_strategy_enabled": False,
    }
    commands_ok = bool(command_results) and all(result["ok"] for result in command_results.values())
    doc_ok = DOC.exists() and all(marker_results.values())
    evidence_ready = all(
        [
            doc_ok,
            m3j20a_closed,
            m3j20_closed,
            m3j19a_closed,
            commands_ok,
            all(value is False for value in no_live_expansion.values()),
        ]
    )
    head = git_head()
    payload: dict[str, Any] = {
        "evidence_kind": "m4-0-design-only-expansion-plan-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "artifact_manifest": {
            "m4_0_doc": artifact(DOC),
            "m3j20a_operator_signoff_freeze": artifact(M3J20A),
            "m3j20_working_limitcancel_evidence": artifact(M3J20),
            "m3j19a_rebound_evidence": artifact(M3J19A),
        },
        "doc_markers": marker_results,
        "command_results": command_results,
        "closed_prerequisites": {
            "m3j19a_closed": m3j19a_closed,
            "m3j20_closed": m3j20_closed,
            "m3j20a_closed": m3j20a_closed,
        },
        "no_live_expansion": no_live_expansion,
        "proposed_m4_layers": [
            "M4-1 tiny position lifecycle design",
            "M4-2 economics / fees / EOD reconciliation",
            "M4-3 persistent live order audit",
            "M4-4 command-consumer-to-real-FINAM gate design",
            "M4-5 strategy runtime live-attachment policy",
            "M4-6 Stop/SLTP/bracket/replace/multi-leg research",
        ],
        "recommended_next_stage": "M4-1a tiny position lifecycle preflight design / no-send runbook",
        "review_policy": {
            "m4_0_authorizes_live_orders": False,
            "m4_0_authorizes_continuous_runtime_live": False,
            "m4_0_authorizes_command_consumer_to_real_finam": False,
            "m4_0_authorizes_stop_sltp_bracket": False,
            "future_live_position_test_requires_explicit_operator_approval": True,
        },
        "checks": {
            "doc_ok": doc_ok,
            "commands_ok": commands_ok,
            "m3j19a_closed": m3j19a_closed,
            "m3j20_closed": m3j20_closed,
            "m3j20a_closed": m3j20a_closed,
            "no_live_expansion": all(value is False for value in no_live_expansion.values()),
        },
        "evidence_ready_for_review": evidence_ready,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    print(json.dumps(payload["checks"] | {"evidence_ready_for_review": evidence_ready}, indent=2))
    return 0 if evidence_ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
