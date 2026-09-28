#!/usr/bin/env python3
"""Fail-closed checker for the P1-d4 R7 precommitted-publication design."""

from __future__ import annotations

import csv
import hashlib
import json
import pathlib
import re
import struct
import subprocess
import sys
import uuid
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[1]
P1D3_REF = "7dc7c802feca6e79d3a1a9902c181ad7b6afc506"
R3_REF = "e1ce6d3baec3974d8dfd05c2f3de00110e0605bf"
R6_REF = "cb6e6ddf863f314cc96b5f8ac0a75809e8c6824a"

GENERAL_SHA256 = "e2a376fda504a691b3dffa5160542c5f184bf59ff9f224ae4159b5bbaf8c06de"
BASE_SHA256 = "f4125113d4f7f3ceafd849c1d32f1097c313e15eccbb353c8bd89a19434edac6"
DESIGN_SHA256 = "43f0abd00dc3a7b8f0805b4d06c4f32b23e45c6ac749696a86f92a1612152d6d"
DISCOVERY_SHA256 = "1c924ee40394de568daf439ee1e77286e81fad9279193ec4b30414c9064a4371"
GM_SHA256 = "99b30f93c2c7b3c281f6f96c7eacab677e45b32bd89ec81d1847259cb39d514e"
AMENDMENT_SHA256 = "6cd2525bd748b913e7a90e1d0448a4f9f86141c3438031832edbfd66e6d44ebf"
SOURCE_SHAPE_SHA256 = "9ba83b6d677e12520ab1f783cc2f557ff1c9aaaee636540651345045f432ba01"
FIXTURE_SHA256 = "5f3f684b6d752199d8c82778c6b56ab9e282b22ad8f130d88dcd6d445923159f"
RESERVATION_CANONICAL_SHA256 = "4b7e0959c5af7be279b81aa5e201f9fcfdddbed3a80b23962862be2bcf1d1621"
BINDING_CANONICAL_SHA256 = "2584a322800b02c78f7de71c1e9c9061d78c518d84ac09786cb05acb0dbb5252"

GENERAL = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv"
BASE = ROOT / "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv"
GM = ROOT / "docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v3.csv"
AMENDMENT = ROOT / "docs/stage-8/stage8b-p1d4-r7-acceptance-amendment.csv"
DESIGN = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-design-r7.md"
DISCOVERY = ROOT / "docs/stage-8/stage8b-p1d4-source-discovery-r7.md"
SOURCE_SHAPE = ROOT / "docs/stage-8/stage8b-p1d4-source-shape-r7.json"
FIXTURE = ROOT / "docs/stage-8/fixtures/stage8b-p1d4-command-publication-binding-v1.json"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-evidence-r7.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"

SOURCE_HASHES = {
    "crates/runtime-durable-service/src/recovery.rs":
        "70cf38671834d81e84b55b90903d1fca26ce7f79f598e92462bd1a62ec0d5284",
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs":
        "17edbe4c41315aef97faa8574eedd884e75ad90cb70e9aa01812bc6e197a7295",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs":
        "ef113596c8f9835669987853ea1ce4bdb976c73d507cf8014dabcff23a923ef8",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs":
        "619a6f7e4aff4d2d6ba2e0c86036da7df96383e3de020afbb6eaa3f907cb8478",
    "crates/strategy-runtime-core/src/stage8b_p1d1_paper_provider.rs":
        "2e9630533aa0470ad73294fb0afff6e155bf0a738eba2dae51eca9d0f7387a2b",
    "crates/strategy-runtime-core/src/stage8b_p1d2_market_feedback.rs":
        "376b0af099cdb95a31c48a4ab1b23272aa34dc436b19dd61898ddc078bde6480",
}

EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/fixtures/stage8b-p1d4-command-publication-binding-v1.json",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r7.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-evidence-r7.json",
    "docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v3.csv",
    "docs/stage-8/stage8b-p1d4-r7-acceptance-amendment.csv",
    "docs/stage-8/stage8b-p1d4-source-discovery-r7.md",
    "docs/stage-8/stage8b-p1d4-source-shape-r7.json",
    "scripts/make_stage8b_p1d4_r7_design_handoff.py",
    "scripts/stage8b_p1d4_r7_design_check.py",
    "scripts/stage8b_p1d4_r7_design_gate.sh",
    "scripts/stage8b_p1d4_r7_design_handoff_safety_check.py",
    "scripts/stage8b_p1d4_r7_design_negative_harness.py",
}

