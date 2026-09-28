#!/usr/bin/env python3
"""Fail-closed checker for the I1A R2 semantic identity correction."""

from __future__ import annotations

import csv
import hashlib
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "docs/stage-8"


def p(name: str) -> Path:
    return BASE / name


IDENTITY_SCHEMA = p("stage8b-p1e-i1a-semantic-identity-v1.schema.json")
IDENTITY_CONTRACT = p("stage8b-p1e-i1a-semantic-identity-contract-v1.json")
ENVELOPE = p("stage8b-p1e-i1a-schedule-envelope-v3.schema.json")
POLICY = p("stage8b-p1e-i1a-schedule-source-policy-v3.json")
PROGRESSION = p("stage8b-p1e-i1a-source-progression-v2.json")
MATRIX = p("stage8b-p1e-i1a-r2-acceptance-matrix-v1.csv")
FIXTURES = p("stage8b-p1e-i1a-r2-semantic-fixtures-v1.json")
DESIGN = p("stage8b-p1e-i1a-schedule-source-design-v3.json")
MARKDOWN = p("stage8b-p1e-i1a-schedule-source-design-v3.md")
BASE_POLICY = p("stage8b-p1e-i1a-schedule-source-policy-v2.json")
BASE_ENVELOPE = p("stage8b-p1e-i1a-schedule-envelope-v2.schema.json")
BASE_PROGRESSION = p("stage8b-p1e-i1a-source-progression-v1.json")
BASE_DESIGN = p("stage8b-p1e-i1a-schedule-source-design-v2.json")
BASE_MARKDOWN = p("stage8b-p1e-i1a-schedule-source-design-v2.md")
BASE_FIXTURES = p("stage8b-p1e-i1a-r1-model-fixtures-v1.json")
SCOPE = p("stage8b-p1e-i1a-implementation-scope-v2.json")

EXPECTED_RAW_SHA256 = {
    IDENTITY_SCHEMA: "3a32e76e09fa3ba304e7e057de25b2e8de8af960d48c845974f1e11f916284a9",
    IDENTITY_CONTRACT: "6c27af443115efc2bec9da51fe51a968ff481d377594f23937291aadf8c8e55d",
    ENVELOPE: "ef96e28bf09aaf8ad5d12cd5734f16bd927992d66aeb90cf6785bf1590fa6f40",
    POLICY: "1f49d2950fb30737317b818cd0c384c21942f0ebf626a3ae894842e327103b23",
    PROGRESSION: "7e5166db7627321a3279028a525c52f016485f89b302ae1a9abda728222f3364",
    MATRIX: "6d4d20c08246f6ea25e53a31d67bc0eb405b681f63ed36005b7e078f72c68c30",
    FIXTURES: "5e7757e1f5288470f51a078c3e0d8cca1e1dae859f0779d899f2bf529ebdd6a9",
    DESIGN: "5ab73b16aac002ddf63417fadcd61cc7774b17e6b672704cd78593ab4ed4c76a",
    MARKDOWN: "c7dedb3448237c092910fb1aae2c2534e4c3dbd89563ea30e30adb66fadbe73b",
    BASE_POLICY: "9571d40f0968b7bc458afe01f21cde18074849727c21bd261690ccdef9507754",
    BASE_ENVELOPE: "0f8cd1ad6203308d4f5a24cb59ed0163d4c54b62e89e0191fc2cd295784c026c",
    BASE_PROGRESSION: "db1127314b3ca8d5ca54def78c05050e3cf297be19ec07080693bb2e06648a89",
    BASE_DESIGN: "e779cf166de82907442bd20ef88742ca07a9a98d927abd6cc670934bfef97b44",
    BASE_MARKDOWN: "54257bc578f8865643650f5db9acdb4e3dc0a489c7630f57e74502b3fb98e40b",
    BASE_FIXTURES: "614eea3f489f16bf597475cde3df389c47bf2084f20f61da5aa4fb71e9bca632",
    SCOPE: "ecc15fc47d8795085f8efb6e0bee4c68119eef27f3769f85c1f86ae0f3c8c27b",
}

