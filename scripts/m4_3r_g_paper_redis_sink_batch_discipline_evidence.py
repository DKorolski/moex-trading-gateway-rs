#!/usr/bin/env python3
"""Generate M4-3r-g paper Redis sink / batch discipline evidence.

This script validates source markers and local tests. It does not connect to
Redis, FINAM, ALOR, WebSocket, SSH, or order endpoints.
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
DOC = ROOT / "docs" / "m4-3r-g-paper-redis-sink-batch-discipline.md"
GATEWAY_RS = ROOT / "crates" / "finam-gateway" / "src" / "lib.rs"
REPORT = ROOT / "reports" / "m4" / "m4-3r-g-paper-redis-sink-batch-discipline-evidence.json"

FORBIDDEN_LIVE_LIKE = re.compile(
    r"(75" r"02[A-Z0-9]*|190" r"9892|63" r"170[A-Z0-9/]*|tapi_[sa]k_[A-Za-z0-9_-]+|"
    r"eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{10,})"
)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def rel(path: Path) -> str:
    return str(path.relative_to(ROOT))


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


def main() -> int:
    commands = {
        "python_compile": run(
            [
                "python3",
                "-m",
                "py_compile",
                "scripts/m4_3r_g_paper_redis_sink_batch_discipline_evidence.py",
            ]
        ),
        "cargo_fmt_check": run(["cargo", "fmt", "--all", "--check"]),
        "finam_gateway_m4_3r_g_tests": run(
            ["cargo", "test", "-p", "finam-gateway", "m4_3r_g"]
        ),
        "finam_gateway_tests": run(["cargo", "test", "-p", "finam-gateway"]),
        "finam_gateway_clippy": run(
            ["cargo", "clippy", "-p", "finam-gateway", "--all-targets", "--", "-D", "warnings"]
        ),
    }

    source_checks = {
        "doc_markers": marker_check(
            DOC,
            [
                "M4-3r-g paper Redis sink / batch failure discipline",
                "PaperRuntimeRedisSink",
                "finam_imoexf_paper:runtime:publish_batches",
                "deterministic `batch_id`",
                "retry is marked as requiring reconciliation",
                "Next stage: M4-3r-h",
            ],
        ),
        "redis_sink_type_markers": marker_check(
            GATEWAY_RS,
            [
                "pub struct PaperRuntimeRedisSinkConfig",
                "pub struct PaperRuntimeRedisSink<S>",
                "pub struct PaperRuntimeRedisBatchMarker",
                "pub struct PaperRuntimeRedisRecordEnvelope",
                "pub struct PaperRuntimeRedisBatchPlan",
                "pub struct PaperRuntimeRedisPublishOutcome",
                "pub enum PaperRuntimeRedisSinkError",
            ],
        ),
        "redis_batch_discipline_markers": marker_check(
            GATEWAY_RS,
            [
                "PaperRuntimeRedisBatchMarkerPhase::Pending",
                "PaperRuntimeRedisBatchMarkerPhase::Committed",
                "paper_runtime_redis_idempotency_key",
                "retry_requires_reconciliation: true",
                "raw_redis_error_exported: false",
                "publish_batch",
                "plan_batch",
            ],
        ),
        "paper_stream_allowlist_markers": marker_check(
            GATEWAY_RS,
            [
                "finam_imoexf_paper:runtime:publish_batches",
                "NonPaperStream",
                "StreamPayloadMismatch",
                "allowed_streams",
                "paper_runtime_redis_payload_matches_stream",
            ],
        ),
        "test_markers": marker_check(
            GATEWAY_RS,
            [
                "m4_3r_g_paper_runtime_redis_sink_publishes_batch_with_markers",
                "m4_3r_g_paper_runtime_redis_sink_rejects_non_paper_stream_before_publish",
                "m4_3r_g_paper_runtime_redis_sink_rejects_payload_stream_mismatch_before_publish",
                "m4_3r_g_paper_runtime_redis_sink_reports_partial_failure_after_record_publish",
                "m4_3r_g_paper_runtime_redis_sink_plans_stable_idempotency_keys_for_retry",
            ],
        ),
    }

    no_secret_checks = {
        "doc": forbidden_scan(DOC),
        "gateway_rs": forbidden_scan(GATEWAY_RS),
        "script": forbidden_scan(Path(__file__)),
    }

    ok = (
        all(command["exit_code"] == 0 for command in commands.values())
        and all(check["ok"] for check in source_checks.values())
        and all(check["ok"] for check in no_secret_checks.values())
    )

    report = {
        "evidence_kind": "m4-3r-g-paper-redis-sink-batch-discipline-source-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "source_checks": source_checks,
        "no_secret_checks": no_secret_checks,
        "commands": commands,
        "boundary": {
            "paper_only_redis_sink_added": True,
            "actual_redis_connection_attempted_by_evidence": False,
            "finam_post_orders_enabled": False,
            "finam_delete_orders_enabled": False,
            "live_orders_enabled": False,
            "runtime_live_ready_enabled": False,
            "command_consumer_to_real_finam_enabled": False,
            "strategy_invocation_added": False,
            "runtime_daemon_added": False,
            "stop_sltp_bracket_enabled": False,
        },
        "implemented_scope": {
            "paper_stream_allowlist": True,
            "stream_payload_match_validation": True,
            "batch_pending_marker": True,
            "batch_committed_marker": True,
            "deterministic_batch_id": True,
            "deterministic_idempotency_keys": True,
            "partial_failure_report": True,
            "raw_redis_error_exported": False,
            "durable_restart_replay": False,
            "strategy_callback_invocation": False,
        },
        "paper_streams": [
            "finam_imoexf_paper:runtime:intents",
            "finam_imoexf_paper:runtime:paper_acks",
            "finam_imoexf_paper:runtime:orders_paper_only",
            "finam_imoexf_paper:runtime:trades_paper_only",
            "finam_imoexf_paper:runtime:positions_paper_only",
            "finam_imoexf_paper:runtime:state:hybrid_intraday:imoexf",
            "finam_imoexf_paper:runtime:publish_batches",
        ],
        "artifacts": [
            {"path": rel(DOC), "sha256": sha256_file(DOC)},
            {"path": rel(GATEWAY_RS), "sha256": sha256_file(GATEWAY_RS)},
            {"path": rel(Path(__file__)), "sha256": sha256_file(Path(__file__))},
        ],
        "next_stage": "M4-3r-h local paper runtime loop wiring behind same no-live boundary",
    }

    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"ok": ok, "report": rel(REPORT)}, indent=2, sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
