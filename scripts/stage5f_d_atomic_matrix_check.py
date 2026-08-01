#!/usr/bin/env python3
"""Fail-closed contract checker for the complete Stage 5F-d Hybrid matrix."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import uuid
from collections import Counter
from pathlib import Path
from typing import Any


DEFAULT_ROOT = Path(__file__).resolve().parents[1]
ACCEPTED_R3 = "e9bcc05deca93e6683abca9b9688b1a814839120"
SCENARIOS = "tests/fixtures/stage5/stage5f/v2/scenarios/atomic-hybrid-scenarios.json"
STATES = "tests/fixtures/stage5/stage5f/v2/states/imoexf-hybrid-state-seeds.json"
RISKGATE = "tests/fixtures/stage5/stage5f/v2/riskgate/imoexf-high180-riskgate-seeds.json"
CONFIG = "tests/fixtures/stage5/stage5f/v2/config/imoexf-target-config.json"
GOLDEN = "docs/stage-5/stage5f-d-golden-results.json"
INVENTORY = "docs/stage-5/stage5f-d-scenario-inventory.json"
R2_MAPPING = "docs/stage-5/stage5f-c-r2-row-semantics-mapping.json"
R3_MAPPING = "docs/stage-5/stage5f-c-r3-row-semantics-mapping.json"
R3_CANDIDATE = "docs/stage-5/stage5f-c-r1-candidate-results.json"
HARNESS = "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs"
STAGE5E = "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs"
R3_SNAPSHOT_GATE = "scripts/stage5f_r3_snapshot_gate.sh"
FUNCTIONAL_GATE = "scripts/stage5f_functional_development_gate.sh"
NEGATIVE_HARNESS = "scripts/stage5f_d_atomic_matrix_negative_harness.py"

EXPECTED_HASHES = {
    SCENARIOS: "a9cfe51f3393b2f0d45881029e251455d727bf233fde9751fd3f2278e793e55e",
    STATES: "93d45c1c003a23ddd0384250a555e694f8d87c7645853bad83598a9df8061901",
    RISKGATE: "dd3ea7894df922984896ee20ebd114412d1675e666683c958fe98eb724a22584",
    CONFIG: "3c46aa4bdfb5a6ac3350d0f3b52ad5050abc472c653bacda512dffebfeb07e41",
    GOLDEN: "aed7e21d7a524fd3dfd2bc6c2b128b379ff812b91556f0021fc70ba3cbf33a3d",
    R2_MAPPING: "9a75aafc064f5f56c874432fdc18911316d218652afdadd2080febf1026efe0f",
    R3_MAPPING: "b1ae5c94a5c76ef8e9a71e6370f61aabbbc13ebf01297035ca8f99fd7eda2f51",
    "crates/strategy-runtime-core/src/hybrid_intraday/intraday_breakout.rs": "a3b125f282f201b66dfa8d2685f22aa94048856a5145d537b76dc8934a5f9ae5",
    "crates/strategy-runtime-core/src/hybrid_intraday/high180.rs": "e1f39a3afdf9745682682da0083f97ac0fa5361f979525d5ea383d6a6aa64456",
    "crates/strategy-runtime-core/src/hybrid_intraday/orchestrator.rs": "1e784411d348fcf090887f7f50062b0cbd34494912288100c1ca1d851d8d5bd9",
    "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs": "aa514c2479a2720a585ce0c386ab91674e125582e013912fba49fe529f8bdd2d",
    "crates/strategy-runtime-core/src/stage5d_persistence.rs": "f790a907d6730e26e731a78ef89c58f993b39acde6ce934602e2fe603d90f083",
    STAGE5E: "87e9f9145a7ead7513c6297da9e5cd45cb5c67ce272a43988b7b1a1729b9d21b",
    HARNESS: "cf8fe7900a2f1f84d3928c0d911db69415f19ee640c26dea47227759e375c508",
}
RESULTS_ARRAY_SHA256 = "e85f15912e3dd97e2a41a3d2617bc9b560769aa964e158b0129bb0d2c89e0f17"
HEX64 = re.compile(r"^[0-9a-f]{64}$")
ROW_IDS = [f"F{ordinal:02}" for ordinal in range(1, 35)]
GROUP_IDS = [
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
]
ONE_INTENT = {
    "F02": ("BO", "buy", "ENTRY", "entry"),
    "F03": ("BO", "sell", "ENTRY", "entry"),
    "F04": ("BO", "sell", "EXIT", "exit"),
    "F05": ("BO", "buy", "EXIT", "exit"),
    "F06": ("BO", "sell", "EXIT", "exit"),
    "F07": ("BO", "sell", "EXIT", "exit"),
    "F08": ("MR", "buy", "ENTRY", "entry"),
    "F09": ("MR", "sell", "ENTRY", "entry"),
    "F10": ("MR", "sell", "EXIT", "exit"),
    "F11": ("MR", "buy", "EXIT", "exit"),
    "F17": ("BO", "buy", "ENTRY", "entry"),
    "F20": ("MR", "buy", "ENTRY", "entry"),
    "F22": ("MR", "buy", "ENTRY", "entry"),
    "F28": ("MR", "buy", "ENTRY", "entry"),
    "F29": ("MR", "sell", "EXIT", "exit"),
}
BLOCKED_OUTCOMES = {
    "F24": "LedgerEvidenceInvalid",
    "F25": "LedgerGenerationMismatch",
    "F30": "LedgerTailMismatch",
}
TERMINAL_OUTCOMES = {
    "F31": "CallbackValidationError",
    "F32": "ChronologyMismatch",
    "F33": "Stage5cIntentValidationFailed",
    "F34": "Stage5cPendingRequestMismatch",
}


class CheckFailure(RuntimeError):
    pass


def fail(message: str) -> None:
    raise CheckFailure(message)


def require(actual: object, expected: object, label: str) -> None:
    if actual != expected:
        fail(f"{label}: expected {expected!r}, got {actual!r}")


def exact_keys(value: object, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != expected:
        fail(f"{label} key-set drift")
    return value


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            fail(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(root: Path, relative: str) -> dict[str, Any]:
    try:
        value = json.loads(
            (root / relative).read_text(encoding="utf-8"),
            object_pairs_hook=strict_object,
        )
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot parse {relative}: {exc}")
    if not isinstance(value, dict):
        fail(f"{relative} must contain an object")
    return value


def sha256_file(root: Path, relative: str) -> str:
    try:
        return hashlib.sha256((root / relative).read_bytes()).hexdigest()
    except OSError as exc:
        fail(f"cannot read {relative}: {exc}")


def require_hash(value: object, label: str) -> str:
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        fail(f"{label} must be a lowercase SHA-256")
    return value


def validate_lineage(root: Path) -> None:
    try:
        subprocess.run(
            ["git", "merge-base", "--is-ancestor", ACCEPTED_R3, "HEAD"],
            cwd=root,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        fail(f"accepted Stage 5F-c R3 commit is not an ancestor: {exc}")


def validate_frozen_hashes(root: Path) -> None:
    for path, expected in EXPECTED_HASHES.items():
        require(sha256_file(root, path), expected, f"frozen SHA-256 {path}")


def vector_sha256(vector: list[object]) -> str:
    payload = json.dumps(
        vector, ensure_ascii=False, separators=(",", ":")
    ).encode("utf-8")
    return hashlib.sha256(b"moex.stage5f.ordered-intent-vector.v1\0" + payload).hexdigest()


def validate_golden(root: Path) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    golden = exact_keys(
        read_json(root, GOLDEN),
        {
            "schema_version",
            "stage",
            "status",
            "predecessor",
            "design_authority",
            "inputs",
            "generation",
            "matrix",
            "closed_surfaces",
            "results",
        },
        "golden",
    )
    require(golden["schema_version"], 2, "golden schema")
    require(golden["stage"], "5F-d-complete-atomic-hybrid-matrix", "golden stage")
    require(
        golden["status"],
        "frozen_golden_pending_independent_acceptance",
        "golden status",
    )
    require(
        golden["predecessor"],
        {"accepted_stage5f_c_r3_commit": ACCEPTED_R3},
        "golden predecessor",
    )

    inputs = golden["inputs"]
    for path_key, hash_key, path in (
        ("scenario_catalog_path", "scenario_catalog_sha256", SCENARIOS),
        ("state_catalog_path", "state_catalog_sha256", STATES),
        ("riskgate_catalog_path", "riskgate_catalog_sha256", RISKGATE),
        ("target_config_path", "target_config_sha256", CONFIG),
    ):
        require(inputs[path_key], path, f"golden input path {path_key}")
        require(inputs[hash_key], EXPECTED_HASHES[path], f"golden input hash {hash_key}")
    require(inputs["cargo_lock_sha256"], sha256_file(root, "Cargo.lock"), "Cargo.lock binding")

    matrix = golden["matrix"]
    require(matrix["row_count"], 34, "golden row count")
    require(matrix["group_count"], 16, "golden group count")
    require(matrix["row_ids"], ROW_IDS, "golden row order")
    require(
        matrix["dispositions"],
        {
            "accepted": 26,
            "structural_invariant": 1,
            "blocked_before_callback": 3,
            "terminal_after_callback": 4,
        },
        "golden disposition summary",
    )
    require(matrix["slices"]["5F-d1"], ROW_IDS[:15], "5F-d1 rows")
    require(matrix["slices"]["5F-d2"], ROW_IDS[15:22], "5F-d2 rows")
    require(matrix["slices"]["5F-d3"], ROW_IDS[22:], "5F-d3 rows")
    if any(value is not False for value in golden["closed_surfaces"].values()):
        fail("a Stage 5F-d closed surface was opened")

    results = golden["results"]
    if not isinstance(results, list) or len(results) != 34:
        fail("golden results must contain exactly 34 rows")
    require([row.get("row_id") for row in results], ROW_IDS, "golden result row order")
    result_bytes = (json.dumps(results, indent=2, ensure_ascii=False) + "\n").encode()
    require(
        hashlib.sha256(result_bytes).hexdigest(),
        RESULTS_ARRAY_SHA256,
        "golden results array SHA-256",
    )
    require(
        golden["generation"]["results_array_sha256"],
        RESULTS_ARRAY_SHA256,
        "declared results array SHA-256",
    )

    result_keys = {
        "schema_version",
        "row_id",
        "scenario_id",
        "disposition",
        "callback_count",
        "observer_count",
        "settlement_attempt_count",
        "pre_state_fingerprint",
        "accepted_post_state_fingerprint",
        "ordered_intent_vector",
        "ordered_intent_vector_sha256",
        "b3f_outcome",
        "settlement_identity_sha256",
    }
    intent_keys = {
        "base_action",
        "broker_order_id_domain_sha256",
        "broker_stop_id_domain_sha256",
        "check_duplicates",
        "comment_domain_sha256",
        "comment_present",
        "condition_flags",
        "cycle_id_domain_sha256",
        "fill_f64_bits_be",
        "intent_class",
        "ordinal",
        "owner",
        "price_f64_bits_be",
        "quantity_f64_bits_be",
        "role",
        "route_symbol",
        "settled_strategy_request_id",
        "side",
        "stop_end_unix_time",
        "trigger_f64_bits_be",
    }
    for row in results:
        row = exact_keys(row, result_keys, f"golden {row.get('row_id')}")
        row_id = row["row_id"]
        require(row["schema_version"], 2, f"{row_id} schema")
        require_hash(row["pre_state_fingerprint"], f"{row_id} pre fingerprint")
        vector = row["ordered_intent_vector"]
        if not isinstance(vector, list):
            fail(f"{row_id} intent vector must be a list")
        expected_intent = ONE_INTENT.get(row_id)
        require(len(vector), 1 if expected_intent else 0, f"{row_id} intent count")
        for ordinal, intent in enumerate(vector):
            intent = exact_keys(intent, intent_keys, f"{row_id} intent {ordinal}")
            require(intent["ordinal"], ordinal, f"{row_id} intent ordinal")
            owner, side, role, intent_class = expected_intent  # type: ignore[misc]
            require(
                (intent["owner"], intent["side"], intent["role"], intent["intent_class"]),
                (owner, side, role, intent_class),
                f"{row_id} semantic intent",
            )
            try:
                parsed = uuid.UUID(intent["settled_strategy_request_id"])
            except (ValueError, TypeError, AttributeError) as exc:
                fail(f"{row_id} request ID is invalid: {exc}")
            require(parsed.version, 5, f"{row_id} deterministic request ID version")

        disposition = row["disposition"]
        if disposition == "accepted":
            require(
                (row["callback_count"], row["observer_count"], row["settlement_attempt_count"]),
                (1, 1, 1),
                f"{row_id} accepted cardinality",
            )
            require_hash(row["accepted_post_state_fingerprint"], f"{row_id} post fingerprint")
            require(row["b3f_outcome"], "settled", f"{row_id} accepted outcome")
            require_hash(row["settlement_identity_sha256"], f"{row_id} settlement identity")
            require(
                row["ordered_intent_vector_sha256"],
                vector_sha256(vector),
                f"{row_id} intent vector hash",
            )
        elif disposition == "structural_invariant":
            require(row_id, "F16", "structural invariant owner")
            require(
                (row["callback_count"], row["observer_count"], row["settlement_attempt_count"]),
                (0, 0, 0),
                "F16 structural cardinality",
            )
            require(row["b3f_outcome"], "active_profile_bo_high180_windows_disjoint", "F16 outcome")
            for key in ("accepted_post_state_fingerprint", "ordered_intent_vector_sha256", "settlement_identity_sha256"):
                require(row[key], None, f"F16 {key}")
        elif disposition == "blocked_before_callback":
            require(row_id in BLOCKED_OUTCOMES, True, f"{row_id} blocker membership")
            require(
                (row["callback_count"], row["observer_count"], row["settlement_attempt_count"]),
                (0, 0, 0),
                f"{row_id} blocker cardinality",
            )
            require(row["b3f_outcome"], BLOCKED_OUTCOMES[row_id], f"{row_id} blocker outcome")
            for key in ("accepted_post_state_fingerprint", "ordered_intent_vector_sha256", "settlement_identity_sha256"):
                require(row[key], None, f"{row_id} {key}")
        elif disposition == "terminal_after_callback":
            require(row_id in TERMINAL_OUTCOMES, True, f"{row_id} terminal membership")
            require(
                (row["callback_count"], row["observer_count"], row["settlement_attempt_count"]),
                (1, 0 if row_id == "F31" else 1, 1),
                f"{row_id} terminal cardinality",
            )
            require(row["b3f_outcome"], TERMINAL_OUTCOMES[row_id], f"{row_id} terminal outcome")
            for key in ("accepted_post_state_fingerprint", "ordered_intent_vector_sha256", "settlement_identity_sha256"):
                require(row[key], None, f"{row_id} {key}")
        else:
            fail(f"{row_id} unknown disposition {disposition!r}")
    require(
        Counter(row["disposition"] for row in results),
        Counter({"accepted": 26, "structural_invariant": 1, "blocked_before_callback": 3, "terminal_after_callback": 4}),
        "actual disposition counts",
    )
    return golden, results


def validate_scenarios_and_inventory(
    root: Path, golden: dict[str, Any], results: list[dict[str, Any]]
) -> None:
    scenarios = read_json(root, SCENARIOS)
    records = scenarios.get("records")
    if not isinstance(records, list) or len(records) != 34:
        fail("scenario catalog must contain exactly 34 records")
    require([row.get("row_id") for row in records], ROW_IDS, "scenario row order")
    require(list(dict.fromkeys(row.get("group_id") for row in records)), GROUP_IDS, "scenario group order")
    if len({row.get("scenario_id") for row in records}) != 34:
        fail("scenario IDs must be unique")
    if len({row.get("owning_test") for row in records}) != 34:
        fail("owning tests must be unique")
    for row in records:
        row_id = row["row_id"]
        target = row["target"]
        require(target["strategy_id"], "hybrid_imoexf", f"{row_id} strategy")
        require(target["account_id"], "ACC_TEST_0001", f"{row_id} account")
        require(target["paper_only"], True, f"{row_id} paper-only boundary")
        require(
            target["instrument"],
            {
                "symbol": "IMOEXF",
                "venue_symbol": "IMOEXF@RTSX",
                "exchange": "Moex",
                "market": "Futures",
            },
            f"{row_id} instrument",
        )
        bar = row["bar"]
        require(
            (bar["origin"], bar["is_final"], bar["timeframe_sec"]),
            ("Live", True, 600),
            f"{row_id} canonical bar",
        )
        require(row["clock"]["event_ts_utc"], bar["close_time_utc"], f"{row_id} event/bar clock")
        require(row["pre_state"]["catalog_path"], STATES, f"{row_id} state catalog")
        require(row["pre_state"]["catalog_sha256"], EXPECTED_HASHES[STATES], f"{row_id} state hash")
        require(row["riskgate"]["catalog_path"], RISKGATE, f"{row_id} riskgate catalog")
        require(row["riskgate"]["catalog_sha256"], EXPECTED_HASHES[RISKGATE], f"{row_id} riskgate hash")

    inventory = read_json(root, INVENTORY)
    require(inventory["schema_version"], 2, "inventory schema")
    require(inventory["stage"], golden["stage"], "inventory stage")
    require(inventory["accepted_predecessor_ref"], ACCEPTED_R3, "inventory predecessor")
    require(inventory["groups"], GROUP_IDS, "inventory groups")
    require(inventory["summary"]["row_count"], 34, "inventory row count")
    require(inventory["summary"]["group_count"], 16, "inventory group count")
    if any(value is not False for value in inventory["closed_surfaces"].values()):
        fail("inventory opens a closed Stage 5F-d surface")
    for path, expected_hash in inventory["source_bindings"].items():
        require(expected_hash, EXPECTED_HASHES[path], f"inventory source binding {path}")
        require(sha256_file(root, path), expected_hash, f"source binding {path}")
    authorities = inventory["authorities"]
    for authority in authorities.values():
        require(sha256_file(root, authority["path"]), authority["sha256"], f"authority {authority['path']}")

    inventory_rows = inventory.get("rows")
    if not isinstance(inventory_rows, list) or len(inventory_rows) != 34:
        fail("inventory must own exactly 34 rows")
    require([row.get("row_id") for row in inventory_rows], ROW_IDS, "inventory row order")
    by_result = {row["row_id"]: row for row in results}
    harness = (root / HARNESS).read_text(encoding="utf-8")
    for source, owned in zip(records, inventory_rows):
        row_id = source["row_id"]
        result = by_result[row_id]
        require(owned["group_id"], source["group_id"], f"{row_id} inventory group")
        require(owned["case_id"], source["case_id"], f"{row_id} inventory case")
        require(owned["owning_test"], source["owning_test"], f"{row_id} owning test")
        if not re.search(rf"\b{re.escape(source['owning_test'])}\b", harness):
            fail(f"{row_id} owning test is absent from the Rust harness")
        for key in (
            "disposition",
            "callback_count",
            "observer_count",
            "settlement_attempt_count",
            "pre_state_fingerprint",
            "accepted_post_state_fingerprint",
            "ordered_intent_vector_sha256",
            "b3f_outcome",
            "settlement_identity_sha256",
        ):
            require(owned[key], result[key], f"{row_id} inventory {key}")
        require(owned["intent_count"], len(result["ordered_intent_vector"]), f"{row_id} inventory intent count")

    inherited = read_json(root, R3_CANDIDATE)["results"]
    current = {row["row_id"]: row for row in results}
    for row in inherited:
        require(current[row["row_id"]], row, f"inherited R3 result {row['row_id']}")


def validate_source_and_gates(root: Path) -> None:
    harness = (root / HARNESS).read_text(encoding="utf-8")
    stage5e = (root / STAGE5E).read_text(encoding="utf-8")
    require(harness.count("invoke_stage5e_authorized_paper_callback_at("), 1, "sole callback call site")
    require(harness.count("validate_and_settle_stage5e_paper_callback_escrow(escrow)"), 1, "sole settlement call site")
    for required in (
        "fn stage5f_d_results()",
        "fn stage5f_d_full_matrix_matches_frozen_golden()",
        "fn stage5f_d_full_matrix_repeat_is_byte_identical()",
        "stage5f_f19_mr_owner_suppresses_paired_source_valid_bo_candidate",
        'assert_eq!(control.ordered_intent_vector[0]["owner"], "BO");',
        'assert_eq!(control.ordered_intent_vector[0]["side"], "buy");',
        "assert!(owner.ordered_intent_vector.is_empty());",
        "stage5f_f26_working_order_reaches_runtime_and_retains_stale_pending",
        "F26 callback must retain the exact pending request",
        "F26 callback must retain the broker-truth working order",
        "Stage5cPendingRequestMismatch",
        "stage5f_d_exact_row_tests!",
    ):
        if required not in harness:
            fail(f"missing Stage 5F-d source proof: {required}")
    begin = stage5e.index("// STAGE5F-TEST-POST-CALLBACK-INSPECTION-BEGIN")
    end = stage5e.index("// STAGE5F-TEST-POST-CALLBACK-INSPECTION-END")
    pending_seam = stage5e.index("pub(crate) fn test_clear_public_pending_entry_request")
    if not begin < pending_seam < end:
        fail("F34 pending mismatch seam escaped the accepted test-only region")

    config = read_json(root, CONFIG)
    require(config["risk_gate_mode"], "normal_append", "target riskgate mode")
    require(config["mr_gate_policy"], "shadow_pnl_lb120_positive", "target MR gate policy")
    riskgate = read_json(root, RISKGATE)
    normal = next(seed for seed in riskgate["seeds"] if seed["seed_id"] == "valid_normal_append")
    require(normal["enforced_for_entry"], False, "normal_append enforcement")

    snapshot_gate = (root / R3_SNAPSHOT_GATE).read_text(encoding="utf-8")
    functional_gate = (root / FUNCTIONAL_GATE).read_text(encoding="utf-8")
    if f'readonly accepted_r3_ref="{ACCEPTED_R3}"' not in snapshot_gate:
        fail("R3 snapshot authority drift")
    require(functional_gate.count("bash scripts/stage5f_r3_snapshot_gate.sh"), 1, "R3 snapshot invocation")
    require(functional_gate.count("stage5f_d_atomic_matrix_check.py"), 1, "Stage 5F-d checker invocation")
    require(functional_gate.count("stage5f_d_atomic_matrix_negative_harness.py"), 1, "Stage 5F-d negative invocation")
    for command in (
        "stage5f_r3_snapshot_gate.sh",
        "stage5f_d_atomic_matrix_check.py",
        "stage5f_d_atomic_matrix_negative_harness.py",
    ):
        if re.search(rf"{re.escape(command)}[^\n]*(?:\|\||\&\&\s*true|;\s*true)", functional_gate):
            fail(f"functional gate makes {command} non-blocking")


def check(root: Path = DEFAULT_ROOT, *, verify_lineage: bool = True) -> None:
    if verify_lineage:
        validate_lineage(root)
    validate_frozen_hashes(root)
    golden, results = validate_golden(root)
    validate_scenarios_and_inventory(root, golden, results)
    validate_source_and_gates(root)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    parser.add_argument("--skip-lineage", action="store_true")
    args = parser.parse_args()
    try:
        check(args.root.resolve(), verify_lineage=not args.skip_lineage)
    except (CheckFailure, KeyError, TypeError, ValueError, StopIteration) as exc:
        print(f"stage5f-d-atomic-matrix-check: FAIL: {exc}", file=sys.stderr)
        return 1
    print("stage5f-d-atomic-matrix-check: ok rows=34 groups=16 golden=true")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