EXPECTED_INCLUDED_PROJECTION = {
    "schema_version",
    "domain",
    "instrument.symbol",
    "instrument.broker_symbol",
    "instrument.exchange",
    "instrument.market",
    "instrument.venue_mic",
    "instrument.board",
    "instrument.tick_size",
    "registry.registry_version",
    "registry.registry_identity_sha256",
    "trading_day",
    "timezone",
    "timeframe_sec",
    "sessions[].session_type",
    "sessions[].start_utc",
    "sessions[].end_utc",
    "stage4_semantic_state.evidence_kind",
    "stage4_semantic_state.schedule_state",
    "stage4_semantic_state.boundary_proof",
}

EXPECTED_EXCLUDED_FIELDS = {
    "source_generation",
    "publication_sequence",
    "published_at_utc",
    "source_observed_at_utc",
    "source_expires_at_utc",
    "raw_response_sha256",
    "normalized_payload_sha256",
    "report_canonical_json_hex",
    "report_sha256",
    "payload_sha256",
    "signature_ed25519_hex",
    "envelope_sha256",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage8b-p1e-i1a-r2-design-check: FAIL: {message}")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate key: {key}")
        result[key] = value
    return result


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(), object_pairs_hook=reject_duplicate_keys)
    except (OSError, UnicodeDecodeError, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p1e-i1a-r2-design-check: FAIL: invalid JSON {path}: {error}") from error
    require(isinstance(value, dict), f"JSON root is not object: {path.relative_to(ROOT)}")
    return value


def main() -> None:
    for path, expected in EXPECTED_RAW_SHA256.items():
        require(path.is_file(), f"missing pinned artifact: {path.relative_to(ROOT)}")
        require(sha256(path) == expected, f"raw SHA-256 drift: {path.relative_to(ROOT)}")

    identity_schema = load_json(IDENTITY_SCHEMA)
    contract = load_json(IDENTITY_CONTRACT)
    envelope = load_json(ENVELOPE)
    policy = load_json(POLICY)
    progression = load_json(PROGRESSION)
    fixtures = load_json(FIXTURES)
    design = load_json(DESIGN)

    required_identity = {
        "schema_version", "domain", "instrument", "registry", "trading_day",
        "timezone", "timeframe_sec", "sessions", "stage4_semantic_state",
    }
    require(set(identity_schema["required"]) == required_identity, "semantic identity field inventory drift")
    require(identity_schema["additionalProperties"] is False, "semantic identity accepts unknown fields")
    instrument = identity_schema["properties"]["instrument"]
    require(instrument["additionalProperties"] is False, "instrument accepts unknown fields")
    require(set(instrument["required"]) == {
        "symbol", "broker_symbol", "exchange", "market", "venue_mic", "board", "tick_size",
    }, "instrument identity field inventory drift")
    registry = identity_schema["properties"]["registry"]
    require(registry["additionalProperties"] is False, "registry accepts unknown fields")
    require(set(registry["required"]) == {
        "registry_version", "registry_identity_sha256",
    }, "registry identity field inventory drift")
    session = identity_schema["$defs"]["session"]
    require(session["additionalProperties"] is False, "session accepts unknown fields")
    require(set(session["required"]) == {
        "session_type", "start_utc", "end_utc",
    }, "session identity field inventory drift")
    boundary = identity_schema["$defs"]["boundaryProof"]
    require(boundary["additionalProperties"] is False, "boundary proof accepts unknown fields")
    require(set(boundary["required"]) == {
        "trading_day", "boundary_ts_utc", "last_eligible_m10_open_ts_utc",
        "last_eligible_m10_close_ts_utc",
    }, "boundary proof field inventory drift")
    state_variants = identity_schema["$defs"]["stage4SemanticState"]["oneOf"]
    require(len(state_variants) == 2, "semantic state variant inventory drift")
    require(all(item["additionalProperties"] is False for item in state_variants), "semantic state accepts unknown fields")
    require(all(set(item["required"]) == {
        "evidence_kind", "schedule_state", "boundary_proof",
    } for item in state_variants), "semantic state field inventory drift")
    require({
        (item["properties"]["evidence_kind"]["const"], item["properties"]["schedule_state"]["const"])
        for item in state_variants
    } == {("tradability", "open"), ("day_boundary", "closed")}, "semantic state pair drift")
    hash_contract = identity_schema["x_hash_contract"]
    require(hash_contract["canonical_encoding"] == "strict-canonical-json-subset-v1", "identity canonical encoding drift")
    require(hash_contract["hash_domain"] == "moex.stage8b.p1e.schedule-semantic-identity.sha256.v1", "identity hash domain drift")

    included = set(contract["included_projection"])
    require(included == EXPECTED_INCLUDED_PROJECTION, "semantic included projection inventory drift")
    excluded = set(contract["excluded_transport_and_freshness_fields"])
    require(excluded == EXPECTED_EXCLUDED_FIELDS, "semantic excluded-field inventory drift")
    encoding = contract["canonical_encoding"]
    require(encoding == {
        "format": "strict-canonical-json-subset-v1",
        "utf8_only": True,
        "object_keys": "lexicographic-utf8-byte-order",
        "arrays": "contract-order",
        "duplicate_keys_rejected_recursively": True,
        "unknown_fields_rejected_recursively": True,
        "floating_point_numbers_allowed": False,
        "timestamps": "RFC3339-microseconds-UTC-Z",
        "digests": "lowercase-hex",
    }, "canonical encoding contract drift")
    require(contract["hash"] == {
        "algorithm": "sha256",
        "domain": "moex.stage8b.p1e.schedule-semantic-identity.sha256.v1",
        "preimage": "utf8(hash.domain) || 0x00 || canonical_semantic_identity_bytes",
        "envelope_field": "schedule_semantic_sha256",
    }, "semantic hash contract drift")
    revision = contract["producer_revision"]
    require(revision["same_hash_as_immediately_prior_publication"] == "retain-exact-prior-semantic-revision", "heartbeat revision drift")
    require(revision["different_hash_from_immediately_prior_publication"] == "exact-prior-semantic-revision-plus-one", "semantic change revision drift")
    require(revision["open_to_closed"] == "different-hash-and-revision-plus-one-even-if-sessions-unchanged", "Open Closed rule drift")
    require(revision["trading_day_change"] == "different-hash-and-revision-plus-one-even-if-sessions-unchanged", "trading-day rule drift")
    require(revision["reset_or_guess_after_restart"] is False, "publisher state reset opened")
    consumer = contract["consumer_progression"]
    require(consumer["semantic_revision_equal_and_hash_different"] == "terminal-conflict", "same-revision conflict weakened")
    require(consumer["require_observation_of_intermediate_semantic_revisions"] is False, "event continuity reintroduced")

    require(envelope["properties"]["schema_version"]["const"] == 3, "envelope version drift")
    require(envelope["properties"]["semantic_identity_contract_version"]["const"] == 1, "identity profile version drift")
    require(envelope["properties"]["semantic_identity"]["$ref"] == "stage8b-p1e-i1a-semantic-identity-v1.schema.json", "identity schema reference drift")
    require(envelope["properties"]["producer_id"]["const"] == "finam-readonly-schedule-normalizer-v3", "envelope producer drift")
    require(envelope["properties"]["producer_contract_version"]["const"] == "finam-rest-schedule-to-broker-neutral-v3", "envelope producer contract drift")
    require(set(envelope["required"]) == {
        "schema_version", "domain", "producer_id", "producer_contract_version",
        "semantic_identity_contract_version", "key_id", "key_generation", "source_generation",
        "publication_sequence", "semantic_revision", "schedule_semantic_sha256",
        "semantic_identity", "published_at_utc", "operational_identity_sha256",
        "runtime_config_fingerprint_sha256", "instrument_map_fingerprint_sha256",
        "payload_sha256", "payload", "signature_ed25519_hex",
    }, "envelope required-field inventory drift")
    require(envelope["x_supersedes"]["raw_sha256"] == EXPECTED_RAW_SHA256[BASE_ENVELOPE], "envelope predecessor drift")
    constraints = set(envelope["x_semantic_constraints"])
    require(constraints == {
        "semantic_identity is the exact projection defined by semantic identity contract v1",
        "semantic_identity must cross-validate field-for-field against payload before signature acceptance",
        "schedule_semantic_sha256 equals SHA256 of the domain-separated canonical semantic_identity bytes",
        "semantic_revision follows the same-hash-retain different-hash-plus-one producer rule",
        "transport freshness and signature fields are excluded from semantic identity",
        "Open-to-Closed and trading-day transitions change semantic identity and revision even when sessions are unchanged",
        "heartbeat publication may advance transport sequence and freshness while retaining semantic hash and revision",
    }, "envelope semantic constraint inventory drift")

    require(policy["base_policy"]["raw_sha256"] == EXPECTED_RAW_SHA256[BASE_POLICY], "policy predecessor drift")
    require(policy["semantic_identity"]["projection_must_cross_validate_payload"] is True, "payload projection cross-validation removed")
    require(policy["semantic_identity"]["hash_must_cross_validate_projection"] is True, "projection hash cross-validation removed")
    require(policy["semantic_identity"]["base_v2_normalized_schedule_only_revision_wording_superseded"] is True, "ambiguous v2 wording not superseded")
    require(policy["producer_revision"]["open_to_closed_same_sessions"] == "hash-changes-revision-plus-one", "policy Open Closed rule drift")
    require(policy["producer_revision"]["freshness_only_heartbeat"] == "publication-sequence-advances-hash-and-revision-unchanged", "policy heartbeat drift")
    require(policy["producer_revision"]["restart_without_exact_persistent_state"] == "fail-closed-no-publication", "policy publisher restart weakened")
    require(policy["consumer_progression"]["same_revision_different_hash"] == "terminal-conflict", "policy conflict weakened")
    require(policy["route_link"]["closed_never_authorizes_market_or_working"] is True, "Closed trading grant opened")
    require(policy["production_implementation_authorized"] is False, "production implementation opened")

    require(progression["base_contract"]["raw_sha256"] == EXPECTED_RAW_SHA256[BASE_PROGRESSION], "progression predecessor drift")
    producer_rules = {item["id"]: item["decision"] for item in progression["producer_rules"]}
    consumer_rules = {item["id"]: item["decision"] for item in progression["consumer_rules"]}
    require(producer_rules == {
        "SPR01": "semantic-revision-1",
        "SPR02": "retain-prior-semantic-revision",
        "SPR03": "prior-semantic-revision-plus-one",
        "SPR04": "blocked-no-publication",
    }, "producer progression rules drift")
    require(consumer_rules["SCR03"] == "terminal-semantic-conflict", "consumer conflict drift")
    require(consumer_rules["SCR04"].endswith("without-intermediate-revision-requirement"), "snapshot revision jump drift")
    require(progression["required_transitions"] == {
        "open_to_closed_same_sessions": "different-hash-next-revision-accepted-then-day-route",
        "unchanged_heartbeat": "same-hash-same-revision-positive-publication-sequence-accepted",
        "trading_day_change_same_sessions": "different-hash-next-revision-accepted",
        "forged_same_revision_changed_hash": "terminal-semantic-conflict",
    }, "required semantic transition inventory drift")

    with MATRIX.open(newline="") as handle:
        reader = csv.DictReader(handle)
        rows = list(reader)
    require(reader.fieldnames == ["id", "area", "case", "precondition", "expected_progression", "expected_route", "status"], "R2 matrix header drift")
    require(len(rows) == 8 and len({row["id"] for row in rows}) == 8, "R2 matrix row inventory drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "non-required R2 matrix row")
    require({row["case"] for row in rows} == {
        "open-to-closed-same-sessions", "unchanged-heartbeat", "trading-day-change-same-sessions",
        "same-revision-different-hash", "semantic-revision-rollback", "heartbeat-freshness-excluded",
        "publisher-restart-state-missing", "closed-market-working-denial",
    }, "R2 matrix cases drift")

    require(fixtures["base_model"]["raw_sha256"] == EXPECTED_RAW_SHA256[BASE_FIXTURES], "base fixture binding drift")
    require(len(fixtures["cases"]) == 8, "linked fixture count drift")
    require(set(fixtures["identities"]) == {"open_day_1", "closed_day_1", "open_day_2"}, "fixture identity inventory drift")
    identities = fixtures["identities"]
    require(identities["open_day_1"]["sessions"] == identities["closed_day_1"]["sessions"], "Open Closed fixture sessions differ")
    require(identities["open_day_1"]["trading_day"] == identities["closed_day_1"]["trading_day"], "Open Closed fixture trading day differs")
    require(identities["open_day_1"]["stage4_semantic_state"]["schedule_state"] == "open", "Open fixture is not Open")
    require(identities["closed_day_1"]["stage4_semantic_state"]["schedule_state"] == "closed", "Closed fixture is not Closed")
    require(identities["open_day_1"]["trading_day"] != identities["open_day_2"]["trading_day"], "trading-day fixture does not advance")
    require({case["id"] for case in fixtures["cases"]} == {
        "SI-open-to-closed", "SI-heartbeat", "SI-trading-day-change",
        "SI-forged-same-revision", "SI-revision-rollback", "SI-freshness-excluded",
        "SI-publisher-restart-state-missing", "SI-closed-denies-market",
    }, "linked fixture id inventory drift")

    require(design["status"] == "DESIGN_SEMANTIC_IDENTITY_CORRECTION_REVIEW_CANDIDATE_IMPLEMENTATION_NOT_AUTHORIZED", "design status drift")
    require(design["base_i1a_r1"]["commit"] == "7686c93eb9f38a0124d2ae558a80b78f51c99f8f", "R1 commit binding drift")
    require(design["base_i1a_r1"]["design_raw_sha256"] == EXPECTED_RAW_SHA256[BASE_DESIGN], "R1 design binding drift")
    require(design["coverage_classification"]["r1_table_lookup_cases"] == 14, "R1 table coverage not disclosed")
    require(design["coverage_classification"]["design_model_is_source_execution_evidence"] is False, "design model misrepresented as source evidence")
    require(design["coverage_classification"]["implementation_requires_real_state_transition_counter_crash_and_latch_tests"] is True, "implementation test obligation removed")
    require(all(value is False for key, value in design["implementation_authorization"].items() if key != "requires_independent_i1a_r2_acceptance"), "production implementation authorized")
    require(design["implementation_authorization"]["requires_independent_i1a_r2_acceptance"] is True, "independent R2 acceptance removed")
    require(all(value is False for value in design["closed_surfaces"].values()), "operational surface opened")

    markdown = MARKDOWN.read_text()
    for needle in (
        "semantic-identity.sha256.v1",
        "Open revision 7 followed by Closed revision 8",
        "heartbeat can refresh authenticated evidence",
        "14/41",
        "source review must execute actual state transitions",
    ):
        require(needle in markdown, f"design markdown missing: {needle}")

    scope = load_json(SCOPE)
    for relative in scope["new_rust_paths"]:
        require(not (ROOT / relative).exists(), f"production source appeared before R2 acceptance: {relative}")

    print("stage8b-p1e-i1a-r2-design-check: PASS")
    print("semantic_identity_fields=9 linked_cases=8 effective_cases=127")
    print("production_schedule_source=absent operational_surfaces=closed")


if __name__ == "__main__":
    main()
