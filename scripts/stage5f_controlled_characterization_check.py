#!/usr/bin/env python3
"""Fail-closed contract check for Stage 5F-c controlled characterization."""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
import re
import subprocess
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any


DEFAULT_ROOT = Path(__file__).resolve().parents[1]
B1_COMMIT = "86b43c448fb65a3c54b6118d04d3f40e08e74ad7"
R1_PREDECESSOR = "11826285d05638b6b0e29c64a3435870091dac38"
V1_SCENARIOS = "tests/fixtures/stage5/stage5f/v1/scenarios/atomic-hybrid-scenarios.json"
V1_STATES = "tests/fixtures/stage5/stage5f/v1/states/imoexf-hybrid-state-seeds.json"
V1_RISKGATE = "tests/fixtures/stage5/stage5f/v1/riskgate/imoexf-high180-riskgate-seeds.json"
V1_CORRECTIONS = "docs/stage-5/stage5f-c-source-validity-corrections.json"
V1_CANDIDATE = "docs/stage-5/stage5f-c-candidate-results.json"
SCENARIOS = "tests/fixtures/stage5/stage5f/v2/scenarios/atomic-hybrid-scenarios.json"
STATES = "tests/fixtures/stage5/stage5f/v2/states/imoexf-hybrid-state-seeds.json"
RISKGATE = "tests/fixtures/stage5/stage5f/v2/riskgate/imoexf-high180-riskgate-seeds.json"
TARGET_CONFIG = "tests/fixtures/stage5/stage5f/v2/config/imoexf-target-config.json"
CANDIDATE = "docs/stage-5/stage5f-c-r1-candidate-results.json"
SCHEMA_OWNER_INVENTORY = "docs/stage-5/stage5f-c-r1-schema-owner-inventory.json"
HARNESS = "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs"
LIB = "crates/strategy-runtime-core/src/lib.rs"
CALLBACK = "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs"
STAGE5C = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
STAGE5D = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
STAGE5E = "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs"
INHERITED_B1_GATE = "scripts/stage5f_inherited_b1_snapshot_gate.sh"
FUNCTIONAL_GATE = "scripts/stage5f_functional_development_gate.sh"
REPORT = "docs/stage-5/5f-c-r1-review-closure.md"
INVENTORY = "docs/stage-5/stage5f-c-r1-review-closure-inventory.json"
TEST_SEAM_MANIFEST = "docs/stage-5/stage5f-c-test-seam-manifest.json"
NEGATIVE_HARNESS = "scripts/stage5f_controlled_characterization_negative_harness.py"
REPORT_SHA256 = "fd442ae417ac9d825ef6be5861c52431faa3f977ca37610c46cfd16ff5efe248"
INVENTORY_SHA256 = "5e5e8232ec4ac1cf2a63d84d956e79e595862c24abc1d0d7506b58d58869e38a"
SCHEMA_OWNER_INVENTORY_SHA256 = "bb5fb56abe4da863956d4f84490191f8578504201200ed9aae34c2e029095998"
TEST_SEAM_MANIFEST_SHA256 = "fae564929196c47ecfa1a97fdd4d9652d5abbbd43a7e81d8714610425c4b8a85"
NEGATIVE_HARNESS_SHA256 = "9d0b732ca1e09518bdfb7a46cd64d8b83524d8d6ddd75f4bcec172cd1805d07f"

