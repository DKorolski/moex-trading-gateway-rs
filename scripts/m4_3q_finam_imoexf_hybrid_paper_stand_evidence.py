#!/usr/bin/env python3
"""Generate M4-3q FINAM IMOEXF hybrid paper-shadow stand source evidence.

This script is source-only. It does not call FINAM, ALOR, Redis, WebSocket, SSH,
or order endpoints.
"""

from __future__ import annotations

import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / "docs" / "m4-3q-finam-imoexf-hybrid-paper-shadow-stand.md"
CONFIG = ROOT / "config" / "finam-imoexf-hybrid-paper-shadow.vps.example.json"
WS_CONFIG = ROOT / "config" / "finam-imoexf-ws-shadow-paper.vps.example.json"
REPORT = ROOT / "reports" / "m4" / "m4-3q-finam-imoexf-hybrid-paper-stand-evidence.json"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def run(cmd: list[str]) -> dict[str, Any]:
    completed = subprocess.run(
        cmd,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    return {
        "cmd": cmd,
        "exit_code": completed.returncode,
        "stdout_sha256": sha256_bytes(completed.stdout.encode()),
        "stderr_sha256": sha256_bytes(completed.stderr.encode()),
        "stdout_tail": completed.stdout[-2000:],
        "stderr_tail": completed.stderr[-2000:],
    }


def git_head() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


def marker_check(path: Path, markers: list[str]) -> dict[str, Any]:
    result: dict[str, Any] = {"path": str(path.relative_to(ROOT)), "exists": path.exists()}
    if not path.exists():
        result.update({"ok": False, "missing": markers, "checked": markers})
        return result
    text = path.read_text()
    missing = [marker for marker in markers if marker not in text]
    result.update({"ok": not missing, "missing": missing, "checked": markers})
    return result


def load_config() -> dict[str, Any]:
    return json.loads(CONFIG.read_text())


def stream_values(config: dict[str, Any]) -> list[str]:
    values: list[str] = []
    values.extend(config["finam_ws_shadow"]["streams"].values())
    values.append(config["canonical_10m"]["source_stream"])
    values.append(config["canonical_10m"]["target_stream"])
    strategy = config["strategy"]
    for key in [
        "paper_intents_stream",
        "paper_acks_stream",
        "runtime_state_stream",
        "orders_stream",
        "trades_stream",
        "positions_stream",
    ]:
        values.append(strategy[key])
    values.append(config["risk_gate"]["ledger_key"])
    values.append(config["risk_gate"]["state_key"])
    return values


def config_checks(config: dict[str, Any]) -> dict[str, Any]:
    streams = stream_values(config)
    alor_streams = list(config["alor_oracle"].values())
    live_disabled = (
        config["mode"]["live_orders"] is False
        and config["mode"]["live_ready_allowed"] is False
        and config["mode"]["command_consumer_to_real_finam"] is False
        and config["mode"]["order_placement_enabled"] is False
        and config["mode"]["stop_sltp_bracket_enabled"] is False
        and config["mode"]["replace_enabled"] is False
    )
    checks = {
        "stand_kind": config.get("stand_kind") == "finam_imoexf_hybrid_paper_shadow",
        "symbol": config.get("symbol") == "IMOEXF@RTSX",
        "portfolio": config.get("portfolio") == "ALOR_LIVE_ORACLE_PORTFOLIO",
        "strategy_id": config["strategy"]["strategy_id"] == "hybrid_imoexf",
        "strategy_kind": config["strategy"]["strategy_kind"] == "hybrid_intraday",
        "strategy_profile": config["strategy"]["profile"] == "imoexf_primary_riskgate_high180_lb120",
        "qty_matches_current_alor_baseline": config["strategy"]["qty"] == 3.0,
        "live_surfaces_disabled": live_disabled,
        "all_finam_streams_isolated": all(value.startswith("finam_imoexf_paper:") for value in streams),
        "raw_m1_strategy_input_blocked": config["canonical_10m"]["raw_m1_strategy_input_allowed"] is False,
        "finam_native_m10_strategy_input_blocked": (
            config["canonical_10m"]["finam_native_m10_strategy_input_allowed"] is False
        ),
        "derived_m10_requires_complete_aggregation": (
            config["canonical_10m"]["aggregation_complete_required"] is True
        ),
        "derived_m10_requires_gap_absence": config["canonical_10m"]["gap_absence_proven_required"] is True,
        "riskgate_shadow_normal_append": (
            config["risk_gate"]["mr_variant"] == "high180"
            and config["risk_gate"]["mr_gate_policy"] == "shadow_pnl_lb120_positive"
            and config["risk_gate"]["risk_gate_mode"] == "normal_append"
            and config["risk_gate"]["enforced_gate_enabled"] is False
        ),
        "alor_oracle_references_current_streams": (
            "md.bars.<ALOR_LIVE_ORACLE_PORTFOLIO>.10m" in alor_streams
            and "cmd.orders.<ALOR_LIVE_ORACLE_PORTFOLIO>" in alor_streams
            and "runtime.state.hybrid_intraday.live.riskgate_shadow.imoexf.<ALOR_LIVE_ORACLE_PORTFOLIO>"
            in alor_streams
        ),
    }
    return {
        "ok": all(checks.values()),
        "checks": checks,
        "finam_streams": sorted(set(streams)),
        "alor_oracle_streams": sorted(value for value in alor_streams if isinstance(value, str)),
    }


def main() -> int:
    config = load_config()
    commands = {
        "python_compile": run(
            ["python3", "-m", "py_compile", "scripts/m4_3q_finam_imoexf_hybrid_paper_stand_evidence.py"]
        ),
        "config_json_valid": run(
            [
                "python3",
                "-c",
                (
                    "import json; "
                    "json.load(open('config/finam-imoexf-hybrid-paper-shadow.vps.example.json')); "
                    "print('ok')"
                ),
            ]
        ),
        "ws_runner_config_json_valid": run(
            [
                "python3",
                "-c",
                (
                    "import json; "
                    "json.load(open('config/finam-imoexf-ws-shadow-paper.vps.example.json')); "
                    "print('ok')"
                ),
            ]
        ),
    }
    source_checks = {
        "doc_markers": marker_check(
            DOC,
            [
                "M4-3q FINAM IMOEXF hybrid paper-shadow stand",
                "finam_imoexf_paper:*",
                "config/finam-imoexf-ws-shadow-paper.vps.example.json",
                "FinamDerivedM1ToM10",
                "risk_gate_mode   = normal_append",
                "not enforced filtering",
                "ALOR remains the live operational oracle",
                "M4-3q must not",
                "command-consumer-to-real-FINAM",
            ],
        ),
        "m4_3r_plan_markers": marker_check(
            ROOT / "docs" / "m4-3r-alor-paper-ledger-extraction-and-local-finam-runtime-plan.md",
            [
                "M4-3r ALOR paper/ledger extraction and local FINAM runtime integration plan",
                "ALOR paper/replay/ledger semantics",
                "PaperIntent",
                "PaperLedgerSnapshot",
                "FINAM WS shadow",
                "runtime state is populated",
                "Freeze replay dataset",
                "no FINAM live orders",
            ],
        ),
        "config_markers": marker_check(
            CONFIG,
            [
                "finam_imoexf_hybrid_paper_shadow",
                "IMOEXF@RTSX",
                "hybrid_imoexf",
                "imoexf_primary_riskgate_high180_lb120",
                "FinamDerivedM1ToM10",
                "normal_append",
                "runtime.state.hybrid_intraday.live.riskgate_shadow.imoexf.<ALOR_LIVE_ORACLE_PORTFOLIO>",
            ],
        ),
        "ws_runner_config_markers": marker_check(
            WS_CONFIG,
            [
                "finam-imoexf-hybrid-paper-shadow-vps",
                "IMOEXF@RTSX",
                "TIME_FRAME_M1",
                "finam_imoexf_paper:ws:health",
                "finam_imoexf_paper:ws:market_data",
                "finam_imoexf_paper:ws:command_acks_disabled",
            ],
        ),
    }
    checks = config_checks(config)
    ok = (
        checks["ok"]
        and all(command["exit_code"] == 0 for command in commands.values())
        and all(check["ok"] for check in source_checks.values())
    )
    report = {
        "evidence_kind": "m4-3q-finam-imoexf-hybrid-paper-shadow-stand-source-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "config_checks": checks,
        "source_checks": source_checks,
        "commands": commands,
        "artifacts": [
            {"path": str(CONFIG.relative_to(ROOT)), "sha256": sha256_file(CONFIG)},
            {"path": str(WS_CONFIG.relative_to(ROOT)), "sha256": sha256_file(WS_CONFIG)},
            {"path": str(DOC.relative_to(ROOT)), "sha256": sha256_file(DOC)},
            {
                "path": "docs/m4-3r-alor-paper-ledger-extraction-and-local-finam-runtime-plan.md",
                "sha256": sha256_file(
                    ROOT
                    / "docs"
                    / "m4-3r-alor-paper-ledger-extraction-and-local-finam-runtime-plan.md"
                ),
            },
        ],
        "boundary": {
            "live_orders_enabled": False,
            "live_ready_allowed": False,
            "command_consumer_to_real_finam_enabled": False,
            "order_placement_enabled": False,
            "stop_sltp_bracket_enabled": False,
            "replace_enabled": False,
            "alor_oracle_read_only": True,
        },
        "next_stage": "M4-3r ALOR paper/ledger extraction and local FINAM executable paper runtime adapter / no-send",
    }
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
