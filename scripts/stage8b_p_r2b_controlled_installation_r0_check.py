#!/usr/bin/env python3
"""Fail-closed design gate for Stage 8B-P R2B controlled installation R0."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = Path("docs/stage-8")
AUTHORITY = BASE / "stage8b-p-r2b-controlled-installation-r0-authority.json"
SUPERSESSION = BASE / "stage8b-p-r2b-preproduction-supersession.json"
TRANSACTION = BASE / "stage8b-p-r2b-implementation-transaction-contract.json"
MATRIX = BASE / "STAGE8B_P_R2B_CONTROLLED_INSTALLATION_R0_ACCEPTANCE_MATRIX_2026-08-30.csv"
ACCEPTED_REF = "6672819e357a3c2a2c1e73e5408c393da01913a1"
ACCEPTED_ARCHIVE_SHA256 = "2bfb9653b71d942cdda46f7da6bc53f4f59b01e117e5475ef936f36c66c23d77"

SOURCES = [
    "trusted_clock",
    "stage7b_current_recovery_seal",
    "stage6_exact_dispatch_ready_command",
    "stage8a_root_config_policy_control",
    "composite_readiness",
    "kill_switch_run_allowed",
    "single_finam_ownership",
    "schedule",
    "instrument_specification",
    "ambiguity_orphan_unresolved_lifecycle",
    "durable_micro_budget",
]


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load(root: Path, relative: Path | str) -> dict:
    return json.loads((root / relative).read_text(encoding="utf-8"))


def expected_phases() -> list[dict]:
    producers = [f"stage8b-r2a5-producer@m8p81{index:02d}.service" for index in range(1, 12)]
    issuers = [f"stage8b-r2a5-issuer@m8i82{index:02d}.service" for index in range(1, 12)]
    return [
        {
            "phase": 1,
            "target": "moex-stage8b-r2b-phase1-current-source.target",
            "predecessor_target": None,
            "invocations": [
                "moex-stage8b-r2a8-upstream-current-authority-publisher.service",
                "moex-stage8b-r2a8-authoritative-intake-creator.service",
                "moex-stage8b-r2a8-production-intake-stager.service",
                "moex-stage8b-r2a8-production-current-source-writer.service",
            ],
        },
        {
            "phase": 2,
            "target": "moex-stage8b-r2b-phase2-manifest-source.target",
            "predecessor_target": "moex-stage8b-r2b-phase1-current-source.target",
            "invocations": [
                "stage8b-r2a8-current-manifest-issuer.service",
                "stage8b-r2a7-source-adapter.service",
            ],
        },
        {
            "phase": 3,
            "target": "moex-stage8b-r2b-phase3-authority-producers.target",
            "predecessor_target": "moex-stage8b-r2b-phase2-manifest-source.target",
            "invocations": producers,
        },
        {
            "phase": 4,
            "target": "moex-stage8b-r2b-phase4-authority-issuers.target",
            "predecessor_target": "moex-stage8b-r2b-phase3-authority-producers.target",
            "invocations": issuers,
        },
        {
            "phase": 5,
            "target": "moex-stage8b-r2b-phase5-run-package.target",
            "predecessor_target": "moex-stage8b-r2b-phase4-authority-issuers.target",
            "invocations": [
                "moex-stage8b-r2b-run-package-draft-builder.service",
                "moex-stage8b-r2b-package-issuer.service",
            ],
        },
        {
            "phase": 6,
            "target": "moex-stage8b-r2b-phase6-readonly-preflight.target",
            "predecessor_target": "moex-stage8b-r2b-phase5-run-package.target",
            "invocations": ["moex-stage8b-r2b-readonly-supervisor.service"],
        },
    ]


def required_paths(root: Path) -> set[Path]:
    authority = load(root, AUTHORITY)
    transaction = load(root, TRANSACTION)
    paths = {
        AUTHORITY,
        SUPERSESSION,
        TRANSACTION,
        MATRIX,
        BASE / "stage8b-p-r2b-runtime-composition-contract.json",
        BASE / "stage8b-p-r2b-proposal-authority.json",
        BASE / "stage8b-p-r2b-issuance-package-r0-r1-authority.json",
        BASE / "stage8b-p-r2b-implementation-r0-r1-authority.json",
        BASE / "stage8b-p-r2a5-production-trust-manifest.json",
        BASE / "stage8b-p-r2a5-production-account-key-manifest.json",
        BASE / "stage8b-p-r2b-accepted-helper-sha256.txt",
    }
    paths.update(Path(item) for item in authority["design_artifacts"])
    paths.update(Path(item) for item in transaction["unit_file_sha256"])
    return paths


def check(root: Path) -> None:
    authority = load(root, AUTHORITY)
    supersession = load(root, SUPERSESSION)
    transaction = load(root, TRANSACTION)
    runtime = load(root, BASE / "stage8b-p-r2b-runtime-composition-contract.json")
    proposal = load(root, BASE / "stage8b-p-r2b-proposal-authority.json")
    issuance = load(root, BASE / "stage8b-p-r2b-issuance-package-r0-r1-authority.json")
    implementation = load(root, BASE / "stage8b-p-r2b-implementation-r0-r1-authority.json")
    trust_path = BASE / "stage8b-p-r2a5-production-trust-manifest.json"
    account_path = BASE / "stage8b-p-r2a5-production-account-key-manifest.json"
    helper_path = BASE / "stage8b-p-r2b-accepted-helper-sha256.txt"
    trust = load(root, trust_path)
    account = load(root, account_path)

    require(authority["schema_version"] == 1, "authority schema drift")
    require(authority["stage"] == "Stage 8B-P R2B Controlled Installation / Full Transaction Proof R0", "authority stage drift")
    require(authority["status"] == "DESIGN_PACKAGE_NOT_INSTALLED_NOT_ISSUED_REVIEW_REQUIRED", "design status opened")
    require(authority["accepted_predecessor"]["source_ref"] == ACCEPTED_REF, "accepted predecessor drift")
    require(authority["accepted_predecessor"]["archive_sha256"] == ACCEPTED_ARCHIVE_SHA256, "accepted archive drift")
    require(authority["authorization"] == "NOT_ISSUED", "R2B authorization issued")
    require(all(value is False for value in authority["repository_state"].values()), "repository state opened")
    require(all(value is False for value in authority["closed_surfaces"].values()), "closed surface opened")
    require(authority["installation_scope"]["isolated_staging_only"] is True, "staging isolation absent")
    for field in (
        "production_account_host_allowed",
        "real_operator_selection_allowed",
        "real_credentials_allowed",
        "finam_network_allowed",
        "service_installation_authorized_by_design",
    ):
        require(authority["installation_scope"][field] is False, f"installation scope opened: {field}")
    for relative, digest in authority["design_artifacts"].items():
        path = root / relative
        require(path.is_file(), f"design artifact missing: {relative}")
        require(sha256(path) == digest, f"design artifact drift: {relative}")

    require(supersession["record_id"] == "stage8b-r2b-preproduction-supersession-2026-08-30-r1", "supersession identity drift")
    require(supersession["status"] == "RECORDED_NOT_INSTALLED_NOT_ISSUED", "supersession status opened")
    require(supersession["accepted_implementation"]["source_ref"] == ACCEPTED_REF, "supersession predecessor drift")
    require(supersession["accepted_implementation"]["archive_sha256"] == ACCEPTED_ARCHIVE_SHA256, "supersession archive drift")
    require(supersession["helper"]["old_executable_sha256"] == "5db401937b5e90e0237f9371a00b5af9ad2c0c3ce8e8b0899cdccafdb514578e", "old helper lineage drift")
    helper_sha = (root / helper_path).read_text(encoding="utf-8").strip()
    require(supersession["helper"]["new_executable_sha256"] == helper_sha, "new helper lineage drift")
    require(supersession["trust_set"]["new_manifest_sha256"] == sha256(root / trust_path), "new trust manifest drift")
    require(supersession["trust_set"]["new_public_key_set_sha256"] == trust["public_key_set_sha256"], "new trust set drift")
    require(supersession["account_key_manifest"]["new_manifest_sha256"] == sha256(root / account_path), "new account manifest drift")
    require(supersession["account_key_manifest"]["new_generation_1_key_sha256"] == account["entries"][0]["key_sha256"], "new account key drift")
    ceremony = supersession["ceremony_lineage"]
    require(ceremony["classification"] == "PRE_PRODUCTION_INITIAL_REBIND", "ceremony classification drift")
    require(ceremony["same_generation_does_not_assert_key_continuity"] is True, "generation ambiguity restored")
    require(ceremony["distinct_ceremony_id_required_for_future_installation"] is True, "future ceremony identity lost")
    require(ceremony["production_installation_before_supersession"] is False, "prior installation asserted")
    require(ceremony["issued_r2b_packages_before_supersession"] == 0, "prior package issuance asserted")
    require(ceremony["real_credentials_materialized_before_supersession"] is False, "prior credentials asserted")
    require(ceremony["finam_requests_before_supersession"] == 0, "prior FINAM request asserted")
    filesystem = supersession["filesystem_contract"]
    require(filesystem["new_unsigned_draft"] == runtime["phase5_phase6_filesystem_contract"]["unsigned_draft"], "unsigned path drift")
    require(filesystem["new_signed_package"] == runtime["phase5_phase6_filesystem_contract"]["signed_package"], "signed path drift")
    require(filesystem["new_package_signer_credential_root"] == runtime["phase5_phase6_filesystem_contract"]["package_signer_credentials"], "signer credential path drift")
    require(filesystem["new_supervisor_credential_root"] == runtime["phase5_phase6_filesystem_contract"]["supervisor_credentials"], "supervisor credential path drift")
    require(filesystem["legacy_paths_authoritative"] is False, "legacy paths restored")
    require(supersession["authorization"] == "NOT_ISSUED" and supersession["installation_allowed_by_this_record"] is False, "supersession became authorization")

    require(transaction["contract_id"] == "stage8b-r2b-full-31-service-transaction-r0", "transaction identity drift")
    require(transaction["status"] == "DESIGN_ONLY_NOT_INSTALLED_NOT_ISSUED", "transaction status opened")
    require(transaction["accepted_implementation_ref"] == ACCEPTED_REF, "transaction predecessor drift")
    relation = transaction["relationship_to_runtime_composition"]
    require(relation["runtime_contract_scope"] == "POST_DRAFT_EXECUTION_COMPOSITION", "runtime contract scope drift")
    require(relation["this_contract_scope"] == "FULL_IMPLEMENTATION_TRANSACTION_INCLUDING_DRAFT_BUILDER", "full transaction scope drift")
    require(relation["runtime_helper_rebuild_required"] is False, "unnecessary helper rebuild introduced")
    require(relation["runtime_contract_sha256"] == sha256(root / relation["runtime_contract_path"]), "runtime contract binding drift")
    require(transaction["source_instances"] == SOURCES, "source instance order drift")
    require(transaction["phases"] == expected_phases(), "exact phase graph drift")
    require(transaction["phase_count"] == 6, "phase count drift")
    require(sum(len(phase["invocations"]) for phase in transaction["phases"]) == 31, "resolved invocation count drift")
    require(transaction["service_invocation_count"] == 31, "declared invocation count drift")
    require(transaction["aggregate_target"] == "moex-stage8b-r2b-issuance.target", "aggregate target drift")
    for relative, digest in transaction["unit_file_sha256"].items():
        require(sha256(root / relative) == digest, f"unit binding drift: {relative}")

    receipt_sources = issuance["package_formation"]["builder"]["receipt_sources"]
    require([item["source"] for item in receipt_sources] == SOURCES, "issuance source order drift")
    require([item["producer_service"] for item in receipt_sources] == expected_phases()[2]["invocations"], "producer instance drift")
    require([item["issuer_service"] for item in receipt_sources] == expected_phases()[3]["invocations"], "issuer instance drift")
    require(set(trust["source_keys"]) == set(SOURCES), "trust source inventory drift")

    production = transaction["production_linux_amd64_sha256"]
    proposal_production = proposal["production_composition"]["production_linux_amd64_sha256"]
    for name, digest in proposal_production.items():
        require(production.get(name) == digest, f"proposal binary binding drift: {name}")
    require(production["stage8b-r2b-run-package-draft-builder"] == implementation["production_linux_amd64_sha256"]["stage8b-r2b-run-package-draft-builder"], "builder binary drift")
    for name, digest in implementation["production_linux_amd64_sha256"].items():
        require(production.get(name) == digest, f"implementation binary binding drift: {name}")
    require(implementation["status"] == "IMPLEMENTED_NOT_INSTALLED_NOT_ISSUED_REVIEW_REQUIRED", "accepted implementation status drift")
    require(all(value is False for value in implementation["repository_state"].values()), "accepted implementation state opened")

    proof = transaction["proof_requirements"]
    for field in ("isolated_staging_only", "canary_or_offline_credentials_only", "success_graph_required", "failure_graph_required", "replay_graph_required", "transaction_reset_proof_required", "post_proof_uninstall_required"):
        require(proof[field] is True, f"proof requirement absent: {field}")
    for field in ("production_account_host_allowed", "real_operator_selection_allowed", "finam_network_allowed"):
        require(proof[field] is False, f"proof scope opened: {field}")
    require(all(value is False for value in transaction["closed_surfaces"].values()), "transaction surface opened")

    with (root / MATRIX).open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 24 and len({row["id"] for row in rows}) == 24, "acceptance matrix inventory drift")
    require(all(row["status"] == "PASS" for row in rows), "acceptance matrix incomplete")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    check(args.root.resolve())
    print(
        "stage8b-p-r2b-controlled-installation-r0-check: PASS "
        "supersession=recorded graph=31 phases=6 scope=isolated-staging "
        "installed=false authorization=NOT_ISSUED finam=false"
    )


if __name__ == "__main__":
    try:
        main()
    except (KeyError, OSError, RuntimeError, ValueError) as error:
        raise SystemExit(f"stage8b-p-r2b-controlled-installation-r0-check: FAIL {error}") from error