RESERVATION_FIELDS = [
    "schema_version", "domain", "source_stream", "source_group",
    "source_m10_redis_id", "semantic_batch_id_sha256", "strategy_request_id",
    "canonical_command_sha256", "canonical_envelope_sha256", "command_stream",
    "command_group", "command_stream_predecessor_id", "reserved_command_entry_id",
    "prepublication_package_generation",
]
BINDING_FIELDS = [
    "schema_version", "domain", "source_stream", "source_group",
    "source_m10_redis_id", "semantic_batch_id_sha256", "strategy_request_id",
    "canonical_command_sha256", "canonical_envelope_sha256", "command_stream",
    "command_group", "command_stream_predecessor_id", "command_entry_id",
    "prepublication_package_generation", "publication_reservation_sha256",
    "prepublication_seal_generation", "prepublication_seal_commitment_sha256",
]


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode()).hexdigest()


def read_rows(value: str) -> list[dict[str, str]]:
    return list(csv.DictReader(value.splitlines()))


def committed_changed_files() -> set[str]:
    parent = subprocess.check_output(["git", "rev-parse", "HEAD^"], cwd=ROOT, text=True).strip()
    require(parent == R6_REF, f"R7 parent drifted: {parent}")
    output = subprocess.check_output(
        ["git", "diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"],
        cwd=ROOT,
        text=True,
    )
    return set(output.splitlines())


def source_at_baseline(path: str) -> str:
    try:
        return subprocess.check_output(["git", "show", f"{R6_REF}:{path}"], cwd=ROOT, text=True)
    except (subprocess.CalledProcessError, FileNotFoundError):
        return (ROOT / path).read_text(encoding="utf-8")


def require_order(value: str, tokens: list[str], label: str) -> None:
    cursor = -1
    for token in tokens:
        position = value.find(token, cursor + 1)
        require(position > cursor, f"{label} missing/reordered token: {token}")
        cursor = position


def lp_utf8(value: Any) -> bytes:
    require(isinstance(value, str) and "\0" not in value, "noncanonical UTF-8 token")
    raw = value.encode("utf-8", errors="strict")
    require(len(raw) <= 0xFFFFFFFF, "UTF-8 token too long")
    return struct.pack(">I", len(raw)) + raw


def raw_digest(value: Any) -> bytes:
    require(isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None, "noncanonical digest")
    return bytes.fromhex(value)


def uuid_bytes(value: Any) -> bytes:
    require(isinstance(value, str), "UUID is not text")
    try:
        parsed = uuid.UUID(value)
    except (ValueError, AttributeError) as error:
        raise CheckFailure("invalid UUID") from error
    require(str(parsed) == value, "UUID text is noncanonical")
    return parsed.bytes


def redis_id_bytes(value: Any) -> bytes:
    require(isinstance(value, str) and re.fullmatch(r"(?:0|[1-9][0-9]*)-(?:0|[1-9][0-9]*)", value) is not None, "noncanonical Redis ID")
    millis_text, sequence_text = value.split("-", 1)
    millis, sequence = int(millis_text), int(sequence_text)
    require(millis <= 0xFFFFFFFFFFFFFFFF and sequence <= 0xFFFFFFFFFFFFFFFF, "Redis ID overflow")
    return struct.pack(">QQ", millis, sequence)


def u64_bytes(value: Any, label: str) -> bytes:
    require(type(value) is int and 0 <= value <= 0xFFFFFFFFFFFFFFFF, f"invalid {label}")
    return struct.pack(">Q", value)


def immediate_successor(value: str) -> str:
    redis_id_bytes(value)
    millis, sequence = (int(item) for item in value.split("-", 1))
    if sequence < 0xFFFFFFFFFFFFFFFF:
        return f"{millis}-{sequence + 1}"
    require(millis < 0xFFFFFFFFFFFFFFFF, "Redis successor overflow")
    return f"{millis + 1}-0"


