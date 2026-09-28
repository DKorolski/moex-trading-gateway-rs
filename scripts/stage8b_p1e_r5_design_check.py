#!/usr/bin/env python3
"""Fail-closed cross-contract checker for Stage 8B-P1-e R5 design."""

from __future__ import annotations

import csv
import hashlib
import io
import json
import pathlib
import subprocess
from typing import Any

import stage8b_p1e_r4_design_check as r4


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "98523fd009712883f73f9b5a15cb545c8e9f13ac"
ACCEPTED = "c2a9e1246dfdd59f3a6297268de907dedcb19903"
R4_REVIEW_SHA256 = "0432058a685317fd8aa3b262a790cb35c146ec5d0edb3ba1995e8dc1b84fcffc"
DOCS = "docs/stage-8/"
FILES = {
    "design": DOCS + "stage8b-p1e-deployable-supervisor-design-r5.md",
    "acceptance": DOCS + "stage8b-p1e-deployable-supervisor-r5-acceptance-matrix.csv",
    "active": DOCS + "stage8b-p1e-active-acceptance-contract-v5.json",
    "semantic": DOCS + "stage8b-p1e-semantic-authority-registry-v5.json",
    "identity": DOCS + "stage8b-p1e-deployment-identity-v2.json",
    "transaction": DOCS + "stage8b-p1e-first-boot-transaction-v4.json",
    "outer": DOCS + "stage8b-p1e-restart-continuation-matrix-v3.csv",
    "operational": DOCS + "stage8b-p1e-operational-pretransition-matrix-v5.csv",
    "precedence": DOCS + "stage8b-p1e-source-timer-precedence-v2.json",
    "manifest": DOCS + "stage8b-p1e-redis-deployment-manifest-v1.json",
    "evidence": DOCS + "stage8b-p1e-deployable-supervisor-r5-design-evidence.json",
    "status": "docs/current-status.md",
    "roadmap": "docs/roadmap.md",
}
EXPECTED_CHANGED = {
    FILES[name] for name in (
        "design", "acceptance", "active", "semantic", "identity",
        "transaction", "outer", "operational", "precedence", "evidence",
        "status", "roadmap",
    )
} | {
    "scripts/stage8b_p1e_r5_design_check.py",
    "scripts/stage8b_p1e_r5_design_negative_harness.py",
    "scripts/stage8b_p1e_r5_design_gate.sh",
    "scripts/stage8b_p1e_r5_design_handoff_safety_check.py",
    "scripts/make_stage8b_p1e_r5_design_handoff.py",
}
R5_ACCEPTANCE_SHA = "e89b1b700123259c2c0dd3573f6771b2758b024a6828ac57f8811f8e80990476"
R4_ACCEPTANCE_SHA = "af576620f7dc57b05e613b0f33f44c5da802248431e5b08b71bdf1de6b6ea70b"
R4_SUPERSEDED = {"P1ER4-002", "P1ER4-003", "P1ER4-004", "P1ER4-006", "P1ER4-007", "P1ER4-008", "P1ER4-009"}
ADDED_SEMANTICS = {
    "systemd.MainAddressFamilies": "AF_UNIX+AF_INET+AF_INET6",
    "systemd.MainPrivateNetwork": "false-host-network-namespace",
    "systemd.MainRedisEndpointPolicy": "systemd-loopback-addresses-only-plus-app-exact-port-6379-DB15-URL",
    "systemd.BootstrapNetworkIsolation": "AF_UNIX+PrivateNetwork=true+Redis-contact=false",
    "systemd.RecoveryNetworkIsolation": "AF_UNIX+PrivateNetwork=true+Redis-contact=false",
    "firstboot.ClassificationDisjointnessOrPrecedence": "pairwise-disjoint-exactly-one-match-no-precedence-mandatory-adopted-marker",
    "restart.P1d3TruthCommittedPhaseSet": "p1d3_s_truth|p1d3_s_cancel_recovered",
    "restart.CancelRecoveredOnlyContinuation": "resume_stage8b_p1d3_truth_with_redis-only-XACK-or-frontier-no-truth-no-ACK-replay",
}
EXPECTED_SEMANTICS = {**r4.EXPECTED_SEMANTIC_VALUES, **ADDED_SEMANTICS}


class CheckFailure(RuntimeError):
    pass


def require(value: bool, message: str) -> None:
    if not value:
        raise CheckFailure(message)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def csv_rows(data: bytes) -> list[dict[str, str]]:
    return list(csv.DictReader(io.StringIO(data.decode("utf-8"))))


