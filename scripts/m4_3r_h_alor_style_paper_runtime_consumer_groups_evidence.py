#!/usr/bin/env python3
"""Generate M4-3r-h ALOR-style paper runtime consumer group evidence."""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / "docs" / "m4-3r-h-alor-style-paper-runtime-consumer-groups.md"
GATEWAY_RS = ROOT / "crates" / "finam-gateway" / "src" / "lib.rs"
CONFIG = ROOT / "config" / "finam-imoexf-hybrid-paper-shadow.vps.example.json"
REPORT = ROOT / "reports" / "m4" / "m4-3r-h-alor-style-paper-runtime-consumer-groups-evidence.json"

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
                "scripts/m4_3r_h_alor_style_paper_runtime_consumer_groups_evidence.py",
            ]
        ),
        "cargo_fmt_check": run(["cargo", "fmt", "--all", "--check"]),
        "finam_gateway_m4_3r_h_tests": run(
            ["cargo", "test", "-p", "finam-gateway", "m4_3r_h"]
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
                "M4-3r-h ALOR-style paper runtime consumer groups",
                "XGROUP CREATE <stream> <group> 0 MKSTREAM",
                "XREADGROUP GROUP <group> <consumer>",
                "XAUTOCLAIM <stream> <group> <consumer>",
                "partial publish failure      -> no XACK / pending retry",
                "Next stage: M4-3r-i",
            ],
        ),
        "consumer_type_markers": marker_check(
            GATEWAY_RS,
            [
                "pub enum PaperRuntimeRedisGroupStart",
                "pub struct PaperRuntimeRedisConsumerConfig",
                "pub struct PaperRuntimeRedisCommandPlan",
                "pub struct PaperRuntimeRedisConsumerLifecyclePlan",
                "pub enum PaperRuntimeRedisEntryDisposition",
                "pub struct PaperRuntimeRedisDlqRecord",
                "pub enum PaperRuntimeRedisConsumerError",
            ],
        ),
        "alor_lifecycle_markers": marker_check(
            GATEWAY_RS,
            [
                "build_paper_runtime_redis_consumer_lifecycle_plan",
                "XGROUP",
                "XREADGROUP",
                "XAUTOCLAIM",
                "XACK",
                "MKSTREAM",
                "classify_paper_runtime_entry_disposition",
            ],
        ),
        "test_markers": marker_check(
            GATEWAY_RS,
            [
                "m4_3r_h_paper_runtime_consumer_plan_repeats_alor_group_lifecycle",
                "m4_3r_h_paper_runtime_consumer_supports_tail_mode_for_future_cutover",
                "m4_3r_h_paper_runtime_consumer_rejects_non_paper_source_before_group_create",
                "m4_3r_h_paper_runtime_dlq_record_is_redacted_and_fingerprinted",
                "m4_3r_h_paper_runtime_entry_disposition_acks_only_success_or_dlq",
            ],
        ),
        "config_markers": marker_check(
            CONFIG,
            [
                "paper_runtime_consumer",
                "finam-imoexf-paper-runtime-m1",
                "finam_imoexf_paper:runtime:publish_batches",
                "finam_imoexf_paper:runtime:health",
                "finam_imoexf_paper:runtime:readiness",
                "finam_imoexf_paper:runtime:dlq",
                "after_successful_batch_or_dlq_only",
            ],
        ),
    }

    no_secret_checks = {
        "doc": forbidden_scan(DOC),
        "gateway_rs": forbidden_scan(GATEWAY_RS),
        "config": forbidden_scan(CONFIG),
        "script": forbidden_scan(Path(__file__)),
    }

    ok = (
        all(command["exit_code"] == 0 for command in commands.values())
        and all(check["ok"] for check in source_checks.values())
        and all(check["ok"] for check in no_secret_checks.values())
    )

    report = {
        "evidence_kind": "m4-3r-h-alor-style-paper-runtime-consumer-groups-source-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "source_checks": source_checks,
        "no_secret_checks": no_secret_checks,
        "commands": commands,
        "alor_oracle_features_ported": [
            "XGROUP CREATE start-id MKSTREAM",
            "auto consumer name",
            "XREADGROUP new messages with >",
            "XAUTOCLAIM idle pending recovery",
            "XACK after success or DLQ",
            "DLQ redacted payload fingerprint",
        ],
        "boundary": {
            "consumer_group_contract_added": True,
            "actual_continuous_runner_added": False,
            "strategy_invocation_added": False,
            "runtime_live_ready_enabled": False,
            "command_consumer_to_real_finam_enabled": False,
            "finam_post_orders_enabled": False,
            "finam_delete_orders_enabled": False,
            "live_orders_enabled": False,
        },
        "artifacts": [
            {"path": rel(DOC), "sha256": sha256_file(DOC)},
            {"path": rel(GATEWAY_RS), "sha256": sha256_file(GATEWAY_RS)},
            {"path": rel(CONFIG), "sha256": sha256_file(CONFIG)},
            {"path": rel(Path(__file__)), "sha256": sha256_file(Path(__file__))},
        ],
        "next_stage": "M4-3r-i actual local paper runtime Redis runner using this lifecycle contract",
    }

    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"ok": ok, "report": rel(REPORT)}, indent=2, sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
