#!/usr/bin/env python3
"""Fail-closed static gate for the Stage 8B-P1-e I1A R1 design correction."""

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


POLICY = p("stage8b-p1e-i1a-schedule-source-policy-v2.json")
BINDING_SCHEMA = p("stage8b-p1e-i1a-schedule-binding-record-v1.schema.json")
PROGRESSION = p("stage8b-p1e-i1a-source-progression-v1.json")
DAY_PROOF = p("stage8b-p1e-i1a-day-boundary-proof-v1.json")
TIMER = p("stage8b-p1e-source-timer-precedence-v5.json")
ENVELOPE_SCHEMA = p("stage8b-p1e-i1a-schedule-envelope-v2.schema.json")
SCOPE = p("stage8b-p1e-i1a-implementation-scope-v2.json")
DESIGN = p("stage8b-p1e-i1a-schedule-source-design-v2.json")
MARKDOWN = p("stage8b-p1e-i1a-schedule-source-design-v2.md")
PHASE_MATRIX = p("stage8b-p1e-i1a-schedule-binding-phase-matrix-v1.csv")
MATRIX = p("stage8b-p1e-i1a-r1-acceptance-matrix-v1.csv")
FIXTURES = p("stage8b-p1e-i1a-r1-model-fixtures-v1.json")
TRUST = p("stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json")
V4_TIMER = p("stage8b-p1e-source-timer-precedence-v4.json")
V1_POLICY = p("stage8b-p1e-i1a-schedule-source-policy-v1.json")
V1_SCHEMA = p("stage8b-p1e-i1a-schedule-envelope-v1.schema.json")
V1_SCOPE = p("stage8b-p1e-i1a-implementation-scope-v1.json")
V1_DESIGN = p("stage8b-p1e-i1a-schedule-source-design-v1.json")
V1_MARKDOWN = p("stage8b-p1e-i1a-schedule-source-design-v1.md")
V1_MATRIX = p("stage8b-p1e-i1a-schedule-source-acceptance-matrix-v1.csv")

