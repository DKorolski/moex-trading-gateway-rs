#!/usr/bin/env python3
"""Generate M4-3r-a ALOR paper/ledger oracle extraction evidence.

This script is source-only. It reads local source and sanitized fixtures only.
It does not call FINAM, ALOR, Redis, WebSocket, SSH, or order endpoints.
"""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
ALOR_ROOT = ROOT.parent / "alor_project" / "bybit_barter_test_sanitized" / "alor-rs-main"
DOC = ROOT / "docs" / "m4-3r-a-alor-paper-ledger-oracle.md"
FIXTURE = ROOT / "fixtures" / "alor" / "paper_ledger_synthetic_round.json"
REPORT = ROOT / "reports" / "m4" / "m4-3r-a-alor-paper-ledger-oracle-evidence.json"

FORBIDDEN_LIVE_LIKE = re.compile(
    r"(75" r"02[A-Z0-9]*|190" r"9892|63" r"170[A-Z0-9/]*|tapi_[sa]k_[A-Za-z0-9_-]+|"
    r"eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{10,})"
)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def rel(path: Path) -> str:
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        try:
            return str(path.relative_to(ALOR_ROOT.parent))
        except ValueError:
            return str(path)


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
    result: dict[str, Any] = {"path": rel(path), "exists": path.exists()}
    if not path.exists():
        result.update({"ok": False, "missing": markers, "checked": markers})
        return result
    text = path.read_text()
    missing = [marker for marker in markers if marker not in text]
    result.update(
        {
            "ok": not missing,
            "missing": missing,
            "checked": markers,
            "sha256": sha256_file(path),
        }
    )
    return result


def forbidden_scan(path: Path) -> dict[str, Any]:
    text = path.read_text()
    matches = sorted(set(FORBIDDEN_LIVE_LIKE.findall(text)))
    return {"path": rel(path), "ok": not matches, "matches": matches}


def fixture_check() -> dict[str, Any]:
    fixture = json.loads(FIXTURE.read_text())
    orders = fixture.get("orders", [])
    trades = fixture.get("trades", [])
    positions = fixture.get("positions", [])
    closed_trades = fixture.get("closed_trades", [])
    invariants = fixture.get("invariants", {})

    checks = {
        "fixture_kind": fixture.get("fixture_kind") == "synthetic_alor_paper_ledger_round_v1",
        "sanitized": fixture.get("sanitized") is True,
        "source_is_synthetic": fixture.get("source") == "synthetic",
        "symbol": fixture.get("symbol") == "IMOEXF",
        "orders_count": len(orders) == 2,
        "trades_count": len(trades) == 2,
        "positions_count": len(positions) == 1,
        "closed_trade_count": len(closed_trades) == 1,
        "final_position_flat": invariants.get("final_position_flat") is True
        and positions
        and positions[0].get("qty") == "0",
        "broker_native_order_ids_absent": invariants.get("broker_native_order_ids") is False
        and all(str(order.get("order_id", "")).startswith("PAPER_ORDER_") for order in orders),
        "live_surface_absent": invariants.get("live_order_surface") is False,
        "live_account_or_portfolio_absent": invariants.get("contains_live_account_or_portfolio") is False,
        "buy_then_sell_round_trip": [order.get("side") for order in orders] == ["buy", "sell"],
        "closed_trade_pnl": closed_trades
        and closed_trades[0].get("pnl_gross") == "2.0"
        and closed_trades[0].get("pnl_net") == "2.0",
    }
    return {
        "ok": all(bool(value) for value in checks.values()),
        "checks": checks,
        "fixture_sha256": sha256_file(FIXTURE),
    }


