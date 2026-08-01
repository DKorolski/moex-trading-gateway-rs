#!/usr/bin/env python3
"""Adversarial mutation matrix for the Stage 5G-b R1 checker."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage5g_b_r1_check.py"
MODULE = "crates/strategy-runtime-core/src/stage5g_mock_ack.rs"
CONTRACT = "docs/stage-5/stage5g-b-r1-contract.json"
DESIGN = "docs/stage-5/5g-b-r1-lifecycle-evidence-hardening.md"
DESCRIPTOR = "docs/stage-5/stage5g-a-acceptance-descriptor.json"
SNAPSHOT_GATE = "scripts/stage5g_a_snapshot_gate.sh"

sys.path.insert(0, str(ROOT / "scripts"))
import stage5g_b_mock_ack_negative_harness as base_negative


def required_paths() -> set[str]:
    return base_negative.required_paths() | {
        CHECKER,
        "scripts/stage5g_b_mock_ack_check.py",
        CONTRACT,
        DESIGN,
        DESCRIPTOR,
        SNAPSHOT_GATE,
    }


def copy_baseline(destination: Path) -> None:
    for relative in required_paths():
        source = ROOT / relative
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def replace_once(path: Path, before: str, after: str) -> None:
    text = read(path)
    if text.count(before) < 1:
        raise RuntimeError(f"mutation anchor missing in {path}: {before}")
    path.write_text(text.replace(before, after, 1), encoding="utf-8")


def load_json(path: Path) -> dict:
    return json.loads(read(path))


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def mutate_constant_fingerprint(root: Path) -> None:
    path = root / MODULE
    before = """fn stage5g_summary_fingerprint<T: Serialize>(value: &T) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b\"moex.stage5g.mock-ack-lifecycle.v2\\0\");
    hasher.update(serde_json::to_vec(value).expect(\"Stage 5G summary serializes\"));
    stage5g_sha256_hex(hasher.finalize())
}"""
    after = """fn stage5g_summary_fingerprint<T: Serialize>(value: &T) -> String {
    let _ = value;
    \"constant-fingerprint\".to_string()
}"""
    replace_once(path, before, after)


def mutate_ack_projection_removed(root: Path) -> None:
    replace_once(
        root / MODULE,
        ".map(stage5g_canonical_ack_fingerprint_projection)",
        ".map(|_| panic!(\"projection omitted\"))",
    )


def mutate_broker_id_hash_rebound(root: Path) -> None:
    replace_once(
        root / MODULE,
        "hasher.update(order_id.as_str().as_bytes());",
        "hasher.update(order_id.as_str().len().to_string().as_bytes());",
    )


def mutate_no_send_and(root: Path) -> None:
    replace_once(
        root / MODULE,
        "slot.observed_broker_order_id.is_some() || ack.broker_order_id.is_some()",
        "slot.observed_broker_order_id.is_some() && ack.broker_order_id.is_some()",
    )


def mutate_contradiction_order(root: Path) -> None:
    replace_once(
        root / MODULE,
        "if stage5g_no_send_proof_contradicts_broker_identity(&state.slots[slot_index], &event.ack) {",
        "if false && stage5g_no_send_proof_contradicts_broker_identity(&state.slots[slot_index], &event.ack) {",
    )


def mutate_duplicate_missing_id(root: Path) -> None:
    replace_once(root / MODULE, "(None, None) => true,", "(None, _) => true,")


def mutate_duplicate_reason(root: Path) -> None:
    replace_once(
        root / MODULE,
        "== Some(CommandAckReasonCode::DuplicateCommand)",
        "!= Some(CommandAckReasonCode::DuplicateCommand)",
    )


def mutate_limit_admission(root: Path) -> None:
    replace_once(
        root / MODULE,
        "place_kind: Stage5gMockPlaceKind::Market",
        "place_kind: Stage5gMockPlaceKind::Limit",
    )


def mutate_wall_clock_fixture(root: Path) -> None:
    replace_once(
        root / MODULE,
        "const ACCEPTED_STAGE5F_BAR_CLOSE_TS: i64 = 1_767_679_800;",
        "fn accepted_bar_close_ts() -> i64 { Utc::now().timestamp() }",
    )


def mutate_snapshot_ref(root: Path) -> None:
    replace_once(
        root / SNAPSHOT_GATE,
        "011fd4b7baaa41fffdad7d3c28e463b7977f5989",
        "1111111111111111111111111111111111111111",
    )


def mutate_descriptor(root: Path) -> None:
    value = load_json(root / DESCRIPTOR)
    value["source"]["full_commit_sha"] = "2" * 40
    write_json(root / DESCRIPTOR, value)


def mutate_expiry_source_guard(root: Path) -> None:
    replace_once(
        root / MODULE,
        "|| input.lifecycle_expires_at_ts_utc < max_source_event_ts",
        "|| input.lifecycle_expires_at_ts_utc < batch_summary.bar_close_ts",
    )


def mutate_schema_downgrade(root: Path) -> None:
    replace_once(
        root / MODULE,
        "pub const STAGE5G_MOCK_ACK_SCHEMA_VERSION: u16 = 2;",
        "pub const STAGE5G_MOCK_ACK_SCHEMA_VERSION: u16 = 1;",
    )


def mutate_status_reason_coherence(root: Path) -> None:
    replace_once(
        root / MODULE,
        "CommandAckStatus::Duplicate => reason == Some(CommandAckReasonCode::DuplicateCommand),",
        "CommandAckStatus::Duplicate => true,",
    )


def mutate_golden(root: Path) -> None:
    replace_once(
        root / MODULE,
        "f03a86a0f9f9e6c64b2a3c6bdabb4a3af86eac5674e75859ad8e13f4cf491308",
        "0000000000000000000000000000000000000000000000000000000000000000",
    )


def mutate_contract_open_surface(root: Path) -> None:
    value = load_json(root / CONTRACT)
    value["closed_surfaces"]["stage5g_c"] = True
    write_json(root / CONTRACT, value)


def mutate_raw_broker_id_export(root: Path) -> None:
    path = root / MODULE
    replace_once(
        path,
        "pub broker_order_id_domain_sha256: Option<String>,",
        "pub broker_order_id_domain_sha256: Option<String>,\n    pub broker_order_id: Option<BrokerOrderId>,",
    )


def mutate_transition_sequence_unbound(root: Path) -> None:
    replace_once(
        root / MODULE,
        "last_total_sequence: state\n            .last_total_sequence",
        "last_total_sequence: None",
    )


CASES: list[tuple[str, Callable[[Path], None]]] = [
    ("constant-lifecycle-fingerprint", mutate_constant_fingerprint),
    ("canonical-ack-projection-removed", mutate_ack_projection_removed),
    ("broker-order-id-hash-rebound", mutate_broker_id_hash_rebound),
    ("no-send-or-weakened-to-and", mutate_no_send_and),
    ("no-send-contradiction-disabled", mutate_contradiction_order),
    ("duplicate-missing-id-accepted", mutate_duplicate_missing_id),
    ("duplicate-reason-drift", mutate_duplicate_reason),
    ("limit-source-authority-opened", mutate_limit_admission),
    ("wall-clock-fixture", mutate_wall_clock_fixture),
    ("stage5g-a-snapshot-ref-rebound", mutate_snapshot_ref),
    ("stage5g-a-descriptor-rebound", mutate_descriptor),
    ("max-source-expiry-guard-removed", mutate_expiry_source_guard),
    ("fingerprint-schema-downgrade", mutate_schema_downgrade),
    ("status-reason-coherence-bypass", mutate_status_reason_coherence),
    ("golden-evidence-rebound", mutate_golden),
    ("closed-stage5g-c-opened", mutate_contract_open_surface),
    ("raw-broker-id-exported", mutate_raw_broker_id_export),
    ("transition-sequence-unbound", mutate_transition_sequence_unbound),
]


def checker_exit(root: Path) -> int:
    result = subprocess.run(
        [sys.executable, str(root / CHECKER), "--root", str(root)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return result.returncode


def main() -> int:
    if checker_exit(ROOT) != 0:
        print("stage5g-b-r1-negative-harness: FAIL: positive baseline rejected", file=sys.stderr)
        return 1
    passed = 0
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-b-r1-negative-") as directory:
            root = Path(directory)
            copy_baseline(root)
            try:
                mutation(root)
            except RuntimeError as error:
                print(f"FAIL {name}: {error}", file=sys.stderr)
                return 1
            if checker_exit(root) == 0:
                print(f"FAIL {name}: mutation was accepted", file=sys.stderr)
                return 1
            print(f"PASS {name}")
            passed += 1
    print(f"stage5g-b-r1-negative-harness: PASS {passed}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
