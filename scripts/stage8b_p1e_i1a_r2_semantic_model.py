#!/usr/bin/env python3
"""Execute linked semantic identity, revision, progression, and route fixtures."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any

import stage8b_p1e_i1a_r1_semantic_model as base


ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "docs/stage-8/stage8b-p1e-i1a-r2-semantic-fixtures-v1.json"
HASH_DOMAIN = b"moex.stage8b.p1e.schedule-semantic-identity.sha256.v1"


def fail(message: str) -> None:
    raise SystemExit(f"stage8b-p1e-i1a-r2-semantic-model: FAIL: {message}")


def load() -> dict[str, Any]:
    try:
        value = json.loads(FIXTURES.read_text(), object_pairs_hook=base.reject_duplicate_keys)
    except (OSError, UnicodeDecodeError, ValueError, json.JSONDecodeError) as error:
        fail(f"invalid fixture JSON: {error}")
    if not isinstance(value, dict):
        fail("fixture root is not an object")
    return value


def canonical_bytes(identity: dict[str, Any]) -> bytes:
    return json.dumps(
        identity,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")


def semantic_hash(identity: dict[str, Any]) -> str:
    return hashlib.sha256(HASH_DOMAIN + b"\0" + canonical_bytes(identity)).hexdigest()


def validate_identity(name: str, identity: dict[str, Any]) -> None:
    required = {
        "schema_version", "domain", "instrument", "registry", "trading_day",
        "timezone", "timeframe_sec", "sessions", "stage4_semantic_state",
    }
    if set(identity) != required:
        fail(f"identity field inventory drift: {name}")
    if identity["schema_version"] != 1 or identity["domain"] != "moex.stage8b.p1e.schedule-semantic-identity.v1":
        fail(f"identity domain drift: {name}")
    if identity["timezone"] != "Europe/Moscow" or identity["timeframe_sec"] != 600:
        fail(f"identity schedule constants drift: {name}")
    state = identity["stage4_semantic_state"]
    pair = (state.get("evidence_kind"), state.get("schedule_state"))
    if pair not in {("tradability", "open"), ("day_boundary", "closed")}:
        fail(f"invalid route semantic state: {name}")
    if pair == ("tradability", "open") and state.get("boundary_proof") is not None:
        fail(f"Open identity has boundary proof: {name}")
    if pair == ("day_boundary", "closed") and not isinstance(state.get("boundary_proof"), dict):
        fail(f"Closed identity lacks boundary proof: {name}")


def route_case(route_name: str, identity: dict[str, Any]) -> dict[str, Any]:
    state = identity["stage4_semantic_state"]
    return {
        "id": route_name,
        "route": route_name,
        "kind": state["evidence_kind"],
        "state": state["schedule_state"],
        "fresh": True,
        "exact_book": True,
        "exact_last_m10": True,
        "no_later_eligible_m10": True,
        "boundary_reached": True,
    }


def evaluate(case: dict[str, Any], identities: dict[str, dict[str, Any]]) -> tuple[str, object, str, str]:
    if case.get("persistent_high_water_available") is False:
        return "not-evaluated", "not-issued", "blocked-no-publication", "not-evaluated"
    prior = identities[case["prior_identity"]]
    candidate = identities[case["candidate_identity"]]
    prior_hash = semantic_hash(prior)
    candidate_hash = semantic_hash(candidate)
    relation = "same" if prior_hash == candidate_hash else "different"
    derived_revision = case["prior_revision"] if relation == "same" else case["prior_revision"] + 1
    revision = derived_revision if case["candidate_revision"] == "derive" else case["candidate_revision"]
    progression_case = {
        "id": case["id"],
        "high_water": {
            "generation": 1,
            "sequence": case["prior_sequence"],
            "published_at_ms": case["prior_published_at_ms"],
            "semantic_revision": case["prior_revision"],
            "semantic_hash": prior_hash,
            "envelope_hash": "11" * 32,
        },
        "candidate": {
            "valid": True,
            "generation": 1,
            "sequence": case["candidate_sequence"],
            "published_at_ms": case["candidate_published_at_ms"],
            "semantic_revision": revision,
            "semantic_hash": candidate_hash,
            "envelope_hash": "22" * 32,
        },
    }
    progression_result = base.progression(progression_case)
    route_result = "not-evaluated"
    if progression_result in {"accept-monotonic-jump", "idempotent"}:
        route_result = base.route(route_case(case["route"], candidate))
    return relation, revision, progression_result, route_result


def main() -> None:
    fixtures = load()
    if fixtures.get("schema_version") != 1 or fixtures.get("domain") != "moex.stage8b.p1e.i1a.r2.semantic-fixtures.v1":
        fail("fixture contract drift")
    identities = fixtures.get("identities")
    cases = fixtures.get("cases")
    if not isinstance(identities, dict) or set(identities) != {"open_day_1", "closed_day_1", "open_day_2"}:
        fail("identity inventory drift")
    if not isinstance(cases, list) or len(cases) != 8:
        fail("linked case count is not 8")
    for name, identity in identities.items():
        if not isinstance(identity, dict):
            fail(f"identity is not object: {name}")
        validate_identity(name, identity)
    seen: set[str] = set()
    for case in cases:
        if case["id"] in seen:
            fail(f"duplicate case id: {case['id']}")
        seen.add(case["id"])
        actual = evaluate(case, identities)
        expected = (
            case["expected_hash_relation"],
            case["expected_revision"],
            case["expected_progression"],
            case["expected_route"],
        )
        if actual != expected:
            fail(f"{case['id']}: expected {expected!r}, got {actual!r}")
        print(f"PASS {case['id']} hash={actual[0]} revision={actual[1]} progression={actual[2]} route={actual[3]}")
    print("stage8b-p1e-i1a-r2-semantic-model: PASS 8/8 linked-computed")
    print("coverage_note=base-41-includes-14-table-lookups; r2-8-are-linked-computed-not-source-execution")


if __name__ == "__main__":
    main()
