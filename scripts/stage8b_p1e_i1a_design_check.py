#!/usr/bin/env python3
"""Fail-closed static gate for the Stage 8B-P1-e I1A schedule-source design."""

from __future__ import annotations

import csv
import hashlib
import json
from collections import Counter
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "docs/stage-8"
POLICY = BASE / "stage8b-p1e-i1a-schedule-source-policy-v1.json"
SCHEMA = BASE / "stage8b-p1e-i1a-schedule-envelope-v1.schema.json"
DESIGN_JSON = BASE / "stage8b-p1e-i1a-schedule-source-design-v1.json"
SCOPE = BASE / "stage8b-p1e-i1a-implementation-scope-v1.json"
DESIGN_MD = BASE / "stage8b-p1e-i1a-schedule-source-design-v1.md"
MATRIX = BASE / "stage8b-p1e-i1a-schedule-source-acceptance-matrix-v1.csv"
TRUST = BASE / "stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json"

EXPECTED_RAW_SHA256 = {
    POLICY: "1f4c2563b398f0e9f2d8dfc3c9e1236ee37dafe55c267f78c0858c61a1354437",
    SCHEMA: "582cd322136683c8dc9042da218fa92bcb8ed6a5100aee3de61e7f9974fb0d56",
    DESIGN_JSON: "42a32e0d74703d9faffa26bfe41c4407201df7e2e691825bf3a068ed4abcb0ae",
    SCOPE: "20b2190c57ca334198401fb1ec65a79cddab778a7a11bbec517ddd63c3246eaa",
    DESIGN_MD: "ef6d5385adebedc49d4adc35e73158af0b0012352a14ae9050c467027f44c049",
    MATRIX: "9faaf7ce8572b968ea9a9dd897546c3621ebb53f29b65f3bc7d47f93931e7b5c",
    TRUST: "dfe61ddb944df042cdf9514f56c14131e4a45bc732435ff89658ceaceb92d4ee",
}

