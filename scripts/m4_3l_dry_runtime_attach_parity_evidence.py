#!/usr/bin/env python3
"""Generate M4-3l dry runtime attach / M1-M10 parity source evidence.

This script is source-only. It does not call FINAM, ALOR, Redis, WebSocket, SSH,
or order endpoints. Active-session ALOR native 10m vs FINAM derived 10m runtime
evidence must be collected separately.
"""

from __future__ import annotations

import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "reports" / "m4" / "m4-3l-dry-runtime-attach-parity-evidence.json"


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
        "stdout_tail": completed.stdout[-4000:],
        "stderr_tail": completed.stderr[-4000:],
    }


def git_head() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


def marker_check(path: Path, markers: list[str]) -> dict[str, Any]:
    full_path = ROOT / path
    result: dict[str, Any] = {"path": str(path), "exists": full_path.exists()}
    if not full_path.exists():
        result.update({"ok": False, "missing": markers, "checked": markers})
        return result
    text = full_path.read_text()
    missing = [marker for marker in markers if marker not in text]
    result.update({"ok": not missing, "missing": missing, "checked": markers})
    return result


def main() -> int:
    doc = Path("docs/m4-3l-dry-runtime-attach-m1-m10-parity.md")
    gateway = Path("crates/finam-gateway/src/lib.rs")
    readme = Path("README.md")
    alor_bars_subscriptions = Path(
        "../alor_project/bybit_barter_test_sanitized/alor-rs-main/alor-gateway/src/ws_subscriptions.rs"
    )
    alor_transport_runner = Path(
        "../alor_project/bybit_barter_test_sanitized/alor-rs-main/alor-gateway/src/bin/alor_gateway_transport_runner.rs"
    )

    source_checks = {
        "doc_markers": marker_check(
            doc,
            [
                "M4-3l dry runtime attach / M1-M10 parity",
                "BarsGetAndSubscribe",
                "tf     = cfg.tf_sec",
                "cfg.tf_sec = 600",
                "FINAM WS final M1 bars",
                "CanonicalBarAggregator(target = 600s)",
                "Raw FINAM 1-minute bars must not become strategy-facing decisions",
                "FINAM-native 10-minute bars are intentionally not treated as equivalent yet",
                "no order endpoints",
            ],
        ),
        "gateway_markers": marker_check(
            gateway,
            [
                "M43lStrategyBarSourceMode",
                "AlorNativeBarsGetAndSubscribeTf600",
                "FinamDerivedM1ToM10",
                "FinamNativeM10CharacterizationPending",
                "m4_3l_adapt_strategy_10m_dry_input",
                "m4_3l_dry_runtime_attach_parity_report",
                "m4_3l_rejects_raw_finam_m1_as_strategy_facing_10m_input",
                "m4_3l_accepts_canonical_m1_to_m10_bar_as_strategy_facing_dry_input",
                "m4_3l_parity_report_requires_raw_m1_reject_and_canonical_m10_accept",
            ],
        ),
        "readme_markers": marker_check(
            readme,
            [
                "M4-3l",
                "ALOR native 10m",
                "FINAM derived M1-to-10m",
                "raw FINAM M1",
            ],
        ),
        "alor_subscription_oracle_markers": marker_check(
            alor_bars_subscriptions,
            [
                "BarsGetAndSubscribe",
                "\"tf\": cfg.tf_sec",
                "\"skipHistory\": skip_history",
            ],
        ),
        "alor_stream_name_markers": marker_check(
            alor_transport_runner,
            [
                "format!(\"{}m\", cfg.tf_sec / 60)",
                "md.bars.{portfolio}.{tf_label}",
            ],
        ),
    }

    commands = {
        "python_compile": run(
            ["python3", "-m", "py_compile", "scripts/m4_3l_dry_runtime_attach_parity_evidence.py"]
        ),
        "targeted_m4_3l_tests": run(
            ["cargo", "test", "-p", "finam-gateway", "m4_3l", "--", "--nocapture"]
        ),
    }

    artifacts = [
        {"path": str(doc), "sha256": sha256_file(ROOT / doc)},
        {"path": str(gateway), "sha256": sha256_file(ROOT / gateway)},
        {"path": str(readme), "sha256": sha256_file(ROOT / readme)},
        {
            "path": str(alor_bars_subscriptions),
            "sha256": sha256_file(ROOT / alor_bars_subscriptions),
        },
        {
            "path": str(alor_transport_runner),
            "sha256": sha256_file(ROOT / alor_transport_runner),
        },
    ]

    ok = (
        all(check.get("ok") for check in source_checks.values())
        and all(command["exit_code"] == 0 for command in commands.values())
    )
    report = {
        "evidence_kind": "m4-3l-dry-runtime-attach-m1-m10-parity-source-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "source_checks": source_checks,
        "commands": commands,
        "artifacts": artifacts,
        "alor_oracle_source_mode": "AlorNativeBarsGetAndSubscribeTf600",
        "finam_source_mode": "FinamDerivedM1ToM10",
        "finam_native_m10_characterization_pending": True,
        "raw_finam_m1_strategy_facing_rejected": True,
        "canonical_m10_strategy_facing_accepted": True,
        "runtime_live_attachment_allowed": False,
        "live_ready_allowed": False,
        "external_order_endpoint_allowed": False,
        "real_finam_order_endpoint_used": False,
        "command_consumer_to_real_finam_enabled": False,
        "post_delete_calls_performed": False,
        "live_orders_performed": False,
        "next_stage": "M4-3m active-session ALOR native 10m vs FINAM derived 10m evidence, still no live orders",
    }

    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
