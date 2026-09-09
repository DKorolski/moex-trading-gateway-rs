#!/usr/bin/env python3
"""Fail-closed checker for the Stage 8B-P1-e R3 corrected design."""

from __future__ import annotations

import csv
import hashlib
import io
import json
import pathlib
import re
import subprocess


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "3aaed81da4f1a558b4d31f4d3a169ddceca61e6f"
ACCEPTED_P1D4 = "c2a9e1246dfdd59f3a6297268de907dedcb19903"
R2_REVIEW_SHA256 = "ea5828a52e2098da1fc2f4eeb52c37e40f9328e634f42840e64dc703e2b15b26"

DOCS = ROOT / "docs/stage-8"
DESIGN = DOCS / "stage8b-p1e-deployable-supervisor-design-r3.md"
R1_ACCEPTANCE = DOCS / "stage8b-p1e-deployable-supervisor-r1-acceptance-matrix.csv"
R2_ACCEPTANCE = DOCS / "stage8b-p1e-deployable-supervisor-r2-acceptance-matrix.csv"
R3_ACCEPTANCE = DOCS / "stage8b-p1e-deployable-supervisor-r3-acceptance-matrix.csv"
ACTIVE = DOCS / "stage8b-p1e-active-acceptance-contract-v3.json"
OUTER = DOCS / "stage8b-p1e-restart-continuation-matrix-v2.csv"
OPERATIONAL = DOCS / "stage8b-p1e-operational-pretransition-matrix-v3.csv"
TRANSACTION = DOCS / "stage8b-p1e-first-boot-transaction-v2.json"
RECEIPT = DOCS / "stage8b-p1e-first-boot-receipt-v1.json"
PACKAGE = DOCS / "stage8b-p1e-authenticated-restart-package-v2.json"
ACQUISITION = DOCS / "stage8b-p1e-source-acquisition-seam-v1.json"
REDIS_POLICY = DOCS / "stage8b-p1e-redis-runtime-policy-v1.json"
EVIDENCE = DOCS / "stage8b-p1e-deployable-supervisor-r3-design-evidence.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"
RECOVERY_SOURCE = ROOT / "crates/runtime-durable-service/src/recovery.rs"

CONTRACT_HASHES = {
    "active_acceptance_contract_v3_sha256": "6447082e81e0adee7aed5c3336231e44ee040b5d0ab47e454a47b0de0725cd90",
    "r3_acceptance_matrix_file_sha256": "7da8c390d6474e32b0b680892cce335eb528d0d633fcc1e6f7ddedf0d1c7b342",
    "first_boot_transaction_v2_sha256": "8008789da8020e0e6470177706031eeac19a64050de091bc94dcfa3328646f51",
    "first_boot_receipt_v1_sha256": "0241146a81f0e8fdd51e78c1584e3b2ffe03337b07e976dd05417b16c990464f",
    "authenticated_restart_package_v2_sha256": "114a2c746ade4ee024b5bf0f81441d712549172fe0ab7bc2724949ca73725b1a",
    "source_acquisition_seam_v1_sha256": "05aaee3c1e27f0b9765c4e4755077a3722a1949c6fa5753234a2690ad4ef2ce0",
    "operational_pretransition_matrix_v3_file_sha256": "e24860e874d264ab0dec35ddf3e91e169204b24076347beed34d4134c0d70ce0",
}

INHERITED_R2_HASHES = {
    "docs/stage-8/stage8b-p1e-deployable-supervisor-design-r2.md": "2d088ef1c00865e255cb2f9c945f10231603d1ac07bde6533b61cb0d13c4b318",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r2-acceptance-matrix.csv": "6d43489edf8039ce7697f81395dd09cfde0d541eadf6ef5ae7df9e4b5774987b",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r2-design-evidence.json": "7e1eb0ef5b3b24903e30fe85f4859d5c53f58fb80b7900bcee9b6316134c6885",
    "docs/stage-8/stage8b-p1e-first-boot-transaction-v1.json": "9fac94e5d2b4ea1661579ba5f6c6a182276de95fa5259f1e863010d4739dd6bf",
    "docs/stage-8/stage8b-p1e-redis-runtime-policy-v1.json": "364d83d539491a71df6c6b9a379121f9c1a85cb3a6f4085a27574a2cef7c9aa9",
    "docs/stage-8/stage8b-p1e-restart-continuation-matrix-v2.csv": "79936494cc20c460f7a4249ca759fc43da3cca0a31c51d1ad39c26d3a03b79df",
    "docs/stage-8/stage8b-p1e-operational-continuation-matrix-v2.csv": "8160c7a072d2aa10ea1d75677d7a20efd65b76f7098901ee812028e606444739",
}

EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1e-active-acceptance-contract-v3.json",
    "docs/stage-8/stage8b-p1e-authenticated-restart-package-v2.json",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-design-r3.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r3-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r3-design-evidence.json",
    "docs/stage-8/stage8b-p1e-first-boot-receipt-v1.json",
    "docs/stage-8/stage8b-p1e-first-boot-transaction-v2.json",
    "docs/stage-8/stage8b-p1e-operational-pretransition-matrix-v3.csv",
    "docs/stage-8/stage8b-p1e-source-acquisition-seam-v1.json",
    "scripts/make_stage8b_p1e_r3_design_handoff.py",
    "scripts/stage8b_p1e_r3_design_check.py",
    "scripts/stage8b_p1e_r3_design_gate.sh",
    "scripts/stage8b_p1e_r3_design_handoff_safety_check.py",
    "scripts/stage8b_p1e_r3_design_negative_harness.py",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def canonical_json_sha256(value: object) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
    return hashlib.sha256(encoded).hexdigest()


def text_sha256(value: str) -> str:
    return hashlib.sha256(value.encode()).hexdigest()


def file_sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def csv_rows(value: str) -> list[dict[str, str]]:
    return list(csv.DictReader(io.StringIO(value)))


def changed_files() -> set[str]:
    tracked = subprocess.run(
        ["git", "diff", "--name-only", BASE], cwd=ROOT, check=True,
        text=True, capture_output=True,
    ).stdout.splitlines()
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT,
        check=True, text=True, capture_output=True,
    ).stdout.splitlines()
    return set(tracked) | set(untracked)


def restart_variants(source: str) -> list[str]:
    body = source.split("pub enum Stage7bRestartOutcome {", 1)[1].split("\n}", 1)[0]
    return re.findall(r"^    ([A-Za-z][A-Za-z0-9]+)(?:\(|,)", body, flags=re.MULTILINE)


def validate_active_contract(
    active: dict[str, object], r1_text: str, r2_text: str, r3_text: str
) -> set[str]:
    require(active.get("schema_version") == 3, "active contract version drifted")
    require(active.get("domain") == "moex.stage8b.p1e.active-acceptance-contract.v3", "active contract domain drifted")
    require(active.get("duplicate_semantic_key_rule") == "reject-active-rows-with-one-semantic-key-and-unequal-semantic-values", "semantic conflict rule weakened")
    source_texts = {"r1": r1_text, "r2": r2_text, "r3": r3_text}
    expected_superseded = {
        "r1": {"P1ER1-002", "P1ER1-003", "P1ER1-004", "P1ER1-043", "P1ER1-074"},
        "r2": {"P1ER2-002", "P1ER2-004", "P1ER2-005", "P1ER2-006", "P1ER2-012", "P1ER2-016", "P1ER2-017", "P1ER2-022", "P1ER2-026", "P1ER2-027", "P1ER2-028", "P1ER2-035", "P1ER2-036", "P1ER2-044"},
        "r3": set(),
    }
    expected_hashes = {
        "r1": "b937e5c2434c665914cc1e27db12aee5f0669cb96e8db9764ca3d1791a938c0c",
        "r2": "6d43489edf8039ce7697f81395dd09cfde0d541eadf6ef5ae7df9e4b5774987b",
        "r3": CONTRACT_HASHES["r3_acceptance_matrix_file_sha256"],
    }
    expected_counts = {"r1": 88, "r2": 48, "r3": 43}
    sources = active.get("sources")
    require(isinstance(sources, list) and [item.get("name") for item in sources] == ["r1", "r2", "r3"], "active source inventory drifted")
    all_rows: dict[str, dict[str, str]] = {}
    active_ids: set[str] = set()
    excluded_ids: set[str] = set()
    for source in sources:
        name = source["name"]
        rows = csv_rows(source_texts[name])
        require(len(rows) == expected_counts[name], f"{name} row count drifted")
        require(source.get("row_count") == len(rows), f"{name} declared row count drifted")
        require(source.get("file_sha256") == expected_hashes[name], f"{name} source digest drifted")
        require(text_sha256(source_texts[name]) == expected_hashes[name], f"{name} bytes drifted")
        superseded = set(source.get("superseded_rows", []))
        require(superseded == expected_superseded[name], f"{name} superseded set drifted")
        ids = {row["id"] for row in rows}
        require(len(ids) == len(rows) and superseded <= ids, f"{name} row identity drifted")
        require(all(row["status"] == "REQUIRED" for row in rows), f"{name} row weakened")
        for row in rows:
            require(row["id"] not in all_rows, f"duplicate acceptance ID: {row['id']}")
            all_rows[row["id"]] = row
        active_ids |= ids - superseded
        excluded_ids |= superseded

    supersession = active.get("supersession_map")
    require(isinstance(supersession, list) and len(supersession) == 10, "supersession map shape drifted")
    mapped: list[str] = []
    for item in supersession:
        old = item.get("superseded", [])
        replacements = item.get("replacement", [])
        require(old and replacements, "empty supersession mapping")
        mapped.extend(old)
        require(all(row_id in active_ids for row_id in replacements), "supersession replacement is not active")
    require(len(mapped) == len(set(mapped)) and set(mapped) == excluded_ids, "superseded row mapping is not exact")

    expectation = active.get("active_contract_expectation", {})
    require(expectation == {
        "r1_active_rows": 83,
        "r2_active_rows": 34,
        "r3_active_rows": 43,
        "total_active_rows": 160,
        "all_status_required": True,
    }, "active row expectation drifted")
    require(len(active_ids) == 160, f"active row count drifted: {len(active_ids)}")

    bindings = active.get("semantic_key_bindings")
    require(isinstance(bindings, list) and len(bindings) == 14, "semantic binding inventory drifted")
    binding_by_id: dict[str, tuple[str, str]] = {}
    for item in bindings:
        row_id = item.get("row_id")
        require(row_id in all_rows and row_id not in binding_by_id, "semantic binding row drifted")
        binding_by_id[row_id] = (item["semantic_key"], item["semantic_value"])
    semantic_values: dict[str, set[str]] = {}
    for row_id in active_ids:
        row = all_rows[row_id]
        if row_id in binding_by_id:
            key, value = binding_by_id[row_id]
        elif row_id.startswith("P1ER3-"):
            key, value = row["semantic_key"], row["requirement"]
        else:
            key, value = f"legacy.{row_id}", row["requirement"]
        semantic_values.setdefault(key, set()).add(value)
    conflicts = {key: values for key, values in semantic_values.items() if len(values) > 1}
    require(not conflicts, f"active semantic conflict: {conflicts}")
    return active_ids