EXPECTED_MATRIX_COUNTS = {
    "positive": 8,
    "authentication": 8,
    "freshness": 8,
    "sequence": 6,
    "identity": 7,
    "normalized": 6,
    "stage4": 7,
    "binding": 6,
    "replay": 8,
    "timer": 6,
    "compile": 5,
    "closed": 6,
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage8b-p1e-i1a-design-check: FAIL: {message}")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate JSON key: {key}")
        value[key] = item
    return value


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(), object_pairs_hook=reject_duplicate_keys)
    except (UnicodeDecodeError, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(
            f"stage8b-p1e-i1a-design-check: FAIL: invalid JSON {path}: {error}"
        ) from error
    require(isinstance(value, dict), f"JSON root is not an object: {path}")
    return value


def exact_false_map(value: object, expected_keys: set[str], name: str) -> None:
    require(isinstance(value, dict), f"{name} is not an object")
    require(set(value) == expected_keys, f"{name} key inventory drift")
    require(all(item is False for item in value.values()), f"{name} opened a surface")


def main() -> None:
    for path, expected in EXPECTED_RAW_SHA256.items():
        require(path.is_file(), f"missing pinned artifact: {path.relative_to(ROOT)}")
        require(sha256(path) == expected, f"raw SHA-256 drift: {path.relative_to(ROOT)}")

    policy = load_json(POLICY)
    schema = load_json(SCHEMA)
    design = load_json(DESIGN_JSON)
    scope = load_json(SCOPE)
    trust = load_json(TRUST)

    require(policy["schema_version"] == 1, "policy schema version drift")
    require(
        policy["domain"] == "moex.stage8b.p1e.i1a.schedule-source-policy.v1",
        "policy domain drift",
    )
    require(
        policy["architecture"]
        == "pre-provisioned-signed-broker-neutral-redis-stream",
        "selected architecture drift",
    )
    producer = policy["producer"]
    require(
        producer["owner"] == "finam-gateway::Stage8bP1eReadonlySchedulePublisherV1",
        "producer owner is not exact",
    )
    require(
        producer["required_input"]
        == "FINAM GET-only AssetScheduleResponse plus accepted broker-neutral Stage4 report",
        "producer input lineage drift",
    )
    require(producer["manual_or_supervisor_publication_allowed"] is False, "supervisor publisher opened")
    require(producer["finam_post_delete_allowed"] is False, "FINAM write opened")
    require(producer["broker_dispatch_allowed"] is False, "broker dispatch opened")

    policy_trust = policy["trust"]
    require(policy_trust["algorithm"] == "ed25519-sha256-prehash-v1", "signature algorithm drift")
    require(policy_trust["source_name"] == "schedule", "trust source drift")
    require(policy_trust["key_id"] == "schedule-ed25519-v1", "schedule key id drift")
    require(policy_trust["key_generation"] == 2, "schedule generation drift")
    require(policy_trust["redis_is_trust_anchor"] is False, "Redis became trust anchor")
    require(
        policy_trust["generation_2_execution_activated_by_this_policy"] is False,
        "Generation-2 execution activated",
    )
    require(
        policy_trust["operational_private_key_installation_authorized"] is False,
        "private-key installation authorized",
    )
    schedule_key = trust["source_keys"]["schedule"]
    for key in (
        "key_id",
        "generation",
        "public_key_ed25519_hex",
        "public_key_sha256",
        "valid_from_utc",
        "valid_until_utc",
    ):
        expected_key = {
            "generation": "key_generation",
            "valid_from_utc": "key_valid_from_utc",
            "valid_until_utc": "key_valid_until_utc",
        }.get(key, key)
        require(schedule_key[key] == policy_trust[expected_key], f"trust manifest mismatch: {key}")
    require(trust["rotation_requires_new_reviewed_package"] is True, "trust rotation review disabled")

    transport = policy["transport"]
    require(transport["redis_url"] == "redis://127.0.0.1:6379/15", "Redis DB15 URL drift")
    require(
        transport["stream_key"]
        == "finam_imoexf_paper:{finam-imoexf-p1}:market-schedule",
        "schedule stream key drift",
    )
    require(transport["key_type"] == "stream", "schedule key type drift")
    require(transport["consumer_group"] is None, "schedule consumer group opened")
    require(transport["entry_fields"] == ["payload"], "wire field inventory drift")
    require(transport["lookup_count"] == 64, "lookup bound drift")
    require(transport["retention_exact_maxlen"] == 4096, "retention bound drift")
    require(transport["supervisor_may_create_repair_trim_xadd_xdel_xack"] is False, "supervisor writes opened")
    require(transport["publisher_may_create_stream"] is False, "implicit stream creation opened")
    require(
        transport["invalid_or_untrusted_newest_entry"] == "fail-closed-no-older-fallback",
        "older-row fallback opened",
    )

    canonical = policy["canonical_encoding"]
    require(canonical["duplicate_keys_rejected_recursively"] is True, "duplicate keys accepted")
    require(canonical["unknown_fields_rejected_recursively"] is True, "unknown fields accepted")
    require(canonical["floating_point_numbers_allowed"] is False, "floats accepted")
    require(canonical["canonical_roundtrip_must_equal_input_bytes"] is True, "canonical byte check disabled")
    freshness = policy["freshness"]
    require(freshness["trusted_clock_max_future_skew_ms"] == 250, "future skew drift")
    require(freshness["transport_envelope_max_age_ms"] == 5_000, "envelope TTL drift")
    require(freshness["normalized_schedule_max_age_ms"] == 86_400_000, "normalized TTL drift")
    require(freshness["stage4_schedule_max_age_ms"] == 5_000, "Stage4 TTL drift")
    require(freshness["cross_source_max_skew_ms"] == 5_000, "cross-source skew drift")
    require(freshness["reserialization_may_refresh_observation"] is False, "freshness refresh opened")
    require(freshness["same_generation_sequence_same_bytes"] == "idempotent", "idempotency drift")
    require(
        freshness["same_generation_sequence_different_bytes"] == "terminal-conflict",
        "same-sequence conflict weakened",
    )
    require(freshness["sequence_rollback"] == "blocked", "sequence rollback opened")

    normalized = policy["normalized_schedule"]
    require(normalized["known_session_types"] == ["tradable_open", "break_or_clearing", "maintenance"], "session inventory drift")
    require(normalized["unknown_session_type_allowed"] is False, "unknown session opened")
    require(normalized["empty_sessions_allowed"] is False, "empty schedule opened")
    require(normalized["tradable_open_required"] is True, "tradable-open requirement removed")
    require(normalized["overlap_or_ambiguous_endpoint_allowed"] is False, "overlap opened")
    require(normalized["approved_gap_requires_explicit_nontradable_grid_coverage"] is True, "gap inference opened")

    stage4 = policy["stage4_evidence"]
    require(stage4["report_status"] == "Accepted", "Stage4 accepted state drift")
    require(stage4["schedule_state"] == "Open", "Stage4 Open state drift")
    require(stage4["schedule_source_status"] == "Present", "Stage4 source presence drift")
    require(stage4["schedule_freshness_status"] == "Fresh", "Stage4 freshness drift")
    require(stage4["schedule_required_for_bootstrap"] is True, "Stage4 schedule requirement removed")
    require(stage4["safety_boundary"] == "closed", "Stage4 safety boundary opened")
    require(stage4["no_live_authorization"] is True, "Stage4 live authorization opened")
    require(stage4["manual_intervention_required"] is False, "manual Stage4 evidence accepted")
    require(stage4["reason_chain"] == "empty", "nonempty Stage4 reasons accepted")
    require(stage4["required_source_expiry"] == "recomputed-from-all-required-report-sections", "Stage4 expiry recomputation drift")
    require(stage4["independent_from_normalized_payload_hash"] is True, "independent evidence collapsed")

    acquisition = policy["acquisition"]
    require(acquisition["redis_operation_timeout_ms"] == 2_000, "operation timeout drift")
    require(acquisition["attempts"] == 12, "attempt budget drift")
    require(acquisition["total_deadline_ms"] == 60_000, "total deadline drift")
    require(acquisition["deadline_exhausted"].startswith("exit-67"), "deadline exit drift")
    require(acquisition["signature_identity_schema_or_conflict_failure"].startswith("exit-66"), "validation exit drift")

    replay = policy["durable_replay"]
    require(replay["record"] == "Stage8bP1eScheduleEvidenceBoundV1", "durable record drift")
    require("exact_envelope_bytes" in replay["retained_fields"], "exact envelope not retained")
    require(replay["historical_reissue"] == "allowed-only-for-the-identical-incomplete-durable-transition", "historical replay widened")
    require(replay["new_transition_from_expired_or_historical_binding"] is False, "historical binding opened for new work")
    require(replay["replace_bound_snapshot_with_newest_during_incomplete_transition"] is False, "bound snapshot substitution opened")
    require(replay["second_strategy_callback_or_paper_provider_call_on_reissue"] is False, "duplicate callback opened")

    issuers = policy["authority_issuers"]
    require(set(issuers) == {
        "market",
        "working_limit_or_cancel_step",
        "day_expiry",
        "raw_accepted_flags_returned",
        "authority_clone_copy_serde_raw_parts_allowed",
        "equivalent_restart_reissue_for_same_durable_binding",
        "double_use_for_effects",
    }, "issuer inventory drift")
    require(issuers["raw_accepted_flags_returned"] is False, "raw accepted flags opened")
    require(issuers["authority_clone_copy_serde_raw_parts_allowed"] is False, "authority bypass opened")
    require(issuers["double_use_for_effects"] is False, "double-use opened")
    timer = policy["timer_precedence"]
    require(timer["policy"] == "SOURCE_FIRST_TIMER_DEFERRED", "timer policy drift")
    require(timer["source_scan_before_timer"] is True, "timer runs before source")
    require(timer["timer_reclassified_against_returned_owner"] is True, "timer reclassification removed")
    require(timer["shutdown_latch_checkpoint_before_schedule_acquisition"] is True, "first latch checkpoint removed")
    require(timer["shutdown_latch_checkpoint_after_schedule_acquisition"] is True, "second latch checkpoint removed")
    require(timer["shutdown_latch_checkpoint_before_timer_execution"] is True, "final latch checkpoint removed")
    require(timer["missing_or_expired_schedule_may_invent_day_expiry"] is False, "invented expiry opened")

    exact_false_map(
        policy["closed_surfaces"],
        {
            "redis_db0_vps_activation",
            "schedule_private_key_installation",
            "finam_post_delete",
            "broker_dispatch",
            "runtime_live",
            "real_orders",
            "generation_2_execution_activation",
        },
        "policy closed surfaces",
    )

    require(schema["$schema"] == "https://json-schema.org/draft/2020-12/schema", "JSON schema dialect drift")
    require(schema["additionalProperties"] is False, "envelope unknown fields opened")
    envelope_required = set(schema["required"])
    require({"source_generation", "source_sequence", "payload_sha256", "signature_ed25519_hex"} <= envelope_required, "envelope binding fields missing")
    properties = schema["properties"]
    require(properties["domain"]["const"] == "moex.stage8b.p1e.schedule-envelope.v1", "envelope domain drift")
    require(properties["key_id"]["const"] == "schedule-ed25519-v1", "schema key id drift")
    require(properties["key_generation"]["const"] == 2, "schema key generation drift")
    payload = schema["$defs"]["payload"]
    require(payload["additionalProperties"] is False, "payload unknown fields opened")
    stage4_schema = schema["$defs"]["stage4Evidence"]
    require(stage4_schema["additionalProperties"] is False, "Stage4 unknown fields opened")
    require(
        {"source_observed_at_utc", "source_expires_at_utc", "report_canonical_json_hex", "report_sha256"}
        <= set(stage4_schema["required"]),
        "Stage4 independent freshness fields missing",
    )
    require(schema["$defs"]["session"]["properties"]["session_type"]["enum"] == normalized["known_session_types"], "schema session inventory mismatch")

    require(design["stage"] == "Stage 8B-P1-e I1A", "design stage drift")
    require(design["status"] == "DESIGN_REVIEW_CANDIDATE_IMPLEMENTATION_NOT_AUTHORIZED", "design authorization drift")
    require(design["foundation_source_candidate"] == "a0b07f6ef16ac8e43004204f1e52184bc615fa97", "foundation source binding drift")
    require(design["selected_architecture"] == "separate-pre-provisioned-authenticated-broker-neutral-schedule-stream-db15", "design architecture drift")
    require(design["source_ownership"]["producer_is_supervisor"] is False, "supervisor source ownership opened")
    require(design["source_ownership"]["producer_is_strategy_runtime"] is False, "runtime source ownership opened")
    require(design["trust_decision"]["redis_transport_is_untrusted"] is True, "Redis trust decision drift")
    require(design["trust_decision"]["selection_does_not_activate_generation_2_execution"] is True, "G2 execution activated by design")
    facade = design["stage5e_facade"]
    require(facade["default_feature_required"] is True, "default-feature facade removed")
    require(facade["artifact_fixture_feature_allowed"] is False, "artifact fixture facade opened")
    require(facade["returns_raw_schedule_rows_or_accepted_flags"] is False, "raw facade output opened")
    require(facade["bypasses_existing_stage5e_classifier"] is False, "Stage5E bypass opened")
    require(len(design["authority_operations"]) == 3, "three authority operations not exact")
    require({item["kind"] for item in design["authority_operations"]} == {"market_execution", "working_limit_or_cancel_step", "day_expiry"}, "authority kind inventory drift")
    durable = design["durable_transition_rule"]
    require(durable["must_exist_before_first_authority_issuance"] is True, "issuance before durability opened")
    require(durable["exact_signed_envelope_bytes_retained"] is True, "exact signed bytes not retained")
    require(durable["newest_snapshot_substitution_for_incomplete_transition_allowed"] is False, "restart source substitution opened")
    require(durable["historical_binding_reuse_for_new_transition_allowed"] is False, "historical binding reuse widened")
    require(durable["source_m10_xack_effect"] == "none", "schedule source gained M10 XACK authority")
    missing = design["missing_source_behavior"]
    require(missing["retain_linear_owner"] is True, "missing source drops owner")
    require(missing["retain_exact_m10_pel"] is True, "missing source drops PEL")
    require(missing["default_session_eligibility"] is False, "default eligibility opened")
    require(missing["m10_xack"] is False, "missing source XACK opened")
    require(missing["second_fresh_m10_read"] is False, "second M10 read opened")
    implementation_authorization = design["implementation_authorization"]
    implementation_false_keys = {
        "production_facade",
        "schedule_publisher",
        "manifest_config_amendment",
        "durable_schedule_binding_record",
        "owner_loop",
    }
    require(
        set(implementation_authorization)
        == implementation_false_keys | {"requires_independent_i1a_design_acceptance"},
        "implementation authorization key inventory drift",
    )
    require(
        all(implementation_authorization[key] is False for key in implementation_false_keys),
        "production implementation authorized before design acceptance",
    )
    require(implementation_authorization["requires_independent_i1a_design_acceptance"] is True, "independent design acceptance removed")
    exact_false_map(
        design["closed_surfaces"],
        {"redis_db0_vps_activation", "redis_db15_operational_schedule_activation", "finam_post_delete", "broker_dispatch", "runtime_live", "real_orders"},
        "design closed surfaces",
    )

    require(scope["status"] == "PROSPECTIVE_ALLOWLIST_REQUIRES_DESIGN_ACCEPTANCE", "implementation scope activated")
    require(len(scope["new_rust_paths"]) == 2, "new Rust path allowlist drift")
    require("public-or-raw authority constructor" in scope["forbidden_implementation_shortcuts"], "constructor bypass not forbidden")
    require("Redis checksum treated as publisher authentication" in scope["forbidden_implementation_shortcuts"], "Redis checksum trust not forbidden")
    require("FINAM POST or DELETE" in scope["forbidden_implementation_shortcuts"], "FINAM writes not forbidden")

    for relative in scope["new_rust_paths"]:
        require(not (ROOT / relative).exists(), f"production implementation exists before design acceptance: {relative}")
    for relative in (
        "docs/stage-8/stage8b-p1e-redis-deployment-manifest-v2.json",
        "docs/stage-8/stage8b-p1e-redis-runtime-policy-v3.json",
        "docs/stage-8/stage8b-p1e-supervisor-config-schema-v2.json",
    ):
        require(not (ROOT / relative).exists(), f"future manifest/config exists before design acceptance: {relative}")

    markdown = DESIGN_MD.read_text()
    for fragment in (
        "DESIGN REVIEW CANDIDATE — IMPLEMENTATION NOT AUTHORIZED",
        "Redis DB15 is transport, not a trust anchor",
        "Generation-2 execution",
        "SOURCE_FIRST_TIMER_DEFERRED",
        "Stage8bP1eScheduleEvidenceBoundV1",
        "untrusted newest data blocks the source",
        "Historical evidence cannot authorize a new transition",
        "FINAM POST/DELETE and broker dispatch",
    ):
        require(fragment in markdown, f"design narrative missing: {fragment}")

    with MATRIX.open(newline="") as handle:
        reader = csv.DictReader(handle)
        require(
            reader.fieldnames
            == ["id", "area", "case", "precondition", "expected_owner", "expected_effects", "expected_readiness", "status"],
            "acceptance matrix columns drift",
        )
        rows = list(reader)
    require(len(rows) == 81, "acceptance matrix row count drift")
    require(len({row["id"] for row in rows}) == len(rows), "duplicate acceptance matrix id")
    require(all(row["status"] == "REQUIRED" for row in rows), "non-required acceptance row")
    require(Counter(row["area"] for row in rows) == Counter(EXPECTED_MATRIX_COUNTS), "acceptance area coverage drift")
    required_ids = {
        "I1A-P01", "I1A-P02", "I1A-P03", "I1A-P04", "I1A-P05", "I1A-P06",
        "I1A-A02", "I1A-F05", "I1A-F06", "I1A-S01", "I1A-S02", "I1A-S06",
        "I1A-I01", "I1A-I04", "I1A-N01", "I1A-N02", "I1A-E07",
        "I1A-B01", "I1A-B02", "I1A-B05", "I1A-B06",
        "I1A-R01", "I1A-R02", "I1A-R03", "I1A-R04", "I1A-R05", "I1A-R06",
        "I1A-T01", "I1A-T03", "I1A-T04", "I1A-T05", "I1A-T06",
        "I1A-C01", "I1A-C02", "I1A-C03", "I1A-X04", "I1A-X06",
    }
    require(required_ids <= {row["id"] for row in rows}, "mandatory acceptance cases missing")

    print(
        "stage8b-p1e-i1a-design-check: ok "
        f"artifacts={len(EXPECTED_RAW_SHA256)} matrix={len(rows)} "
        f"trust={policy_trust['public_key_sha256']}"
    )


if __name__ == "__main__":
    main()
