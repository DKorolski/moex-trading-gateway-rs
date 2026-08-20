#!/usr/bin/env python3
"""Exact mutation checks for the Stage 8A-4 I4 Design R2 contract."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage8a4_durable_composition_i4_design_check.py"

MUTATIONS = [
    ("predecessor", "593ff255ef7826a22e66c9aff6f7ea47acf47644", "0" * 40),
    ("review-sha", "1da167c3e7f1266473133d2d8a1412906a26d7f83b5dc026ce84dc7969090257", "0" * 64),
    ("receipt-authority", '"receipt_alone_is_authority": false', '"receipt_alone_is_authority": true'),
    ("complete-v2", '"complete_v2_exact_suffix_required": true', '"complete_v2_exact_suffix_required": false'),
    ("covering-s1", '"covering_s1_required": true', '"covering_s1_required": false'),
    ("request-finalized", '"request_finalized_required": true', '"request_finalized_required": false'),
    ("hold-authorized", '"pending_or_hold_authorized": false', '"pending_or_hold_authorized": true'),
    ("cancel-working", "ExactWorking | none | none | unresolved", "ExactWorking | none | Recovered | accepted"),
    ("place-rejected", "ExactTerminalRejected | Rejected | Rejected | BrokerRejected", "ExactTerminalRejected | Rejected | Recovered | RecoveredByBrokerTruth"),
    ("cancel-filled", "ExactTerminalFilled | ExecutionObserved", "ExactTerminalFilled | Canceled"),
    ("cancel-expired", "ExactTerminalExpired | AlreadyTerminalNonExecution", "ExactTerminalExpired | Canceled"),
    ("cancel-cancelled", "ExactTerminalCancelled | Canceled", "ExactTerminalCancelled | ExecutionObserved"),
    ("readiness-independent", '"independent_from_terminal_ack": true', '"independent_from_terminal_ack": false'),
    ("stop-ready", '"fresh_run_allowed_required": true', '"fresh_run_allowed_required": false'),
    ("stale-ready", '"stop_stale_unreadable_unknown_or_orphan_block": true', '"stop_stale_unreadable_unknown_or_orphan_block": false'),
    ("broker-freshness", '"fresh_composite_and_broker_truth_required": true', '"fresh_composite_and_broker_truth_required": false'),
    ("unknown-orphan", "unknown or orphan account safety blocks readiness", "unknown or orphan account safety permits readiness"),
    ("post-effect-reuse", '"i3_post_effect_snapshot_reusable": false', '"i3_post_effect_snapshot_reusable": true'),
    ("duplicate-append", "duplicate derivation appends no journal record", "duplicate derivation may append journal record"),
    ("stable-identity-current", '"current_seal_checkpoint_or_readiness_in_stable_identity": false', '"current_seal_checkpoint_or_readiness_in_stable_identity": true'),
    ("public-constructor", "There is no public constructor", "There is a public constructor"),
    ("opaque-types", "facade and authorities are nonserializable opaque types", "facade and authorities are serializable public types"),
    ("redis-open", '"redis_ack_xack": true', '"redis_ack_xack": false'),
    ("live-open", '"runtime_live": true', '"runtime_live": false'),
    ("cancel-target-client", '"cancel_target_client_id_can_replace_ack_client_id": false', '"cancel_target_client_id_can_replace_ack_client_id": true'),
    ("trade-b1-erased", '"trade_established_broker_id_survives_idless_selected_order": true', '"trade_established_broker_id_survives_idless_selected_order": false'),
    ("current-truth-fills-id", '"current_truth_can_fill_broker_order_id": false', '"current_truth_can_fill_broker_order_id": true'),
    ("host-now-timestamp", '"timestamp_model": "timestamp_free_model_a"', '"timestamp_model": "host_utc_now"'),
    ("second-ack-identity", '"second_request_identity_domain_allowed": false', '"second_request_identity_domain_allowed": true'),
    ("checkpoint-in-identity", '"received_ts_in_stable_identity": false', '"received_ts_in_stable_identity": true'),
    ("caller-readiness", "not supplied as public caller\nsnapshots", "supplied as public caller\nsnapshots"),
    ("execution-capability", '"execution_capability_required_or_minted": false', '"execution_capability_required_or_minted": true'),
    ("cross-scope", '"exact_scope_binding_required": true', '"exact_scope_binding_required": false'),
    ("active-working-ready", '"account_active_orders_must_be_zero": true', '"account_active_orders_must_be_zero": false'),
    ("expired-readiness", '"observed_at_and_valid_until_required": true', '"observed_at_and_valid_until_required": false'),
    ("seal-repair", '"seal_write_advance_repair_allowed": false', '"seal_write_advance_repair_allowed": true'),
    ("journal-append", '"journal_or_suffix_append_allowed": false', '"journal_or_suffix_append_allowed": true'),
    ("read-side-forbidden", '"seal_reread_authentication_allowed": true', '"seal_reread_authentication_allowed": false'),
]


def mutate(tree: Path, old: str, new: str) -> None:
    candidates = list((tree / "docs/stage-8").glob("*I4*")) + [
        tree / "docs/stage-8/stage8a4-durable-composition-i4-design-authority.json"
    ]
    for path in candidates:
        text = path.read_text(encoding="utf-8")
        if old in text:
            path.write_text(text.replace(old, new, 1), encoding="utf-8")
            return
    raise RuntimeError(f"mutation source missing: {old}")


def main() -> None:
    if len(MUTATIONS) != 38:
        raise SystemExit("FAIL mutation inventory is not exact 38")
    with tempfile.TemporaryDirectory(prefix="stage8a4-i4-design-r2-negative-") as raw:
        base = Path(raw) / "tree"
        shutil.copytree(ROOT / "docs", base / "docs")
        shutil.copytree(ROOT / "scripts", base / "scripts")
        for name, old, new in MUTATIONS:
            case = Path(raw) / name
            shutil.copytree(base, case)
            mutate(case, old, new)
            environment = os.environ.copy()
            environment["STAGE8A4_I4_ROOT"] = str(case)
            result = subprocess.run(
                ["python3", str(ROOT / CHECKER)],
                cwd=ROOT,
                env=environment,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if result.returncode == 0:
                raise SystemExit(f"FAIL {name}")
            print(f"PASS {name}")
    print(f"stage8a4-durable-composition-i4-design-negative: PASS {len(MUTATIONS)}/{len(MUTATIONS)}")


if __name__ == "__main__":
    main()