def reservation_bytes(value: dict[str, Any]) -> bytes:
    require(type(value.get("schema_version")) is int and value["schema_version"] == 1, "reservation schema drifted")
    body = (
        struct.pack(">H", value["schema_version"])
        + lp_utf8(value["domain"])
        + lp_utf8(value["source_stream"])
        + lp_utf8(value["source_group"])
        + redis_id_bytes(value["source_m10_redis_id"])
        + raw_digest(value["semantic_batch_id_sha256"])
        + uuid_bytes(value["strategy_request_id"])
        + raw_digest(value["canonical_command_sha256"])
        + raw_digest(value["canonical_envelope_sha256"])
        + lp_utf8(value["command_stream"])
        + lp_utf8(value["command_group"])
        + redis_id_bytes(value["command_stream_predecessor_id"])
        + redis_id_bytes(value["reserved_command_entry_id"])
        + u64_bytes(value["prepublication_package_generation"], "prepublication package generation")
    )
    return b"moex.stage8b.p1d4.command-publication-reservation.canonical.v1\0" + struct.pack(">Q", len(body)) + body


def binding_bytes(value: dict[str, Any]) -> bytes:
    require(type(value.get("schema_version")) is int and value["schema_version"] == 1, "binding schema drifted")
    body = (
        struct.pack(">H", value["schema_version"])
        + lp_utf8(value["domain"])
        + lp_utf8(value["source_stream"])
        + lp_utf8(value["source_group"])
        + redis_id_bytes(value["source_m10_redis_id"])
        + raw_digest(value["semantic_batch_id_sha256"])
        + uuid_bytes(value["strategy_request_id"])
        + raw_digest(value["canonical_command_sha256"])
        + raw_digest(value["canonical_envelope_sha256"])
        + lp_utf8(value["command_stream"])
        + lp_utf8(value["command_group"])
        + redis_id_bytes(value["command_stream_predecessor_id"])
        + redis_id_bytes(value["command_entry_id"])
        + u64_bytes(value["prepublication_package_generation"], "prepublication package generation")
        + raw_digest(value["publication_reservation_sha256"])
        + u64_bytes(value["prepublication_seal_generation"], "prepublication seal generation")
        + raw_digest(value["prepublication_seal_commitment_sha256"])
    )
    return b"moex.stage8b.p1d4.command-publication-binding.canonical.v1\0" + struct.pack(">Q", len(body)) + body


def validate_fixture(fixture: dict[str, Any]) -> None:
    require(type(fixture.get("schema_version")) is int and fixture["schema_version"] == 1, "fixture schema drifted")
    reservation = fixture.get("reservation", {})
    binding = fixture.get("binding", {})
    require(set(reservation) == set(RESERVATION_FIELDS) | {"publication_reservation_sha256"}, "reservation field inventory drifted")
    require(set(binding) == set(BINDING_FIELDS) | {"publication_binding_sha256"}, "binding field inventory drifted")
    require(reservation["domain"] == "moex.stage8b.p1d4.command-publication-reservation.v1", "reservation domain drifted")
    require(binding["domain"] == "moex.stage8b.p1d4.command-publication-binding.v1", "binding domain drifted")
    require(immediate_successor(reservation["command_stream_predecessor_id"]) == reservation["reserved_command_entry_id"], "reserved ID is not immediate successor")
    for source, target in (
        ("source_stream", "source_stream"), ("source_group", "source_group"),
        ("source_m10_redis_id", "source_m10_redis_id"),
        ("semantic_batch_id_sha256", "semantic_batch_id_sha256"),
        ("strategy_request_id", "strategy_request_id"),
        ("canonical_command_sha256", "canonical_command_sha256"),
        ("canonical_envelope_sha256", "canonical_envelope_sha256"),
        ("command_stream", "command_stream"), ("command_group", "command_group"),
        ("command_stream_predecessor_id", "command_stream_predecessor_id"),
        ("reserved_command_entry_id", "command_entry_id"),
        ("prepublication_package_generation", "prepublication_package_generation"),
        ("publication_reservation_sha256", "publication_reservation_sha256"),
    ):
        require(reservation[source] == binding[target], f"reservation/binding mismatch: {source}")
    reserved = reservation_bytes(reservation)
    bound = binding_bytes(binding)
    require(binding["prepublication_seal_generation"] == reservation["prepublication_package_generation"] + 1, "fixture generation relation drifted")
    require(len(reserved) == 377 and len(bound) == 441, "canonical fixture length drifted")
    require(reserved.hex() == fixture.get("reservation_canonical_hex"), "reservation canonical bytes drifted")
    require(bound.hex() == fixture.get("binding_canonical_hex"), "binding canonical bytes drifted")
    reservation_digest = hashlib.sha256(reserved).hexdigest()
    binding_digest = hashlib.sha256(bound).hexdigest()
    require(reservation_digest == RESERVATION_CANONICAL_SHA256, "golden reservation identity drifted")
    require(binding_digest == BINDING_CANONICAL_SHA256, "golden binding identity drifted")
    require(reservation_digest == reservation["publication_reservation_sha256"], "reservation hash drifted")
    require(binding_digest == binding["publication_binding_sha256"], "binding hash drifted")