def main() -> int:
    commands = {
        "python_compile": run(
            ["python3", "-m", "py_compile", "scripts/m4_3r_a_alor_paper_ledger_oracle_evidence.py"]
        ),
        "fixture_json_valid": run(
            [
                "python3",
                "-c",
                "import json; json.load(open('fixtures/alor/paper_ledger_synthetic_round.json')); print('ok')",
            ]
        ),
    }

    source_checks = {
        "doc_markers": marker_check(
            DOC,
            [
                "M4-3r-a ALOR paper/ledger oracle extraction",
                "source-only oracle spec",
                "PaperExecutionMode",
                "HistorySim",
                "LiveOnly",
                "Synthetic paper feedback",
                "paper intent",
                "paper order",
                "paper position delta",
                "Risk-gate oracle",
                "normal_append",
                "M4-3r-b broker-neutral paper domain model",
            ],
        ),
        "alor_runtime_config_markers": marker_check(
            ALOR_ROOT / "strategy-runtime" / "src" / "lib.rs",
            [
                "pub enum TradeMode",
                "pub enum PaperExecutionMode",
                "pub struct PaperConfig",
                "pub struct ReplayConfig",
            ],
        ),
        "alor_strategy_host_markers": marker_check(
            ALOR_ROOT / "strategy-runtime" / "src" / "strategy_host.rs",
            [
                "pub enum Intent",
                "fn on_runtime_state_restored",
                "fn warmup_from_history",
                "pub struct RiskGateRuntimeState",
                "pub enum DataOrigin",
                "HistoryGap",
            ],
        ),
        "alor_runtime_loop_markers": marker_check(
            ALOR_ROOT / "strategy-runtime" / "src" / "runtime.rs",
            [
                "fn can_advance_paper_execution",
                "PaperExecutionMode::HistorySim",
                "PaperExecutionMode::LiveOnly",
                "record_non_live_intents",
                "simulate_fills",
                "simulate_intents",
                "Synthetic paper feedback",
                "persist_ledger_reports",
            ],
        ),
        "alor_trade_ledger_markers": marker_check(
            ALOR_ROOT / "strategy-runtime" / "src" / "trade_ledger.rs",
            [
                "pub struct TradeRecord",
                "pub struct OrderRecord",
                "pub struct ClosedTradeRecord",
                "pub struct TradeLedger",
                "pub fn record_order",
                "pub fn record_fill",
                "pub fn persist_reports",
                "fn apply_fill",
            ],
        ),
        "alor_risk_gate_store_markers": marker_check(
            ALOR_ROOT / "strategy-runtime" / "src" / "risk_gate_store.rs",
            [
                "RiskGateStartupMode",
                "BootstrapFromSeed",
                "NormalAppend",
                "RebuildFromHistory",
                "risk gate ledger stream strategy_id mismatch",
                "runtime.riskgate.sessions.",
            ],
        ),
        "alor_redis_transport_markers": marker_check(
            ALOR_ROOT / "strategy-runtime" / "src" / "redis_transport.rs",
            [
                "pub async fn xadd_state",
                "MAXLEN",
                "pub async fn publish_command_and_state",
                "redis::pipe()",
                ".atomic()",
            ],
        ),
        "alor_state_markers": marker_check(
            ALOR_ROOT / "strategy-runtime" / "src" / "state.rs",
            [
                "pub enum StrategyState",
                "pub struct StrategyStateEnvelope",
                "pub enum StrategyStateEnvelopeCompat",
                "from_strategy_state",
                "into_payload",
            ],
        ),
        "alor_hybrid_strategy_markers": marker_check(
            ALOR_ROOT / "strategy-runtime" / "src" / "strategies" / "hybrid_intraday_runtime.rs",
            [
                "fn can_execute_now",
                "TradeMode::Paper",
                "PaperExecutionMode::HistorySim",
                "PaperExecutionMode::LiveOnly",
                "fn warmup_from_history",
                "sync_state",
            ],
        ),
    }

    no_secret_checks = {
        "doc": forbidden_scan(DOC),
        "fixture": forbidden_scan(FIXTURE),
        "script": forbidden_scan(Path(__file__)),
    }
    fixture = fixture_check()

    ok = (
        all(command["exit_code"] == 0 for command in commands.values())
        and all(check["ok"] for check in source_checks.values())
        and all(check["ok"] for check in no_secret_checks.values())
        and fixture["ok"]
    )

    report = {
        "evidence_kind": "m4-3r-a-alor-paper-ledger-oracle-extraction-source-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "source_inventory_complete": all(
            source_checks[key]["ok"]
            for key in [
                "alor_runtime_config_markers",
                "alor_strategy_host_markers",
                "alor_runtime_loop_markers",
                "alor_trade_ledger_markers",
                "alor_risk_gate_store_markers",
                "alor_redis_transport_markers",
                "alor_state_markers",
                "alor_hybrid_strategy_markers",
            ]
        ),
        "source_checks": source_checks,
        "fixture_check": fixture,
        "no_secret_checks": no_secret_checks,
        "commands": commands,
        "boundary": {
            "source_only": True,
            "finam_post_orders_enabled": False,
            "finam_delete_orders_enabled": False,
            "live_orders_enabled": False,
            "runtime_live_ready_enabled": False,
            "command_consumer_to_real_finam_enabled": False,
            "stop_sltp_bracket_enabled": False,
            "alor_live_access_required": False,
        },
        "artifacts": [
            {"path": rel(DOC), "sha256": sha256_file(DOC)},
            {"path": rel(FIXTURE), "sha256": sha256_file(FIXTURE)},
            {"path": rel(Path(__file__)), "sha256": sha256_file(Path(__file__))},
        ],
        "next_stage": "M4-3r-b broker-neutral paper domain model",
    }

    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"ok": ok, "report": rel(REPORT)}, indent=2, sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
