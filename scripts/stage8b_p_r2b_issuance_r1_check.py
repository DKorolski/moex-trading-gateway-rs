#!/usr/bin/env python3
"""Fail-closed checker for Stage 8B-P R2B issuance-package R0-R1."""

from __future__ import annotations

import csv
import hashlib
import json
from pathlib import Path

import stage8b_p_r2b_read_contract_refresh as read_refresh


ROOT = Path(__file__).resolve().parents[1]
AUTHORITY = ROOT / "docs/stage-8/stage8b-p-r2b-issuance-package-r0-r1-authority.json"
MATRIX = ROOT / "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_R1_ACCEPTANCE_MATRIX_2026-08-29.csv"
DESIGN = ROOT / "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_R1_2026-08-29.md"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p-r2b-issuance-package-r0-r1-evidence.json"
ABSENT_IMPLEMENTATION = (
    "deploy/stage8b-r2b/moex-stage8b-r2b-issuance.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase1-current-source.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase2-manifest-source.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase3-authority-producers.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase4-authority-issuers.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase5-run-package.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase6-readonly-preflight.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-run-package-draft-builder.service",
    "deploy/stage8b-r2b/moex-stage8b-r2b-package-issuer.service",
    "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-run-package-draft-builder.rs",
)
SNAPSHOT_SHA = "7c8e6bcd02f907af93ea1386499d03bff194da76a1eb2b19dd9c2ff1f97403c5"


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def check(root: Path) -> None:
    read_refresh.verify_evidence(root)
    authority = json.loads((root / AUTHORITY.relative_to(ROOT)).read_text(encoding="utf-8"))
    evidence = json.loads((root / EVIDENCE.relative_to(ROOT)).read_text(encoding="utf-8"))
    rows = list(csv.DictReader((root / MATRIX.relative_to(ROOT)).open(encoding="utf-8")))
    design = " ".join((root / DESIGN.relative_to(ROOT)).read_text(encoding="utf-8").split())

    require(authority["schema_version"] == 1 and authority["revision"] == "R0-R1", "revision drift")
    require(authority["status"] == "DESIGN_CLOSURE_CANDIDATE_NOT_ISSUED", "status opened")
    require(authority["accepted_predecessor"] == {
        "stage": "Stage 8B-P R2B R4-R2A Acceptance Closure",
        "source_ref": "f24f1044ac0b29c2f588853b817e519cfe8d3d8b",
        "verdict": "ACCEPTED",
    }, "accepted predecessor drift")
    require(authority["corrects_r0"]["source_ref"] == "928168ed47e5b9dd873cd73815fbccecde7a8981", "R0 binding drift")
    require(authority["corrects_r0"]["finding_count"] == 2, "finding count drift")

    contract = authority["read_contract"]
    require(contract["snapshot_path"] == read_refresh.SNAPSHOT.as_posix(), "read snapshot path drift")
    require(contract["snapshot_sha256"] == SNAPSHOT_SHA, "read snapshot hash drift")
    require(contract["helper_embedded_snapshot_sha256"] == SNAPSHOT_SHA, "helper snapshot binding drift")
    require(contract["future_run_package_contract_snapshot_sha256"] == SNAPSHOT_SHA, "run-package snapshot binding drift")
    require(contract["document_names"] == list(read_refresh.DOCUMENTS), "read document inventory drift")
    require(contract["document_count"] == 6, "read document count drift")
    require(contract["effect_contract_snapshot_sufficient_for_r2b"] is False, "effect contract substituted")
    require(contract["activation_refresh_required"] is True, "activation refresh disabled")
    require(contract["activation_max_age_seconds"] == 1800, "activation refresh age drift")

    formation = authority["package_formation"]
    require(formation["selected_model"] == "SEPARATE_DRAFT_BUILDER_THEN_SIGNER", "draft model drift")
    builder = formation["builder"]
    require(builder["executable"] == "stage8b-r2b-run-package-draft-builder", "builder executable drift")
    require(builder["service"] == "moex-stage8b-r2b-run-package-draft-builder.service", "builder service drift")
    require(builder["implemented_by_r0_r1"] is False, "builder prematurely implemented")
    require(builder["uid"] == builder["gid"] == 0, "builder identity drift")
    require(builder["signing_key_access"] is False and builder["credential_path_access"] is False, "builder secret access opened")
    require(builder["network_allowed"] is False, "builder network opened")
    require(len(builder["fixed_inputs"]) == 7, "builder fixed-input inventory drift")
    require(builder["required_receipt_count"] == 11, "receipt count drift")
    require(builder["required_same_run_nonce"] is True, "mixed nonce allowed")
    require(builder["stale_receipts_allowed"] is False, "stale receipts allowed")
    require(builder["controlled_fixture_producer_allowed"] is False, "controlled producer allowed")
    require(builder["schema"] == "R2a5RunPackage", "draft schema drift")
    require(builder["signature_ed25519_hex"] == "EMPTY", "draft unexpectedly signed")
    require(builder["validity_seconds"] == 30, "draft validity drift")
    output = builder["output"]
    require(output["path"] == "/var/lib/moex-trading/stage8b/r2a5/r2b-run-package.unsigned.json", "unsigned path drift")
    require(output["owner_uid"] == output["owner_gid"] == 0 and output["mode"] == "0600", "unsigned custody drift")
    require(all(output[key] is True for key in ("atomic_publish", "nofollow_regular_single_link", "file_fsync", "directory_fsync")), "unsigned durability drift")
    require(output["existing_output_reuse_allowed"] is False, "stale unsigned output allowed")
    signer = formation["signer"]
    require(signer["executable"] == "stage8b-r2a5-package-issuer", "signer drift")
    require(signer["fixed_input"] == output["path"], "signer input drift")
    require(signer["only_component_with_package_authorization_key"] is True, "signing authority spread")
    require(signer["requires_current_run_unsigned_draft"] is True, "signer accepts stale draft")

    activation = authority["future_activation_target"]
    require(activation["unit"] == "moex-stage8b-r2b-issuance.target", "aggregate target drift")
    require(all(activation[key] is False for key in ("implemented_by_r0_r1", "installed_by_r0_r1", "enabled_by_r0_r1", "manual_start_allowed")), "activation opened")
    require(activation["signed_local_activation_required"] is True, "signed activation requirement removed")
    require(all(not (root / relative).exists() for relative in ABSENT_IMPLEMENTATION), "implementation artifact present in design closure")

    transaction = authority["transaction"]
    phases = transaction["phases"]
    require(transaction["phase_count"] == len(phases) == 6, "phase count drift")
    require([phase["ordinal"] for phase in phases] == list(range(1, 7)), "phase ordinal drift")
    services = [service for phase in phases for service in phase["services"]]
    require(transaction["service_invocation_count"] == len(services) == 31, "service cardinality drift")
    require(len(set(services)) == 31, "duplicate service invocation")
    require(len(phases[2]["services"]) == len(phases[3]["services"]) == 11, "authority fanout drift")
    require(phases[4]["services"] == [builder["service"], signer["service"]], "phase 5 order drift")
    require(phases[5]["services"] == ["moex-stage8b-r2b-readonly-supervisor.service"], "terminal supervisor drift")
    targets = [phase["target"] for phase in phases]
    require(len(set(targets)) == 6 and all(target.startswith("moex-stage8b-r2b-phase") for target in targets), "phase target drift")
    require(phases[0]["after_target"] is None, "phase 1 predecessor drift")
    require(all(phases[index]["after_target"] == phases[index - 1]["target"] for index in range(1, 6)), "phase edge drift")
    barriers = transaction["barrier_contract"]
    require(all(barriers[key] is True for key in (
        "phase_target_requires_all_phase_services", "phase_target_after_all_phase_services",
        "downstream_requires_previous_phase_target", "downstream_after_previous_phase_target",
        "failed_component_blocks_downstream", "skipped_component_blocks_downstream",
        "same_run_nonce_required", "package_issuer_requires_draft_builder",
        "supervisor_requires_package_issuer",
    )), "required barrier opened")
    require(barriers["condition_skip_semantics_allowed"] is False, "condition skip allowed")
    require(barriers["partial_fanout_allowed"] is False, "partial fanout allowed")
    require(barriers["stale_output_allowed"] is False, "stale output allowed")

    state = authority["implementation_state"]
    require(all(value is False for value in state.values()), "implementation state opened")
    require(all(str(value).startswith("ABSENT") for value in authority["operator_local_inputs"].values()), "operator input invented")
    require(authority["authorization"] == {
        "r2b": "NOT_ISSUED", "operator_arm_issued": False,
        "activation_authority_present": False, "target_start_allowed": False,
    }, "authorization opened")
    require(all(value is False for value in authority["closed_surfaces"].values()), "effect surface opened")
    require(len(rows) == authority["acceptance_rows"] == 40, "acceptance row count drift")
    require(all(row["expected"] == "PASS" for row in rows), "acceptance expectation drift")
    require(authority["targeted_negative_mutations"] == 25, "negative count drift")
    require(evidence["status"] == "DESIGN_CLOSURE_CANDIDATE_NOT_ISSUED", "evidence status drift")
    require(evidence["read_contract"]["document_count"] == 6, "evidence contract count drift")
    require(evidence["read_contract"]["snapshot_sha256"] == SNAPSHOT_SHA, "evidence contract binding drift")
    refresh_path = root / read_refresh.EVIDENCE
    require(evidence["read_contract"]["fresh_refresh_evidence_sha256"] == sha256(refresh_path.read_bytes()), "refresh evidence digest drift")
    require(evidence["package_formation"]["model"] == "SEPARATE_DRAFT_BUILDER_THEN_SIGNER", "evidence builder model drift")
    require(evidence["package_formation"]["service_invocations"] == 31, "evidence service count drift")
    require(evidence["acceptance_rows"] == "40/40" and evidence["targeted_negative_mutations"] == "25/25", "evidence coverage drift")
    require(evidence["production_rust_or_cargo_changed"] is False, "evidence production drift")
    require(evidence["activation_target_implemented"] is False, "evidence target opened")
    require(evidence["authorization_status"] == "NOT_ISSUED", "evidence authorization opened")
    for key in (
        "finam_credentials_accessed", "auth_service_called", "broker_account_get_sent",
        "order_post_sent", "order_delete_sent", "dispatch_attempt_recorded",
        "transport_entered", "redis_live_consumer", "broker_dispatch", "runtime_live",
        "strategy_live", "real_orders",
    ):
        require(evidence[key] is False, f"evidence surface opened: {key}")
    for marker in (
        "31 service invocations", "separate future", "no signing key",
        "1,800 seconds", "failed, skipped or missing", "NOT_ISSUED",
    ):
        require(marker in design, f"design marker missing: {marker}")


def main() -> None:
    check(ROOT)
    print(
        "stage8b-p-r2b-issuance-r1-check: PASS revision=R0-R1 rows=40 "
        "read_documents=6 services=31 phases=6 negatives=25 builder=SEPARATE "
        "target_implemented=false authorization=NOT_ISSUED finam=false broker_get=false"
    )


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError, KeyError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2b-issuance-r1-check: FAIL {error}") from error