def validate_source_shape(shape: dict[str, Any], sources: dict[str, str]) -> None:
    require(shape.get("baseline_ref") == R6_REF, "source-shape baseline drifted")
    require(shape.get("status") == "R7_SOURCE_SHAPE_PINNED", "source-shape status drifted")
    require(shape.get("source_sha256") == SOURCE_HASHES, "source hash inventory drifted")
    facts = shape.get("facts", {})
    require(facts.get("existing_command_publication_uses_auto_id") is True, "existing publication fact drifted")
    require(facts.get("existing_p1d2_classifier_suffix_lengths") == [3, 4], "existing suffix fact drifted")
    require(facts.get("existing_restart_route_order") == ["P1d3V3", "P1d2V1", "GenericP1"], "existing route-order fact drifted")
    require(facts.get("generated_market_journal_version") == "V1" and facts.get("generated_market_v3_authorized") is False, "journal version fact drifted")
    require(facts.get("r7_source_change_requires_explicit_reserved_id") is True, "explicit ID correction lost")
    require(facts.get("r7_source_change_requires_package_aware_routing") is True, "package-aware correction lost")
    for path, digest in SOURCE_HASHES.items():
        require(path in sources and sha256_text(sources[path]) == digest, f"committed source shape drifted: {path}")

    recovery = sources["crates/runtime-durable-service/src/recovery.rs"]
    require_order(recovery, [
        "classify_stage8b_p1d3_journal_ahead_candidate",
        "classify_stage8b_p1d2_journal_ahead_candidate",
        "classify_stage8b_p1_journal_ahead_candidate",
    ], "accepted restart routing")
    redis = sources["crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs"]
    require("redis.call('XADD', command_stream, '*', 'payload', envelope_payload)" in redis, "accepted auto-ID source fact missing")
    require("command_entry_id = output_id" in redis, "accepted marker ID fact missing")
    live = sources["crates/strategy-runtime-core/src/stage6d_live_core.rs"]
    require("if !matches!(suffix_len, 3 | 4)" in live, "accepted P1-d2 suffix fact missing")
    require("let [dispatch, order, trade, rest @ ..] = v1.as_slice()" in live, "accepted P1-d2 shape fact missing")
    feedback = sources["crates/strategy-runtime-core/src/stage8b_p1d2_market_feedback.rs"]
    require_order(feedback, [".stage8b_p1d2_allocate_sequence_pair()", "p1d2-after-sequence-pair-before-ack", "apply_stage5g_mock_ack"], "accepted sequence ordering")