EXPECTED_RAW_SHA256 = {
    POLICY: "9571d40f0968b7bc458afe01f21cde18074849727c21bd261690ccdef9507754",
    BINDING_SCHEMA: "a3cc618cf6da433fb72a63887242b5b4f01b26c7fe380b7f5399afe74d47071b",
    PROGRESSION: "db1127314b3ca8d5ca54def78c05050e3cf297be19ec07080693bb2e06648a89",
    DAY_PROOF: "256418e8913e7be8a03ecc1690b1a421e8403599f03bfee0cd58f02084060ad5",
    TIMER: "f46d50e8701366ae89a57f9f0beaa36e444c479b463d7ed926067105fe14018f",
    ENVELOPE_SCHEMA: "0f8cd1ad6203308d4f5a24cb59ed0163d4c54b62e89e0191fc2cd295784c026c",
    SCOPE: "ecc15fc47d8795085f8efb6e0bee4c68119eef27f3769f85c1f86ae0f3c8c27b",
    DESIGN: "e779cf166de82907442bd20ef88742ca07a9a98d927abd6cc670934bfef97b44",
    MARKDOWN: "54257bc578f8865643650f5db9acdb4e3dc0a489c7630f57e74502b3fb98e40b",
    PHASE_MATRIX: "3bddd1de1ee36776f3acb2a6accd46fac03a2575f4edeec5e331ffcb9bf4ad49",
    MATRIX: "060eae3c3ae9fbb430a1c696197e11030dcc64f02eae78514be903f862e7d613",
    FIXTURES: "614eea3f489f16bf597475cde3df389c47bf2084f20f61da5aa4fb71e9bca632",
    TRUST: "dfe61ddb944df042cdf9514f56c14131e4a45bc732435ff89658ceaceb92d4ee",
    V4_TIMER: "097e1ae6a5164280e8694c7aee27513851d509955bdb0aa1dd9e8c5a12cc6b16",
    V1_POLICY: "1f4c2563b398f0e9f2d8dfc3c9e1236ee37dafe55c267f78c0858c61a1354437",
    V1_SCHEMA: "582cd322136683c8dc9042da218fa92bcb8ed6a5100aee3de61e7f9974fb0d56",
    V1_SCOPE: "20b2190c57ca334198401fb1ec65a79cddab778a7a11bbec517ddd63c3246eaa",
    V1_DESIGN: "42a32e0d74703d9faffa26bfe41c4407201df7e2e691825bf3a068ed4abcb0ae",
    V1_MARKDOWN: "ef6d5385adebedc49d4adc35e73158af0b0012352a14ae9050c467027f44c049",
    V1_MATRIX: "9faaf7ce8572b968ea9a9dd897546c3621ebb53f29b65f3bc7d47f93931e7b5c",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage8b-p1e-i1a-r1-design-check: FAIL: {message}")


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
        raise SystemExit(f"stage8b-p1e-i1a-r1-design-check: FAIL: invalid JSON {path}: {error}") from error
    require(isinstance(value, dict), f"JSON root is not object: {path.relative_to(ROOT)}")
    return value


def load_csv(path: Path) -> tuple[list[str], list[dict[str, str]]]:
    with path.open(newline="") as handle:
        reader = csv.DictReader(handle)
        fields = reader.fieldnames
        rows = list(reader)
    require(fields is not None, f"missing CSV header: {path.relative_to(ROOT)}")
    require(all(None not in row for row in rows), f"malformed CSV row: {path.relative_to(ROOT)}")
    return fields, rows


def exact_false_map(value: object, expected_keys: set[str], name: str) -> None:
    require(isinstance(value, dict), f"{name} is not an object")
    require(set(value) == expected_keys, f"{name} inventory drift")
    require(all(item is False for item in value.values()), f"{name} opened a surface")


def main() -> None:
    for path, expected in EXPECTED_RAW_SHA256.items():
        require(path.is_file(), f"missing pinned artifact: {path.relative_to(ROOT)}")
        require(sha256(path) == expected, f"raw SHA-256 drift: {path.relative_to(ROOT)}")

    policy = load_json(POLICY)
    binding = load_json(BINDING_SCHEMA)
    progression = load_json(PROGRESSION)
    day = load_json(DAY_PROOF)
    timer = load_json(TIMER)
    envelope = load_json(ENVELOPE_SCHEMA)
    scope = load_json(SCOPE)
    design = load_json(DESIGN)
    fixtures = load_json(FIXTURES)
    trust = load_json(TRUST)

    require(policy["schema_version"] == 2, "policy version drift")
    require(policy["architecture"] == "pre-provisioned-signed-broker-neutral-snapshot-stream", "architecture drift")
    require(policy["supersedes"]["raw_sha256"] == EXPECTED_RAW_SHA256[V1_POLICY], "policy predecessor drift")
    producer = policy["producer"]
    require(producer["required_input"] == "FINAM GET-only AssetScheduleResponse plus accepted broker-neutral Stage4 report", "GET-only source drift")
    require(producer["manual_or_supervisor_publication_allowed"] is False, "supervisor publication opened")
    require(producer["finam_post_delete_allowed"] is False, "FINAM write opened")
    require(producer["broker_dispatch_allowed"] is False, "broker dispatch opened")

    policy_trust = policy["trust"]
    schedule_key = trust["source_keys"]["schedule"]
    require(policy_trust["algorithm"] == "ed25519-sha256-prehash-v1", "signature algorithm drift")
    require(policy_trust["signature_domain"] == "moex.stage8b.p1e.schedule-envelope.signature.v2", "signature domain drift")
    require(policy_trust["key_id"] == schedule_key["key_id"], "key id mismatch")
    require(policy_trust["key_generation"] == schedule_key["generation"] == 2, "key generation mismatch")
    require(policy_trust["public_key_ed25519_hex"] == schedule_key["public_key_ed25519_hex"], "public key mismatch")
    require(policy_trust["public_key_sha256"] == schedule_key["public_key_sha256"], "public key hash mismatch")
    require(policy_trust["redis_is_trust_anchor"] is False, "Redis became trust anchor")
    require(policy_trust["operational_private_key_installation_authorized"] is False, "private key install opened")

    transport = policy["transport"]
    expected_command = "XADD <stream> NOMKSTREAM MAXLEN = 4096 * payload <canonical-signed-envelope>"
    require(transport["publisher_command"] == expected_command, "publisher command drift")
    require(transport["publisher_command_requires_nomkstream"] is True, "NOMKSTREAM not required")
    require(transport["publisher_preflight_type_is_not_write_fence"] is True, "TYPE race treated as write fence")
    require(transport["missing_stream_result"] == "fail-no-create-no-retry-without-nomkstream", "missing stream policy drift")
    require(transport["consumer_group"] is None, "schedule consumer group opened")
    require(transport["lookup_count"] == 64, "lookup bound drift")
    require(transport["retention_exact_maxlen"] == 4096, "retention drift")
    require(transport["publisher_may_create_stream"] is False, "publisher create opened")
    require(transport["supervisor_may_create_repair_trim_xadd_xdel_xack"] is False, "supervisor write opened")
    require(transport["invalid_or_untrusted_newest_entry"] == "fail-closed-no-older-fallback", "older fallback opened")

    source_progression = policy["progression"]
    require(source_progression["semantics"] == "authenticated-monotonic-snapshot-not-event-log", "snapshot semantics drift")
    require(source_progression["positive_sequence_jump"].startswith("allowed-with-same-generation"), "positive jump disabled")
    require(source_progression["ordinary_m10_cadence"] == "does-not-require-observing-each-one-second-publication", "M10 cadence continuity reintroduced")
    require(source_progression["same_sequence_different_bytes"] == "terminal-conflict", "sequence conflict weakened")
    require(source_progression["same_semantic_revision_different_semantic_hash"] == "terminal-conflict", "semantic conflict weakened")
    require(source_progression["invalid_newest_fallback"] is False, "invalid newest fallback opened")
    require(source_progression["unbounded_paging_or_catchup"] is False, "unbounded paging opened")
    require(source_progression["durable_commit_each_publication"] is False, "per-heartbeat durability opened")

    routes = policy["route_evidence"]
    require(routes["market_execution"]["required_stage4_state"] == "open", "Market no longer requires Open")
    require(routes["market_execution"]["closed_state_authorizes"] is False, "Closed authorizes Market")
    require(routes["working_limit_evaluation"]["required_stage4_state"] == "open", "Working no longer requires Open")
    require(routes["working_limit_evaluation"]["closed_state_authorizes"] is False, "Closed authorizes Working")
    require(routes["cancel_step"]["accepted_stage4_states"] == ["open", "closed"], "cancel evidence drift")
    require(routes["day_expiry"]["required_stage4_state"] == "closed", "Day no longer requires Closed")
    require(routes["day_expiry"]["requires_last_eligible_m10_evaluated"] is True, "Day last M10 proof removed")
    require(routes["day_expiry"]["closed_state_authorizes_market_or_working"] is False, "Closed trading grant opened")

    durable = policy["durable_binding"]
    require(durable["record_type"] == "Stage6JournalRecordV4::ScheduleEvidenceBound", "V4 record drift")
    require(durable["journal_sequence"] == "exact-prior-lifecycle-sequence-plus-one", "V4 sequence drift")
    require(durable["covering_seal_generation"] == "exact-prior-covering-seal-generation-plus-one", "seal generation drift")
    require(durable["authority_before_covering_seal_and-reread"] is False, "early authority opened")
    require(durable["binding_seal_is_business_terminal_boundary"] is False, "binding became business terminal")
    require(durable["source_m10_xack_effect"] == "none", "binding gained XACK effect")
    counter_effects = policy["binding_counter_effects"]
    require(counter_effects["stage6_lifecycle_sequence_increment"] == 1, "binding sequence increment drift")
    require(counter_effects["stage6_covering_seal_generation_increment"] == 1, "binding seal increment drift")
    require(counter_effects["ack_truth_local_adjacency"] == "seq_truth-equals-seq_ack-plus-one-unchanged", "ACK truth adjacency drift")
    require(counter_effects["m10_pel_membership_change"] is False, "binding changes M10 PEL")
    require(counter_effects["m10_xack_count_increment"] == 0, "binding increments M10 XACK")
    require(counter_effects["strategy_callback_count_increment"] == 0, "binding invokes callback")
    require(counter_effects["paper_provider_count_increment"] == 0, "binding invokes provider")
    require(counter_effects["business_outcome_count_increment"] == 0, "binding creates business outcome")

    timer_policy = policy["timer_precedence"]
    require(timer_policy["policy"] == "SOURCE_FIRST_THREE_OWNER_LOOP_STEPS", "timer policy drift")
    for key in (
        "latch_after_source_before_reclassification",
        "latch_after_reclassification_before_schedule_work",
        "latch_after_volatile_schedule_before-binding",
        "latch_after_committed_binding",
        "latch_immediately_before_timer_execution",
    ):
        require(timer_policy[key] is True, f"required timer checkpoint removed: {key}")
    require(timer_policy["timer_execution_same_step_as_reclassification_or_binding"] is False, "timer steps collapsed")

    exact_false_map(policy["closed_surfaces"], {
        "redis_db0_vps_activation", "redis_db15_schedule_activation", "schedule_private_key_installation",
        "finam_post_delete", "broker_dispatch", "runtime_live", "real_orders",
        "generation_2_execution_activation",
    }, "policy closed surfaces")

    require(envelope["properties"]["schema_version"]["const"] == 2, "envelope version drift")
    required = set(envelope["required"])
    require({"publication_sequence", "semantic_revision", "schedule_semantic_sha256"} <= required, "progression fields missing from envelope")
    evidence = envelope["$defs"]["stage4Evidence"]["oneOf"]
    require(evidence == [{"$ref": "#/$defs/tradabilityEvidence"}, {"$ref": "#/$defs/dayBoundaryEvidence"}], "route evidence union drift")
    require(envelope["$defs"]["tradabilityEvidence"]["allOf"][1]["properties"]["schedule_state"]["const"] == "open", "tradability Open drift")
    require(envelope["$defs"]["dayBoundaryEvidence"]["allOf"][1]["properties"]["schedule_state"]["const"] == "closed", "day Closed drift")
    require(envelope["x_supersedes"]["raw_sha256"] == EXPECTED_RAW_SHA256[V1_SCHEMA], "envelope predecessor drift")

    require(binding["properties"]["schema_version"]["const"] == 4, "binding schema version drift")
    require(binding["properties"]["record_kind"]["const"] == "schedule_evidence_bound", "binding kind drift")
    binding_required = set(binding["required"])
    require({"lifecycle_sequence", "prior_covering_seal_generation", "expected_covering_seal_generation", "transition_binding_sha256", "exact_envelope_hex", "envelope_sha256"} <= binding_required, "binding fields missing")
    constraints = set(binding["x_semantic_constraints"])
    require("lifecycle_sequence equals prior journal lifecycle sequence plus one" in constraints, "sequence constraint missing")
    require("expected_covering_seal_generation equals prior_covering_seal_generation plus one" in constraints, "seal constraint missing")

    rules = {item["id"]: item["decision"] for item in progression["rules"]}
    require(len(rules) == 9 and set(rules) == {f"PR{index:02d}" for index in range(1, 10)}, "progression rule inventory drift")
    require(rules["PR05"] == "accept-monotonic-jump", "monotonic jump rule drift")
    require(rules["PR09"] == "blocked-no-fallback", "newest fallback rule drift")
    require(progression["cadence_examples"]["continuity_non_requirement"] == "intermediate-publications-are-not-business-events-and-need-not-be-observed", "event-log continuity reintroduced")

    require(day["routes"]["market_execution"]["accepted_state"] == ["open"], "Day proof Market state drift")
    require(day["routes"]["working_limit_evaluation"]["closed_authorizes"] is False, "Day proof opened Working")
    require(day["routes"]["day_expiry"]["accepted_state"] == ["closed"], "Day proof Closed drift")
    require(day["restart_before_binding"].startswith("requires-new-fresh-closed-source"), "restart-before binding weakened")
    require(day["restart_after_binding"].startswith("reissues-only-from-exact-durable-historical-envelope"), "restart-after binding widened")

    require(timer["base_contract"]["raw_sha256"] == EXPECTED_RAW_SHA256[V4_TIMER], "timer V4 predecessor drift")
    steps = timer["owner_loop_steps"]
    require([item["step"] for item in steps] == [1, 2, 3], "owner-loop step inventory drift")
    require(steps[0]["new_schedule_binding_seal_allowed"] is False, "step1 schedule seal opened")
    require(steps[0]["timer_execution_allowed"] is False, "step1 timer execution opened")
    require(steps[1]["new_schedule_binding_seal_allowed"] == "exactly-one-only-after-latch-D-clear", "step2 binding fence drift")
    require(steps[1]["timer_execution_allowed"] is False, "step2 timer execution opened")
    require(steps[2]["timer_execution_allowed"] == "at-most-one", "step3 timer cardinality drift")

    phase_fields, phases = load_csv(PHASE_MATRIX)
    require(phase_fields == ["phase", "record_state", "sequence_and_seal", "returned_owner", "restart_route", "allowed_effects", "forbidden_effects", "boundary"], "phase matrix header drift")
    expected_phases = [f"B0{index}_" for index in range(7)]
    require(len(phases) == 7 and all(row["phase"].startswith(prefix) for row, prefix in zip(phases, expected_phases)), "phase inventory drift")
    require(phases[1]["restart_route"] == "ResumeExactScheduleBindingJournalAhead", "journal-ahead restart drift")
    require(phases[2]["allowed_effects"] == "reread_only", "reread-pending effects drift")
    require(phases[3]["boundary"] == "authenticated_internal_boundary_not_business_terminal", "binding boundary drift")
    require("xack" in phases[3]["forbidden_effects"], "early XACK not forbidden")

    expected_stage6_paths = {
        "crates/strategy-runtime-core/src/stage6_durable_identity.rs",
        "crates/strategy-runtime-core/src/stage6_journal_backend.rs",
        "crates/strategy-runtime-core/src/stage6_reconciliation_v2.rs",
        "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    }
    require(expected_stage6_paths <= set(scope["modifiable_rust_paths"]), "Stage6 implementation allowlist incomplete")
    v4 = scope["stage6_v4_contract"]
    require(v4["old_v1_v2_v3_wire_bytes_change"] is False, "old Stage6 bytes opened")
    require(v4["old_v1_v2_v3_decode_and_replay_change"] is False, "old Stage6 replay opened")
    require(v4["v4_is_business_terminal_boundary"] is False, "V4 business terminal opened")
    require(v4["v4_is_source_xack_authority"] is False, "V4 XACK authority opened")
    require(scope["operational_activation_authorized"] is False, "operational activation opened")

    matrix_fields, matrix = load_csv(MATRIX)
    require(matrix_fields == ["id", "area", "case", "precondition", "expected_owner", "expected_effects", "expected_readiness", "status", "supersedes"], "correction matrix header drift")
    require(len(matrix) == 46, "correction matrix must contain 46 rows")
    ids = [row["id"] for row in matrix]
    require(len(set(ids)) == len(ids), "duplicate correction matrix id")
    require(all(row["status"] == "REQUIRED" for row in matrix), "non-required correction row")
    superseded = {row["supersedes"] for row in matrix if row["supersedes"]}
    require(superseded == {"I1A-P04", "I1A-S04", "I1A-E05", "I1A-T01", "I1A-T03", "I1A-T04", "I1A-T05", "I1A-T06"}, "superseded v1 row inventory drift")
    _, old_matrix = load_csv(V1_MATRIX)
    require(len(old_matrix) == 81, "immutable v1 matrix row count drift")
    require(len(old_matrix) - len(superseded) + len(matrix) == 119, "effective matrix count drift")

    require(fixtures["schema_version"] == 1, "fixture version drift")
    require([len(fixtures[name]) for name in ("progression_cases", "route_cases", "binding_phase_cases", "timer_cases", "publisher_cases")] == [12, 12, 7, 7, 3], "fixture group counts drift")

    require(design["status"] == "DESIGN_CORRECTION_REVIEW_CANDIDATE_IMPLEMENTATION_NOT_AUTHORIZED", "design status drift")
    require(design["accepted_predecessors"]["foundation_r2_source"] == "37088964e50c0ceb4d82887a30c32103110749b0", "Foundation R2 binding drift")
    require(design["progression"]["ordinary_n_to_n_plus_600"].startswith("allowed"), "design ordinary M10 jump drift")
    require(design["durable_binding"]["business_terminal_boundary"] is False, "design binding terminal drift")
    require(design["owner_loop"]["reclassify_bind_execute_same_step"] is False, "design steps collapsed")
    require(all(value is False for key, value in design["implementation_authorization"].items() if key != "requires_independent_i1a_r1_design_acceptance"), "source implementation authorized")
    require(design["implementation_authorization"]["requires_independent_i1a_r1_design_acceptance"] is True, "review requirement removed")
    exact_false_map(design["closed_surfaces"], set(policy["closed_surfaces"]), "design closed surfaces")

    markdown = MARKDOWN.read_text()
    for needle in (
        "`NOMKSTREAM` is the atomic no-create fence",
        "N → N+600",
        "Stage6JournalRecordVersioned::V4",
        "not a business-terminal",
        "latch A immediately after source",
        "Restart before",
        "Production implementation still needs a separate source review",
    ):
        require(needle in markdown, f"design markdown missing exact contract: {needle}")

    for relative in scope["new_rust_paths"]:
        require(not (ROOT / relative).exists(), f"production implementation appeared before design acceptance: {relative}")

    print("stage8b-p1e-i1a-r1-design-check: PASS")
    print("pinned_artifacts=20 correction_rows=46 effective_rows=119 model_cases=41")
    print("production_schedule_source=absent operational_surfaces=closed")


if __name__ == "__main__":
    main()
