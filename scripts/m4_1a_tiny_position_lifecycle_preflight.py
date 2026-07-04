#!/usr/bin/env python3
"""Generate M4-1a tiny position lifecycle preflight runbook evidence.

No broker calls are performed. This is a design/no-send runbook evidence
package for a future separately approved tiny position lifecycle test.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


DOC = Path("docs/m4-1a-tiny-position-lifecycle-preflight.md")
M4_0 = Path("reports/m4/m4-0-design-only-expansion-plan-evidence.json")
M3J20A = Path("reports/m3j-pre-live/m3j20a-operator-signoff-freeze.json")

COMMANDS = {
    "forbidden_surface_scan": ["bash", "scripts/forbidden_surface_scan.sh"],
    "forbidden_surface_negative_harness": ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    "order_endpoint_scanner_transition_spec": [
        "bash",
        "scripts/order_endpoint_scanner_transition_spec.sh",
    ],
    "script_py_compile": [
        "python3",
        "-m",
        "py_compile",
        "scripts/m4_1a_tiny_position_lifecycle_preflight.py",
    ],
}

REQUIRED_DOC_MARKERS = [
    "M4-1a defines the no-send runbook",
    "does not send broker orders",
    "does not open a position",
    "entry qty=1 -> broker position snapshot -> exit qty=1 -> final flat reconciliation",
    "Entry type: marketable limit preferred for first test",
    "Maximum position lifetime: 30-120 seconds",
    "Required explicit operator approval text",
    "active/unknown/orphan orders = `0`",
    "positions = `0`",
    "command-consumer-to-real-FINAM disabled",
    "continuous runtime-live disabled",
    "Stop/SLTP/bracket/replace/multi-leg blocked",
    "do not retry blindly",
    "M4-1b tiny position lifecycle no-send preflight evidence",
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
        default=Path("reports/m4/m4-1a-tiny-position-lifecycle-preflight-evidence.json"),
    )
    parser.add_argument("--skip-command-checks", action="store_true")
    args = parser.parse_args()

    doc = DOC.read_text() if DOC.exists() else ""
    markers = {marker: marker in doc for marker in REQUIRED_DOC_MARKERS}
    m4_0 = load(M4_0) if M4_0.exists() else {}
    m3j20a = load(M3J20A) if M3J20A.exists() else {}
    command_results = {}
    if not args.skip_command_checks:
        command_results = {name: run_command(cmd) for name, cmd in COMMANDS.items()}

    m4_0_closed = (
        m4_0.get("evidence_ready_for_review") is True
        and m4_0.get("review_policy", {}).get("m4_0_authorizes_live_orders") is False
        and m4_0.get("recommended_next_stage")
        == "M4-1a tiny position lifecycle preflight design / no-send runbook"
    )
    m3j20a_closed = (
        m3j20a.get("operator_signoff_status") == "SignedOff"
        and m3j20a.get("closure_ready_for_review") is True
    )
    no_live_expansion = {
        "m4_1a_performs_broker_calls": False,
        "position_opened": False,
        "live_entry_authorized": False,
        "market_order_authorized": False,
        "continuous_runtime_live_enabled": False,
        "command_consumer_to_real_finam_enabled": False,
        "stop_sltp_bracket_replace_multileg_enabled": False,
        "portfolio_live_strategy_enabled": False,
    }
    commands_ok = bool(command_results) and all(result["ok"] for result in command_results.values())
    doc_ok = DOC.exists() and all(markers.values())
    evidence_ready = all(
        [
            doc_ok,
            commands_ok,
            m4_0_closed,
            m3j20a_closed,
            all(value is False for value in no_live_expansion.values()),
        ]
    )
    head = git_head()
    payload: dict[str, Any] = {
        "evidence_kind": "m4-1a-tiny-position-lifecycle-preflight-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "artifact_manifest": {
            "m4_1a_doc": artifact(DOC),
            "m4_0_design_evidence": artifact(M4_0),
            "m3j20a_operator_signoff_freeze": artifact(M3J20A),
        },
        "doc_markers": markers,
        "command_results": command_results,
        "closed_prerequisites": {
            "m4_0_closed": m4_0_closed,
            "m3j20a_closed": m3j20a_closed,
        },
        "proposed_future_live_scope": {
            "symbol": "IMOEXF@RTSX",
            "qty": "1",
            "entry_type_preferred": "marketable_limit",
            "exit_type_preferred": "marketable_limit",
            "max_position_lifetime_sec_range": "30-120",
            "max_orders_total": 2,
            "strategy_runtime": "Disabled",
            "command_consumer_to_real_finam": "Disabled",
            "stop_sltp_bracket_replace_multileg": "Blocked",
        },
        "no_live_expansion": no_live_expansion,
        "review_policy": {
            "m4_1a_authorizes_live_entry": False,
            "m4_1a_authorizes_market_order": False,
            "m4_1a_authorizes_command_consumer_to_real_finam": False,
            "m4_1a_authorizes_runtime_live": False,
            "future_m4_1_actual_requires_explicit_operator_approval": True,
            "next_stage": "M4-1b tiny position lifecycle no-send preflight evidence",
        },
        "checks": {
            "doc_ok": doc_ok,
            "commands_ok": commands_ok,
            "m4_0_closed": m4_0_closed,
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