def validate(
    general: str,
    base: str,
    gm: str,
    amendment: str,
    design: str,
    discovery: str,
    shape_text: str,
    fixture_text: str,
    evidence: dict[str, Any],
    status: str,
    roadmap: str,
    sources: dict[str, str],
    *,
    verify_hashes: bool = True,
) -> None:
    shape = json.loads(shape_text)
    fixture = json.loads(fixture_text)
    if verify_hashes:
        for value, expected, label in (
            (general, GENERAL_SHA256, "general"), (base, BASE_SHA256, "base"),
            (gm, GM_SHA256, "GM"), (amendment, AMENDMENT_SHA256, "amendment"),
            (design, DESIGN_SHA256, "design"), (discovery, DISCOVERY_SHA256, "discovery"),
            (shape_text, SOURCE_SHAPE_SHA256, "source-shape"), (fixture_text, FIXTURE_SHA256, "fixture"),
        ):
            require(sha256_text(value) == expected, f"{label} artifact drifted")

    validate_source_shape(shape, sources)
    validate_fixture(fixture)

    for token in (
        "Status: R7 design-only review candidate", R6_REF, R3_REF, P1D3_REF,
        "active total: 105 exact positive cells",
        "Stage8bP1d4CommandPublicationReservationV1",
        "last-generated-id", "immediate successor", "dynamic ID", "repick",
        "explicit `XADD command_stream reserved_command_entry_id`", "never `XADD *`",
        "byte-identical E2", "PresentValid", "PresentInvalid", "Absent",
        "P1-d4 V1 routing before ordinary P1-d2",
        "hard Blocked/Corrupt; no classifier fallback",
        "fixed-order bytes", "RFC 4122 network-order 16 bytes",
        "exactly 64 lowercase hexadecimal", "u64_be(milliseconds)",
        "W1 = W0 + 1", "W2 = W1 + 1 = W0 + 2",
        "G1 = G0 + 1", "G2 = G1 + 1 = G0 + 2",
        "paused until independent R7 acceptance",
    ):
        require(token in design, f"design invariant missing: {token}")
    for token in (
        "accepted Lua command publication uses `XADD *`", "last-generated-id",
        "P1-d3 V3 classifier", "P1-d2 complete V1 classifier", "generic P1 classifier",
        "`PresentInvalid` blocks without fallback", "`Absent` retains existing standalone",
        "saved eight-file Rust WIP is",
    ):
        require(token in discovery, f"discovery invariant missing: {token}")

    general_rows, base_rows, gm_rows, amendment_rows = map(read_rows, (general, base, gm, amendment))
    require(len(general_rows) == 88 and len(base_rows) == 92 and len(gm_rows) == 13 and len(amendment_rows) == 40, "matrix row inventory drifted")
    require([row["cell_id"] for row in gm_rows] == [f"P1D4GM-{index:03d}" for index in range(1, 14)], "GM IDs drifted")
    require([row["frontier_id"] for row in gm_rows] == [f"GM{index:02d}" for index in range(13)], "GM frontiers drifted")
    require(all(row["parent_scenario_id"] == "S05" for row in gm_rows), "GM parent drifted")
    require(all(row["publication_reservation_expectation"] == "mandatory_hmac_covered_exact_reservation" for row in gm_rows), "reservation not mandatory in every phase")
    require(gm_rows[0]["publication_binding_expectation"] == "binding_absent_before_xadd", "pre-XADD binding drifted")
    require(all("reservation_marker_entry" in row["publication_binding_expectation"] or "persist_in_s_ack" in row["publication_binding_expectation"] for row in gm_rows[1:10]), "pre-S_ack binding derivation drifted")
    require(all(row["publication_binding_expectation"] == "mandatory_hmac_covered_exact_binding" for row in gm_rows[10:]), "post-S_ack binding drifted")
    require([row["classifier_route"] for row in gm_rows] == [
        "composite_present_valid_zero_suffix", "composite_present_valid_zero_suffix", "composite_present_valid_zero_suffix",
        "composite_present_valid_len1_before_p1d2", "composite_present_valid_len1_before_p1d2",
        "composite_present_valid_len2_before_p1d2", "composite_present_valid_len3_wrap_p1d2_before_return",
        "composite_present_valid_len4_wrap_p1d2_before_return", "composite_present_valid_len4_wrap_p1d2_before_return",
        "composite_present_valid_len4_wrap_p1d2_before_return", "authenticated_ack_phase_direct",
        "authenticated_ack_phase_direct", "authenticated_truth_phase_direct",
    ], "package-aware route graph drifted")
    require(gm_rows[0]["only_legal_continuation"] == "explicit_xadd_reserved_id_after_predecessor_validation", "explicit reserved publication lost")
    require(all(row["source_pel_before"] == "exact_source_pending_1" and row["source_pel_after"] == "exact_source_pending_1" for row in gm_rows), "source retention weakened")
    require(all(row["xack_delta"] == "0" for row in gm_rows[:-1]) and gm_rows[-1]["xack_delta"] == "+1_exact", "XACK-last drifted")
    require(all(row["duplicate_variant_required"] == "true" and row["conflict_variant_required"] == "true" for row in base_rows + gm_rows), "duplicate/conflict coverage weakened")
    require([row["id"] for row in amendment_rows] == [f"P1D4R7-{index:03d}" for index in range(1, 41)], "R7 amendment IDs drifted")
    require(all(row["status"] == "REQUIRED" for row in amendment_rows), "R7 requirement weakened")

    require(evidence.get("stage") == "Stage 8B-P1-d4 crash/replay design R7 correction", "evidence stage drifted")
    require(evidence.get("status") == "DESIGN_R7_REVIEW_CANDIDATE", "evidence status drifted")
    require(evidence.get("design_parent_ref") == R6_REF, "evidence parent drifted")
    require(evidence.get("accepted_r3_design_ref") == R3_REF and evidence.get("accepted_p1d3_closure_ref") == P1D3_REF, "evidence lineage drifted")
    for key, expected in {
        "design_r7_sha256": DESIGN_SHA256,
        "source_discovery_r7_sha256": DISCOVERY_SHA256,
        "source_shape_r7_sha256": SOURCE_SHAPE_SHA256,
        "publication_fixture_sha256": FIXTURE_SHA256,
        "inherited_base_matrix_sha256": BASE_SHA256,
        "generated_market_matrix_sha256": GM_SHA256,
        "acceptance_amendment_sha256": AMENDMENT_SHA256,
    }.items():
        require(evidence.get(key) == expected, f"evidence hash drifted: {key}")
    require(evidence.get("base_matrix_rows") == 92 and evidence.get("generated_market_matrix_rows") == 13 and evidence.get("active_positive_cells") == 105, "evidence proof inventory drifted")
    require(evidence.get("acceptance_amendment_rows") == 40, "evidence amendment inventory drifted")
    require(evidence.get("targeted_negative_cases") == 60 and evidence.get("total_contract_negative_cases") == 188, "negative inventory drifted")
    require(evidence.get("design_only") is True and evidence.get("implementation_authorized") is False and evidence.get("source_wip_included") is False, "design boundary opened")
    require(all(value is False for value in evidence.get("closed_surfaces", {}).values()), "closed surface opened")
    reservation = evidence.get("reservation_contract", {})
    require(reservation == {
        "dynamic_repick_allowed": False,
        "explicit_xadd_id_required": True,
        "predecessor_source": "XINFO_STREAM_last-generated-id",
        "reserved_id_rule": "checked_immediate_successor",
        "single_writer_required": True,
        "xadd_star_allowed": False,
    }, "reservation evidence drifted")
    routing = evidence.get("routing_contract", {})
    require(routing.get("present_invalid") == "hard_conflict_no_fallback", "invalid-composite fallback opened")
    require(routing.get("present_valid") == "p1d4_v1_before_ordinary_p1d2", "composite precedence drifted")
    require(routing.get("absent") == "ordinary_p1d2_then_generic_p1", "standalone P1-d2 drifted")
    canonical = evidence.get("canonical_fixture", {})
    require(canonical.get("publication_reservation_sha256") == fixture["reservation"]["publication_reservation_sha256"], "fixture reservation evidence drifted")
    require(canonical.get("publication_binding_sha256") == fixture["binding"]["publication_binding_sha256"], "fixture binding evidence drifted")
    require(evidence.get("generation_contract") == {
        "prepublication": "W0_G0",
        "s_ack": "W0_plus_1_G0_plus_1",
        "s_truth": "W0_plus_2_G0_plus_2",
        "successor_seal_substitution_allowed": False,
    }, "generation evidence drifted")

    for value, label in ((status, "status"), (roadmap, "roadmap")):
        for token in (R6_REF, "P1-d4 R7", "precommit", "P1-d2", "105", "paused", "independent R7 acceptance", "Redis DB0/VPS"):
            require(token in value, f"{label} missing token: {token}")