INPUT_HASHES = {
    SCENARIOS: "251dbbdb363a2e6e09fd9ab08df3df5473ca2d298e2bbdbfc0fe58d806efa744",
    STATES: "4bc6aa42b0a411aab489ada3618930fc63d87c00f1a290e8efd8f61ce8d56213",
    RISKGATE: "dd3ea7894df922984896ee20ebd114412d1675e666683c958fe98eb724a22584",
    TARGET_CONFIG: "3c46aa4bdfb5a6ac3350d0f3b52ad5050abc472c653bacda512dffebfeb07e41",
}
V1_INPUT_HASHES = {
    V1_SCENARIOS: "e83f10b58ba6c72efbf95d561edc9f7de84ce8e092129f6a9b449d2683e84184",
    V1_STATES: "bb732fcebc0da78d3acdc88a3ceeb3db11a6a5a0719a92aeb91bcdcaf11729b4",
    V1_RISKGATE: "20e95ace0c1d92746c2198083d6b73fd0e78e1e58bc0b9b4bbcebf696fb5a1fc",
    V1_CORRECTIONS: "3639d59331716e4247860cc3a6aa7f6032e677e63f30fc31b6e4b1eb50902c21",
    V1_CANDIDATE: "9c9a33573a4e599252a1e9b4ec4813ad7565f1cf50d82a33f69b0a1268550092",
}
RESULTS_ARRAY_SHA256 = "e02643a004e9b325a276a1a65e41eda45347e8bb6efe0b0269e838099d909c81"
HEX64 = re.compile(r"^[0-9a-f]{64}$")
EXPECTED_TEST_REGION_SHA256 = {
    "observer module": "a4201f2e741df60187eff06dd249a39a0d4c18ed6ef1fa7d2b3653caab4e514e",
    "observer call": "0b39bfe30a0b713e784df9751146547bf528084124dc089a885023e4a34ea3f6",
    "Stage 5C ownership factory": "bce460970420c0eadcf7580f107f96859dd52a11138c81fe7f9dc2a91f1d3de6",
    "Stage 5D full restart oracle": "b8681197e56e5dc88b74b9ddce5547dde639ebba550c27a9f491891c2273f145",
    "callback-validation seam": "9357d9b17646a80ebd13c20df6bc3d7b1626eb440230cf818a8cbd5dcd1ae81e",
    "B3C factory": "3ebd8b83ebeab201c500cc54a0e2386cb11714ecb36980ae6f0eb5ee63bb3e92",
}
EXPECTED_NORMALIZED_SOURCE_SHA256 = {
    "observer module": "4a248db1a97799604bcfcb094abd1b22abebc98aec67882c829e1fa5a884e7ae",
    "observer call": "7f5e3ad070c1bbc3ddca1e642d59b3f4cf75b9bb0d1651068df363323f1cd427",
    "Stage 5C ownership factory": "0fce95557b2e7673d7e7e74a5b4d65dd3ec28360fab3674c20e3e6de6be02ff3",
    "Stage 5D full restart oracle": "02bbfb225f33a8f60bab860817f01d340bdf91076d3cbcd59114386c8ac12f4d",
    "callback-validation seam": "34ed25d3ee188d3f0c52d4b655c6105349e9761b7bd3a5af934e52cab14fb2d6",
    "B3C factory": "34ed25d3ee188d3f0c52d4b655c6105349e9761b7bd3a5af934e52cab14fb2d6",
}
EXPECTED_REGION_PATH_MARKERS = {
    "observer module": (LIB, "STAGE5F-TEST-OBSERVATION-MODULE-BEGIN", "STAGE5F-TEST-OBSERVATION-MODULE-END"),
    "observer call": (CALLBACK, "STAGE5F-TEST-OBSERVATION-CALL-BEGIN", "STAGE5F-TEST-OBSERVATION-CALL-END"),
    "Stage 5C ownership factory": (STAGE5C, "STAGE5F-TEST-OWNERSHIP-FACTORY-BEGIN", "STAGE5F-TEST-OWNERSHIP-FACTORY-END"),
    "Stage 5D full restart oracle": (STAGE5D, "STAGE5F-TEST-FULL-RESTART-ORACLE-BEGIN", "STAGE5F-TEST-FULL-RESTART-ORACLE-END"),
    "callback-validation seam": (STAGE5E, "STAGE5F-TEST-CALLBACK-VALIDATION-SEAM-BEGIN", "STAGE5F-TEST-CALLBACK-VALIDATION-SEAM-END"),
    "B3C factory": (STAGE5E, "STAGE5F-TEST-B3C-FACTORY-BEGIN", "STAGE5F-TEST-B3C-FACTORY-END"),
}


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
    for label, commit in (
        ("accepted Stage 5F-b1", B1_COMMIT),
        ("rejected R1 predecessor retained in history", R1_PREDECESSOR),
    ):
        try:
            subprocess.run(
                ["git", "merge-base", "--is-ancestor", commit, "HEAD"],
                cwd=root,
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
            )
        except (OSError, subprocess.CalledProcessError) as exc:
            fail(f"{label} commit is not an ancestor: {exc}")

    rejected_v1_evidence = [
        V1_SCENARIOS,
        V1_STATES,
        V1_RISKGATE,
        V1_CORRECTIONS,
        V1_CANDIDATE,
    ]
    changed = subprocess.run(
        ["git", "diff", "--name-only", R1_PREDECESSOR, "--", *rejected_v1_evidence],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if changed:
        fail(f"rejected Stage 5F-c v1 evidence was rewritten: {changed}")


def parse_utc(value: object, label: str) -> datetime:
    if not isinstance(value, str) or not value.endswith("Z"):
        fail(f"{label} must be an RFC3339 UTC timestamp")
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as exc:
        fail(f"{label} is not a valid timestamp: {exc}")
    if parsed.tzinfo != timezone.utc:
        fail(f"{label} must use UTC")
    return parsed


def validate_cycle(cycle: object, label: str) -> dict[str, Any]:
    value = exact_keys(
        cycle,
        {"created_ts_utc", "sequence", "value"},
        label,
    )
    created = parse_utc(value["created_ts_utc"], f"{label}.created_ts_utc")
    sequence = value["sequence"]
    if type(sequence) is not int or not 0 <= sequence <= 0xFF:
        fail(f"{label}.sequence must be a u8-compatible integer")
    identity = value["value"]
    if not isinstance(identity, str) or not re.fullmatch(r"[0-9a-f]{10}", identity):
        fail(f"{label}.value must be ten lowercase hexadecimal characters")
    expected = f"{int(created.timestamp()) & 0xFFFF_FFFF:08x}{sequence & 0xFF:02x}"
    require(identity, expected, f"{label} production-equivalent identity")
    return value


def validate_schema_owner_inventory(
    root: Path,
    scenarios: dict[str, Any],
    states: dict[str, Any],
    riskgate: dict[str, Any],
    config: dict[str, Any],
) -> None:
    require(
        sha256_file(root, SCHEMA_OWNER_INVENTORY),
        SCHEMA_OWNER_INVENTORY_SHA256,
        "schema-owner inventory hash",
    )
    inventory = read_json(root, SCHEMA_OWNER_INVENTORY)
    exact_keys(
        inventory,
        {"files", "schema_version", "stage", "status"},
        "schema-owner inventory",
    )
    exact_int(inventory["schema_version"], 1, "schema-owner schema_version")
    require(inventory["stage"], "5F-c-R2", "schema-owner stage")
    require(
        inventory["status"],
        "canonical_v2_reachability_corrected_non_golden",
        "schema-owner status",
    )
    files = inventory["files"]
    if not isinstance(files, list):
        fail("schema-owner files must be an array")
    expected = [
        {
            "path": SCENARIOS,
            "owner": "stage5f_scenario_catalog_v2",
            "top_level_keys": sorted(scenarios),
            "owned_nested_keys": {
                "bar": sorted(scenarios["records"][0]["bar"]),
                "broker_truth": sorted(scenarios["records"][0]["broker_truth"]),
                "clock": sorted(scenarios["records"][0]["clock"]),
                "record": sorted(scenarios["records"][0]),
            },
        },
        {
            "path": STATES,
            "owner": "stage5f_typed_runtime_state_materializer",
            "top_level_keys": sorted(states),
            "owned_nested_keys": {
                "seed": sorted(states["seeds"][0]),
                "seed_defaults": sorted(states["seed_defaults"]),
            },
        },
        {
            "path": RISKGATE,
            "owner": "stage5d_authoritative_riskgate_bridge",
            "top_level_keys": sorted(riskgate),
            "owned_nested_keys": {
                "seed": sorted(riskgate["seeds"][0]),
            },
        },
        {
            "path": TARGET_CONFIG,
            "owner": "stage5f_target_config_v1",
            "top_level_keys": sorted(config),
            "owned_nested_keys": {
                "breakout": sorted(config["breakout"]),
                "classic_mr": sorted(config["classic_mr"]),
                "orchestrator": sorted(config["orchestrator"]),
            },
        },
    ]
    require(files, expected, "schema-key-to-owner inventory")


def validate_v2_fixtures(root: Path) -> None:
    scenarios = read_json(root, SCENARIOS)
    states = read_json(root, STATES)
    riskgate = read_json(root, RISKGATE)
    config = read_json(root, TARGET_CONFIG)

    exact_keys(
        scenarios,
        {
            "characterization_policy",
            "clock_ownership",
            "fixture_kind",
            "records",
            "schema_version",
            "source_v1",
            "status",
            "target_config",
        },
        "v2 scenario catalog",
    )
    exact_int(scenarios["schema_version"], 2, "scenario schema_version")
    require(
        scenarios["fixture_kind"],
        "stage5f-atomic-hybrid-scenario-catalog-v2",
        "scenario fixture kind",
    )
    require(scenarios["status"], "canonical_r2_non_golden", "scenario status")
    require(
        scenarios["source_v1"],
        {
            "path": V1_SCENARIOS,
            "sha256": V1_INPUT_HASHES[V1_SCENARIOS],
            "status": "rejected_development_evidence_immutable",
        },
        "scenario v1 lineage",
    )
    require(
        scenarios["target_config"],
        {"path": TARGET_CONFIG, "sha256": INPUT_HASHES[TARGET_CONFIG]},
        "scenario target config binding",
    )
    require(
        scenarios["characterization_policy"],
        {
            "current_status": "canonical_v2_pending_source_characterization",
            "pending_outputs_are_acceptance_evidence": False,
            "source_callback_allowed_for_existing_seven_rows_only": True,
            "candidate_outputs_require_separate_freeze": True,
            "correction_overlay_allowed": False,
        },
        "scenario characterization policy",
    )
    require(
        scenarios["clock_ownership"],
        {
            "event_ts_utc": "broker_neutral_event_context_and_bar_close",
            "lifecycle_ts_utc": "stage5c_admission_schedule_and_recovery",
            "callback_ts_utc": "stage5e_callback_authority_issue_and_invoke",
        },
        "clock ownership",
    )

    exact_keys(
        states,
        {"fixture_kind", "schema_version", "seed_defaults", "seeds", "source_v1", "target"},
        "v2 state catalog",
    )
    exact_int(states["schema_version"], 2, "state schema_version")
    require(states["fixture_kind"], "stage5f-hybrid-state-seed-catalog-v2", "state kind")
    require(
        states["source_v1"],
        {
            "path": V1_STATES,
            "sha256": V1_INPUT_HASHES[V1_STATES],
            "status": "rejected_development_evidence_immutable",
        },
        "state v1 lineage",
    )
    defaults = exact_keys(
        states["seed_defaults"],
        {
            "current_day_close",
            "current_day_high",
            "current_day_low",
            "day_before_close",
            "entry_ready",
            "last_bar_close",
            "last_day_local",
            "next_cycle_seq",
            "prev_day_close",
            "prev_day_range",
            "prev_day_return",
            "private_state",
            "riskgate_semantic_state",
            "safe_mode_close_only",
            "today_start_local",
        },
        "state defaults",
    )
    require(
        defaults["riskgate_semantic_state"],
        {
            "mr_enabled_current_session": True,
            "rolling_sum_lb120": "158.60000000000008",
            "last_finalized_session_date": "2026-01-05",
            "ledger_rows_count": 221,
            "current_shadow_session_date": "2026-01-06",
            "current_shadow_pnl_points": "0.0",
            "current_shadow_trade_count": 0,
        },
        "state riskgate defaults",
    )
    seeds = states["seeds"]
    if not isinstance(seeds, list) or len(seeds) != 14:
        fail("v2 state catalog must contain exactly 14 seeds")
    expected_seed_ids = [
        "flat_ready",
        "bo_long_open",
        "bo_short_open",
        "bo_long_carried",
        "mr_long_open",
        "mr_short_open",
        "bo_owner_active",
        "mr_owner_active",
        "duplicate_bar_flat",
        "post_cleanup_flat",
        "pending_entry",
        "pending_exit",
        "deferred_entry",
        "deferred_exit",
    ]
    require([seed.get("seed_id") for seed in seeds], expected_seed_ids, "state seed order")
    seed_keys = {
        "active_cycle",
        "current_owner",
        "current_side",
        "deferred_entry",
        "deferred_exit",
        "last_processed_bar_ts_utc",
        "orchestrator_state",
        "overnight_exit_armed_date",
        "pending_entry",
        "pending_exit",
        "position_qty",
        "seed_id",
        "state_class",
        "was_long_today",
        "was_short_today",
    }
    for seed in seeds:
        seed_id = seed["seed_id"]
        exact_keys(seed, seed_keys, f"state seed {seed_id}")
        last_processed = parse_utc(
            seed["last_processed_bar_ts_utc"],
            f"{seed_id}.last_processed_bar_ts_utc",
        )
        cycles: list[tuple[str, dict[str, Any]]] = []
        if seed["active_cycle"] is not None:
            cycles.append(("active_cycle", validate_cycle(seed["active_cycle"], f"{seed_id}.active_cycle")))
        for field in ("pending_entry", "pending_exit", "deferred_entry", "deferred_exit"):
            nested = seed[field]
            if nested is not None:
                if not isinstance(nested, dict) or "cycle" not in nested:
                    fail(f"{seed_id}.{field} must own a cycle")
                cycles.append((field, validate_cycle(nested["cycle"], f"{seed_id}.{field}.cycle")))
        for field, cycle in cycles:
            created = parse_utc(cycle["created_ts_utc"], f"{seed_id}.{field}.created")
            if created > last_processed.replace(microsecond=0) + timedelta(seconds=1):
                fail(f"{seed_id}.{field} cycle is newer than last processed bar")
        for field in ("pending_entry", "pending_exit"):
            nested = seed[field]
            if nested is not None:
                require(seed["active_cycle"], nested["cycle"], f"{seed_id} active/{field} cycle")
        if seed_id == "pending_entry":
            require((seed["current_owner"], seed["current_side"]), (None, None), "source-valid pending entry ownership")

    exact_keys(
        riskgate,
        {"fixture_kind", "schema_version", "seeds", "source_v1", "target"},
        "v2 riskgate catalog",
    )
    exact_int(riskgate["schema_version"], 2, "riskgate schema_version")
    require(riskgate["fixture_kind"], "stage5f-riskgate-seed-catalog-v2", "riskgate kind")
    require(riskgate["target"], None, "riskgate target")
    require(
        [seed.get("seed_id") for seed in riskgate["seeds"]],
        ["valid_normal_append", "missing_authority", "inconsistent_authority", "materialization_terminal"],
        "riskgate seed order",
    )

    exact_int(config["schema_version"], 1, "target config schema_version")
    require(config["fixture_kind"], "stage5f-target-config-v1", "target config kind")
    require(config["status"], "canonical_r1_non_golden", "target config status")
    require(config["model_session_start_time"], "09:00:00", "session start")
    require(config["model_session_end_time"], "23:49:59", "session end")
    exact_int(config["pending_timeout_sec"], 60, "pending timeout")
    require(
        config["expected_stage5d_config_fingerprint"],
        "stage5d_cfg_sha256:56141846cb180b8a224a1db7e1f5188c99c28f0fab88a27ebe65fbcb9d7cf626",
        "canonical config fingerprint",
    )
    if any(value is not False for value in config["closed_surfaces"].values()):
        fail("target config opened an operational surface")

    records = scenarios["records"]
    if not isinstance(records, list) or len(records) != 34:
        fail("v2 scenario catalog must contain exactly 34 records")
    require(
        [row.get("row_id") for row in records],
        [f"F{index:02d}" for index in range(1, 35)],
        "scenario row order",
    )
    for row in records:
        row_id = row["row_id"]
        exact_int(row["schema_version"], 2, f"{row_id}.schema_version")
        event = parse_utc(row["clock"]["event_ts_utc"], f"{row_id}.event")
        lifecycle = parse_utc(row["clock"]["lifecycle_ts_utc"], f"{row_id}.lifecycle")
        callback = parse_utc(row["clock"]["callback_ts_utc"], f"{row_id}.callback")
        if not event < lifecycle < callback:
            fail(f"{row_id} clock ownership must be event < lifecycle < callback")
        require((lifecycle - event).total_seconds(), 1.0, f"{row_id} lifecycle offset")
        require((callback - lifecycle).total_seconds(), 1.0, f"{row_id} callback offset")
        require(row["bar"]["close_time_utc"], row["clock"]["event_ts_utc"], f"{row_id} event/bar binding")
        require(row["bar"]["origin"], "Live", f"{row_id} bar origin")
        require(row["bar"]["is_final"], True, f"{row_id} final bar")
        exact_int(row["bar"]["timeframe_sec"], 600, f"{row_id} timeframe")
        require(row["pre_state"]["catalog_path"], STATES, f"{row_id} state path")
        require(row["pre_state"]["catalog_sha256"], INPUT_HASHES[STATES], f"{row_id} state hash")
        require(row["riskgate"]["catalog_path"], RISKGATE, f"{row_id} riskgate path")
        require(row["riskgate"]["catalog_sha256"], INPUT_HASHES[RISKGATE], f"{row_id} riskgate hash")
        broker_truth = exact_keys(
            row["broker_truth"],
            {"working_order_ids"},
            f"{row_id} broker truth",
        )
        working_order_ids = broker_truth["working_order_ids"]
        if not isinstance(working_order_ids, list) or any(
            not isinstance(order_id, str) or not order_id.strip()
            for order_id in working_order_ids
        ):
            fail(f"{row_id} working order evidence must be non-empty strings")
        if len(working_order_ids) != len(set(working_order_ids)):
            fail(f"{row_id} working order evidence must be unique")
    f02 = records[1]
    require(f02["bar"]["close_time_utc"], "2026-01-06T09:10:00Z", "F02 post-wait close")
    f04 = records[3]
    require((f04["bar"]["low"], f04["bar"]["close"]), ("98.0", "98.5"), "F04 source-valid stop2")
    if V1_CORRECTIONS in json.dumps(scenarios, ensure_ascii=False):
        fail("canonical v2 scenarios depend on the rejected correction overlay")

    validate_schema_owner_inventory(root, scenarios, states, riskgate, config)


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
    exact_int(payload["schema_version"], 2, "candidate schema_version")
    require(payload["stage"], "5F-c-R1-controlled-paper-invocation", "candidate stage")
    require(
        payload["status"],
        "candidate_source_characterized_not_golden",
        "candidate status",
    )
    require(
        payload["inputs"],
        {
            "scenario_catalog_path": SCENARIOS,
            "scenario_catalog_sha256": INPUT_HASHES[SCENARIOS],
            "state_catalog_path": STATES,
            "state_catalog_sha256": INPUT_HASHES[STATES],
            "riskgate_catalog_path": RISKGATE,
            "riskgate_catalog_sha256": INPUT_HASHES[RISKGATE],
            "target_config_path": TARGET_CONFIG,
            "target_config_sha256": INPUT_HASHES[TARGET_CONFIG],
            "canonical_config_fingerprint": "stage5d_cfg_sha256:56141846cb180b8a224a1db7e1f5188c99c28f0fab88a27ebe65fbcb9d7cf626",
            "cargo_lock_sha256": "ff535d0490a848e43631906ee8abd8633630d162714299f7628c0e5fe8a0b36b",
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
        "F24": ("blocked_before_callback", 0, 0, 0, "LedgerEvidenceInvalid", 0),
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
        exact_int(row["schema_version"], 2, f"{row_id}.schema_version")
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


def validate_negative_harness_inventory(root: Path) -> None:
    require(
        sha256_file(root, NEGATIVE_HARNESS),
        NEGATIVE_HARNESS_SHA256,
        "Stage 5F-c negative harness hash",
    )
    try:
        tree = ast.parse((root / NEGATIVE_HARNESS).read_text())
    except (OSError, SyntaxError) as exc:
        fail(f"cannot parse Stage 5F-c negative harness: {exc}")
    cases_node: ast.List | None = None
    for node in tree.body:
        if (
            isinstance(node, ast.AnnAssign)
            and isinstance(node.target, ast.Name)
            and node.target.id == "CASES"
            and isinstance(node.value, ast.List)
        ):
            cases_node = node.value
            break
    if cases_node is None:
        fail("Stage 5F-c negative CASES inventory is missing")
    names: list[str] = []
    for index, case in enumerate(cases_node.elts, start=1):
        if (
            not isinstance(case, ast.Tuple)
            or len(case.elts) != 2
            or not isinstance(case.elts[0], ast.Constant)
            or not isinstance(case.elts[0].value, str)
        ):
            fail(f"negative case {index} has a non-canonical declaration")
        names.append(case.elts[0].value)
    exact_int(len(names), 51, "negative case count")
    if len(set(names)) != len(names):
        fail("negative case names must be unique")
    required = {
        "review-bypass-lib-unguarded-module",
        "review-bypass-callback-unguarded-statement",
        "review-bypass-stage5c-unguarded-function",
        "review-bypass-stage5e-unguarded-function",
        "stage5d-full-restart-oracle-not-cfg-test",
        "stage5d-full-restart-extra-item",
        "v2-cycle-production-identity-forged",
        "v2-clock-order-swapped",
        "v2-correction-overlay-dependency",
    }
    missing = sorted(required.difference(names))
    if missing:
        fail(f"required negative mutations missing: {missing}")


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
    exact_int(inventory["schema_version"], 2, "inventory schema_version")
    require(inventory["stage"], "5F-c-R1-review-closure", "inventory stage")
    require(
        inventory["status"],
        "review_required_before_5f_d",
        "inventory status",
    )
    require(
        inventory["lineage"],
        {
            "accepted_b1_source_ref": B1_COMMIT,
            "accepted_b3f_source_ref": "e14654f7129aa61011931306140a3bfefe2fcfbc",
            "rejected_predecessor_ref": R1_PREDECESSOR,
            "v1_evidence_immutable": True,
        },
        "inventory lineage",
    )
    require(
        inventory["target"],
        {
            "account_id": "ACC_TEST_0001",
            "instrument_symbol": "IMOEXF",
            "profile": "imoexf_primary_riskgate_high180_lb120",
            "timeframe_sec": 600,
            "paper_only": True,
        },
        "inventory target",
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
            "direct_callback_allowed": False,
        },
        "inventory sole route",
    )
    require(
        inventory["minimum_matrix"],
        {
            "required_row_count": 7,
            "candidate_row_count": 7,
            "row_ids": ["F01", "F02", "F04", "F24", "F31", "F32", "F33"],
        },
        "inventory minimum matrix",
    )
    require(
        inventory["evidence"],
        {
            "candidate_results_path": CANDIDATE,
            "candidate_results_array_sha256": RESULTS_ARRAY_SHA256,
            "schema_owner_inventory_path": SCHEMA_OWNER_INVENTORY,
            "scenario_catalog_path": SCENARIOS,
            "state_catalog_path": STATES,
            "riskgate_catalog_path": RISKGATE,
            "target_config_path": TARGET_CONFIG,
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
            "state_seed_count": 14,
            "representative_full_chain_count": 4,
            "candidate_row_count": 7,
            "stage5f_rust_test_count": 17,
            "negative_case_count": 51,
        },
        "inventory test boundary",
    )
    require(
        inventory["review_decisions"],
        [
            "exact_cfg_test_seam_manifest_enforced",
            "canonical_v2_replaces_rejected_overlay",
            "representative_actual_stage4_stage5d_chain_required_and_present",
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
        "seven-row characterization remains non-golden",
        "The v2 path has no correction overlay.",
        "Stage 5F-d remains closed until independent R1 acceptance.",
    ):
        if statement not in report:
            fail(f"Stage 5F-c report decision missing: {statement}")


def marker_region(text: str, begin: str, end: str, label: str) -> str:
    if text.count(begin) != 1 or text.count(end) != 1:
        fail(f"{label} marker cardinality drift")
    start = text.index(begin)
    finish = text.index(end, start) + len(end)
    return text[start:finish]


def require_exact_region(region: str, label: str) -> None:
    require(
        sha256_bytes(region.encode()),
        EXPECTED_TEST_REGION_SHA256[label],
        f"{label} exact structural digest",
    )


def strip_region(text: str, begin: str, end: str) -> str:
    marker_start = text.index("// " + begin)
    start = text.rfind("\n", 0, marker_start) + 1
    finish = text.index("// " + end, start) + len("// " + end)
    if finish < len(text) and text[finish] == "\n":
        finish += 1
    if text[:start].endswith("\n\n") and text[finish:].startswith("\n"):
        finish += 1
    return text[:start] + text[finish:]


def replace_region(text: str, begin: str, end: str, replacement: str) -> str:
    marker_start = text.index("// " + begin)
    start = text.rfind("\n", 0, marker_start) + 1
    finish = text.index("// " + end, start) + len("// " + end)
    if finish < len(text) and text[finish] == "\n":
        finish += 1
    return text[:start] + replacement + text[finish:]


def normalized_source_sha256(root: Path, label: str) -> str:
    relative, begin, end = EXPECTED_REGION_PATH_MARKERS[label]
    text = (root / relative).read_text()
    if label == "observer call":
        normalized = replace_region(
            text,
            begin,
            end,
            "        Ok(Strategy::on_bar(self, &context, &bar))\n",
        )
    else:
        normalized = strip_region(text, begin, end)
        if relative == STAGE5E:
            other_label = (
                "B3C factory"
                if label == "callback-validation seam"
                else "callback-validation seam"
            )
            _, other_begin, other_end = EXPECTED_REGION_PATH_MARKERS[other_label]
            normalized = strip_region(normalized, other_begin, other_end)
    return sha256_bytes(normalized.encode())


def validate_test_seam_manifest(root: Path) -> None:
    require(
        sha256_file(root, TEST_SEAM_MANIFEST),
        TEST_SEAM_MANIFEST_SHA256,
        "test-seam manifest hash",
    )
    mode = (root / TEST_SEAM_MANIFEST).stat().st_mode & 0o777
    require(mode, 0o644, "test-seam manifest filesystem mode")
    payload = read_json(root, TEST_SEAM_MANIFEST)
    exact_keys(
        payload,
        {
            "closed_surfaces",
            "development_predecessor",
            "normalized_source_base",
            "regions",
            "schema_version",
            "stage",
            "status",
        },
        "test-seam manifest",
    )
    exact_int(payload["schema_version"], 1, "test-seam schema_version")
    require(payload["stage"], "5F-c-R1-test-seam-boundary", "test-seam stage")
    require(payload["status"], "review_closure_candidate", "test-seam status")
    require(
        payload["development_predecessor"],
        "11826285d05638b6b0e29c64a3435870091dac38",
        "test-seam development predecessor",
    )
    require(payload["normalized_source_base"], B1_COMMIT, "normalized source base")
    require(
        payload["closed_surfaces"],
        {
            "redis": False,
            "finam": False,
            "transport": False,
            "dispatch": False,
            "runtime_live": False,
            "broker_execution": False,
        },
        "test-seam closed surfaces",
    )

    regions = payload["regions"]
    labels = list(EXPECTED_REGION_PATH_MARKERS)
    if not isinstance(regions, list):
        fail("test-seam regions must be an array")
    require([region.get("label") for region in regions], labels, "test-seam region order")
    for region in regions:
        label = region["label"]
        exact_keys(
            region,
            {
                "begin",
                "end",
                "git_mode",
                "label",
                "normalized_source_sha256",
                "path",
                "region_sha256",
            },
            f"test-seam region {label}",
        )
        relative, begin, end = EXPECTED_REGION_PATH_MARKERS[label]
        require(region["path"], relative, f"{label} path")
        require(region["git_mode"], "100644", f"{label} git mode")
        require(region["begin"], begin, f"{label} begin marker")
        require(region["end"], end, f"{label} end marker")
        require(
            region["region_sha256"],
            EXPECTED_TEST_REGION_SHA256[label],
            f"{label} manifest region hash",
        )
        require(
            region["normalized_source_sha256"],
            EXPECTED_NORMALIZED_SOURCE_SHA256[label],
            f"{label} manifest normalized hash",
        )
        source_mode = (root / relative).stat().st_mode & 0o777
        require(source_mode, 0o644, f"{label} source filesystem mode")
        source = (root / relative).read_text()
        require(
            sha256_bytes(marker_region(source, begin, end, label).encode()),
            region["region_sha256"],
            f"{label} source-to-manifest region binding",
        )
        require(
            normalized_source_sha256(root, label),
            region["normalized_source_sha256"],
            f"{label} normalized source binding",
        )


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
    stage5d = (root / STAGE5D).read_text()
    stage5e = (root / STAGE5E).read_text()
    harness = (root / HARNESS).read_text()

    lib_region = marker_region(lib, "STAGE5F-TEST-OBSERVATION-MODULE-BEGIN", "STAGE5F-TEST-OBSERVATION-MODULE-END", "observer module")
    require_exact_region(lib_region, "observer module")
    callback_region = marker_region(callback, "STAGE5F-TEST-OBSERVATION-CALL-BEGIN", "STAGE5F-TEST-OBSERVATION-CALL-END", "observer call")
    require_exact_region(callback_region, "observer call")

    stage5c_region = marker_region(stage5c, "STAGE5F-TEST-OWNERSHIP-FACTORY-BEGIN", "STAGE5F-TEST-OWNERSHIP-FACTORY-END", "Stage 5C ownership factory")
    require_exact_region(stage5c_region, "Stage 5C ownership factory")
    stage5d_region = marker_region(stage5d, "STAGE5F-TEST-FULL-RESTART-ORACLE-BEGIN", "STAGE5F-TEST-FULL-RESTART-ORACLE-END", "Stage 5D full restart oracle")
    require_exact_region(stage5d_region, "Stage 5D full restart oracle")
    b3c_region = marker_region(stage5e, "STAGE5F-TEST-B3C-FACTORY-BEGIN", "STAGE5F-TEST-B3C-FACTORY-END", "B3C factory")
    callback_error_region = marker_region(stage5e, "STAGE5F-TEST-CALLBACK-VALIDATION-SEAM-BEGIN", "STAGE5F-TEST-CALLBACK-VALIDATION-SEAM-END", "callback-validation seam")
    require_exact_region(b3c_region, "B3C factory")
    require_exact_region(callback_error_region, "callback-validation seam")

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
        "stage5f_v2_all_state_seeds_roundtrip_exact",
        "stage5f_v2_full_restart_flat_equivalence",
        "stage5f_v2_full_restart_nonflat_owner_cycle_equivalence",
        "stage5f_v2_full_restart_pending_equivalence",
        "stage5f_v2_full_restart_missing_riskgate_is_typed_blocker",
        "stage5f_v2_candidate_repeat_is_byte_identical",
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
    stage5d_stripped = strip_region(stage5d, "STAGE5F-TEST-FULL-RESTART-ORACLE-BEGIN", "STAGE5F-TEST-FULL-RESTART-ORACLE-END")
    stage5e_stripped = strip_region(stage5e, "STAGE5F-TEST-CALLBACK-VALIDATION-SEAM-BEGIN", "STAGE5F-TEST-CALLBACK-VALIDATION-SEAM-END")
    stage5e_stripped = strip_region(stage5e_stripped, "STAGE5F-TEST-B3C-FACTORY-BEGIN", "STAGE5F-TEST-B3C-FACTORY-END")
    if "stage5f_atomic_hybrid_semantics" in lib_stripped or "observe_exact_on_bar_result" in callback_stripped:
        fail("observer dependency escaped its cfg(test) marker region")
    if "stage5f_test_seams" in stage5c_stripped or "sequence_inputs_from_owned_strategy" in stage5c_stripped:
        fail("Stage 5F ownership factory escaped its marker region")
    if "stage5f_test_seams" in stage5d_stripped or "run_full_restart_oracle" in stage5d_stripped:
        fail("Stage 5F full-restart oracle escaped its marker region")
    if "stage5f_test_seams" in stage5e_stripped or "b3c_from_sequence_inputs" in stage5e_stripped:
        fail("Stage 5F B3C seam escaped its marker region")

    if check_lineage:
        require(
            lib_stripped,
            baseline_text(root, LIB),
            "lib source outside Stage 5F region",
        )
        callback_normalized = replace_region(
            callback,
            "STAGE5F-TEST-OBSERVATION-CALL-BEGIN",
            "STAGE5F-TEST-OBSERVATION-CALL-END",
            "        Ok(Strategy::on_bar(self, &context, &bar))\n",
        )
        require(callback_normalized, baseline_text(root, CALLBACK), "callback source outside Stage 5F region")
        require(
            stage5c_stripped,
            baseline_text(root, STAGE5C),
            "Stage 5C source outside test-only factory",
        )
        require(
            stage5d_stripped,
            baseline_text(root, STAGE5D),
            "Stage 5D source outside test-only full-restart oracle",
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
    for relative, expected in V1_INPUT_HASHES.items():
        require(sha256_file(root, relative), expected, f"immutable v1 hash {relative}")
    if check_lineage:
        validate_lineage(root)
    validate_v2_fixtures(root)
    validate_candidate(root)
    validate_negative_harness_inventory(root)
    validate_delivery_contract(root)
    validate_test_seam_manifest(root)
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
