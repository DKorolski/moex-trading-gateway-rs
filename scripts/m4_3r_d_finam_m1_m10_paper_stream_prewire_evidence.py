#!/usr/bin/env python3
"""Generate M4-3r-d FINAM M1-to-M10 paper stream prewire evidence.

This script is source-only. It validates Rust source markers and runs local
compile/test checks. It does not call FINAM, ALOR, Redis, WebSocket, SSH, or
order endpoints.
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
DOC = ROOT / "docs" / "m4-3r-d-finam-m1-m10-paper-stream-prewire.md"
PAPER_RS = ROOT / "crates" / "broker-core" / "src" / "paper.rs"
LIB_RS = ROOT / "crates" / "broker-core" / "src" / "lib.rs"
REPORT = ROOT / "reports" / "m4" / "m4-3r-d-finam-m1-m10-paper-stream-prewire-evidence.json"

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
                "scripts/m4_3r_d_finam_m1_m10_paper_stream_prewire_evidence.py",
            ]
        ),
        "cargo_fmt_check": run(["cargo", "fmt", "--all", "--check"]),
        "broker_core_paper_tests": run(["cargo", "test", "-p", "broker-core", "paper"]),
        "broker_core_tests": run(["cargo", "test", "-p", "broker-core"]),
        "broker_core_clippy": run(["cargo", "clippy", "-p", "broker-core", "--all-targets", "--", "-D", "warnings"]),
    }

    source_checks = {
        "doc_markers": marker_check(
            DOC,
            [
                "M4-3r-d FINAM M1-to-M10 paper stream prewire",
                "CanonicalBarAggregator(target = 600s)",
                "RuntimeBarInput(origin = Live, provenance = FinamDerivedM1ToM10)",
                "Raw M1 bars never become `RuntimeBarInput`",
                "DroppedIncompleteBucket",
                "Next stage: M4-3r-e",
            ],
        ),
        "publisher_type_markers": marker_check(
            PAPER_RS,
            [
                "pub struct PaperRuntimeBarPublisherConfig",
                "pub struct PaperRuntimeBarPublisher",
                "pub enum PaperRuntimeBarPublishOutcome",
                "pub enum PaperRuntimeBarPublishRejectReason",
                "finam_m1_to_m10_paper",
                "FinamDerivedM1ToM10",
                "CanonicalBarAggregator::new",
                "observe_source_bar",
            ],
        ),
        "publisher_gate_markers": marker_check(
            PAPER_RS,
            [
                "MarketDataSourceKind::LiveStream",
                "NonLiveSourceKind",
                "SourceTimeframeMismatch",
                "AggregationRejected",
                "PaperRuntimeBarPublishRejectReason::LiveBoundaryOpen",
                "PaperRuntimeBarPublishOutcome::DroppedIncompleteBucket",
            ],
        ),
        "publisher_tests_markers": marker_check(
            PAPER_RS,
            [
                "paper_runtime_bar_publisher_buffers_m1_until_complete_m10_and_publishes_runtime_input",
                "paper_runtime_bar_publisher_rejects_raw_non_final_non_live_and_native_m10_inputs",
                "paper_runtime_bar_publisher_drops_incomplete_bucket_on_gap",
                "paper_runtime_bar_publisher_rejects_open_safety_boundary",
            ],
        ),
        "broker_core_exports": marker_check(
            LIB_RS,
            [
                "PaperRuntimeBarPublisherConfig",
                "PaperRuntimeBarPublisher",
                "PaperRuntimeBarPublishOutcome",
                "PaperRuntimeBarPublishRejectReason",
            ],
        ),
    }

    no_secret_checks = {
        "doc": forbidden_scan(DOC),
        "paper_rs": forbidden_scan(PAPER_RS),
        "script": forbidden_scan(Path(__file__)),
    }

    ok = (
        all(command["exit_code"] == 0 for command in commands.values())
        and all(check["ok"] for check in source_checks.values())
        and all(check["ok"] for check in no_secret_checks.values())
    )

    report = {
        "evidence_kind": "m4-3r-d-finam-m1-m10-paper-stream-prewire-source-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "source_checks": source_checks,
        "no_secret_checks": no_secret_checks,
        "commands": commands,
        "boundary": {
            "source_only": True,
            "redis_publication_added": False,
            "strategy_invocation_added": False,
            "finam_post_orders_enabled": False,
            "finam_delete_orders_enabled": False,
            "live_orders_enabled": False,
            "runtime_live_ready_enabled": False,
            "command_consumer_to_real_finam_enabled": False,
            "stop_sltp_bracket_enabled": False,
        },
        "implemented_scope": {
            "finam_m1_to_m10_prewire": True,
            "canonical_aggregator_reused": True,
            "runtime_bar_input_published_in_memory": True,
            "raw_m1_runtime_input_blocked": True,
            "finam_native_m10_blocked": True,
            "gap_incomplete_bucket_blocked": True,
            "redis_xadd": False,
            "runtime_adapter_loop": False,
        },
        "artifacts": [
            {"path": rel(DOC), "sha256": sha256_file(DOC)},
            {"path": rel(PAPER_RS), "sha256": sha256_file(PAPER_RS)},
            {"path": rel(LIB_RS), "sha256": sha256_file(LIB_RS)},
            {"path": rel(Path(__file__)), "sha256": sha256_file(Path(__file__))},
        ],
        "next_stage": "M4-3r-e paper runtime adapter skeleton / Redis prewire, still no live orders",
    }

    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"ok": ok, "report": rel(REPORT)}, indent=2, sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
