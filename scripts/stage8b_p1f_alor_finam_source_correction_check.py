#!/usr/bin/env python3
"""Fail-closed static/fixture gate for the ALOR→FINAM source correction."""

from __future__ import annotations

import csv
import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"FAIL {message}")


def canonical_sha256(path: Path) -> str:
    value = json.loads(path.read_text(encoding="utf-8"))
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def csv_rows(path: Path) -> int:
    with path.open(newline="", encoding="utf-8") as stream:
        return sum(1 for _ in csv.DictReader(stream))


def main() -> int:
    intake_path = ROOT / "docs/stage-8/stage8b-p1f-alor-finam-freeze-intake-2026-09-27.json"
    intake = json.loads(intake_path.read_text(encoding="utf-8"))
    require(intake["migration_target"] == "baseline07_bo_only", "migration target drift")
    require(intake["candidate09_status"] == "REFERENCE_ONLY_NOT_MIGRATION_TARGET", "candidate promotion")

    guide = ROOT / "docs/stage-8/stage8b-p1f-alor-finam-parity-guide-2026-09-27.md"
    require(sha256(guide) == intake["freeze"]["guide_sha256"], "guide hash drift")
    for name in ("raw_m10", "baseline07", "candidate09"):
        binding = intake["fixtures"][name]
        path = ROOT / binding["path"]
        require(sha256(path) == binding["sha256"], f"{name} hash drift")
    require(csv_rows(ROOT / intake["fixtures"]["raw_m10"]["path"]) == 7307, "raw M10 row drift")
    require(csv_rows(ROOT / intake["fixtures"]["baseline07"]["path"]) == 38, "baseline07 row drift")
    require(csv_rows(ROOT / intake["fixtures"]["candidate09"]["path"]) == 41, "candidate09 row drift")

    profile_path = ROOT / intake["corrected_runtime_profile"]["path"]
    profile = json.loads(profile_path.read_text(encoding="utf-8"))
    semantic = profile["semantic_config"]
    require(profile["profile_id"] == "imoexf-baseline07-bo-only-paper-v1", "profile id drift")
    require(canonical_sha256(profile_path) == intake["corrected_runtime_profile"]["canonical_sha256"], "profile hash drift")
    require(semantic["live_mr_entries_enabled"] is False, "MR execution reopened")
    require(semantic["model_session_start_time"] == "07:00:00", "model start drift")
    require(semantic["model_session_end_time"] == "23:49:59", "model end drift")
    require(semantic["weekends_off"] is True, "weekend policy drift")
    require(semantic["orchestrator"]["breakout_eod_mode"] == "same_day", "EOD mode drift")

    runtime = (ROOT / "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs").read_text(encoding="utf-8")
    orchestrator = (ROOT / "crates/strategy-runtime-core/src/hybrid_intraday/orchestrator.rs").read_text(encoding="utf-8")
    paper_host = (ROOT / "crates/strategy-runtime-core/src/stage5c_paper_host.rs").read_text(encoding="utf-8")
    bridge = (ROOT / "crates/runtime-durable-service/src/stage8b_p1_semantic.rs").read_text(encoding="utf-8")
    supervisor = (ROOT / "crates/runtime-durable-service/src/stage8b_p1_supervisor.rs").read_text(encoding="utf-8")
    require("if !self.config.live_mr_entries_enabled" in runtime, "MR pre-ownership guard missing")
    require("frozen_baseline07_replay_matches_all_38_alor_rounds" in runtime, "Rust parity replay missing")
    require("bar.dt.minute() >= 30" in orchestrator, "same-day no-new-entry guard missing")
    require("strategy_model_bar_label_utc_ms" in bridge, "M10 model-label bridge missing")
    require("canonical_close_time_utc" in paper_host, "canonical close identity accessor missing")
    require("with_strategy_model_bar_label_utc" in paper_host, "model-label binding missing")
    require("callback_bar.close_time_utc = self.strategy_model_bar_label_utc" in paper_host, "callback-only model label missing")
    require("live_mr_entries_enabled: semantic.live_mr_entries_enabled" in supervisor, "profile MR policy binding missing")
    require(intake["corrected_runtime_profile"]["runtime_config_fingerprint_sha256"] == "6ac8994e5fc8777035c48c0b871b2d15a6662cdae6be88220f2bcdcadf0a244d", "runtime fingerprint drift")

    closed = intake["closed_surfaces"]
    require(not any(closed.values()), "a closed operational surface was opened")
    print("PASS stage8b-p1f-alor-finam-source-correction")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