def validate(
    design: str,
    r1_acceptance: str,
    r2_acceptance: str,
    r3_acceptance: str,
    active: dict[str, object],
    outer_text: str,
    operational_text: str,
    transaction: dict[str, object],
    receipt: dict[str, object],
    package: dict[str, object],
    acquisition: dict[str, object],
    redis_policy: dict[str, object],
    evidence: dict[str, object],
    status: str,
    roadmap: str,
    recovery_source: str,
) -> None:
    for token in (
        "Status: R3 design-only review candidate",
        ACCEPTED_P1D4,
        BASE,
        R2_REVIEW_SHA256,
        "HOLD / SUPERSEDED BY R3",
        "160 active REQUIRED rows",
        "Two active rows with the same semantic key and unequal",
        "stage8b-p1-paper-bootstrap-recover@.service",
        "RECOVER_EXISTING_STAGE8B_P1_FIRST_BOOT_V2",
        "Stage8bP1FirstBootReceiptV1",
        "Stage7bRestartOutcome::Ready",
        "Stage6dAuthenticatedRestartPackageV2",
        "byte-identical provenance",
        "52 unique",
        "cannot inspect or manufacture `zero_intent`",
        "Stage8bP1eClaimedM10DeliveryV1",
        "process_claimed_working_limit",
        "Neither facade may call `XPENDING`, `XAUTOCLAIM` or `XREADGROUP`",
        "No preview callback exists",
        "exact `S_cancel_recovered`",
        "This R3 does not authorize that implementation",
        "0 < child_pid <= u32::MAX",
    ):
        require(token in design, f"missing R3 design invariant: {token}")

    r3_rows = csv_rows(r3_acceptance)
    require(len(r3_rows) == 43, f"R3 acceptance rows drifted: {len(r3_rows)}")
    require(list(r3_rows[0]) == ["id", "area", "semantic_key", "requirement", "status"], "R3 acceptance fields drifted")
    require([row["id"] for row in r3_rows] == [f"P1ER3-{i:03d}" for i in range(1, 44)], "R3 acceptance IDs drifted")
    require(all(row["status"] == "REQUIRED" for row in r3_rows), "R3 acceptance weakened")
    active_ids = validate_active_contract(active, r1_acceptance, r2_acceptance, r3_acceptance)
    require(all(row["id"] in active_ids for row in r3_rows), "R3 row inactive")

    variants = restart_variants(recovery_source)
    outer = csv_rows(outer_text)
    require(len(variants) == 22 and len(outer) == 22, "outer restart inventory drifted")
    require([row["variant"] for row in outer] == variants, "outer restart enum mapping drifted")

    operational = csv_rows(operational_text)
    expected_ids = ["OC01", "OC02"] + [f"OC{i:02d}" for i in range(5, 55)]
    require(len(operational) == 52 and [row["id"] for row in operational] == expected_ids, "pre-transition matrix inventory drifted")
    expected_fields = [
        "id", "local_restart_variant", "authenticated_package_phase", "redis_pel_state",
        "timer_state", "starting_boundary", "equivalent_authority_reissue",
        "first_legal_transition", "next_owner", "s06r_completion_boundary",
        "source_disposition", "xack_legality", "fresh_poll_legality",
        "paper_ready_legality", "accepted_proof_cells", "post_transition_outcomes",
    ]
    require(list(operational[0]) == expected_fields, "pre-transition matrix fields drifted")
    classifiers = [tuple(row[key] for key in ("local_restart_variant", "authenticated_package_phase", "redis_pel_state", "timer_state")) for row in operational]
    require(len(classifiers) == len(set(classifiers)), "duplicate pre-transition classifier cell")
    forbidden_discriminators = ("zero_intent", "one_intent", "later_filled", "preview")
    require(all(not any(token in row["redis_pel_state"] for token in forbidden_discriminators) for row in operational), "callback result used as PEL discriminator")
    op = {row["id"]: row for row in operational}
    require(op["OC02"]["redis_pel_state"] == "one_exact_claimable_later_m10", "Ready claimable state drifted")
    require(op["OC02"]["accepted_proof_cells"] == "P1D4C-031|P1D4C-032|P1D4C-036|P1D4C-037|P1D4C-041|P1D4C-042", "Ready proof cells split by callback result")
    require(op["OC02"]["post_transition_outcomes"] == "ZeroIntentTerminal|OneIntentPrepublication|LaterFilledTerminal|PendingOrBlockedFailure", "post-callback outcomes drifted")
    require("Stage8bP1eClaimedM10DeliveryV1" in op["OC02"]["first_legal_transition"], "S06 delivery is not linear")
    require("without_second_acquisition" in op["OC10"]["first_legal_transition"], "S08 second acquisition opened")
    require("Stage8bP1eClaimedM10DeliveryV1" in op["OC09"]["first_legal_transition"], "source/timer row lacks linear source")
    require("s_cancel_recovered" in op["OC28"]["s06r_completion_boundary"], "cancel recovered boundary generalized")
    for row_id in ("OC15", "OC22", "OC26"):
        require("truth" in op[row_id]["s06r_completion_boundary"], f"ACK row stops before truth: {row_id}")
    require(all(row["fresh_poll_legality"] == "forbidden" for row in operational[-19:]), "unclaimable row permits fresh read")

    require(transaction.get("schema_version") == 2 and transaction.get("domain") == "moex.stage8b.p1e.first-boot-transaction.v2", "transaction V2 drifted")
    boundary = transaction.get("administrative_boundary", {})
    require(boundary.get("systemd_template_unit") == "stage8b-p1-paper-bootstrap-recover@.service", "recovery unit drifted")
    require(boundary.get("missing_credentials_directory") == "exit-66-before-filesystem-mutation", "credential failure mutation opened")
    require(boundary.get("direct_manual_execution") == "not-an-accepted-production-boundary", "direct recovery opened")
    require(boundary.get("action_grammar") == "remove-marker-temp|resume-prepared|quarantine-root|finalize-quarantine|remove-receipt-temp-and-adopt|adopt-committed-root", "recovery action grammar drifted")
    classifications = transaction.get("classifications", [])
    require(isinstance(classifications, list) and len(classifications) == 10, "transaction classifications drifted")
    class_by = {item["classification"]: item for item in classifications}
    require(class_by["CommittedRootReceiptTemp"]["required_action_selector"] == "remove-receipt-temp-and-adopt", "receipt temp recovery drifted")
    require(class_by["CommittedRootResponseLost"]["required_action_selector"] == "adopt-committed-root", "adoption selector drifted")
    require(class_by["AdoptedCommittedRoot"]["run_allowed"] is True, "adopted root cannot run")
    require(all(not item["run_allowed"] for name, item in class_by.items() if name != "AdoptedCommittedRoot"), "run opened before adoption")
    require(len(transaction.get("required_sigkill_hooks", [])) == 5, "first-boot crash hooks drifted")
    adoption = transaction.get("adoption_protocol", {})
    predicate = adoption.get("ready_owner_adoption_predicate_v1", {})
    require(predicate.get("restart_variant") == "Ready" and predicate.get("authenticated_stage5g_phase") == "TimerReady", "adoption owner predicate drifted")
    require(predicate.get("pending_lifecycle_owner_count") == 0 and predicate.get("journal_mutation_uncertain") is False, "adoption accepted unresolved owner")

    require(receipt.get("schema_version") == 1 and receipt.get("rust_type") == "Stage8bP1FirstBootReceiptV1", "receipt contract drifted")
    require(receipt.get("hmac_key") == "stage8b-p1-lifecycle.key", "receipt key drifted")
    require(receipt.get("deny_unknown_fields") is True and receipt.get("deny_duplicate_fields") is True, "receipt parser widened")
    receipt_fields = receipt.get("fields_in_canonical_order", [])
    require(len(receipt_fields) == 16 and receipt_fields[-1] == "receipt_hmac_sha256", "receipt fields drifted")
    for field in ("transaction_id_sha256", "bootstrap_attempt_generation", "canonical_root_identity_sha256", "restart_package_canonical_sha256", "first_boot_provenance_canonical_sha256", "seal_generation", "seal_commitment_sha256", "adoption_ready_owner_sha256"):
        require(field in receipt_fields, f"receipt binding missing: {field}")
    persistence = receipt.get("persistence", {})
    require(persistence.get("commit_point") == "final-rename-plus-parent-fsync", "receipt commit point drifted")
    require(len(persistence.get("protocol", [])) == 6, "receipt persistence protocol drifted")
    require(len(receipt.get("replay_rejection", [])) == 8, "receipt replay rejection drifted")

    require(package.get("schema_version") == 2 and package.get("rust_type") == "Stage6dAuthenticatedRestartPackageV2", "package V2 drifted")
    fields = package.get("package_fields_in_order", [])
    require("first_boot_provenance_v1" in fields and "first_boot_provenance_canonical_sha256" in fields, "package provenance missing")
    commitment = package.get("restart_commitment_v2", {})
    require("first_boot_provenance_canonical_sha256" in commitment.get("ordered_inputs", []), "provenance omitted from HMAC commitment")
    construction = package.get("construction", {})
    require(construction.get("provenance_regeneration_allowed") is False and "copies-exact-provenance-canonical-bytes" in construction.get("seal_advance", ""), "provenance replacement weakened")
    decode = package.get("decode_policy", {})
    require(decode.get("p1e_run_accepts_schema_versions") == [2] and decode.get("v1_downgrade_allowed") is False, "package downgrade opened")
    require(decode.get("validation_order", [])[-1] == "only-then-allow-redis-contact", "provenance validated after Redis")

    require(acquisition.get("linear_type") == "Stage8bP1eClaimedM10DeliveryV1", "linear delivery type drifted")
    require(acquisition.get("clone_allowed") is False and acquisition.get("reconstruction_allowed") is False, "delivery owner duplicable")
    forbidden = acquisition.get("forbidden_pre_transition_observations", [])
    require(all(token in forbidden for token in ("strategy_callback_result", "zero_intent", "one_intent", "later_filled", "preview_runtime_state")), "pre-transition callback discriminator opened")
    consumers = acquisition.get("consumers", {})
    for name in ("process_claimed_working_limit", "process_claimed_ready_source"):
        require(consumers[name].get("redis_acquisition_calls_allowed") == [], f"{name} may reacquire Redis source")
    retained = consumers["retain_after_shutdown_latch"]
    require(all(retained.get(key) is False for key in ("parse_allowed", "callback_allowed", "provider_allowed", "schedule_allowed", "xack_allowed")), "shutdown delivery performs semantic effect")
    require(acquisition.get("globally_forbidden_after_owner_construction") == ["XPENDING", "XAUTOCLAIM", "XREADGROUP"], "post-owner Redis acquire opened")
    require("exactly-one-real-callback" in acquisition.get("callback_rule", ""), "preview or duplicate callback opened")

    require(redis_policy.get("claim_idle_ms") == 30000, "inherited Redis policy drifted")
    require(redis_policy.get("systemd_restart", {}).get("start_limit_interval_sec") == 600, "active start limit is not 600")

    actual_hashes = {
        "active_acceptance_contract_v3_sha256": canonical_json_sha256(active),
        "r3_acceptance_matrix_file_sha256": text_sha256(r3_acceptance),
        "first_boot_transaction_v2_sha256": canonical_json_sha256(transaction),
        "first_boot_receipt_v1_sha256": canonical_json_sha256(receipt),
        "authenticated_restart_package_v2_sha256": canonical_json_sha256(package),
        "source_acquisition_seam_v1_sha256": canonical_json_sha256(acquisition),
        "operational_pretransition_matrix_v3_file_sha256": text_sha256(operational_text),
    }
    require(actual_hashes == CONTRACT_HASHES, f"R3 contract hash drift: {actual_hashes}")

    require(evidence.get("stage") == "Stage 8B-P1-e R3 deployable paper supervisor design correction", "evidence stage drifted")
    require(evidence.get("status") == "R3_DESIGN_REVIEW_CANDIDATE", "evidence status drifted")
    require(evidence.get("r2_ref") == BASE and evidence.get("r2_review_sha256") == R2_REVIEW_SHA256, "evidence R2 binding drifted")
    require(evidence.get("total_active_rows") == 160 and evidence.get("superseded_rows") == 19, "evidence active contract drifted")
    require(evidence.get("operational_pretransition_rows") == 52 and evidence.get("r3_negative_cases") == 36, "evidence matrix/negative inventory drifted")
    require(evidence.get("canonical_contracts") == CONTRACT_HASHES, "evidence contract hashes drifted")
    require(all(value == "COVERED" for value in evidence.get("review_closure", {}).values()), "review finding not covered")
    require(evidence.get("design_only") is True and evidence.get("implementation_authorized") is False, "source implementation opened")
    require(evidence.get("operational_activation_authorized") is False, "operational activation opened")
    require(all(value is False for value in evidence.get("closed_surfaces", {}).values()), "closed surface opened")
    require(evidence.get("deferred_p2", {}).get("required_bound") == "0 < child_pid <= u32::MAX", "deferred PID bound drifted")

    for token in (
        "P1-e R2 design at `3aaed81da4f1a558b4d31f4d3a169ddceca61e6f` is HOLD",
        "active candidate is P1-e R3",
        "P1-e source implementation remains unauthorized",
    ):
        require(token in status, f"status drifted: {token}")
    for token in (
        "P1-e R2 design is HOLD",
        "P1-e R3 design correction is the active candidate",
        "Only independent R3 design acceptance may open P1-e source implementation",
    ):
        require(token in roadmap, f"roadmap drifted: {token}")


