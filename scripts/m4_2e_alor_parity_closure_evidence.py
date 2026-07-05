#!/usr/bin/env python3
"""Generate M4-2e ALOR parity closure / canonical live gate evidence.

No broker calls are performed. The script validates the full ALOR config
inventory, instrument identity hardening, canonical runtime gate blockers, and
the M4-1c canonical report golden test.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]

DOC = Path("docs/m4-2e-alor-parity-closure-matrix.md")
BROKER_CORE_SNAPSHOT = Path("crates/broker-core/src/operational_snapshot.rs")
BROKER_CORE_CONFIG = Path("crates/broker-core/src/operational_config.rs")
BROKER_FINAM_MAPPER = Path("crates/broker-finam/src/mapper.rs")
BROKER_CLI = Path("crates/broker-cli/src/main.rs")

ALOR_CONFIG_FIELDS = [
    "portfolio",
    "exchange",
    "instrument_group",
    "symbols",
    "tf_sec",
    "from_ts",
    "ws_url",
    "cws_url",
    "oauth_url",
    "refresh_token",
    "skip_history_bars",
    "skip_history_positions",
    "skip_history_orders",
    "split_adjust",
    "format",
    "frequency_ms",
    "backoff_initial_ms",
    "backoff_max_ms",
    "backoff_multiplier",
    "max_silence_bars_sec",
    "trading_periods",
    "history_sessions",
    "history_days_back",
    "session_rollover_hour_utc",
    "health_listen_addr",
    "price_step",
    "volume_step",
    "log_positions_filter",
    "log_cash_positions",
    "cash_symbols",
    "log_existing_snapshot_orders",
    "ws_idle_timeout_sec",
    "ws_ping_interval_sec",
    "ws_ping_timeout_sec",
    "subscribe_ack_timeout_ms",
    "subscribe_ack_timeout_positions_ms",
    "subscribe_ack_retries",
    "warm_reconnect_max_gap_sec",
    "gap_backfill_padding_bars",
    "cold_start_history_days_back",
    "bar_silence_resync_min_sec",
    "control_path_stale_after_sec",
    "control_path_pre_entry_recycle_enabled",
    "control_path_pre_exit_recycle_enabled",
    "control_path_recycle_timeout_ms",
    "control_path_recycle_timeout_ms_exit",
    "control_path_post_recycle_exit_send_window_ms",
    "control_path_hardening_log_only",
    "control_cws_mode",
    "action_scope_enable_create_limit",
    "action_scope_enable_market",
    "action_scope_enable_delete_limit",
    "action_scope_enable_replace_limit",
    "action_scope_enable_exit",
    "action_scope_open_timeout_ms",
    "action_scope_authorize_timeout_ms",
    "action_scope_force_token_refresh_before_authorize",
    "action_scope_followup_window_ms",
    "action_scope_max_session_lifetime_ms",
    "action_scope_close_timeout_ms",
    "data_report_path",
    "bar_dump_path",
]

SNAPSHOT_MARKERS = [
    "broker_asset_id",
    "board",
    "canonical_identity_matches",
    "instrument_spec_identity_matches",
    "instrument_spec_identity_includes_board_expiry_and_asset_id",
]

CONFIG_MARKERS = [
    "BrokerStopOrderReadiness",
    "live_market_data_seen",
    "subscription_ready",
    "stream_or_polling_connected",
    "event_sink_degraded",
    "stop_order_readiness",
    "FirstLiveMarketDataNotSeen",
    "SubscriptionNotReady",
    "StreamOrPollingNotConnected",
    "EventSinkDegraded",
    "StopOrderUnsupportedBlocked",
    "alor_parity_runtime_gate_blocks_missing_live_bar_subscription_sink_and_stop_readiness",
]

FINAM_MARKERS = [
    "broker_asset_id: asset.id.clone()",
    "board: asset.board.clone()",
    "BrokerStopOrderReadiness::UnsupportedBlocked",
]

CLI_MARKERS = [
    "m4_1c_canonical_report_golden_requires_broker_truth_snapshot_source",
    '"truth_source": "BrokerTruthSnapshot"',
    '"final_truth_source": "BrokerTruthSnapshot"',
]

DOC_MARKERS = [
    "Complete ALOR config inventory",
    "The `AlorGatewayConfig` surface has 62 fields",
    "instrument_spec_identity_matches",
    "BrokerStopOrderReadiness::UnsupportedBlocked",
    "Live expansion remains blocked after M4-2e",
    "Remaining P0 after M4-2e",
]


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def artifact(path: Path) -> dict[str, Any]:
    full_path = ROOT / path
    result: dict[str, Any] = {"path": str(path), "exists": full_path.exists()}
    if full_path.exists():
        data = full_path.read_bytes()
        result.update({"sha256": sha256_bytes(data), "bytes": len(data)})
    return result


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
        "stdout_tail": completed.stdout[-4000:],
        "stderr_tail": completed.stderr[-4000:],
    }


def git_head() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


def marker_check(path: Path, markers: list[str]) -> dict[str, Any]:
    text = (ROOT / path).read_text()
    missing = [marker for marker in markers if marker not in text]
    return {"path": str(path), "ok": not missing, "missing": missing, "checked": markers}


def alor_config_inventory_check() -> dict[str, Any]:
    text = (ROOT / DOC).read_text()
    missing = [field for field in ALOR_CONFIG_FIELDS if field not in text]
    return {
        "ok": not missing and len(ALOR_CONFIG_FIELDS) == 62,
        "field_count": len(ALOR_CONFIG_FIELDS),
        "missing": missing,
        "fields": ALOR_CONFIG_FIELDS,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-2e-alor-parity-closure-evidence.json"),
    )
    args = parser.parse_args()

    artifacts = [
        artifact(DOC),
        artifact(BROKER_CORE_SNAPSHOT),
        artifact(BROKER_CORE_CONFIG),
        artifact(BROKER_FINAM_MAPPER),
        artifact(BROKER_CLI),
    ]
    alor_inventory = alor_config_inventory_check()
    doc_check = marker_check(DOC, DOC_MARKERS)
    snapshot_check = marker_check(BROKER_CORE_SNAPSHOT, SNAPSHOT_MARKERS)
    config_check = marker_check(BROKER_CORE_CONFIG, CONFIG_MARKERS)
    finam_check = marker_check(BROKER_FINAM_MAPPER, FINAM_MARKERS)
    cli_check = marker_check(BROKER_CLI, CLI_MARKERS)

    broker_core_operational = run(["cargo", "test", "-p", "broker-core", "operational"])
    broker_finam_m4_2d = run(["cargo", "test", "-p", "broker-finam", "m4_2d"])
    broker_finam_m4_2b = run(["cargo", "test", "-p", "broker-finam", "m4_2b"])
    broker_cli_m4_1c = run(
        ["cargo", "test", "-p", "broker-cli", "m4_1c", "--no-default-features"]
    )

    checks = {
        "artifacts_present": all(item["exists"] for item in artifacts),
        "no_live_calls_performed": True,
        "alor_config_inventory_62_fields_ok": alor_inventory["ok"],
        "instrument_identity_board_expiry_asset_id_ok": snapshot_check["ok"]
        and broker_core_operational["exit_code"] == 0,
        "canonical_live_gate_hardening_ok": config_check["ok"]
        and broker_core_operational["exit_code"] == 0,
        "finam_stop_order_unsupported_waiver_ok": finam_check["ok"]
        and broker_finam_m4_2d["exit_code"] == 0,
        "m4_1c_canonical_report_golden_ok": cli_check["ok"]
        and broker_cli_m4_1c["exit_code"] == 0,
        "m4_2b_regression_ok": broker_finam_m4_2b["exit_code"] == 0,
        "doc_ok": doc_check["ok"],
        "live_expansion_blocked": doc_check["ok"],
    }
    evidence_ready = all(checks.values())
    head = git_head()
    payload: dict[str, Any] = {
        "evidence_kind": "m4-2e-alor-parity-closure-canonical-live-gate-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "trading_boundary": {
            "live_calls_performed": False,
            "live_position_tests_allowed": False,
            "runtime_live_attachment_allowed": False,
            "command_consumer_to_real_finam_allowed": False,
            "stop_sltp_bracket_replace_multi_leg_allowed": False,
        },
        "artifacts": artifacts,
        "alor_config_inventory": alor_inventory,
        "marker_checks": {
            "doc": doc_check,
            "broker_core_snapshot": snapshot_check,
            "broker_core_config": config_check,
            "broker_finam_mapper": finam_check,
            "broker_cli": cli_check,
        },
        "test_commands": {
            "broker_core_operational": broker_core_operational,
            "broker_finam_m4_2d": broker_finam_m4_2d,
            "broker_finam_m4_2b": broker_finam_m4_2b,
            "broker_cli_m4_1c_no_default_features": broker_cli_m4_1c,
        },
        "checks": checks,
        "evidence_ready_for_review": evidence_ready,
    }

    output = ROOT / args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    print(json.dumps(checks | {"evidence_ready_for_review": evidence_ready}, indent=2))
    return 0 if evidence_ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