def read_all(overrides: dict[str, bytes] | None = None) -> dict[str, bytes]:
    overrides = overrides or {}
    return {name: overrides.get(path, (ROOT / path).read_bytes()) for name, path in FILES.items()}


def load_json(blobs: dict[str, bytes], name: str) -> dict[str, Any]:
    value = json.loads(blobs[name])
    require(isinstance(value, dict), f"{name} must be object")
    return value


def changed_files() -> set[str]:
    tracked = subprocess.run(["git", "diff", "--name-only", BASE], cwd=ROOT, check=True, text=True, capture_output=True).stdout.splitlines()
    untracked = subprocess.run(["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT, check=True, text=True, capture_output=True).stdout.splitlines()
    return set(tracked) | set(untracked)


def validate_active(blobs: dict[str, bytes], active: dict[str, Any]) -> None:
    require(active.get("schema_version") == 5 and active.get("domain") == "moex.stage8b.p1e.active-acceptance-contract.v5", "active identity")
    require(active.get("direct_parent") == BASE and active.get("accepted_predecessor") == ACCEPTED, "active lineage")
    sources = active["sources"]
    require([source["name"] for source in sources] == ["r1", "r2", "r3", "r4", "r5"], "active sources")
    expected = {
        "r1": (88, r4.SOURCE_EXPECTATIONS["r1"][1], r4.SOURCE_EXPECTATIONS["r1"][2], 83),
        "r2": (48, r4.SOURCE_EXPECTATIONS["r2"][1], r4.SOURCE_EXPECTATIONS["r2"][2], 34),
        "r3": (43, r4.SOURCE_EXPECTATIONS["r3"][1], r4.SOURCE_EXPECTATIONS["r3"][2], 9),
        "r4": (56, R4_ACCEPTANCE_SHA, R4_SUPERSEDED, 49),
        "r5": (42, R5_ACCEPTANCE_SHA, set(), 42),
    }
    all_ids: set[str] = set()
    active_ids: set[str] = set()
    excluded: set[str] = set()
    active_counts: dict[str, int] = {}
    for source in sources:
        name = source["name"]
        path = ROOT / source["path"]
        data = blobs["acceptance"] if name == "r5" else path.read_bytes()
        count, digest, exact_excluded, active_count = expected[name]
        rows = csv_rows(data)
        ids = {row["id"] for row in rows}
        require(len(rows) == len(ids) == count and all(row["status"] == "REQUIRED" for row in rows), f"{name} matrix")
        require(sha256(data) == digest == source["file_sha256"], f"{name} digest")
        actual_excluded = set(source["superseded_rows"])
        require(actual_excluded == exact_excluded and actual_excluded <= ids, f"{name} excluded")
        require(not all_ids & ids, f"{name} duplicate ids")
        all_ids |= ids
        active_ids |= ids - actual_excluded
        excluded |= actual_excluded
        active_counts[name] = len(ids - actual_excluded)
        require(active_counts[name] == active_count, f"{name} active count")
    mapped: list[str] = []
    for item in active["supersession_map"]:
        require(item["superseded"] and item["replacement"], "empty supersession")
        require(all(row in active_ids for row in item["replacement"]), "inactive replacement")
        mapped.extend(item["superseded"])
    require(len(mapped) == len(set(mapped)) and set(mapped) == excluded, "supersession exactness")
    require(active_counts == {"r1": 83, "r2": 34, "r3": 9, "r4": 49, "r5": 42} and len(active_ids) == 217, "active row total")
    require(active["active_contract_expectation"] == {
        "r1_active_rows": 83, "r2_active_rows": 34, "r3_active_rows": 9,
        "r4_active_rows": 49, "r5_active_rows": 42,
        "total_active_rows": 217, "all_status_required": True,
    }, "active expectation")


def validate_semantic(value: dict[str, Any]) -> None:
    require(value.get("schema_version") == 5 and value.get("domain") == "moex.stage8b.p1e.semantic-authority-registry.v5", "semantic identity")
    require(value["required_keys"] == list(EXPECTED_SEMANTICS), "semantic key inventory")
    require(value["expected_active_values"] == EXPECTED_SEMANTICS, "semantic expected values")
    grouped: dict[str, set[str]] = {}
    for item in value["authorities"]:
        require(item["key"] in EXPECTED_SEMANTICS, "unknown semantic key")
        if item["active"]:
            grouped.setdefault(item["key"], set()).add(item["value"])
        else:
            require(item.get("superseded_by"), "inactive semantic unbound")
    require(set(grouped) == set(EXPECTED_SEMANTICS), "semantic coverage")
    for key, expected in EXPECTED_SEMANTICS.items():
        require(grouped[key] == {expected}, f"semantic conflict: {key}")


def validate_network(identity: dict[str, Any], manifest: dict[str, Any]) -> None:
    require(identity.get("schema_version") == 2 and identity.get("domain") == "moex.stage8b.p1e.deployment-identity.v2", "identity V2")
    shared = identity["shared_unit_contract"]
    require("RestrictAddressFamilies" not in shared and "PrivateNetwork" not in shared, "network policy incorrectly shared")
    modes = identity["mode_specific_network_contracts"]
    main = modes["main_run"]
    require(main == {
        "RestrictAddressFamilies": ["AF_UNIX", "AF_INET", "AF_INET6"],
        "PrivateNetwork": False, "IPAddressDeny": "any",
        "IPAddressAllow": ["127.0.0.1/32", "::1/128"],
        "redis_url_allowlist": ["redis://127.0.0.1:6379/15", "redis://[::1]:6379/15"],
        "redis_database": 15, "destination_port": 6379,
        "config_url_match": "byte-exact-one-of-redis-url-allowlist",
        "outbound_policy": "only-loopback-addresses-systemd-plus-exact-host-port-db-in-application-preflight",
        "non_loopback": "exit-66-before-connect", "wrong_port_or_database": "exit-66-before-connect",
    }, "main network contract")
    isolated = {"RestrictAddressFamilies": ["AF_UNIX"], "PrivateNetwork": True, "redis_contact_allowed": False, "network_syscalls_required": False}
    require(modes["bootstrap"] == isolated and modes["bootstrap_recover"] == isolated, "bootstrap network isolation")
    require(main["redis_url_allowlist"] == manifest["redis_url_allowlist"], "Redis URL compatibility")
    require(main["redis_database"] == manifest["redis_db_index"] == 15, "Redis DB compatibility")
    require(manifest["run_mode"] == "verify-only" and manifest["run_may_create_or_repair"] is False, "manifest run boundary")
    binding = identity["network_compatibility_binding"]
    require(binding["manifest_urls_must_equal_main_allowlist"] is True and binding["bootstrap_and_recovery_must_not_reach_redis"] is True, "network binding")
    require(len(identity["required_network_tests"]) == 7, "network test inventory")


def validate_first_boot(value: dict[str, Any]) -> None:
    require(value.get("schema_version") == 4 and value.get("domain") == "moex.stage8b.p1e.first-boot-transaction.v4", "transaction V4")
    classes = {item["classification"]: item for item in value["base_classifications"]}
    expected_names = {
        "NoRoot", "UnpublishedMarkerTemp", "PreparedWithoutRoot", "RootWithoutJournal",
        "JournalWithoutSeal", "CommittedRootReceiptTemp", "CommittedRootResponseLost",
        "QuarantinedIncompleteRoot", "ReceiptCommittedMarkerUpdatePending",
        "AdoptedCommittedRoot", "CorruptOrIdentityMismatch",
    }
    require(set(classes) == expected_names and len(classes) == 11, "base classification inventory")
    receipt = classes["ReceiptCommittedMarkerUpdatePending"]
    adopted = classes["AdoptedCommittedRoot"]
    require(receipt["precondition"] == "valid-final-receipt-v2-package-v2-seal-provenance-ready-owner-binding-canonical-marker-phase-seal_committed-and-adopted-marker-temp-absent", "post-receipt predicate")
    require(receipt["required_action"] == "start-seal-committed-to-adopted" and receipt["run_allowed"] is False, "post-receipt action")
    require(adopted["precondition"] == "valid-receipt-v2-package-v2-seal-provenance-ready-owner-binding-canonical-marker-phase-adopted-and-marker-temp-absent", "adopted predicate")
    require(adopted["required_action"] == "ordinary-run-only" and adopted["run_allowed"] is True, "adopted action")
    temp = next(item for item in value["marker_update_temp_classifications"] if item["classification"] == "SealCommittedToAdoptedMarkerTempPending")
    require(temp["exclusive_precondition"].startswith("valid-final-receipt-present-plus-valid-adopted-marker-temp-present-excludes"), "adopted temp exclusivity")
    require(temp["required_phase_effect"].endswith("canonical-marker-remains-seal_committed"), "adopted temp phase")
    model = value["classification_model"]
    require(model == {
        "selected_model": "mandatory-adopted-marker-completion-before-ordinary-run",
        "evaluation": "evaluate-all-authenticated-predicates-and-require-exactly-one-match",
        "precedence_allowed": False,
        "zero_matches": "CorruptOrIdentityMismatch-exit-66-preserve-evidence",
        "multiple_matches": "CorruptOrIdentityMismatch-exit-66-preserve-evidence",
        "receipt_commit_effect": "receipt-is-durable-prerequisite-but-does-not-authorize-ordinary-run-until-adopted-marker-commit",
        "ordinary_run_commit_point": "canonical-adopted-marker-rename-plus-mutable-state-parent-fsync-plus-reread-authentication",
        "adopted_root_exact_predicate": [
            "canonical-marker-phase-adopted", "transaction-marker-temp-absent",
            "receipt-temp-absent", "valid-final-receipt-v2",
            "valid-package-v2-seal-provenance-and-Ready-binding",
        ],
    }, "classification model")
    proof = value["classification_disjointness_proof"]
    require(len(proof) == 12 and len(set(proof.values())) == 12, "disjointness proof")
    require(value["adoption_protocol"]["ordinary_run_requires_marker_phase"] == "adopted", "ordinary run marker phase")
    require(value["adoption_protocol"]["marker_temp_must_be_absent"] is True and value["adoption_protocol"]["receipt_is_sufficient_without_adopted_marker"] is False, "receipt authority overlap")
    require(len(value["marker_update_temp_classifications"]) == 4 and len(value["required_sigkill_hooks"]) == 9, "retained marker frontiers")


def validate_restart(outer_data: bytes, operational_data: bytes, precedence: dict[str, Any]) -> None:
    outer = csv_rows(outer_data)
    require(len(outer) == 23, "outer row count")
    truth = [row for row in outer if row["variant"] == "P1d3TruthCommitted"]
    require({row["authenticated_package_phase"] for row in truth} == {"p1d3_s_truth", "p1d3_s_cancel_recovered"} and len(truth) == 2, "truth phase set")
    cancel = next(row for row in truth if row["authenticated_package_phase"] == "p1d3_s_cancel_recovered")
    require(cancel["starting_boundary"] == "s_cancel_recovered_committed", "cancel boundary")
    require(cancel["first_legal_transition"] == "resume_stage8b_p1d3_truth_with_redis", "cancel continuation")
    require(cancel["truth_replay_legality"] == cancel["ack_replay_legality"] == "forbidden", "cancel replay")
    require(cancel["logical_phase_key"] == "P1d3TruthCommitted+p1d3_s_cancel_recovered", "cancel route key")

    operational = csv_rows(operational_data)
    require(len(operational) == 56 and len({row["id"] for row in operational}) == 56, "operational matrix")
    cancel_rows = [row for row in operational if row["authenticated_package_phase"] == "p1d3_s_cancel_recovered"]
    require({row["id"] for row in cancel_rows} == {"OC55", "OC56", "OC57", "OC58"}, "cancel operational inventory")
    by_id = {row["id"]: row for row in cancel_rows}
    require(by_id["OC55"]["redis_pel_state"] == "one_exact_claimable_source" and by_id["OC55"]["xack_legality"] == "exact_source_xack_only", "cancel claimable")
    require(by_id["OC56"]["redis_pel_state"] == "source_already_acknowledged_with_continuous_frontier" and by_id["OC56"]["xack_legality"] == "no_second_xack", "cancel already acked")
    require(
        by_id["OC57"]["redis_pel_state"] == "one_exact_not_yet_claimable"
        and by_id["OC57"]["next_owner"] == "same_durable_owner"
        and by_id["OC57"]["fresh_poll_legality"] == "forbidden"
        and by_id["OC57"]["first_legal_transition"] == "emit_degraded_and_retry_without_xack",
        "cancel not claimable",
    )
    require(by_id["OC58"]["timer_state"] == "exact_day_expiry_due" and by_id["OC58"]["first_legal_transition"] == "resume_stage8b_p1d3_truth_with_redis", "cancel due timer source first")
    for row in cancel_rows:
        require(row["starting_boundary"] == "s_cancel_recovered_committed", "cancel starting boundary")
        require(row["truth_replay_legality"] == row["ack_replay_legality"] == "forbidden", "cancel replay cell")
        require(row["phase_route"] == "P1d3TruthCommitted+p1d3_s_cancel_recovered", "cancel phase route")
        require(row["unlisted_disposition"] == "exit-67-before-transition", "cancel fallback")
    require(by_id["OC58"]["timer_derivation"] == "source-first-timer-deferred-v2" and "TimerReclassified" in by_id["OC58"]["post_transition_outcomes"], "cancel timer derivation")
    require(all(row["paper_ready_legality"] != "immediate" for row in cancel_rows), "premature PaperReady")

    logical = precedence["source_bearing_owner_derivation"]["logical_cancel_recovered_representation"]
    require(logical["authenticated_package_phase"] == "p1d3_s_cancel_recovered", "precedence cancel phase")
    require(logical["outer_matrix_row"] == "P1d3TruthCommitted+p1d3_s_cancel_recovered", "precedence outer route")
    require(logical["operational_rows"] == ["OC55-claimable", "OC56-already-acknowledged", "OC57-not-yet-claimable", "OC58-claimable-plus-due-Day-timer"], "precedence cells")
    require(logical["truth_replay_allowed"] is False and logical["ack_replay_allowed"] is False, "precedence replay")
    contract = precedence["cancel_recovered_exact_contract"]
    require(contract["authenticated_book_phase"] == "Stage8bP1d3BookPhase::CancelRecovered", "source phase binding")
    require(len(contract["legal_source_transitions"]) == 4 and "truth-replay" in contract["forbidden"] and "ACK-replay" in contract["forbidden"], "cancel continuation contract")


def validate_evidence(blobs: dict[str, bytes], evidence: dict[str, Any]) -> None:
    design, status, roadmap = blobs["design"].decode(), blobs["status"].decode(), blobs["roadmap"].decode()
    for token in ("Status: design-only review candidate", "217 active rows", "PrivateNetwork=false", "ReceiptCommittedMarkerUpdatePending", "OC55", "OC58"):
        require(token in design, f"design token: {token}")
    require(
        "a valid receipt is a durable prerequisite, but ordinary" in design
        and "prior \u201cvalid receipt wins\u201d/best-effort-marker rule\n"
        "is explicitly superseded" in design,
        "design receipt/marker authority rule",
    )
    for text, name in ((status, "status"), (roadmap, "roadmap")):
        for token in ("P1-e R5", BASE, "source implementation remains unauthorized"):
            require(token in text, f"{name} token: {token}")
    require(evidence.get("status") == "R5_DESIGN_REVIEW_CANDIDATE" and evidence.get("parent") == BASE, "evidence status")
    require(evidence.get("r4_review_sha256") == R4_REVIEW_SHA256, "review binding")
    require(evidence.get("design_only") is True and evidence.get("source_implementation_authorized") is False, "evidence scope")
    require((evidence.get("active_rows"), evidence.get("restart_outer_rows"), evidence.get("operational_rows"), evidence.get("semantic_authority_keys"), evidence.get("negative_cases")) == (217, 23, 56, 22, 44), "evidence counts")
    require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")
    hashes = evidence["contract_sha256"]
    expected_names = {"design", "acceptance", "active", "semantic", "identity", "transaction", "outer", "operational", "precedence"}
    require(set(hashes) == expected_names, "evidence hash inventory")
    for name in expected_names:
        require(hashes[name] == sha256(blobs[name]), f"evidence hash: {name}")


def validate(overrides: dict[str, bytes] | None = None, check_scope: bool = False) -> None:
    r4.validate()
    blobs = read_all(overrides)
    active = load_json(blobs, "active")
    semantic = load_json(blobs, "semantic")
    identity = load_json(blobs, "identity")
    transaction = load_json(blobs, "transaction")
    precedence = load_json(blobs, "precedence")
    manifest = load_json(blobs, "manifest")
    evidence = load_json(blobs, "evidence")
    validate_active(blobs, active)
    validate_semantic(semantic)
    validate_network(identity, manifest)
    validate_first_boot(transaction)
    validate_restart(blobs["outer"], blobs["operational"], precedence)
    validate_evidence(blobs, evidence)
    if check_scope:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"scope mismatch missing={sorted(EXPECTED_CHANGED-actual)} extra={sorted(actual-EXPECTED_CHANGED)}")


def main() -> None:
    try:
        validate(check_scope=True)
    except (CheckFailure, r4.CheckFailure, OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-r5-design-check: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1e-r5-design-scope files=17 production_rust=0 cargo=0 workflow=0 active_unit=0")
    print("PASS stage8b-p1e-r5-active-contract rows=217 r1=83 r2=34 r3=9 r4=49 r5=42")
    print("PASS stage8b-p1e-r5-network main=tcp-loopback-db15 bootstrap=isolated recovery=isolated")
    print("PASS stage8b-p1e-r5-first-boot base=11 temp=4 disjoint=true ordinary_run=adopted-marker-only")
    print("PASS stage8b-p1e-r5-restart outer=23 operational=56 cancel_recovered=4")
    print("PASS stage8b-p1e-r5-semantic-authority keys=22 conflicts=0")


if __name__ == "__main__":
    main()
