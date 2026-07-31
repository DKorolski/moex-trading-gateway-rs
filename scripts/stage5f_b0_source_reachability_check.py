#!/usr/bin/env python3
"""Static fail-closed check for the Stage 5F-b0 reachability audit."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "docs/stage-5/stage5f-b0-source-reachability-inventory.json"
PLAN = ROOT / "docs/stage-5/5f-b0-source-reachability-fingerprint-audit.md"
BASE = "0fcab80e4c13822891eeae9bceb0f895b4d453a9"
STAGE = "5F-b0-source-reachability-fingerprint-audit"

EXPECTED_TOP_LEVEL_KEYS = {
    "accepted_b3f_source_ref",
    "classification_summary",
    "closed_surfaces",
    "design_inputs",
    "development_base",
    "fingerprint_contract",
    "future_observation_authority",
    "rows",
    "schema_version",
    "source_baseline",
    "stage",
    "status",
    "target_contract",
}
EXPECTED_INPUTS = {
    "completion_spec_sha256": "4205f948b795a9d4369283e8565f68cb56b509f2e79bf05549de2f3a8b6dc6c1",
    "acceptance_matrix_sha256": "979e85d5a64352b1158622688d0322ff17349d8ce4ac5c2c4d5d4373912df59b",
    "completion_package_sha256": "4ee13305616cb151bf53d5f1d929666c110446064f84cb754d5c2331e982bb17",
}
EXPECTED_SOURCE_BASELINE = {
    "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs": "7f5e3ad070c1bbc3ddca1e642d59b3f4cf75b9bb0d1651068df363323f1cd427",
    "crates/strategy-runtime-core/src/runtime_compat.rs": "a48476fa286b467d06d2725655359f4d16ae65abb8c37d898089420b7f532199",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": "0fce95557b2e7673d7e7e74a5b4d65dd3ec28360fab3674c20e3e6de6be02ff3",
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": "34ed25d3ee188d3f0c52d4b655c6105349e9761b7bd3a5af934e52cab14fb2d6",
}
EXPECTED_GROUPS = {
    "G01_NO_SIGNAL",
    "G02_BO_LONG",
    "G03_BO_SHORT",
    "G04_BO_EXIT",
    "G05_BO_EOD",
    "G06_MR_LONG",
    "G07_MR_SHORT",
    "G08_MR_TIME",
    "G09_MR_TARGET",
    "G10_MR_STOP",
    "G11_ARBITRATION",
    "G12_OWNER_CYCLE",
    "G13_RISKGATE_NORMAL",
    "G14_RISKGATE_BLOCK",
    "G15_PENDING_DEFERRED",
    "G16_TERMINAL",
}
EXPECTED_TERMINALS = {"F31", "F32", "F33", "F34"}
EXPECTED_BLOCKED = {"F24", "F25", "F30"}
EXPECTED_RESTORED_STATE = {"F21", "F22", "F26", "F27", "F28", "F29"}
EXPECTED_ROW_KEYS = {
    "case_id",
    "evidence_kind",
    "group_id",
    "matrix_disposition",
    "reachability",
    "row_id",
    "source_anchor",
}


class CheckFailure(RuntimeError):
    pass


def fail(message: str) -> None:
    raise CheckFailure(message)


def strict_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise CheckFailure(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def load_json(path: Path) -> dict[str, object]:
    try:
        value = json.loads(path.read_text(), object_pairs_hook=strict_object)
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot load {path.relative_to(ROOT)}: {exc}")
    if not isinstance(value, dict):
        fail(f"{path.relative_to(ROOT)} must contain a JSON object")
    return value


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def require(actual: object, expected: object, message: str) -> None:
    if actual != expected:
        fail(f"{message}: expected {expected!r}, got {actual!r}")


def validate_inventory(payload: dict[str, object]) -> None:
    require(set(payload), EXPECTED_TOP_LEVEL_KEYS, "top-level key drift")
    require(payload["schema_version"], 1, "schema version drift")
    require(payload["stage"], STAGE, "stage drift")
    require(
        payload["status"],
        "development_audit_complete_not_release_authority",
        "status drift",
    )
    require(payload["development_base"], BASE, "development base drift")
    require(
        payload["accepted_b3f_source_ref"],
        "e14654f7129aa61011931306140a3bfefe2fcfbc",
        "accepted B3F source drift",
    )
    require(payload["design_inputs"], EXPECTED_INPUTS, "design input drift")
    require(payload["source_baseline"], EXPECTED_SOURCE_BASELINE, "source baseline drift")
    require(
        payload["target_contract"],
        {
            "instrument_symbol": "IMOEXF",
            "strategy_profile": "imoexf_primary_riskgate_high180_lb120",
            "bar_origin": "Live",
            "bar_final": True,
            "timeframe_sec": 600,
            "execution_mode": "paper_only_no_send",
        },
        "target contract drift",
    )

    fingerprints = payload["fingerprint_contract"]
    if not isinstance(fingerprints, dict):
        fail("fingerprint contract must be an object")
    transition = fingerprints.get("transition_state")
    persistence = fingerprints.get("persistence_semantic_projection")
    if not isinstance(transition, dict) or not isinstance(persistence, dict):
        fail("both fingerprint domains must be present")
    require(transition.get("function"), "stage5c_state_fingerprint", "transition function drift")
    require(
        transition.get("algorithm"),
        "sha256(serde_json::to_vec(StrategyState))",
        "transition algorithm drift",
    )
    require(
        persistence.get("function"),
        "stage5c_semantic_payload_fingerprint",
        "persistence function drift",
    )
    require(
        fingerprints.get("cross_algorithm_equality_required"),
        False,
        "fingerprint domains must remain distinct",
    )

    authority = payload["future_observation_authority"]
    if not isinstance(authority, dict):
        fail("future observation authority must be an object")
    require(authority.get("implementation_status"), "not_added", "observer added during b0")
    require(authority.get("test_only"), True, "observer must be test-only")
    require(authority.get("crate_private"), True, "observer must be crate-private")
    require(authority.get("single_consume"), True, "observer must be single-consume")
    require(authority.get("returned_vector_unchanged"), True, "observer may not mutate vector")
    require(
        authority.get("production_stage5c_or_b3f_change_allowed"),
        False,
        "frozen Stage 5C/B3F source may not be opened",
    )

    closed = payload["closed_surfaces"]
    if not isinstance(closed, dict) or not closed or any(value is not False for value in closed.values()):
        fail("all Stage 5F-b0 execution surfaces must remain closed")


def validate_rows(payload: dict[str, object]) -> None:
    rows = payload["rows"]
    if not isinstance(rows, list):
        fail("rows must be a list")
    require(len(rows), 34, "row count drift")
    expected_ids = [f"F{index:02d}" for index in range(1, 35)]
    row_ids: list[str] = []
    groups: set[str] = set()
    dispositions: Counter[str] = Counter()
    reachability: Counter[str] = Counter()
    for row in rows:
        if not isinstance(row, dict):
            fail("every row must be an object")
        require(set(row), EXPECTED_ROW_KEYS, f"row key drift for {row.get('row_id')}")
        row_id = row["row_id"]
        if not isinstance(row_id, str):
            fail("row_id must be a string")
        row_ids.append(row_id)
        group_id = row["group_id"]
        if not isinstance(group_id, str):
            fail(f"group_id must be a string for {row_id}")
        groups.add(group_id)
        dispositions[str(row["matrix_disposition"])] += 1
        reachability[str(row["reachability"])] += 1
        if not all(isinstance(row[key], str) and row[key] for key in EXPECTED_ROW_KEYS):
            fail(f"all row fields must be non-empty strings for {row_id}")

        if row_id in EXPECTED_TERMINALS:
            require(row["matrix_disposition"], "terminal_after_callback", f"{row_id} disposition")
            require(
                row["reachability"],
                "test_negative_terminal_after_callback",
                f"{row_id} must remain negative-only",
            )
            require(row["evidence_kind"], "negative_mutation_proof", f"{row_id} evidence")
        elif row_id in EXPECTED_BLOCKED:
            require(row["matrix_disposition"], "blocked_before_callback", f"{row_id} disposition")
            require(
                row["reachability"],
                "source_chain_blocked_before_callback",
                f"{row_id} reachability",
            )
            require(row["evidence_kind"], "typed_pre_callback_blocker", f"{row_id} evidence")
        else:
            require(row["matrix_disposition"], "accepted", f"{row_id} disposition")
            require(row["reachability"], "source_callback_accepted", f"{row_id} reachability")
            expected_evidence = (
                "golden_restored_state_scenario"
                if row_id in EXPECTED_RESTORED_STATE
                else "golden_source_scenario"
            )
            require(row["evidence_kind"], expected_evidence, f"{row_id} evidence")

    require(row_ids, expected_ids, "row ordering or identity drift")
    require(groups, EXPECTED_GROUPS, "official group coverage drift")
    require(
        dispositions,
        Counter({"accepted": 27, "blocked_before_callback": 3, "terminal_after_callback": 4}),
        "matrix disposition counts drift",
    )
    require(
        reachability,
        Counter(
            {
                "source_callback_accepted": 27,
                "source_chain_blocked_before_callback": 3,
                "test_negative_terminal_after_callback": 4,
            }
        ),
        "reachability counts drift",
    )
    require(
        payload["classification_summary"],
        {
            "official_group_count": 16,
            "row_count": 34,
            "source_callback_accepted": 27,
            "source_chain_blocked_before_callback": 3,
            "test_negative_terminal_after_callback": 4,
            "deferred_to_stage5g": 0,
        },
        "classification summary drift",
    )


def validate_source() -> None:
    for relative, expected in EXPECTED_SOURCE_BASELINE.items():
        path = ROOT / relative
        require(sha256_bytes(path.read_bytes()), expected, f"current source drift: {relative}")
        if (ROOT / ".git").exists():
            baseline = subprocess.run(
                ["git", "show", f"{BASE}:{relative}"],
                cwd=ROOT,
                check=True,
                capture_output=True,
            ).stdout
            require(sha256_bytes(baseline), expected, f"baseline source drift: {relative}")

    source_fragments = {
        "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs": [
            "bar.close_time_utc <= last_ts",
            "fn maybe_reissue_deferred_entry(",
            "fn maybe_reissue_deferred_exit(",
            "fn update_risk_gate_shadow(",
            "fn append_breakout_no_overnight_guard(",
            "fn on_broker_bar(",
            "Ok(Strategy::on_bar(self, &context, &bar))",
        ],
        "crates/strategy-runtime-core/src/stage5c_paper_host.rs": [
            "fn stage5c_state_fingerprint(state: &StrategyState) -> String",
            "serde_json::to_vec(state)",
            "fn stage5c_semantic_payload_fingerprint(",
            "invoke_authorized_callback_once(",
        ],
        "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": [
            "validate_and_settle_stage5e_paper_callback_escrow(",
            "Stage5ePaperSettlementTerminalReason::ChronologyMismatch",
            "Stage5cIntentSettlementError::MissingPendingRequest",
        ],
    }
    for relative, fragments in source_fragments.items():
        text = (ROOT / relative).read_text()
        for fragment in fragments:
            if fragment not in text:
                fail(f"source anchor missing in {relative}: {fragment}")

    if (ROOT / "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs").exists():
        fail("Stage 5F observer module must not exist during b0 audit")
    if "STAGE5F-TEST-OBSERVATION" in (
        ROOT / "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs"
    ).read_text():
        fail("Stage 5F observation marker must not exist during b0 audit")


def validate_plan() -> None:
    text = PLAN.read_text()
    required = [
        "27 rows are source-reachable accepted transitions",
        "3 rows are existing capability-chain blockers",
        "4 rows are negative-only terminal proofs",
        "F31–F34 are negative terminal proofs",
        "sha256(serde_json::to_vec(StrategyState))",
        "No observer is implemented by this audit.",
        "Redis consumption, FINAM transport, HTTP POST/DELETE",
    ]
    for fragment in required:
        if fragment not in text:
            fail(f"audit document fragment missing: {fragment}")


def main() -> int:
    try:
        payload = load_json(INVENTORY)
        validate_inventory(payload)
        validate_rows(payload)
        validate_source()
        validate_plan()
    except (CheckFailure, OSError, subprocess.CalledProcessError) as exc:
        print(f"stage5f-b0-source-reachability-check: FAIL: {exc}", file=sys.stderr)
        return 1
    print("stage5f-b0-source-reachability-check: ok groups=16 rows=34 accepted=27 blocked=3 terminal_negative=4")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
