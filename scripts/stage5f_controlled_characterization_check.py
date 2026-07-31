#!/usr/bin/env python3
"""Fail-closed contract check for Stage 5F-c controlled characterization."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


DEFAULT_ROOT = Path(__file__).resolve().parents[1]
B1_COMMIT = "86b43c448fb65a3c54b6118d04d3f40e08e74ad7"
SCENARIOS = "tests/fixtures/stage5/stage5f/v1/scenarios/atomic-hybrid-scenarios.json"
STATES = "tests/fixtures/stage5/stage5f/v1/states/imoexf-hybrid-state-seeds.json"
RISKGATE = "tests/fixtures/stage5/stage5f/v1/riskgate/imoexf-high180-riskgate-seeds.json"
CORRECTIONS = "docs/stage-5/stage5f-c-source-validity-corrections.json"
CANDIDATE = "docs/stage-5/stage5f-c-candidate-results.json"
HARNESS = "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs"
LIB = "crates/strategy-runtime-core/src/lib.rs"
CALLBACK = "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs"
STAGE5C = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
STAGE5E = "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs"
INHERITED_B1_GATE = "scripts/stage5f_inherited_b1_snapshot_gate.sh"
FUNCTIONAL_GATE = "scripts/stage5f_functional_development_gate.sh"
REPORT = "docs/stage-5/5f-c-controlled-paper-invocation.md"
INVENTORY = "docs/stage-5/stage5f-c-controlled-paper-invocation-inventory.json"
REPORT_SHA256 = "b3cb68946a4ef33ad2f185d7bca7e995b350ef8ccb39728111baa8007145c97e"
INVENTORY_SHA256 = "613ea9e8957f3b8e8c1ee89552d2754fee5ecaccaf8da16722c827b7d9c8cfa6"

INPUT_HASHES = {
    SCENARIOS: "e83f10b58ba6c72efbf95d561edc9f7de84ce8e092129f6a9b449d2683e84184",
    STATES: "bb732fcebc0da78d3acdc88a3ceeb3db11a6a5a0719a92aeb91bcdcaf11729b4",
    RISKGATE: "20e95ace0c1d92746c2198083d6b73fd0e78e1e58bc0b9b4bbcebf696fb5a1fc",
    CORRECTIONS: "3639d59331716e4247860cc3a6aa7f6032e677e63f30fc31b6e4b1eb50902c21",
}
RESULTS_ARRAY_SHA256 = "1a1d2b39369156ad6f75f68c3218b086047c4470270d04c957427c98dd933910"
HEX64 = re.compile(r"^[0-9a-f]{64}$")


class CheckFailure(RuntimeError):
    pass


def fail(message: str) -> None:
    raise CheckFailure(message)


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
            (root / relative).read_text(), object_pairs_hook=strict_object
        )
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot parse {relative}: {exc}")
    if not isinstance(value, dict):
        fail(f"{relative} must contain an object")
    return value


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(root: Path, relative: str) -> str:
    try:
        return sha256_bytes((root / relative).read_bytes())
    except OSError as exc:
        fail(f"cannot read {relative}: {exc}")


def require(actual: object, expected: object, message: str) -> None:
    if actual != expected:
        fail(f"{message}: expected {expected!r}, got {actual!r}")


def exact_keys(value: object, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        fail(f"{label} must be an object")
    if set(value) != expected:
        fail(f"{label} key-set drift")
    return value


def exact_int(value: object, expected: int, label: str) -> None:
    if type(value) is not int or value != expected:
        fail(f"{label} must be exact integer {expected}")


def require_hash(value: object, label: str) -> str:
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        fail(f"{label} must be lowercase SHA-256")
    return value


def validate_lineage(root: Path) -> None:
    try:
        subprocess.run(
            ["git", "merge-base", "--is-ancestor", B1_COMMIT, "HEAD"],
            cwd=root,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        fail(f"accepted Stage 5F-b1 commit is not an ancestor: {exc}")

    immutable = [
        SCENARIOS,
        STATES,
        RISKGATE,
        "docs/stage-5/5f-b-fixture-input-fingerprint-contract.md",
        "docs/stage-5/stage5f-b-fixture-inventory.json",
        "docs/stage-5/stage5f-controlled-observation-extension.json",
    ]
    changed = subprocess.run(
        ["git", "diff", "--name-only", B1_COMMIT, "--", *immutable],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if changed:
        fail(f"accepted Stage 5F-b1 inputs were rewritten: {changed}")


def validate_corrections(root: Path) -> dict[str, Any]:
    payload = read_json(root, CORRECTIONS)
    exact_keys(
        payload,
        {
            "base_scenario_catalog",
            "corrections",
            "invariants",
            "schema_version",
            "stage",
            "status",
        },
        "correction overlay",
    )
    exact_int(payload["schema_version"], 1, "correction schema_version")
    require(payload["stage"], "5F-c-controlled-paper-invocation", "correction stage")
    require(
        payload["status"],
        "candidate_input_correction_pending_review",
        "correction status",
    )
    require(
        payload["base_scenario_catalog"],
        {"path": SCENARIOS, "sha256": INPUT_HASHES[SCENARIOS]},
        "correction base binding",
    )
    require(
        payload["invariants"],
        {
            "strategy_parameters_changed": False,
            "production_source_changed": False,
            "expected_output_guessed": False,
            "base_fixture_rewritten": False,
            "review_required_before_golden_freeze": True,
        },
        "correction invariants",
    )
    corrections = payload["corrections"]
    if not isinstance(corrections, list) or len(corrections) != 2:
        fail("correction overlay must contain exactly F02 and F04")
    require([item.get("row_id") for item in corrections], ["F02", "F04"], "correction rows")
    for item in corrections:
        exact_keys(item, {"overrides", "reason", "row_id"}, f"correction {item.get('row_id')}")
        if not isinstance(item["reason"], str) or len(item["reason"]) < 40:
            fail(f"correction {item['row_id']} requires an explicit rationale")
        overrides = exact_keys(
            item["overrides"], {"bar", "clock", "state_seed"}, "correction overrides"
        )
        if not all(
            isinstance(overrides[name], dict)
            for name in ("bar", "clock", "state_seed")
        ):
            fail("correction sections must be objects")
    require(
        corrections[0]["overrides"],
        {
            "bar": {"close_time_utc": "2026-01-06T09:10:00Z"},
            "clock": {
                "event_ts_utc": "2026-01-06T09:10:00Z",
                "callback_ts_utc": "2026-01-06T09:10:01Z",
                "lifecycle_ts_utc": "2026-01-06T09:10:02Z",
            },
            "state_seed": {},
        },
        "F02 source-valid correction",
    )
    require(
        corrections[1]["overrides"],
        {
            "bar": {"low": "98.0", "close": "98.5"},
            "clock": {},
            "state_seed": {"active_cycle_id": "b000000001"},
        },
        "F04 source-valid correction",
    )
    return payload


def vector_hash(vector: list[object]) -> str:
    encoded = json.dumps(
        vector, ensure_ascii=False, separators=(",", ":")
    ).encode()
    return sha256_bytes(b"moex.stage5f.ordered-intent-vector.v1\0" + encoded)


def validate_candidate(root: Path) -> None:
    payload = read_json(root, CANDIDATE)
    exact_keys(
        payload,
        {
            "closed_surfaces",
            "generation",
            "inputs",
            "results",
            "schema_version",
            "stage",
            "status",
        },
        "candidate evidence",
    )
    exact_int(payload["schema_version"], 1, "candidate schema_version")
    require(payload["stage"], "5F-c-controlled-paper-invocation", "candidate stage")
    require(
        payload["status"],
        "candidate_source_characterized_not_golden",
        "candidate status",
    )
    require(
        payload["inputs"],
        {
            "scenario_catalog_sha256": INPUT_HASHES[SCENARIOS],
            "state_catalog_sha256": INPUT_HASHES[STATES],
            "riskgate_catalog_sha256": INPUT_HASHES[RISKGATE],
            "source_validity_corrections_sha256": INPUT_HASHES[CORRECTIONS],
        },
        "candidate input binding",
    )
    closed = payload["closed_surfaces"]
    if not isinstance(closed, dict) or not closed or any(value is not False for value in closed.values()):
        fail("all Stage 5F-c operational surfaces must remain closed")
    generation = exact_keys(
        payload["generation"],
        {"cargo", "command", "results_array_sha256", "rustc"},
        "candidate generation",
    )
    require(
        generation["command"],
        "cargo test -q -p strategy-runtime-core stage5f_c_candidate_matrix_evidence -- --nocapture --test-threads=1",
        "candidate command",
    )
    require(generation["results_array_sha256"], RESULTS_ARRAY_SHA256, "results hash authority")

    results = payload["results"]
    if not isinstance(results, list):
        fail("candidate results must be an array")
    require([row.get("row_id") for row in results], ["F01", "F02", "F04", "F24", "F31", "F32", "F33"], "minimum matrix order")
    serialized = json.dumps(results, indent=2, ensure_ascii=False).encode() + b"\n"
    require(sha256_bytes(serialized), RESULTS_ARRAY_SHA256, "candidate results array digest")

    expected = {
        "F01": ("accepted", 1, 1, 1, "settled", 0),
        "F02": ("accepted", 1, 1, 1, "settled", 1),
        "F04": ("accepted", 1, 1, 1, "settled", 1),
        "F24": ("blocked_before_callback", 0, 0, 0, "riskgate_authority_missing", 0),
        "F31": ("terminal_after_callback", 1, 0, 1, "CallbackValidationError", 0),
        "F32": ("terminal_after_callback", 1, 1, 1, "ChronologyMismatch", 0),
        "F33": ("terminal_after_callback", 1, 1, 1, "Stage5cIntentValidationFailed", 0),
    }
    row_keys = {
        "accepted_post_state_fingerprint",
        "b3f_outcome",
        "callback_count",
        "disposition",
        "observer_count",
        "ordered_intent_vector",
        "ordered_intent_vector_sha256",
        "pre_state_fingerprint",
        "row_id",
        "scenario_id",
        "schema_version",
        "settlement_attempt_count",
        "settlement_identity_sha256",
    }
    forbidden_raw_keys = {"account_id", "comment", "broker_order_id", "broker_stop_id", "cycle_id"}
    for row in results:
        row_id = row["row_id"]
        exact_keys(row, row_keys, f"candidate {row_id}")
        exact_int(row["schema_version"], 1, f"{row_id}.schema_version")
        disposition, callbacks, observers, settlements, outcome, intent_count = expected[row_id]
        require(
            (
                row["disposition"],
                row["callback_count"],
                row["observer_count"],
                row["settlement_attempt_count"],
                row["b3f_outcome"],
                len(row["ordered_intent_vector"]),
            ),
            (disposition, callbacks, observers, settlements, outcome, intent_count),
            f"{row_id} lifecycle result",
        )
        require_hash(row["pre_state_fingerprint"], f"{row_id}.pre_state")
        if disposition == "accepted":
            require_hash(row["accepted_post_state_fingerprint"], f"{row_id}.post_state")
            require_hash(row["settlement_identity_sha256"], f"{row_id}.settlement")
            require(
                vector_hash(row["ordered_intent_vector"]),
                row["ordered_intent_vector_sha256"],
                f"{row_id} vector hash",
            )
        else:
            require(row["accepted_post_state_fingerprint"], None, f"{row_id} terminal post-state")
            require(row["settlement_identity_sha256"], None, f"{row_id} terminal settlement")
            require(row["ordered_intent_vector_sha256"], None, f"{row_id} terminal vector")
        for projection in row["ordered_intent_vector"]:
            if not isinstance(projection, dict):
                fail(f"{row_id} projection must be an object")
            if forbidden_raw_keys.intersection(projection):
                fail(f"{row_id} projection exports raw sensitive fields")

    f02 = results[1]["ordered_intent_vector"][0]
    require((f02["intent_class"], f02["base_action"], f02["owner"], f02["role"], f02["side"]), ("entry", "market", "BO", "ENTRY", "buy"), "F02 semantic projection")
    f04 = results[2]["ordered_intent_vector"][0]
    require((f04["intent_class"], f04["base_action"], f04["owner"], f04["role"], f04["side"]), ("exit", "market", "BO", "EXIT", "sell"), "F04 semantic projection")


def validate_delivery_contract(root: Path) -> None:
    require(sha256_file(root, REPORT), REPORT_SHA256, "Stage 5F-c report hash")
    require(sha256_file(root, INVENTORY), INVENTORY_SHA256, "Stage 5F-c inventory hash")
    inventory = read_json(root, INVENTORY)
    exact_keys(
        inventory,
        {
            "closed_surfaces",
            "evidence",
            "lineage",
            "minimum_matrix",
            "next_stage",
            "review_decisions",
            "schema_version",
            "sole_route",
            "stage",
            "status",
            "target",
            "test_boundary",
        },
        "Stage 5F-c inventory",
    )
    exact_int(inventory["schema_version"], 1, "inventory schema_version")
    require(inventory["stage"], "5F-c-controlled-paper-invocation", "inventory stage")
    require(
        inventory["status"],
        "candidate_review_required_before_5f_d",
        "inventory status",
    )
    require(
        inventory["lineage"],
        {
            "accepted_b1_source_ref": B1_COMMIT,
            "accepted_b3f_source_ref": "e14654f7129aa61011931306140a3bfefe2fcfbc",
            "accepted_b1_inputs_immutable": True,
        },
        "inventory lineage",
    )
    route = inventory["sole_route"]
    require(
        route,
        {
            "callback_invocation_site_count": 1,
            "source_on_bar_expression_count": 1,
            "observer_call_site_count": 1,
            "observer_consume_count_per_scenario": 1,
            "b3f_settlement_site_count": 1,
            "alternate_orchestrator_allowed": False,
            "direct_stage5c_callback_allowed": False,
        },
        "inventory sole route",
    )
    candidate = read_json(root, CANDIDATE)
    expected_rows = [
        {
            "row_id": row["row_id"],
            "disposition": row["disposition"],
            "callback_count": row["callback_count"],
            "observer_count": row["observer_count"],
            "settlement_attempt_count": row["settlement_attempt_count"],
        }
        for row in candidate["results"]
    ]
    require(
        inventory["minimum_matrix"],
        {
            "required_row_count": 7,
            "candidate_row_count": 7,
            "rows": expected_rows,
        },
        "inventory minimum matrix",
    )
    require(
        inventory["evidence"],
        {
            "candidate_results_path": CANDIDATE,
            "candidate_results_array_sha256": RESULTS_ARRAY_SHA256,
            "source_validity_corrections_path": CORRECTIONS,
            "source_validity_corrections_sha256": INPUT_HASHES[CORRECTIONS],
            "candidate_is_golden": False,
        },
        "inventory candidate evidence",
    )
    require(
        inventory["test_boundary"],
        {
            "cfg_test_only": True,
            "crate_private": True,
            "production_source_outside_marked_regions_equals_b1": True,
            "raw_intents_retained": False,
            "observer_serializable": False,
            "observer_debuggable": False,
            "inherited_b1_negative_cases": 30,
            "current_stage5f_c_negative_cases": 37,
            "stage5f_rust_test_count": 11,
        },
        "inventory test boundary",
    )
    require(
        inventory["review_decisions"],
        [
            "approve_or_reject_additional_cfg_test_ownership_factories",
            "approve_or_reject_f02_f04_source_validity_overlay",
            "accept_compositional_stage5d_precondition_or_require_full_constructor_replay",
        ],
        "inventory review decisions",
    )
    if not isinstance(inventory["closed_surfaces"], dict) or any(
        value is not False for value in inventory["closed_surfaces"].values()
    ):
        fail("Stage 5F-c inventory opened an operational surface")
    require(
        inventory["next_stage"],
        {
            "stage": "5F-d-complete-atomic-hybrid-matrix",
            "allowed_before_independent_review": False,
        },
        "inventory review hold",
    )
    report = (root / REPORT).read_text()
    for statement in (
        "candidate implementation; independent review required before Stage 5F-d",
        "It is a compositional proof, not a claim that",
        "Until all three are resolved, candidate results stay non-golden",
    ):
        if statement not in report:
            fail(f"Stage 5F-c report decision missing: {statement}")


def marker_region(text: str, begin: str, end: str, label: str) -> str:
    if text.count(begin) != 1 or text.count(end) != 1:
        fail(f"{label} marker cardinality drift")
    start = text.index(begin)
    finish = text.index(end, start) + len(end)
    return text[start:finish]


def strip_region(text: str, begin: str, end: str) -> str:
    marker_start = text.index("// " + begin)
    start = text.rfind("\n", 0, marker_start) + 1
    finish = text.index("// " + end, start) + len("// " + end)
    if finish < len(text) and text[finish] == "\n":
        finish += 1
    if text[:start].endswith("\n\n") and text[finish:].startswith("\n"):
        finish += 1
    return text[:start] + text[finish:]


def baseline_text(root: Path, relative: str) -> str:
    try:
        return subprocess.run(
            ["git", "show", f"{B1_COMMIT}:{relative}"],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError) as exc:
        fail(f"cannot load B1 source baseline for {relative}: {exc}")


def validate_source_boundary(root: Path, check_lineage: bool) -> None:
    lib = (root / LIB).read_text()
    callback = (root / CALLBACK).read_text()
    stage5c = (root / STAGE5C).read_text()
    stage5e = (root / STAGE5E).read_text()
    harness = (root / HARNESS).read_text()

    lib_region = marker_region(lib, "STAGE5F-TEST-OBSERVATION-MODULE-BEGIN", "STAGE5F-TEST-OBSERVATION-MODULE-END", "observer module")
    if "#[cfg(test)]\nmod stage5f_atomic_hybrid_semantics;" not in lib_region or "pub mod" in lib_region:
        fail("observer module must remain private and cfg(test)-only")
    callback_region = marker_region(callback, "STAGE5F-TEST-OBSERVATION-CALL-BEGIN", "STAGE5F-TEST-OBSERVATION-CALL-END", "observer call")
    expected_order = [
        "let intents = Strategy::on_bar(self, &context, &bar);",
        "#[cfg(test)]",
        "observe_exact_on_bar_result(&intents);",
    ]
    positions = [callback_region.find(token) for token in expected_order]
    if any(position < 0 for position in positions) or positions != sorted(positions):
        fail("observer must run immediately after the exact on_bar expression")
    if any(token in callback_region for token in ("if ", "match ", "return ", "unwrap", "expect(")):
        fail("observer call region may not control callback flow")
    if "Ok(intents)" not in callback[callback.index(callback_region) + len(callback_region):callback.index(callback_region) + len(callback_region) + 80]:
        fail("callback must return the unchanged observed vector")

    stage5c_region = marker_region(stage5c, "STAGE5F-TEST-OWNERSHIP-FACTORY-BEGIN", "STAGE5F-TEST-OWNERSHIP-FACTORY-END", "Stage 5C ownership factory")
    if "#[cfg(test)]" not in stage5c_region or "pub(crate) fn stage5f_test_sequence_inputs_from_owned_strategy" not in stage5c_region:
        fail("Stage 5C ownership factory must remain crate-private and cfg(test)")
    b3c_region = marker_region(stage5e, "STAGE5F-TEST-B3C-FACTORY-BEGIN", "STAGE5F-TEST-B3C-FACTORY-END", "B3C factory")
    callback_error_region = marker_region(stage5e, "STAGE5F-TEST-CALLBACK-VALIDATION-SEAM-BEGIN", "STAGE5F-TEST-CALLBACK-VALIDATION-SEAM-END", "callback-validation seam")
    for label, region in (("B3C factory", b3c_region), ("callback-validation seam", callback_error_region)):
        if "#[cfg(test)]" not in region or "pub(crate) fn stage5f_" not in region:
            fail(f"{label} must remain crate-private and cfg(test)")

    if "pub(crate) struct Stage5fObservationScope" not in harness or "pub(crate) struct Stage5fObservedIntentVector" not in harness:
        fail("linear observer types missing")
    if "pub(crate) fn consume_once(mut self) -> Option<Stage5fObservedIntentVector>" not in harness:
        fail("observer result is not consumed by linear ownership")
    if 'panic!("Stage 5F observer saw a second callback before consume")' not in harness:
        fail("observer no longer fails closed on a second callback")
    if "thread_local!" not in harness or "impl Drop for Stage5fObservationScope" not in harness:
        fail("observer scope isolation/cleanup contract missing")
    if (
        'value.bytes().all(|byte| byte.is_ascii_hexdigit())' not in harness
        or '"source cycle id must be production-valid ASCII hex"' not in harness
        or 'format!("{:08x}01"' in harness
    ):
        fail("fixture cycle IDs are not validated against production parser semantics")
    for declaration in ("Stage5fObservationScope", "Stage5fObservedIntentVector"):
        prefix = harness[: harness.index(f"struct {declaration}")].splitlines()[-3:]
        if any(re.search(r"derive\([^)]*(Clone|Copy|Serialize|Deserialize|Debug|Display|Default)", line) for line in prefix):
            fail(f"{declaration} gained a forbidden trait")
    forbidden_tokens = [
        "unsafe {",
        "reqwest",
        "redis::",
        "TcpStream",
        "std::process::Command",
        "RiskGateMode::Enforced",
        "HybridOrchestrator::new",
        "IntradayBreakoutEngine::new",
        "MeanReversionEngine::new",
        "raw_intents",
    ]
    for token in forbidden_tokens:
        if token in harness:
            fail(f"Stage 5F harness opened forbidden surface: {token}")
    if harness.count("invoke_stage5e_authorized_paper_callback_at(authority") != 1:
        fail("Stage 5F harness must contain one accepted callback invocation site")
    if harness.count("validate_and_settle_stage5e_paper_callback_escrow(escrow)") != 1:
        fail("Stage 5F harness must contain one canonical B3F settlement site")
    if "Strategy::on_bar(" in harness or ".on_broker_bar(" in harness:
        fail("Stage 5F harness contains a direct callback bypass")
    required_tests = {
        "stage5f_f01_no_signal_zero_intent",
        "stage5f_f02_bo_long_entry",
        "stage5f_f04_bo_long_normal_exit",
        "stage5f_f24_riskgate_missing_authority_blocks_before_callback",
        "stage5f_f31_callback_validation_terminal",
        "stage5f_f32_b3f_identity_or_chronology_preflight_terminal",
        "stage5f_f33_stage5c_intent_validation_terminal",
        "stage5f_observer_rejects_second_callback_before_consume",
        "stage5f_unconsumed_scope_drop_clears_only_its_generation",
    }
    missing = sorted(name for name in required_tests if f"fn {name}(" not in harness)
    if missing:
        fail(f"required Stage 5F-c tests missing: {missing}")
    if harness.count("stage5e_test_reset_b3e_callback_count();") != 1:
        fail("the thread-local callback counter must be reset exactly once per scenario")
    if harness.count("stage5e_test_b3e_callback_count();") != 1:
        fail("the thread-local callback counter must be read exactly once per scenario")

    lib_stripped = strip_region(lib, "STAGE5F-TEST-OBSERVATION-MODULE-BEGIN", "STAGE5F-TEST-OBSERVATION-MODULE-END")
    callback_stripped = strip_region(callback, "STAGE5F-TEST-OBSERVATION-CALL-BEGIN", "STAGE5F-TEST-OBSERVATION-CALL-END")
    stage5c_stripped = strip_region(stage5c, "STAGE5F-TEST-OWNERSHIP-FACTORY-BEGIN", "STAGE5F-TEST-OWNERSHIP-FACTORY-END")
    stage5e_stripped = strip_region(stage5e, "STAGE5F-TEST-CALLBACK-VALIDATION-SEAM-BEGIN", "STAGE5F-TEST-CALLBACK-VALIDATION-SEAM-END")
    stage5e_stripped = strip_region(stage5e_stripped, "STAGE5F-TEST-B3C-FACTORY-BEGIN", "STAGE5F-TEST-B3C-FACTORY-END")
    if "stage5f_atomic_hybrid_semantics" in lib_stripped or "observe_exact_on_bar_result" in callback_stripped:
        fail("observer dependency escaped its cfg(test) marker region")
    if "stage5f_test_sequence_inputs_from_owned_strategy" in stage5c_stripped:
        fail("Stage 5F ownership factory escaped its marker region")
    if "stage5f_test_b3c_from_sequence_inputs" in stage5e_stripped or "stage5f_test_force_callback_validation_error" in stage5e_stripped:
        fail("Stage 5F B3C seam escaped its marker region")

    if check_lineage:
        require(
            lib_stripped,
            baseline_text(root, LIB),
            "lib source outside Stage 5F region",
        )
        callback_stripped = callback_stripped.replace("        Ok(intents)\n", "        Ok(Strategy::on_bar(self, &context, &bar))\n", 1)
        require(callback_stripped, baseline_text(root, CALLBACK), "callback source outside Stage 5F region")
        require(
            stage5c_stripped,
            baseline_text(root, STAGE5C),
            "Stage 5C source outside test-only factory",
        )
        require(stage5e_stripped, baseline_text(root, STAGE5E), "Stage 5E source outside test-only regions")


def validate_inherited_b1_gate(root: Path) -> None:
    inherited = (root / INHERITED_B1_GATE).read_text()
    functional = (root / FUNCTIONAL_GATE).read_text()

    require(
        inherited.count(
            'readonly accepted_b1_ref="86b43c448fb65a3c54b6118d04d3f40e08e74ad7"'
        ),
        1,
        "immutable B1 scanner authority",
    )
    required_fragments = (
        'git -C "$repo_root" cat-file -e "${accepted_b1_ref}^{commit}"',
        'git -C "$repo_root" merge-base --is-ancestor "$accepted_b1_ref" HEAD',
        'git -C "$repo_root" archive --format=tar "$accepted_b1_ref"',
        'bash scripts/forbidden_surface_scan.sh',
        '"$python_bin" scripts/stage5f_atomic_hybrid_semantics_negative_harness.py',
        '"$python_bin" scripts/stage5f_ci_snapshot_inheritance_negative_harness.py',
    )
    for fragment in required_fragments:
        if fragment not in inherited:
            fail(f"inherited forbidden snapshot gate fragment missing: {fragment}")
    for token in ('|| true', 'accepted_b1_ref="HEAD"', 'archive --format=tar HEAD'):
        if token in inherited:
            fail(f"inherited forbidden snapshot gate gained bypass token: {token}")

    expected_call = "bash scripts/stage5f_inherited_b1_snapshot_gate.sh"
    require(functional.count(expected_call), 1, "functional inherited scanner call")
    if "bash scripts/forbidden_surface_scan.sh" in functional:
        fail("functional gate may not run the historical scanner on the current tree")
    if functional.index(expected_call) > functional.index(
        'scripts/stage5f_controlled_characterization_check.py'
    ):
        fail("inherited scanner proof must run before Stage 5F-c characterization")
    if "|| true" in functional:
        fail("functional gate may not make a required check non-blocking")


def run(root: Path, check_lineage: bool = True) -> None:
    root = root.resolve()
    for relative, expected in INPUT_HASHES.items():
        require(sha256_file(root, relative), expected, f"input hash {relative}")
    if check_lineage:
        validate_lineage(root)
    validate_corrections(root)
    validate_candidate(root)
    validate_delivery_contract(root)
    validate_source_boundary(root, check_lineage)
    validate_inherited_b1_gate(root)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    parser.add_argument(
        "--isolated-negative-harness",
        action="store_true",
        help=argparse.SUPPRESS,
    )
    args = parser.parse_args()
    try:
        run(args.root, check_lineage=not args.isolated_negative_harness)
    except (CheckFailure, OSError, subprocess.CalledProcessError) as exc:
        print(f"stage5f-controlled-characterization-check: FAIL: {exc}", file=sys.stderr)
        return 1
    print("stage5f-controlled-characterization-check: ok rows=7 candidate_not_golden=true")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