def read_inputs() -> tuple[Any, ...]:
    return (
        GENERAL.read_text(encoding="utf-8"), BASE.read_text(encoding="utf-8"),
        GM.read_text(encoding="utf-8"), AMENDMENT.read_text(encoding="utf-8"),
        DESIGN.read_text(encoding="utf-8"), DISCOVERY.read_text(encoding="utf-8"),
        SOURCE_SHAPE.read_text(encoding="utf-8"), FIXTURE.read_text(encoding="utf-8"),
        json.loads(EVIDENCE.read_text(encoding="utf-8")), STATUS.read_text(encoding="utf-8"),
        ROADMAP.read_text(encoding="utf-8"), {path: source_at_baseline(path) for path in SOURCE_HASHES},
    )


def main() -> None:
    try:
        content_only = sys.argv[1:] == ["--content-only"]
        require(not sys.argv[1:] or content_only, "usage: stage8b_p1d4_r7_design_check.py [--content-only]")
        if not content_only:
            require(committed_changed_files() == EXPECTED_CHANGED, "R7 committed path scope drifted")
        validate(*read_inputs())
    except (OSError, KeyError, TypeError, ValueError, struct.error, json.JSONDecodeError, subprocess.CalledProcessError, CheckFailure) as error:
        print(f"stage8b-p1d4-r7-design-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("PASS stage8b-p1d4-r7-design-scope files=14 active_cells=105 amendments=40 source_shape=exact reservation=precommitted routing=package_aware fixture=exact design_only=true")


if __name__ == "__main__":
    main()
