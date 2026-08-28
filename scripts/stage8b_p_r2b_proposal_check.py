#!/usr/bin/env python3
"""Fail-closed checker for Stage 8B-P R2B Proposal R3."""

from __future__ import annotations

import csv
import hashlib
import json
import re
from pathlib import Path

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
    authority = load("docs/stage-8/stage8b-p-r2b-proposal-authority.json")
    runtime_path = ROOT / "docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json"
    runtime_bytes = runtime_path.read_bytes()
    runtime = json.loads(runtime_bytes)
    build = load("docs/stage-8/stage8b-p-r2b-r3-build-evidence.json")
    helper_sha = text("docs/stage-8/stage8b-p-r2b-accepted-helper-sha256.txt").strip()
    proposal = text("docs/stage-8/STAGE8B_P_R2B_PROPOSAL_2026-08-27.md")
    status = text("docs/current-status.md")
    adapter = text("crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs")
    capability = text("crates/finam-gateway/src/stage8a1_execution_capability.rs")
    gateway_lib = text("crates/finam-gateway/src/lib.rs")
    gateway_cargo = text("crates/finam-gateway/Cargo.toml")
    stager_bin = text("crates/finam-gateway/src/bin/stage8b-r2a8-production-intake-stager.rs")
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
    require(authority["stage"] == "Stage 8B-P R2B" and authority["revision"] == "R3", "stage/revision drift")
    require(authority["status"] == "PROPOSAL_ONLY_NOT_AUTHORIZED", "proposal status opened")
    require(authority["authorization_status"] == "NOT_ISSUED", "R2B authorization issued")
    require(authority["accepted_predecessor"]["source_ref"] == "5b2079d7d524d2fa6f084f44f961c4b5958c042a", "predecessor drift")

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
        "stage8b-r2a8-authoritative-intake-creator-service",
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
    require(runtime["revision"] == "R3" and runtime["exact_component_sequence"] == sequence, "runtime sequence drift")
    require(runtime["authorization_status"] == "NOT_ISSUED", "runtime contract authorized")
    require(all(value is False for value in runtime["closed_surfaces"].values()), "runtime surface opened")
    embedded = composition["embedded_runtime_composition_contract"]
    require(embedded["sha256"] == hashlib.sha256(runtime_bytes).hexdigest(), "runtime contract binding drift")
    require(not embedded["contains_executable_hashes"] and embedded["hash_cycle_prevented"], "hash cycle drift")

    creator = authority["authoritative_intake_creator"]
    require(creator["service"] == sequence[0] and creator["uid"] == creator["gid"] == 8094, "creator identity drift")
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

    stager = authority["production_intake_stager"]
    require(stager["executable"] == sequence[1] and not stager["creates_or_signs_authority"], "stager misrepresented")
    require("std::env::args_os().len() != 1" in stager_bin, "stager accepts arguments")
    require("run_stage8b_r2a8_production_intake_stager" in stager_bin, "stager detached")
    stager_body = adapter.split("pub fn run_stage8b_r2a8_production_intake_stager", 1)[1].split("pub(crate) fn publish_stage8b_r2a8_trusted_current_source_from_owner", 1)[0]
    require_all(stager_body, ("read_fixed_regular_file(", "validate_production_writer_intake(", "atomic_write_fixed("), "stager")
    require("sign_stage8b" not in stager_body, "stager gained signing authority")
    require('name = "stage8b-r2a8-production-intake-stager"' in gateway_cargo, "stager Cargo target absent")
    require("production-intake-producer" not in gateway_cargo, "misnamed producer target retained")

    writer = authority["production_current_source_writer"]
    require(writer["executable"] == sequence[2] and writer["uid"] == writer["gid"] == 8095, "writer identity drift")
    require("std::env::args_os().len() != 1" in writer_bin, "writer accepts arguments")
    require("pub(crate) fn publish_stage8b_r2a8_trusted_current_source_from_owner(" in adapter, "owner seam absent")
    require("publish_stage8b_r2a8_trusted_current_source_from_owner," not in gateway_lib, "owner seam re-exported")

    admission = authority["r2b_launcher_and_admission"]
    require(admission["launcher_uid"] == admission["launcher_gid"] == 0, "launcher privilege drift")
    require(admission["receipt_provenance"].startswith("root-owned sealed memfd"), "root provenance drift")
    require([admission[name] for name in ("sealed_receipt_fd", "terminal_channel_fd", "admission_record_fd", "nonce_marker_fd", "helper_executable_fd")] == [3, 4, 5, 6, 7], "FD contract drift")
    require(admission["inherited_fd_allowlist"] == list(range(8)), "FD allowlist drift")
    for field in ("root_supervisor_survives_child", "no_new_privs", "saved_uid_gid_rejected", "setuid_setgid_helper_rejected", "helper_file_capability_rejected"):
        require(admission[field] is True, f"launcher hardening absent: {field}")
    require(admission["helper_acceptance_model"].endswith("no repository test-key approval claim"), "test-key claim restored")
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
    ), "root supervisor")
    require(launcher.index("open_accepted_helper(&accepted)") < launcher.index("prepare_r2b_privileged_admission("), "helper not checked before admission")
    require("stage8b-p-r2a5-accepted-helper-sha256.txt" in old_launcher, "historical R2A5 launcher mutated")
    require("stage8b-p-r2b-accepted-helper-sha256.txt" in launcher, "R2B helper pin absent")

    require_all(helper, (
        "receipt_stat.st_uid != 0", "receipt_stat.st_gid != 0", "admission_stat.st_uid != 0",
        "nonce_stat.st_uid != 0", "terminal_stat.st_uid != 0", "receipt.nonce_marker_device",
        "receipt.admission_record_device", "receipt.terminal_channel_device",
        "R2bRootTerminalRecordV1", "pub child_pid: Option<i32>",
        "pub child_exit_code: Option<i32>", "pub child_signal: Option<i32>",
        "R2B_EVIDENCE_DIRECTORY_MODE: u32 = 0o700", "R2B_EVIDENCE_FILE_MODE: u32 = 0o400",
    ), "helper admission/evidence")
    run_body = helper.split("pub async fn run_r2b_one_shot()", 1)[1].split("fn controlled_client_from_fixed_files", 1)[0]
    require("claim_nonce(" not in run_body, "UID8301 helper claims nonce")
    require(run_body.index("consume_sealed_r2b_admission_receipt") < run_body.index("execute_r2a3_pipeline_preserving_attempts"), "receipt validation order drift")
    require(run_body.index("consume_sealed_r2b_admission_receipt") < run_body.index("load_r2a5_credentials_at"), "credential read precedes authenticated admission")
    admission_body = helper.split("fn prepare_r2b_privileged_admission_against", 1)[1].split("pub fn record_r2b_supervisor_state", 1)[0]
    require("validate_local_authority_at(" in admission_body and "validate_local_package_at(" not in admission_body, "root supervisor can read FINAM credential")
    credential = authority["credential_contract"]
    require(credential["root_supervisor_reads_credentials"] is False and credential["authenticated_receipt_verified_before_credential_read"] is True, "credential ordering authority drift")
    require_all(rehearsal, (
        "stage8b-r2a5-controlled-server",
        '"$SERVER" "$operation"',
        '\"terminal_outcome\":\"SUCCESS\"',
        "run_supervisor_fault FEXECVE_FAILURE",
        "run_supervisor_fault HELPER_CRASH_AFTER_STARTED",
        "run_supervisor_fault FINALIZER_FSYNC_FAILURE",
        "terminal-persistence-failure",
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
    executable_names = set(sequence) - {"stage8b-r2a8-authoritative-intake-creator-service"}
    require(set(hashes) == executable_names and all(exact_hash(value) for value in hashes.values()), "production hash inventory drift")
    require(hashes["accepted-stage8b-readonly-preflight"] == helper_sha, "helper SHA binding drift")
    require(build["stage"] == "Stage 8B-P R2B Proposal R3" and build["run_count"] == 2, "R3 build evidence drift")
    require(build["authorization_status"] == "NOT_ISSUED" and not build["fixture_dependencies_in_production"], "build scope drift")
    for name, record in build["production_binaries"].items():
        require(name in hashes and record["build_a_sha256"] == record["build_b_sha256"] == hashes[name] and record["reproducible"], f"production build drift: {name}")
    for name, expected in build["inherited_accepted_production_binaries"].items():
        require(hashes.get(name) == expected, f"inherited production binding drift: {name}")
    controlled_hashes = composition["controlled_qualification_linux_amd64_sha256"]
    require(set(build["controlled_qualification_binaries"]) == set(controlled_hashes), "controlled build inventory drift")
    for name, record in build["controlled_qualification_binaries"].items():
        require(record["build_a_sha256"] == record["build_b_sha256"] == controlled_hashes[name] and record["reproducible"], f"controlled build drift: {name}")
    for field in ("controlled_place_regression", "controlled_cancel_regression", "linux_terminal_evidence_test", "linux_adversarial_custody_rehearsal", "direct_helper_rejected", "forged_uid8301_receipt_rejected", "uid8301_terminal_mutation_rejected", "fexecve_failure_finalized", "helper_crash_finalized", "finalizer_fsync_failure_marked"):
        require(build[field] == "PASS", f"build/rehearsal evidence drift: {field}")
    for relative, expected in build["source_sha256"].items():
        require(hashlib.sha256((ROOT / relative).read_bytes()).hexdigest() == expected, f"source binding drift: {relative}")
    for field in ("finam_network_accessed", "finam_credentials_accessed", "order_post_sent", "order_delete_sent", "redis_live_accessed", "broker_dispatch_entered", "runtime_live_entered", "real_orders_sent"):
        require(build[field] is False, f"build evidence opened {field}")

    require(all(value is False for value in authority["closed_surfaces"].values()), "closed surface opened")
    require(authority["issuance_preconditions"]["r2b_authorization"] == "NOT_ISSUED", "issuance state drift")
    require_all(status, ("Stage 8B-P R2B Proposal R3", "NOT_ISSUED", "FINAM network access", "POST/DELETE", "runtime-live"), "status")
    require_all(proposal, ("Proposal R3", "root-owned inode-bound admission", "root:root 0400", "does not issue R2B"), "proposal")

    with (BASE / "STAGE8B_P_R2B_PROPOSAL_ACCEPTANCE_MATRIX_2026-08-27.csv").open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 60, "acceptance row count drift")
    require([row["id"] for row in rows] == [f"R2B-P-{index:03d}" for index in range(1, 61)], "acceptance IDs drift")
    require(all(row["status"] == "PASS" for row in rows), "acceptance row not PASS")

    print("stage8b-p-r2b-proposal-check: PASS revision=R3 rows=60 creator=true stager=true root_authenticated=true immutable_terminal=true supervisor=true authorization=NOT_ISSUED network=false post_delete=false runtime_live=false")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        raise SystemExit(f"stage8b-p-r2b-proposal-check: FAIL {error}")
