#!/usr/bin/env python3
"""Fail-closed checker for Stage 8B-P R2B Proposal R4-R2."""

from __future__ import annotations

import csv
import hashlib
import json
import re
from pathlib import Path

import stage8b_p_r2b_systemd_unit_check as systemd_unit_check

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "docs/stage-8"
HEX64 = re.compile(r"[0-9a-f]{64}")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def load(relative: str) -> object:
    return json.loads((ROOT / relative).read_text(encoding="utf-8"))


def text(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def exact_hash(value: object) -> bool:
    return isinstance(value, str) and HEX64.fullmatch(value) is not None


def require_all(source: str, markers: tuple[str, ...], area: str) -> None:
    for marker in markers:
        require(marker in source, f"{area} missing: {marker}")


def main() -> None:
    systemd_unit_check.check(ROOT)
    authority = load("docs/stage-8/stage8b-p-r2b-proposal-authority.json")
    runtime_path = ROOT / "docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json"
    runtime_bytes = runtime_path.read_bytes()
    runtime = json.loads(runtime_bytes)
    build = load("docs/stage-8/stage8b-p-r2b-r4-build-evidence.json")
    systemd_evidence = load(
        "docs/stage-8/stage8b-p-r2b-r4-r2a-systemd-verify-evidence.json"
    )
    helper_sha = text("docs/stage-8/stage8b-p-r2b-accepted-helper-sha256.txt").strip()
    proposal = text("docs/stage-8/STAGE8B_P_R2B_PROPOSAL_2026-08-27.md")
    status = text("docs/current-status.md")
    adapter = text("crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs")
    capability = text("crates/finam-gateway/src/stage8a1_execution_capability.rs")
    gateway_lib = text("crates/finam-gateway/src/lib.rs")
    gateway_cargo = text("crates/finam-gateway/Cargo.toml")
    stager_bin = text("crates/finam-gateway/src/bin/stage8b-r2a8-production-intake-stager.rs")
    creator_bin = text("crates/finam-gateway/src/bin/stage8b-r2a8-authoritative-intake-creator.rs")
    publisher_bin = text("crates/finam-gateway/src/bin/stage8b-r2a8-upstream-current-authority-publisher.rs")
    creator_chain_seeder = text("crates/finam-gateway/src/bin/stage8b-r2b-creator-chain-seeder.rs")
    creator_unit = text("deploy/stage8b-r2b/moex-stage8b-r2a8-authoritative-intake-creator.service")
    publisher_unit = text("deploy/stage8b-r2b/moex-stage8b-r2a8-upstream-current-authority-publisher.service")
    stager_unit = text("deploy/stage8b-r2b/moex-stage8b-r2a8-production-intake-stager.service")
    writer_unit = text("deploy/stage8b-r2b/moex-stage8b-r2a8-production-current-source-writer.service")
    supervisor_unit = text("deploy/stage8b-r2b/moex-stage8b-r2b-readonly-supervisor.service")
    writer_bin = text("crates/finam-gateway/src/bin/stage8b-r2a8-production-current-source-writer.rs")
    launcher = text("tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs")
    old_launcher = text("tools/stage8b-readonly-preflight/src/bin/stage8b-r2a5-launcher.rs")
    helper_lib = text("tools/stage8b-readonly-preflight/src/lib.rs")
    pipeline = text("tools/stage8b-readonly-preflight/src/r2a3.rs")
    helper = text("tools/stage8b-readonly-preflight/src/r2a5.rs")
    rehearsal = text("scripts/stage8b_p_r2b_r3_linux_custody_rehearsal.sh")
    gate = text("scripts/stage8b_p_r2b_proposal_gate.sh")
    handoff_maker = text("scripts/make_stage8b_p_r2b_handoff.py")
    handoff_safety = text("scripts/stage8b_p_r2b_handoff_safety_check.py")

    require(authority["schema_version"] == 1, "schema drift")
    require(authority["stage"] == "Stage 8B-P R2B" and authority["revision"] == "R4-R2", "stage/revision drift")
    require(authority["status"] == "PROPOSAL_ONLY_NOT_AUTHORIZED", "proposal status opened")
    require(authority["authorization_status"] == "NOT_ISSUED", "R2B authorization issued")
    require(authority["accepted_predecessor"]["source_ref"] == "5b2079d7d524d2fa6f084f44f961c4b5958c042a", "predecessor drift")
    roots = authority["external_accepted_roots"]
    require(len(roots) == 1, "external-root cardinality drift")
    external_c0 = roots[0]
    require(
        external_c0["root_id"] == "stage8b-p-r2a8-r1-trusted-current-source-v2"
        and external_c0["producer_stage"] == "Stage 8B-P R2A8-R1"
        and external_c0["accepted_commit"] == "5b2079d7d524d2fa6f084f44f961c4b5958c042a"
        and external_c0["artifact_role"] == "accepted_external_current_source_C0",
        "external C0 identity drift",
    )
    for field in (
        "required_signature_valid", "required_readiness_valid",
        "required_freshness_valid", "required_exact_config_binding",
        "required_exact_durable_request_binding",
    ):
        require(external_c0[field] is True, f"external C0 requirement absent: {field}")
    require(external_c0["max_age_seconds"] == 30, "external C0 maximum age drift")
    for field in ("caller_supplied", "manual_operator_artifact", "controlled_production_component"):
        require(external_c0[field] is False, f"external C0 forbidden source opened: {field}")

    roles = authority["temporal_current_source_roles"]
    c0 = roles["accepted_external_current_source_C0"]
    c1 = roles["r2b_refreshed_current_source_C1"]
    require(c0["produced_by_r2b_owned_sequence"] is False, "C0 became R2B output")
    require(c0["consumed_by"] == "stage8b-r2a8-upstream-current-authority-publisher", "C0 consumer drift")
    require(c1["produced_by_r2b_owned_sequence"] is True, "C1 ceased to be R2B output")
    require(c1["produced_by"] == "stage8b-r2a8-production-current-source-writer", "C1 producer drift")
    require(roles["causal_sequence"] == "C0 -> R2B publisher/creator/stager/writer -> C1", "C0/C1 causal sequence drift")
    require(roles["same_generation_cycle"] is False, "C0/C1 role collapse")
    require(roles["physical_path_reuse_requires_atomic_replacement_and_chronology"] is True, "C0/C1 chronology drift")

    qualification = authority["qualification_rehearsal_boundary"]
    require(qualification["r2b_owned_roots_start_empty"] is True, "R2B roots not empty")
    require(qualification["c0_materialized_by_controlled_qualification_fixture"] is True, "C0 fixture role drift")
    require(qualification["post_boundary_sequence_uses_exact_production_binaries"] is True, "post-C0 production sequence drift")
    require(qualification["controlled_component_after_external_c0_boundary"] is False, "controlled post-C0 component entered")
    require(qualification["controlled_component_in_production_composition"] is False, "controlled component entered production")
    require(build["stage"] == "Stage 8B-P R2B Proposal R4-R2", "build stage drift")
    for field in (
        "production_upstream_publisher_rehearsal",
        "production_source_chain_rehearsal",
        "post_c0_empty_r2b_root_rehearsal",
        "stale_upstream_rejected",
        "upstream_refresh_rehearsal",
        "empty_root_generation_one_rehearsal",
        "generation_two_renewal_rehearsal",
        "expired_predecessor_continuity_policy",
    ):
        require(build[field] == "PASS", f"bootstrap evidence absent: {field}")

    proposed = authority["proposed_capability"]
    require(proposed["one_shot"] and proposed["operation_choices"] == ["PLACE", "CANCEL"], "one-shot drift")
    require(proposed["selection_count"] == 1, "selection count drift")
    require(not proposed["background_loop"] and not proposed["unattended_execution"], "unattended surface opened")
    require(proposed["result_may_influence_execution"] is False, "evidence became authority")

    network = authority["network_contract"]
    require(network["scheme"] == "https" and network["exact_host"] == "api.finam.ru", "endpoint drift")
    require(network["outbound_destinations"] == ["api.finam.ru:443"], "allowlist drift")
    for field in ("dns_or_ip_rebinding_allowed", "redirects_allowed", "proxy_allowed", "automatic_retries_allowed", "order_post_allowed", "order_delete_allowed", "arbitrary_request_allowed"):
        require(network[field] is False, f"network closure drift: {field}")

    sequence = [
        "stage8b-r2a8-upstream-current-authority-publisher",
        "stage8b-r2a8-authoritative-intake-creator",
        "stage8b-r2a8-production-intake-stager",
        "stage8b-r2a8-production-current-source-writer",
        "stage8b-r2a8-current-manifest-issuer",
        "stage8b-r2a7-source-adapter",
        "stage8b-r2a5-authority-producer",
        "stage8b-r2a5-authority-issuer",
        "stage8b-r2a5-package-issuer",
        "stage8b-r2b-launcher",
        "accepted-stage8b-readonly-preflight",
    ]
    composition = authority["production_composition"]
    require(composition["exact_executable_sequence"] == sequence, "exact sequence drift")
    expected_cardinality = {name: (11 if name in {"stage8b-r2a5-authority-producer", "stage8b-r2a5-authority-issuer"} else 1) for name in sequence}
    require(composition["exact_invocation_cardinality"] == expected_cardinality, "cardinality drift")
    require(runtime["revision"] == "R4-R2" and runtime["exact_component_sequence"] == sequence, "runtime sequence drift")
    require(runtime["authorization_status"] == "NOT_ISSUED", "runtime contract authorized")
    require(all(value is False for value in runtime["closed_surfaces"].values()), "runtime surface opened")
    embedded = composition["embedded_runtime_composition_contract"]
    require(embedded["sha256"] == hashlib.sha256(runtime_bytes).hexdigest(), "runtime contract binding drift")
    require(not embedded["contains_executable_hashes"] and embedded["hash_cycle_prevented"], "hash cycle drift")

    publisher = authority["upstream_current_authority_publisher"]
    require(publisher["executable"] == sequence[0], "publisher is not first")
    require(publisher["uid"] == publisher["gid"] == 8095, "publisher identity drift")
    require(publisher["supplementary_groups"] == [], "publisher supplementary-group drift")
    require(
        [publisher[name] for name in (
            "authority_root_owner_uid", "authority_root_gid", "signing_key_owner_uid", "signing_key_gid"
        )] == [8095, 8094, 8095, 8095]
        and publisher["authority_root_mode"] == "0750"
        and publisher["signing_key_mode"] == "0600",
        "publisher authority custody drift",
    )
    require(publisher["invocation_cardinality"] == 1, "publisher invocation drift")
    for field in (
        "caller_arguments_allowed", "caller_json_allowed", "caller_readiness_allowed",
        "caller_broker_truth_allowed", "caller_broker_readiness_allowed",
        "caller_timestamps_allowed", "caller_paths_allowed", "network_access_allowed",
        "finam_credential_access_allowed", "order_post_delete_authority",
        "redis_access_allowed", "runtime_live_authority",
    ):
        require(publisher[field] is False, f"publisher boundary opened: {field}")
    for field in ("atomic_write", "file_fsync", "directory_fsync", "recovered_owner_required", "opaque_current_sources_required"):
        require(publisher[field] is True, f"publisher property absent: {field}")
    require_all(publisher_bin, (
        "std::env::args_os().len() != 1",
        "run_stage8b_r2a8_upstream_current_authority_publisher",
        "upstream current-authority publisher accepts no arguments",
    ), "publisher binary")
    require_all(publisher_unit, (
        "User=8095", "Group=8095",
        "ExecStart=/opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-upstream-current-authority-publisher",
        "RestrictAddressFamilies=AF_UNIX", "IPAddressDeny=any",
        "RefuseManualStart=yes",
        "Before=moex-stage8b-r2a8-authoritative-intake-creator.service",
    ), "publisher unit")
    shared_owner_restore_body = adapter.split("fn restore_stage7b_owner_from_fixed_layout", 1)[1].split("/// Fixed-input production publisher", 1)[0]
    require_all(shared_owner_restore_body, (
        "read_lifecycle_key_file(", "Stage7bRecoveryReadyOwner::restart(",
        "Stage7bDurableRootAuthority::validate(", "fixed_runtime_profile(",
    ), "shared fixed-layout owner restore")
    require(adapter.count("restore_stage7b_owner_from_fixed_layout(") == 3, "shared owner-restore cardinality drift")
    publisher_body = adapter.split("pub fn run_stage8b_r2a8_upstream_current_authority_publisher", 1)[1].split("pub(crate) fn create_stage8b_r2a8_owner_signed_intake_from_owner", 1)[0]
    require_all(publisher_body, (
        "read_fixed_regular_file(", "validate_trusted_current_source(&current_source, mode)",
        "restore_stage7b_owner_from_fixed_layout(", ".single_exact_dispatch_ready_request()",
        "Stage8a1OperationalAuthorityIssuer::from_stage7b_owner(", ".issue_current_sources(",
        "publish_stage8b_r2a8_upstream_current_authority_from_owner(",
        "PRODUCTION_UPSTREAM_CURRENT_AUTHORITY_LOCK_FILE", "create_new(true)",
        "caller_supplied_input_accepted: false", "caller_supplied_timestamp_accepted: false",
        "network_accessed: false", "finam_credential_accessed: false",
    ), "production publisher call graph")
    require("reqwest" not in publisher_body and "PRODUCTION_CREDENTIALS" not in publisher_body, "publisher gained network/credentials")
    require(adapter.count("publish_stage8b_r2a8_upstream_current_authority_from_owner(") == 2, "publisher call-site cardinality drift")
    require('name = "stage8b-r2a8-upstream-current-authority-publisher"' in gateway_cargo, "publisher Cargo target absent")
    require("--bin stage8b-r2a8-upstream-current-authority-publisher" in build["production_build_command"], "publisher absent from production build")
    require("stage8b-r2a8-upstream-current-authority-publisher" in build["production_binaries"], "publisher production hash absent")
    require(exact_hash(composition["production_linux_amd64_sha256"][sequence[0]]), "publisher machine hash absent")
    require_all(creator_unit, (
        "Requires=moex-stage8b-r2a8-upstream-current-authority-publisher.service",
        "After=local-fs.target moex-stage8b-r2a8-upstream-current-authority-publisher.service",
    ), "creator publisher dependency")

    creator = authority["authoritative_intake_creator"]
    require(creator["executable"] == sequence[1] and creator["uid"] == creator["gid"] == 8095, "creator identity drift")
    require(creator["supplementary_groups"] == [], "creator supplementary-group drift")
    require(
        [creator[name] for name in (
            "authority_root_owner_uid", "authority_root_gid", "signing_key_owner_uid", "signing_key_gid"
        )] == [8095, 8094, 8095, 8095]
        and creator["authority_root_mode"] == "0750"
        and creator["signing_key_mode"] == "0600",
        "creator authority custody drift",
    )
    require(creator["invocation_cardinality"] == 1, "creator invocation drift")
    require(creator["empty_root_generation_one_supported"] is True, "empty-root bootstrap absent")
    require(creator["bootstrap_predecessor_required"] is False, "creator still requires predecessor")
    require(creator["predecessor_snapshot_source"] is False, "predecessor remains snapshot source")
    require(creator["expired_predecessor_policy"] == "signature_and_continuity_only", "expired predecessor policy drift")
    require(creator["production_bootstrap_uses_controlled_seeder"] is False, "controlled seeder entered production bootstrap")
    for field in ("caller_arguments_allowed", "caller_json_allowed", "caller_readiness_allowed", "caller_broker_truth_allowed", "caller_broker_readiness_allowed", "caller_timestamps_allowed", "network_access_allowed", "finam_credential_access_allowed", "runtime_live_authority"):
        require(creator[field] is False, f"creator boundary opened: {field}")
    for field in ("atomic_write", "file_fsync", "directory_fsync"):
        require(creator[field] is True, f"creator durability absent: {field}")
    creator_body = adapter.split("pub(crate) fn create_stage8b_r2a8_owner_signed_intake_from_owner", 1)[1].split("pub fn run_stage8b_r2a8_production_current_source_writer", 1)[0]
    require_all(creator_body, (
        "Stage7bRecoveryReadyOwner", "Stage8a1TrustedCurrentSources",
        ".single_exact_dispatch_ready_request()", ".stage8b_r2a8_current_snapshots(issuer)",
        "sign_stage8b_r2a8_current_source_commitment", "create_new(true)",
        "atomic_write_fixed(", "network_accessed: false", "finam_credential_accessed: false",
    ), "authoritative creator")
    require("reqwest" not in creator_body and "PRODUCTION_CREDENTIALS" not in creator_body, "creator gained network/credentials")
    require("pub(crate) fn stage8b_r2a8_current_snapshots" in capability, "opaque source extraction absent")
    require_all(adapter, (
        "pub fn run_stage8b_r2a8_authoritative_intake_creator(",
        "create_stage8b_r2a8_owner_signed_intake_from_owner(",
        "publish_stage8b_r2a8_upstream_current_authority_from_owner(",
        "Stage8bR2a8UpstreamCurrentAuthorityV1",
        "validate_production_writer_intake_continuity",
        "identity != upstream.durable_request_identity || command != upstream.durable_command",
        "predecessor_intake_commitment_sha256: Option<String>",
        "predecessor_used_as_snapshot_source: false",
        ".and_then(|file| file.sync_all())",
        "expired_predecessor_is_continuity_only_while_fresh_intake_requires_freshness",
    ), "reachable authoritative creator")
    require("allow(dead_code" not in adapter.split("create_stage8b_r2a8_owner_signed_intake_from_owner", 1)[0][-256:], "creator dead-code exemption retained")
    require_all(creator_bin, (
        "std::env::args_os().len() != 1",
        "run_stage8b_r2a8_authoritative_intake_creator",
    ), "creator binary")
    require_all(creator_unit, (
        "User=8095", "Group=8095",
        "ExecStart=/opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-authoritative-intake-creator",
        "RestrictAddressFamilies=AF_UNIX", "IPAddressDeny=any",
        "RefuseManualStart=yes",
        "Before=moex-stage8b-r2a8-production-intake-stager.service",
    ), "creator unit")
    require('name = "stage8b-r2a8-authoritative-intake-creator"' in gateway_cargo, "creator Cargo target absent")
    require(
        "--bin stage8b-r2a8-authoritative-intake-creator" in build["production_build_command"],
        "creator absent from production build command",
    )
    require_all(creator_chain_seeder, (
        "seed_stage8b_r2b_creator_chain_qualification",
        "creator-chain qualification seeder accepts no arguments",
    ), "creator-chain qualification seeder")
    require_all(rehearsal, (
        "stage8b-r2b-r4-r2-post-c0-empty-r2b-root-chain",
        '"$UPSTREAM_PUBLISHER"',
        "empty_root_generation_one",
        "predecessor_continuity_renewal",
        "predecessor_used_as_snapshot_source",
        '"$CURRENT_SOURCE_WRITER"',
        '"$PRODUCTION_MANIFEST_ISSUER"',
        '"$PRODUCTION_SOURCE_ADAPTER"',
        "source_chain=true",
    ), "empty-root and renewal rehearsal")
    require('name = "stage8b-r2b-creator-chain-seeder"' in gateway_cargo, "creator-chain qualification target absent")
    require(
        "--bin stage8b-r2b-creator-chain-seeder" in build["controlled_build_command"],
        "creator-chain seeder absent from controlled build command",
    )

    stager = authority["production_intake_stager"]
    require(stager["executable"] == sequence[2] and not stager["creates_or_signs_authority"], "stager misrepresented")
    require(stager["systemd_unit"].endswith("production-intake-stager.service"), "stager unit binding absent")
    require(stager["requires_creator_unit"] == "moex-stage8b-r2a8-authoritative-intake-creator.service", "stager creator dependency drift")
    require(stager["authority_root_traverse_gid"] == 8094 and stager["private_signing_key_readable"] is False, "stager custody drift")
    require("std::env::args_os().len() != 1" in stager_bin, "stager accepts arguments")
    require("run_stage8b_r2a8_production_intake_stager" in stager_bin, "stager detached")
    require_all(stager_unit, (
        "Requires=moex-stage8b-r2a8-authoritative-intake-creator.service",
        "After=moex-stage8b-r2a8-authoritative-intake-creator.service",
        "User=8094", "Group=8094",
        "ExecStart=/opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-production-intake-stager",
        "RestrictAddressFamilies=AF_UNIX", "IPAddressDeny=any",
        "RefuseManualStart=yes",
    ), "stager unit")
    stager_body = adapter.split("pub fn run_stage8b_r2a8_production_intake_stager", 1)[1].split("pub(crate) fn publish_stage8b_r2a8_trusted_current_source_from_owner", 1)[0]
    require_all(stager_body, ("read_fixed_regular_file(", "validate_production_writer_intake(", "atomic_write_fixed("), "stager")
    require("sign_stage8b" not in stager_body, "stager gained signing authority")
    require('name = "stage8b-r2a8-production-intake-stager"' in gateway_cargo, "stager Cargo target absent")
    require("production-intake-producer" not in gateway_cargo, "misnamed producer target retained")

    writer = authority["production_current_source_writer"]
    require(writer["executable"] == sequence[3] and writer["uid"] == writer["gid"] == 8095, "writer identity drift")
    require(writer["systemd_unit"] == "deploy/stage8b-r2b/moex-stage8b-r2a8-production-current-source-writer.service", "writer unit path drift")
    require(writer["requires_unit"] == "moex-stage8b-r2a8-production-intake-stager.service", "writer stager dependency drift")
    require(writer["before_unit"] == "stage8b-r2a8-current-manifest-issuer.service", "writer manifest ordering drift")
    require(writer["invocation_cardinality"] == 1 and "one fixed-input oneshot" in writer["systemd_invocation"], "writer invocation drift")
    systemd_contract = writer["systemd_syntax_contract"]
    require(
        systemd_contract["exact_chain_units"] == 4
        and systemd_contract["refuse_manual_start_section"] == "Unit"
        and systemd_contract["condition_path_is_regular_allowed"] is False
        and systemd_contract["target_systemd_parser_required"] is True
        and systemd_contract["binary_fixed_input_validation_authoritative"] is True,
        "systemd syntax authority drift",
    )
    require("ConditionPathIsRegular=" not in publisher_unit + creator_unit + stager_unit + writer_unit, "unsupported systemd condition restored")
    require_all(writer_unit, (
        "Requires=moex-stage8b-r2a8-production-intake-stager.service",
        "After=local-fs.target moex-stage8b-r2a8-production-intake-stager.service",
        "Before=stage8b-r2a8-current-manifest-issuer.service",
        "User=8095", "Group=8095", "SupplementaryGroups=",
        "WorkingDirectory=/var/lib/moex-trading/stage8b/r2a8/current-source",
        "ExecStart=/opt/moex-trading/stage8b-r2b/bin/stage8b-r2a8-production-current-source-writer",
        "RefuseManualStart=yes",
        "RestrictAddressFamilies=AF_UNIX", "IPAddressDeny=any",
        "ReadOnlyPaths=/var/lib/moex-trading/stage7b /var/lib/moex-trading/stage8a1-authority /var/lib/moex-trading/stage8b/r2a7/production /var/lib/moex-trading/stage8b/r2a8/intake",
        "ReadWritePaths=/var/lib/moex-trading/stage8b/r2a8/current-source",
    ), "writer unit")
    require("std::env::args_os().len() != 1" in writer_bin, "writer accepts arguments")
    require("pub(crate) fn publish_stage8b_r2a8_trusted_current_source_from_owner(" in adapter, "owner seam absent")
    require("publish_stage8b_r2a8_trusted_current_source_from_owner," not in gateway_lib, "owner seam re-exported")
    require(
        systemd_evidence["systemd_analyze_verify_exit_code"] == 0
        and systemd_evidence["unknown_key_warnings"] == 0
        and systemd_evidence["unknown_lvalue_warnings"] == 0
        and systemd_evidence["refuse_manual_start_section"] == "Unit"
        and systemd_evidence["condition_path_is_regular_present"] is False
        and systemd_evidence["units_loaded"] is False
        and systemd_evidence["units_started"] is False
        and systemd_evidence["result"] == "PASS",
        "target systemd evidence drift",
    )
    require(
        build["r2b_negative_mutations"] == "293/293"
        and build["systemd_section_aware_static_gate"] == "PASS"
        and build["systemd_target_verify"] == "PASS"
        and build["systemd_unknown_key_or_lvalue_warnings"] == "0"
        and build["systemd_units_loaded_or_started"] is False,
        "systemd build evidence drift",
    )

    admission = authority["r2b_launcher_and_admission"]
    require(admission["launcher_uid"] == admission["launcher_gid"] == 0, "launcher privilege drift")
    require(admission["receipt_provenance"].startswith("root-owned sealed memfd"), "root provenance drift")
    require([admission[name] for name in ("sealed_receipt_fd", "terminal_channel_fd", "admission_record_fd", "nonce_marker_fd", "helper_executable_fd")] == [3, 4, 5, 6, 7], "FD contract drift")
    require(admission["inherited_fd_allowlist"] == list(range(8)), "FD allowlist drift")
    for field in ("root_supervisor_survives_child", "no_new_privs", "saved_uid_gid_rejected", "setuid_setgid_helper_rejected", "helper_file_capability_rejected"):
        require(admission[field] is True, f"launcher hardening absent: {field}")
    require(admission["helper_acceptance_model"].endswith("no repository test-key approval claim"), "test-key claim restored")
    require(admission["kernel_yama_ptrace_scope_minimum"] >= 1, "Yama isolation absent")
    require(admission["dedicated_helper_uid_process_count_before_admission"] == 0, "exclusive UID absent")
    require(admission["helper_pr_set_dumpable_zero_before_runtime_bootstrap"], "helper nondumpability absent")
    require(admission["pidfd_supervision"] and admission["bounded_reap_ms"] == 2000, "bounded pidfd supervision absent")
    controlled_custody = admission["controlled_custody_qualification"]
    require(controlled_custody["loopback_tls_server_required"] is True and controlled_custody["external_network_allowed"] is False, "controlled TLS isolation drift")
    require(not (BASE / "stage8b-p-r2b-helper-acceptance-authority.json").exists(), "pseudo helper acceptance authority retained")
    require_all(launcher, (
        "O_NOFOLLOW", "security.capability", "libc::S_ISUID", "libc::S_ISGID",
        "memfd_create", "F_ADD_SEALS", "socketpair", "close_range(8, u32::MAX, 0)",
        "close_non_allowlisted_descriptors", "Some(libc::ENOSYS)",
        'std::fs::read_dir("/proc/self/fd")',
        "libc::close(parent_channel.as_raw_fd())",
        "PR_SET_NO_NEW_PRIVS", "PR_CAP_AMBIENT_CLEAR_ALL", "setgroups",
        "setresgid", "setresuid", "getresgid", "getresuid", "fexecve",
        "R2bAdmissionState::HelperExecAttempted", "R2bAdmissionState::HelperProcessStarted",
        "R2bAdmissionState::HelperTerminalReceived", "R2bAdmissionState::HelperExitedFailure",
        "R2bAdmissionState::TerminalEvidenceDurable", "TerminalPersistenceFailure",
        "r2b_root_terminal_record", "persist_r2b_root_terminal_json",
        'controlled_fault("FEXECVE_FAILURE")',
        'controlled_fault("HELPER_CRASH_AFTER_STARTED")',
        'controlled_fault("FINALIZER_FSYNC_FAILURE")',
        'controlled_fault("TERMINAL_THEN_HANG")',
        'controlled_fault("PARTIAL_FRAME_HEADER")',
        "verify_runtime_isolation_before_admission", "kernel.yama.ptrace_scope >= 1",
        "SYS_pidfd_open", "PR_SET_DUMPABLE", "read_frame_before", "wait_child_before",
    ), "root supervisor")
    require(launcher.index("verify_runtime_isolation_before_admission()") < launcher.index("prepare_r2b_privileged_admission("), "isolation check follows nonce admission")
    require_all(supervisor_unit, (
        "User=root", "ProtectProc=invisible", "ProcSubset=pid",
        "ExecStart=/opt/moex-trading/stage8b-r2b/bin/stage8b-r2b-launcher",
    ), "supervisor unit")
    require_all(text("tools/stage8b-readonly-preflight/src/main.rs"), (
        "PR_SET_DUMPABLE", ".block_on(run())",
    ), "helper bootstrap")
    require(launcher.index("open_accepted_helper(&accepted)") < launcher.index("prepare_r2b_privileged_admission("), "helper not checked before admission")
    require("stage8b-p-r2a5-accepted-helper-sha256.txt" in old_launcher, "historical R2A5 launcher mutated")
    require("stage8b-p-r2b-accepted-helper-sha256.txt" in launcher, "R2B helper pin absent")

    require_all(helper, (
        "receipt_stat.st_uid != 0", "receipt_stat.st_gid != 0", "admission_stat.st_uid != 0",
        "nonce_stat.st_uid != 0", "terminal_stat.st_uid != 0", "receipt.nonce_marker_device",
        "receipt.admission_record_device", "receipt.terminal_channel_device",
        "R2bRootTerminalRecordV1", "pub child_pid: Option<i32>",
        "pub child_exit_code: Option<i32>", "pub child_signal: Option<i32>",
        "pub root_terminal_outcome: R2bTerminalOutcome",
        "pub child_protocol_valid: bool", "pub child_exit_consistent: bool",
        "pub validated_helper_terminal: Option<R2bTerminalEvidenceV1>",
        "validate_r2b_helper_terminal",
        "R2B_EVIDENCE_DIRECTORY_MODE: u32 = 0o700", "R2B_EVIDENCE_FILE_MODE: u32 = 0o400",
    ), "helper admission/evidence")
    terminal_validation = helper.split("pub fn validate_r2b_helper_terminal", 1)[1].split(
        "pub fn send_r2b_supervisor_message", 1
    )[0]
    require_all(terminal_validation, (
        'evidence.stage == "Stage 8B-P R2B"',
        "evidence.operation == receipt.operation",
        "evidence.contract_snapshot_sha256 == receipt.contract_snapshot_sha256",
        "evidence.production_composition_sha256 == sha256(R2B_RUNTIME_COMPOSITION_CONTRACT)",
        "attempt.ordinal == index + 1",
        "expected_routes.get(index).copied() == Some(attempt.route_template.as_str())",
        'attempt.method == "POST"', 'attempt.method == "GET"',
        "evidence.started_at_utc <= evidence.finished_at_utc",
        "outcome_consistent", "attempts_valid", "broker_truth_valid",
    ), "typed helper terminal validation")
    admitted_child = launcher.split("fn run_admitted_child", 1)[1].split(
        "fn supervise", 1
    )[0]
    require_all(admitted_child, (
        "(r2a5::R2bTerminalOutcome::Success, true)",
        "(r2a5::R2bTerminalOutcome::Failure, false)",
        "!protocol_valid || !exit_consistent",
        "success\n            && !lifecycle_state_failed\n            && protocol_valid\n            && !timed_out\n            && exit_consistent",
    ), "root child/terminal reconciliation")
    run_body = helper.split("pub async fn run_r2b_one_shot()", 1)[1].split("fn controlled_client_from_fixed_files", 1)[0]
    require("claim_nonce(" not in run_body, "UID8301 helper claims nonce")
    require(run_body.index("consume_sealed_r2b_admission_receipt") < run_body.index("execute_r2a3_pipeline_preserving_attempts"), "receipt validation order drift")
    require(run_body.index("consume_sealed_r2b_admission_receipt") < run_body.index("load_r2a5_credentials_at"), "credential read precedes authenticated admission")
    admission_body = helper.split("fn prepare_r2b_privileged_admission_against", 1)[1].split("pub fn record_r2b_supervisor_state", 1)[0]
    require("validate_local_authority_at(" in admission_body and "validate_local_package_at(" not in admission_body, "root supervisor can read FINAM credential")
    persist_body = helper.split("fn persist_admission_state", 1)[1].split(
        "pub fn prepare_r2b_privileged_admission", 1
    )[0]
    nonce_body = helper.split("fn claim_nonce", 1)[1].split(
        "fn terminal_error_category", 1
    )[0]
    post_chmod_fsync = (
        "file.set_permissions(std::fs::Permissions::from_mode(0o400))?;\n"
        "    file.sync_all()?;"
    )
    require(post_chmod_fsync in persist_body, "admission-state chmod metadata fsync absent")
    require(post_chmod_fsync in nonce_body, "nonce chmod metadata fsync absent")
    credential = authority["credential_contract"]
    require(credential["root_supervisor_reads_credentials"] is False and credential["authenticated_receipt_verified_before_credential_read"] is True, "credential ordering authority drift")
    require_all(rehearsal, (
        "stage8b-r2a5-controlled-server",
        '"$SERVER" "$operation"',
        '\"terminal_outcome\":\"SUCCESS\"',
        "run_supervisor_fault FEXECVE_FAILURE",
        "run_supervisor_fault HELPER_CRASH_AFTER_STARTED",
        "run_supervisor_fault FINALIZER_FSYNC_FAILURE",
        "terminal-persistence-failure", "same-uid-isolation", "pidfd_getfd=false",
        "for target_fd in (3, 4, 6, 7)",
        "TERMINAL_THEN_HANG", "SLOW_DRIP_FRAME", "PARTIAL_FRAME_BODY",
        'terminal["root_error_category"] == "TIMEOUT"', "stage8b-r2b-r4-r2-post-c0-empty-r2b-root-chain",
        '"$AUTHORITATIVE_CREATOR"', '"$INTAKE_STAGER"',
    ), "adversarial supervisor rehearsal")
    require_all(gate, (
        "stage8b_p_r2a8_review_closure_check.py",
        "stage8b_p_r2a8_negative_harness.py",
        "stage8b_p_r2a8_r1_readiness_negative_harness.py",
        "stage8b_p_r2b_proposal_negative_harness.py",
        "stage8b_p_r2b_r3_linux_custody_rehearsal.sh",
    ), "aggregate R3 gate")
    for marker in (
        '"authorization_status": "NOT_ISSUED"',
        '"finam_network_accessed": False',
        '"order_post_delete_sent": False',
        '"redis_live_accessed": False',
        '"broker_dispatch_entered": False',
        '"runtime_live_entered": False',
        '"real_orders_sent": False',
    ):
        require(marker in handoff_maker and marker in handoff_safety, f"handoff closure absent: {marker}")

    evidence = authority["evidence_contract"]
    require((evidence["directory_uid"], evidence["directory_gid"], evidence["directory_mode"]) == (0, 0, "0700"), "terminal directory custody drift")
    require((evidence["file_uid"], evidence["file_gid"], evidence["file_mode"]) == (0, 0, "0400"), "terminal file custody drift")
    for field in ("helper_write_access", "helper_unlink_access"):
        require(evidence[field] is False, f"helper evidence mutation opened: {field}")
    for field in ("admission_commitment_bound", "launcher_hash_bound", "nonce_marker_inode_bound", "admission_record_inode_bound", "child_pid_and_exit_status_bound", "create_new", "no_follow", "single_link_required", "file_fsync", "directory_fsync", "one_terminal_record_per_nonce"):
        require(evidence[field] is True, f"evidence invariant absent: {field}")

    query = authority["freshness_and_validation"]["query_policy"]
    require(query["window_end_semantics"] == "request_requested_at_exclusive", "end boundary drift")
    require(runtime["query_policy"] == query, "runtime query policy drift")
    require_all(helper_lib, ("pub const TRADES_LIMIT: usize = 1_000;", "pub const TRADES_WINDOW_MS: i64 = 24 * 60 * 60 * 1_000;", '"interval.start_time"', '"interval.end_time"'), "query implementation")
    require_all(pipeline, ("pub status: Option<u16>", "pub observed_body_length: Option<usize>", "pub configured_body_cap: usize", "pub body_overflow: bool", "pub response_stage_error: bool"), "response evidence")

    require(exact_hash(helper_sha), "accepted helper SHA malformed")
    hashes = composition["production_linux_amd64_sha256"]
    executable_names = set(sequence)
    require(set(hashes) == executable_names and all(exact_hash(value) for value in hashes.values()), "production hash inventory drift")
    require(hashes["accepted-stage8b-readonly-preflight"] == helper_sha, "helper SHA binding drift")
    require(build["stage"] == "Stage 8B-P R2B Proposal R4-R2" and build["run_count"] == 2, "R4-R2 build evidence drift")
    require(build["authorization_status"] == "NOT_ISSUED" and not build["fixture_dependencies_in_production"], "build scope drift")
    for name, record in build["production_binaries"].items():
        require(name in hashes and record["build_a_sha256"] == record["build_b_sha256"] == hashes[name] and record["reproducible"], f"production build drift: {name}")
    for name, expected in build["inherited_accepted_production_binaries"].items():
        require(hashes.get(name) == expected, f"inherited production binding drift: {name}")
    controlled_hashes = composition["controlled_qualification_linux_amd64_sha256"]
    require(set(build["controlled_qualification_binaries"]) == set(controlled_hashes), "controlled build inventory drift")
    for name, record in build["controlled_qualification_binaries"].items():
        require(record["build_a_sha256"] == record["build_b_sha256"] == controlled_hashes[name] and record["reproducible"], f"controlled build drift: {name}")
    for field in ("controlled_place_regression", "controlled_cancel_regression", "linux_terminal_evidence_test", "linux_adversarial_custody_rehearsal", "direct_helper_rejected", "forged_uid8301_receipt_rejected", "uid8301_terminal_mutation_rejected", "fexecve_failure_finalized", "helper_crash_finalized", "finalizer_fsync_failure_marked", "same_uid_isolation_rejected", "typed_terminal_protocol", "absolute_deadline_fault_matrix", "reachable_creator_rehearsal", "post_chmod_metadata_fsync"):
        require(build[field] == "PASS", f"build/rehearsal evidence drift: {field}")
    for relative, expected in build["source_sha256"].items():
        require(hashlib.sha256((ROOT / relative).read_bytes()).hexdigest() == expected, f"source binding drift: {relative}")
    for field in ("finam_network_accessed", "finam_credentials_accessed", "order_post_sent", "order_delete_sent", "redis_live_accessed", "broker_dispatch_entered", "runtime_live_entered", "real_orders_sent"):
        require(build[field] is False, f"build evidence opened {field}")

    require(all(value is False for value in authority["closed_surfaces"].values()), "closed surface opened")
    require(authority["issuance_preconditions"]["r2b_authorization"] == "NOT_ISSUED", "issuance state drift")
    require_all(status, ("Stage 8B-P R2B Proposal R4-R2", "NOT_ISSUED", "FINAM network access", "POST/DELETE", "runtime-live"), "status")
    require_all(proposal, (
        "Proposal R4-R2", "root-owned inode-bound admission", "root:root 0400",
        "does not issue R2B", "accepted_external_current_source_C0",
        "r2b_refreshed_current_source_C1", "no same-generation causal cycle",
        "no controlled binary belongs to the production",
    ), "proposal")

    with (BASE / "STAGE8B_P_R2B_PROPOSAL_ACCEPTANCE_MATRIX_2026-08-27.csv").open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 85, "acceptance row count drift")
    require([row["id"] for row in rows] == [f"R2B-P-{index:03d}" for index in range(1, 86)], "acceptance IDs drift")
    require(all(row["status"] == "PASS" for row in rows), "acceptance row not PASS")

    print("stage8b-p-r2b-proposal-check: PASS revision=R4-R2 closure=R4-R2A rows=85 external_c0=true refreshed_c1=true causal_cycle=false writer_unit=true upstream_publisher=true production_reachable=true empty_root_generation_one=true renewal=true source_chain=true predecessor_snapshot_source=false creator=true isolation=true typed_terminal=true absolute_deadline=true metadata_fsync=true stager=true root_authenticated=true immutable_terminal=true supervisor=true authorization=NOT_ISSUED network=false post_delete=false runtime_live=false")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        raise SystemExit(f"stage8b-p-r2b-proposal-check: FAIL {error}")
