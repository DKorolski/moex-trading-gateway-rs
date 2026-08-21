#!/usr/bin/env python3
"""Exact mutation harness for Stage 8B Design R1."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage8b_design_check.py"

MUTATIONS = [
    ("accepted-ref", "bf58b47fdef8af774a4107455dfcc6204e594283", "0" * 40),
    ("review-sha", "72fa3c350dd34aef2d98230dec5547ba25bd7bc752b5b74eedf046e8502b13fc", "0" * 64),
    ("design-base", "0ce76a334f12bf7b13e682ca976c9a4cde6be137", "1" * 40),
    ("matrix-count", '"acceptance_rows": 48', '"acceptance_rows": 47'),
    ("negative-count", '"negative_cases": 36', '"negative_cases": 35'),
    ("open-implementation", '"next_after_acceptance": "Stage 8B implementation specification only"', '"next_after_acceptance": "Stage 8B implementation"'),
    ("remove-phase", '"8B-I no-send implementation and rehearsal acceptance",', ''),
    ("multi-command", '"exactly_one_command": true', '"exactly_one_command": false'),
    ("multi-action", '"action_is_singleton_in_reviewed_run_contract": true', '"action_is_singleton_in_reviewed_run_contract": false'),
    ("followup", '"automatic_followup_command_allowed": false', '"automatic_followup_command_allowed": true'),
    ("account-unbound", '"account_bound_by_broker_account_id_hash": true', '"account_bound_by_broker_account_id_hash": false'),
    ("symbol-drift", '"venue_symbol": "IMOEXF@RTSX"', '"venue_symbol": "OTHER@RTSX"'),
    ("market-open", '"market_order_allowed": false', '"market_order_allowed": true'),
    ("protective-open", '"protective_or_multi_leg_allowed": false', '"protective_or_multi_leg_allowed": true'),
    ("reusable-arm", '"durable_one_use": true', '"durable_one_use": false'),
    ("arm-unbound", '"exact_build_config_endpoint_and_body_hashes_required": true', '"exact_build_config_endpoint_and_body_hashes_required": false'),
    ("restart-arm", '"reconstructible_after_restart": false', '"reconstructible_after_restart": true'),
    ("caller-snapshot", '"caller_supplied_snapshot_allowed": false', '"caller_supplied_snapshot_allowed": true'),
    ("stale-truth", '"fresh_broker_truth_required": true', '"fresh_broker_truth_required": false'),
    ("ignore-kill", '"run_allowed_kill_switch_required": true', '"run_allowed_kill_switch_required": false'),
    ("ambiguity-open", '"zero_ambiguity_required": true', '"zero_ambiguity_required": false'),
    ("send-before-attempt", '"transport_may_run_before_attempt_commit": false', '"transport_may_run_before_attempt_commit": true'),
    ("redis-authority", '"redis_is_not_execution_authority": true', '"redis_is_not_execution_authority": false'),
    ("automatic-retry", '"same_request_automatic_retry_after_transport_boundary": false', '"same_request_automatic_retry_after_transport_boundary": true'),
    ("timeout-no-send", "Timeout, disconnect, malformed/truncated success", "Timeout is definitely not sent; malformed/truncated success"),
    ("resend-unknown", "An ambiguous request is never automatically sent again", "An ambiguous request may be automatically sent again"),
    ("truth-rewrites-id", "Current broker truth cannot rewrite durable identity", "Current broker truth may rewrite durable identity"),
    ("empty-proves-flat", "account-wide row counts cannot prove no order or flat", "account-wide row counts can prove no order or flat"),
    ("conflict-continues", "Conflict or still unknown disarms Stage 8B", "Conflict or still unknown permits Stage 8B"),
    ("omit-post-run", "Post-run closure requires exact target order truth", "Post-run closure may omit exact target order truth"),
    ("dual-broker", "Only FINAM may hold execution ownership", "ALOR and FINAM may share execution ownership"),
    ("alternate-host", '"redirects_proxies_and_alternate_hosts_allowed": false', '"redirects_proxies_and_alternate_hosts_allowed": true'),
    ("secret-leak", "raw authorization headers never enter logs", "raw authorization headers may enter logs"),
    ("omit-rehearsal", "deterministic no-send positive rehearsal and complete fault matrix", "partial happy-path rehearsal"),
    ("open-finam", '"finam_post_delete": true', '"finam_post_delete": false'),
    ("real-without-x", "A real request remains forbidden until the later exact", "A real request is allowed before the later exact"),
]


def mutate(tree: Path, old: str, new: str) -> None:
    authority = tree / "docs/stage-8/stage8b-design-authority.json"
    candidates = [authority] + [
        path for path in sorted((tree / "docs/stage-8").glob("*8B*"))
        if path != authority
    ]
    for path in candidates:
        text = path.read_text(encoding="utf-8")
        if old in text:
            path.write_text(text.replace(old, new, 1), encoding="utf-8")
            return
    raise RuntimeError(f"mutation source missing: {old}")


def main() -> None:
    if len(MUTATIONS) != 36:
        raise SystemExit("stage8b-design-negative: FAIL inventory is not exact 36")
    with tempfile.TemporaryDirectory(prefix="stage8b-design-negative-") as raw:
        base = Path(raw) / "base"
        shutil.copytree(ROOT / "docs", base / "docs")
        shutil.copytree(ROOT / "scripts", base / "scripts")
        for name, old, new in MUTATIONS:
            case = Path(raw) / name
            shutil.copytree(base, case)
            mutate(case, old, new)
            env = os.environ.copy()
            env["STAGE8B_ROOT"] = str(case)
            result = subprocess.run(
                ["python3", str(CHECKER), "--no-git"],
                cwd=ROOT,
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8b-design-negative: FAIL {name}")
            print(f"PASS {name}")
    print("stage8b-design-negative: PASS cases=36/36")


if __name__ == "__main__":
    main()
