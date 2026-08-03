#!/usr/bin/env python3
"""Mutation matrix for Stage 5G-e-b R2."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage5g_eb_r2_check as checker

ROOT = Path(__file__).resolve().parents[1]
PATHS = (
    "crates/strategy-runtime-core/src/stage5g_order_position.rs",
    "crates/strategy-runtime-core/src/stage5g_timer.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "docs/current-status.md",
    "docs/stage-5/stage5g-e-b-r2-historical-exact-replay-metadata.json",
    "docs/stage-5/stage5g-e-b-r2-historical-exact-replay-metadata.md",
)


def mutate(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    source = path.read_text()
    if old not in source:
        raise RuntimeError(f"mutation anchor missing: {relative}: {old}")
    path.write_text(source.replace(old, new, 1))


def must_fail(label: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix="stage5g-eb-r2-negative-") as raw:
        root = Path(raw)
        for relative in PATHS:
            destination = root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        mutation(root)
        try:
            checker.validate(root, check_git=False)
        except (checker.CheckFailure, ValueError, KeyError, json.JSONDecodeError):
            print(f"PASS {label}")
            return
        raise SystemExit(f"FAIL mutation escaped Stage 5G-e-b R2 checker: {label}")


def main() -> int:
    order = "crates/strategy-runtime-core/src/stage5g_order_position.rs"
    timer = "crates/strategy-runtime-core/src/stage5g_timer.rs"
    descriptor = "docs/stage-5/stage5g-e-b-r2-historical-exact-replay-metadata.json"
    cases = (
        ("continuation-before-replay-classification", lambda r: mutate(r, order, "    match apply_stage5g_exact_replay_metadata(&mut session, evidence, &identity, &fingerprint) {", "    let _ = stage5g_order_position_new_package_preflight(&session, evidence);\n    match apply_stage5g_exact_replay_metadata(&mut session, evidence, &identity, &fingerprint) {")),
        ("current-slot-lookup-before-replay-classification", lambda r: mutate(r, order, "    match classify_evidence_replay(&session.state, identity, fingerprint)? {", "    let _ = session.state.slots.iter().position(|_| true);\n    match classify_evidence_replay(&session.state, identity, fingerprint)? {")),
        ("historical-chain-witness-removed", lambda r: mutate(r, order, "fn stage5ge_b_r2_historical_a_b_exact_a_then_c_is_continuous()", "fn removed_historical_chain_witness()")),
        ("cross-request-policy-witness-removed", lambda r: mutate(r, order, "fn stage5ge_b_r2_inherited_older_request_exact_replay_preserves_current_slot()", "fn removed_cross_request_witness()")),
        ("exact-replay-mutates-slot", lambda r: mutate(r, order, "            session.state.last_total_sequence = Some(evidence.total_sequence);", "            session.state.slots.clear();\n            session.state.last_total_sequence = Some(evidence.total_sequence);")),
        ("exact-replay-appends-identity", lambda r: mutate(r, order, "            session.state.last_total_sequence = Some(evidence.total_sequence);", "            session.state.evidence_identities.push(EvidenceIdentity { identity: identity.to_string(), fingerprint: fingerprint.to_string() });\n            session.state.last_total_sequence = Some(evidence.total_sequence);")),
        ("exact-replay-changes-current-identity", lambda r: mutate(r, order, "            session.state.last_total_sequence = Some(evidence.total_sequence);", "            session.state.current_evidence_identity = Some(identity.to_string());\n            session.state.last_total_sequence = Some(evidence.total_sequence);")),
        ("exact-replay-changes-watermark", lambda r: mutate(r, order, "            session.state.last_total_sequence = Some(evidence.total_sequence);", "            session.state.last_broker_truth_received_at = Some(evidence.broker_truth.received_ts);\n            session.state.last_total_sequence = Some(evidence.total_sequence);")),
        ("exact-replay-invokes-callback", lambda r: mutate(r, order, "            session.state.last_total_sequence = Some(evidence.total_sequence);", "            let _ = converge_through_stage5c;\n            session.state.last_total_sequence = Some(evidence.total_sequence);")),
        ("new-package-bypasses-continuation", lambda r: mutate(r, order, "evidence.broker_truth.received_ts.timestamp_millis() < checkpoint", "false && evidence.broker_truth.received_ts.timestamp_millis() < checkpoint")),
        ("fingerprint-conflict-accepted", lambda r: mutate(r, order, "if previous.fingerprint != fingerprint {", "if false && previous.fingerprint != fingerprint {")),
        ("release-validation-weakened", lambda r: mutate(r, timer, "if validate_stage5g_timer_checkpoint(&committed_checkpoint).is_err()", "if { debug_assert!(validate_stage5g_timer_checkpoint(&committed_checkpoint).is_ok()); false }")),
        ("open-stage5g-f", lambda r: mutate(r, descriptor, '"stage5g_f": false', '"stage5g_f": true')),
    )
    for label, mutation in cases:
        must_fail(label, mutation)
    print(f"stage5g-eb-r2-negative-harness: PASS {len(cases)}/{len(cases)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