def load_json(path: pathlib.Path) -> dict[str, object]:
    def reject_duplicates(pairs: list[tuple[str, object]]) -> dict[str, object]:
        result: dict[str, object] = {}
        for key, value in pairs:
            if key in result:
                raise CheckFailure(f"duplicate JSON key in {path}: {key}")
            result[key] = value
        return result
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=reject_duplicates)


def main() -> None:
    try:
        require(changed_files() == EXPECTED_CHANGED, f"R3 scope drift: {sorted(changed_files())}")
        for path, digest in INHERITED_R2_HASHES.items():
            require(file_sha256(ROOT / path) == digest, f"inherited R2 artifact drifted: {path}")
        validate(
            DESIGN.read_text(encoding="utf-8"),
            R1_ACCEPTANCE.read_text(encoding="utf-8"),
            R2_ACCEPTANCE.read_text(encoding="utf-8"),
            R3_ACCEPTANCE.read_text(encoding="utf-8"),
            load_json(ACTIVE),
            OUTER.read_text(encoding="utf-8"),
            OPERATIONAL.read_text(encoding="utf-8"),
            load_json(TRANSACTION),
            load_json(RECEIPT),
            load_json(PACKAGE),
            load_json(ACQUISITION),
            load_json(REDIS_POLICY),
            load_json(EVIDENCE),
            STATUS.read_text(encoding="utf-8"),
            ROADMAP.read_text(encoding="utf-8"),
            RECOVERY_SOURCE.read_text(encoding="utf-8"),
        )
    except (CheckFailure, OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-r3-design-check: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1e-r3-design-scope files=16 active_acceptance=160 superseded=19 restart=22 pretransition=52 design_only=true")


if __name__ == "__main__":
    main()
