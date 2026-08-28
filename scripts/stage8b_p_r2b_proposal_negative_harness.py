#!/usr/bin/env python3
"""Adversarial static matrix for Stage 8B-P R2B Proposal R3."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage8b_p_r2b_proposal_check.py"
AUTHORITY = "docs/stage-8/stage8b-p-r2b-proposal-authority.json"
BUILD = "docs/stage-8/stage8b-p-r2b-r3-build-evidence.json"

FILES = (
    AUTHORITY,
    BUILD,
    "docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json",
    "docs/stage-8/stage8b-p-r2b-accepted-helper-sha256.txt",
    "docs/stage-8/STAGE8B_P_R2B_PROPOSAL_2026-08-27.md",
    "docs/stage-8/STAGE8B_P_R2B_PROPOSAL_ACCEPTANCE_MATRIX_2026-08-27.csv",
    "docs/current-status.md",
    "crates/finam-gateway/Cargo.toml",
    "crates/finam-gateway/src/lib.rs",
    "crates/finam-gateway/src/stage8a1_execution_capability.rs",
    "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs",
    "crates/finam-gateway/src/bin/stage8b-r2a8-production-intake-stager.rs",
    "crates/finam-gateway/src/bin/stage8b-r2a8-production-current-source-writer.rs",
    "tools/stage8b-readonly-preflight/src/lib.rs",
    "tools/stage8b-readonly-preflight/src/r2a3.rs",
    "tools/stage8b-readonly-preflight/src/r2a5.rs",
    "tools/stage8b-readonly-preflight/src/bin/stage8b-r2a5-launcher.rs",
    "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs",
    "tools/stage8b-readonly-preflight/src/bin/stage8b-r2a5-controlled-server.rs",
    "scripts/stage8b_p_r2b_r3_linux_custody_rehearsal.sh",
    "scripts/stage8b_p_r2b_proposal_gate.sh",
    "scripts/make_stage8b_p_r2b_handoff.py",
    "scripts/stage8b_p_r2b_handoff_safety_check.py",
    CHECKER,
)


def set_path(document: dict[str, Any], dotted: str, value: Any) -> None:
    parts = dotted.split(".")
    current: Any = document
    for part in parts[:-1]:
        current = current[int(part)] if isinstance(current, list) else current[part]
    if isinstance(current, list):
        current[int(parts[-1])] = value
    else:
        current[parts[-1]] = value


JSON_CASES: tuple[tuple[str, str, Any], ...] = (
    ("authorization-issued", "authorization_status", "ISSUED"),
    ("proposal-authorized", "status", "AUTHORIZED"),
    ("background-loop", "proposed_capability.background_loop", True),
    ("unattended", "proposed_capability.unattended_execution", True),
    ("evidence-authority", "proposed_capability.result_may_influence_execution", True),
    ("selection-count", "proposed_capability.selection_count", 2),
    ("endpoint-host", "network_contract.exact_host", "evil.invalid"),
    ("destination", "network_contract.outbound_destinations.0", "0.0.0.0:443"),
    ("redirect", "network_contract.redirects_allowed", True),
    ("proxy", "network_contract.proxy_allowed", True),
    ("retry", "network_contract.automatic_retries_allowed", True),
    ("order-post", "network_contract.order_post_allowed", True),
    ("order-delete", "network_contract.order_delete_allowed", True),
    ("arbitrary-request", "network_contract.arbitrary_request_allowed", True),
    ("creator-sequence-missing", "production_composition.exact_executable_sequence.0", "manual-json"),
    ("creator-cardinality", "production_composition.exact_invocation_cardinality.stage8b-r2a8-authoritative-intake-creator-service", 0),
    ("creator-wrong-uid", "authoritative_intake_creator.uid", 0),
    ("creator-caller-json", "authoritative_intake_creator.caller_json_allowed", True),
    ("creator-caller-readiness", "authoritative_intake_creator.caller_readiness_allowed", True),
    ("creator-caller-truth", "authoritative_intake_creator.caller_broker_truth_allowed", True),
    ("creator-caller-broker-readiness", "authoritative_intake_creator.caller_broker_readiness_allowed", True),
    ("creator-caller-time", "authoritative_intake_creator.caller_timestamps_allowed", True),
    ("creator-network", "authoritative_intake_creator.network_access_allowed", True),
    ("creator-credential", "authoritative_intake_creator.finam_credential_access_allowed", True),
    ("creator-not-atomic", "authoritative_intake_creator.atomic_write", False),
    ("creator-no-file-fsync", "authoritative_intake_creator.file_fsync", False),
    ("creator-no-dir-fsync", "authoritative_intake_creator.directory_fsync", False),
    ("stager-claims-signing", "production_intake_stager.creates_or_signs_authority", True),
    ("stager-network", "production_intake_stager.network_access_allowed", True),
    ("stager-credential", "production_intake_stager.finam_credential_access_allowed", True),
    ("launcher-not-root", "r2b_launcher_and_admission.launcher_uid", 8301),
    ("receipt-fd", "r2b_launcher_and_admission.sealed_receipt_fd", 9),
    ("terminal-fd", "r2b_launcher_and_admission.terminal_channel_fd", 9),
    ("admission-fd", "r2b_launcher_and_admission.admission_record_fd", 9),
    ("nonce-fd", "r2b_launcher_and_admission.nonce_marker_fd", 9),
    ("helper-fd", "r2b_launcher_and_admission.helper_executable_fd", 9),
    ("fd-leak", "r2b_launcher_and_admission.inherited_fd_allowlist", list(range(9))),
    ("supervisor-does-not-survive", "r2b_launcher_and_admission.root_supervisor_survives_child", False),
    ("no-new-privs-off", "r2b_launcher_and_admission.no_new_privs", False),
    ("saved-id-open", "r2b_launcher_and_admission.saved_uid_gid_rejected", False),
    ("setid-helper-open", "r2b_launcher_and_admission.setuid_setgid_helper_rejected", False),
    ("file-cap-open", "r2b_launcher_and_admission.helper_file_capability_rejected", False),
    ("test-key-claim", "r2b_launcher_and_admission.helper_acceptance_model", "RFC test key"),
    ("controlled-loopback-missing", "r2b_launcher_and_admission.controlled_custody_qualification.loopback_tls_server_required", False),
    ("root-reads-credential", "credential_contract.root_supervisor_reads_credentials", True),
    ("credential-before-receipt", "credential_contract.authenticated_receipt_verified_before_credential_read", False),
    ("nonce-group-writable", "r2b_launcher_and_admission.nonce_registry_mode", "0770"),
    ("helper-writes-nonce", "r2b_launcher_and_admission.helper_can_write_nonce_registry", True),
    ("helper-deletes-nonce", "r2b_launcher_and_admission.helper_can_delete_nonce_marker", True),
    ("directory-owner", "evidence_contract.directory_uid", 8301),
    ("directory-group", "evidence_contract.directory_gid", 8301),
    ("directory-mode", "evidence_contract.directory_mode", "0730"),
    ("file-owner", "evidence_contract.file_uid", 8301),
    ("file-group", "evidence_contract.file_gid", 8301),
    ("file-mode", "evidence_contract.file_mode", "0640"),
    ("helper-write", "evidence_contract.helper_write_access", True),
    ("helper-unlink", "evidence_contract.helper_unlink_access", True),
    ("admission-unbound", "evidence_contract.admission_commitment_bound", False),
    ("launcher-unbound", "evidence_contract.launcher_hash_bound", False),
    ("nonce-inode-unbound", "evidence_contract.nonce_marker_inode_bound", False),
    ("admission-inode-unbound", "evidence_contract.admission_record_inode_bound", False),
    ("child-status-unbound", "evidence_contract.child_pid_and_exit_status_bound", False),
    ("overwrite", "evidence_contract.create_new", False),
    ("symlink", "evidence_contract.no_follow", False),
    ("multilink", "evidence_contract.single_link_required", False),
    ("file-fsync", "evidence_contract.file_fsync", False),
    ("dir-fsync", "evidence_contract.directory_fsync", False),
    ("duplicate-terminal", "evidence_contract.one_terminal_record_per_nonce", False),
    ("query-end-inclusive", "freshness_and_validation.query_policy.window_end_semantics", "request_requested_at_inclusive"),
    ("query-limit", "freshness_and_validation.query_policy.trades_limit", 999),
    ("query-window", "freshness_and_validation.query_policy.trades_window_ms", 1),
    ("query-override", "freshness_and_validation.query_policy.caller_override_allowed", True),
    ("redis-live", "closed_surfaces.redis_live_consumer", True),
    ("dispatch", "closed_surfaces.broker_dispatch", True),
    ("runtime-live", "closed_surfaces.runtime_live", True),
    ("real-orders", "closed_surfaces.real_orders", True),
)

BUILD_CASES: tuple[tuple[str, str, Any], ...] = (
    ("build-authorized", "authorization_status", "ISSUED"),
    ("build-run-count", "run_count", 1),
    ("fixture-production", "fixture_dependencies_in_production", True),
    ("stager-hash", "production_binaries.stage8b-r2a8-production-intake-stager.build_a_sha256", "0" * 64),
    ("launcher-hash", "production_binaries.stage8b-r2b-launcher.build_a_sha256", "0" * 64),
    ("helper-hash", "production_binaries.accepted-stage8b-readonly-preflight.build_a_sha256", "0" * 64),
    ("place-not-pass", "controlled_place_regression", "FAIL"),
    ("cancel-not-pass", "controlled_cancel_regression", "FAIL"),
    ("custody-not-pass", "linux_adversarial_custody_rehearsal", "FAIL"),
    ("direct-helper-not-pass", "direct_helper_rejected", "FAIL"),
    ("forged-receipt-not-pass", "forged_uid8301_receipt_rejected", "FAIL"),
    ("terminal-mutation-not-pass", "uid8301_terminal_mutation_rejected", "FAIL"),
    ("fexecve-failure-not-finalized", "fexecve_failure_finalized", "FAIL"),
    ("helper-crash-not-finalized", "helper_crash_finalized", "FAIL"),
    ("fsync-failure-not-marked", "finalizer_fsync_failure_marked", "FAIL"),
    ("network-used", "finam_network_accessed", True),
    ("credential-used", "finam_credentials_accessed", True),
    ("post-sent", "order_post_sent", True),
    ("delete-sent", "order_delete_sent", True),
    ("redis-used", "redis_live_accessed", True),
    ("dispatch-used", "broker_dispatch_entered", True),
    ("runtime-used", "runtime_live_entered", True),
    ("orders-sent", "real_orders_sent", True),
)

TEXT_CASES: tuple[tuple[str, str, str, str], ...] = (
    ("runtime-authorized", "docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json", '"authorization_status": "NOT_ISSUED"', '"authorization_status": "ISSUED"'),
    ("runtime-contract-live", "docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json", '"runtime_live": false', '"runtime_live": true'),
    ("creator-public", "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs", "pub(crate) fn create_stage8b_r2a8_owner_signed_intake_from_owner", "pub fn create_stage8b_r2a8_owner_signed_intake_from_owner"),
    ("creator-no-owner", "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs", ".single_exact_dispatch_ready_request()", ".single_exact_dispatch_ready_request_removed()"),
    ("creator-no-sources", "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs", ".stage8b_r2a8_current_snapshots(issuer)", ".stage8b_r2a8_current_snapshots_removed(issuer)"),
    ("creator-no-sign", "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs", "sign_stage8b_r2a8_current_source_commitment", "sign_removed"),
    ("creator-no-lock", "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs", "create_new(true)", "create_new(false)"),
    ("creator-no-atomic", "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs", "atomic_write_fixed(", "atomic_write_removed("),
    ("opaque-source-public", "crates/finam-gateway/src/stage8a1_execution_capability.rs", "pub(crate) fn stage8b_r2a8_current_snapshots", "pub fn stage8b_r2a8_current_snapshots"),
    ("stager-cli", "crates/finam-gateway/src/bin/stage8b-r2a8-production-intake-stager.rs", "std::env::args_os().len() != 1", "false"),
    ("stager-no-validation", "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs", "validate_production_writer_intake(&intake, &config, config_sha)?;", "drop(intake.clone());"),
    ("stager-no-atomic", "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs", "atomic_write_fixed(&output, &bytes, STAGE8B_R2A8_CURRENT_SOURCE_INPUT_UID)?;", "drop(output);"),
    ("launcher-no-nofollow", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "libc::O_CLOEXEC | libc::O_NOFOLLOW", "libc::O_CLOEXEC"),
    ("launcher-no-setid-check", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "libc::S_ISUID | libc::S_ISGID", "0"),
    ("launcher-no-filecap", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", 'CString::new("security.capability")', 'CString::new("ignored.capability")'),
    ("launcher-no-close-range", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "libc::close_range(8, u32::MAX, 0)", "libc::close_range(99, u32::MAX, 0)"),
    ("launcher-no-close-range-fallback", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", 'std::fs::read_dir("/proc/self/fd")', 'std::fs::read_dir("/proc/self/fd-disabled")'),
    ("launcher-parent-channel-leak", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "libc::close(parent_channel.as_raw_fd())", "drop(parent_channel)"),
    ("launcher-no-nnp", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "libc::PR_SET_NO_NEW_PRIVS", "libc::PR_GET_NO_NEW_PRIVS"),
    ("launcher-no-groups", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "libc::setgroups", "libc::getgroups"),
    ("launcher-no-resuid", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "libc::setresuid", "libc::getresuid"),
    ("launcher-path-exec", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "libc::fexecve", "libc::execve"),
    ("launcher-no-terminal-wrap", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "r2b_root_terminal_record", "terminal_without_root_envelope"),
    ("launcher-no-root-persist", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "persist_r2b_root_terminal_json", "persist_removed"),
    ("launcher-no-fexecve-fault-proof", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", 'controlled_fault("FEXECVE_FAILURE")', "controlled_fault_removed"),
    ("rehearsal-no-helper-crash", "scripts/stage8b_p_r2b_r3_linux_custody_rehearsal.sh", "run_supervisor_fault HELPER_CRASH_AFTER_STARTED", "helper_crash_test_removed"),
    ("rehearsal-no-fsync-failure", "scripts/stage8b_p_r2b_r3_linux_custody_rehearsal.sh", "run_supervisor_fault FINALIZER_FSYNC_FAILURE", "fsync_test_removed"),
    ("rehearsal-no-loopback-server", "scripts/stage8b_p_r2b_r3_linux_custody_rehearsal.sh", '"$SERVER" "$operation"', "controlled_tls_server_removed"),
    ("gate-drops-inherited-r2a8-negatives", "scripts/stage8b_p_r2b_proposal_gate.sh", "python3 scripts/stage8b_p_r2a8_negative_harness.py", "true # inherited negatives removed"),
    ("handoff-safety-drops-runtime-closure", "scripts/stage8b_p_r2b_handoff_safety_check.py", '"runtime_live_entered": False', '"runtime_live_entered": True'),
    ("helper-credential-before-receipt", "tools/stage8b-readonly-preflight/src/r2a5.rs", "let receipt = consume_sealed_r2b_admission_receipt(&executable)?;", "let receipt = consume_sealed_r2b_admission_receipt_removed(&executable)?;"),
    ("receipt-owner", "tools/stage8b-readonly-preflight/src/r2a5.rs", "receipt_stat.st_uid != 0", "receipt_stat.st_uid != 8301"),
    ("admission-owner", "tools/stage8b-readonly-preflight/src/r2a5.rs", "admission_stat.st_uid != 0", "admission_stat.st_uid != 8301"),
    ("nonce-owner", "tools/stage8b-readonly-preflight/src/r2a5.rs", "nonce_stat.st_uid != 0", "nonce_stat.st_uid != 8301"),
    ("terminal-owner", "tools/stage8b-readonly-preflight/src/r2a5.rs", "terminal_stat.st_uid != 0", "terminal_stat.st_uid != 8301"),
    ("nonce-inode", "tools/stage8b-readonly-preflight/src/r2a5.rs", "receipt.nonce_marker_device != nonce_stat.st_dev", "false"),
    ("admission-inode", "tools/stage8b-readonly-preflight/src/r2a5.rs", "receipt.admission_record_device != admission_stat.st_dev", "false"),
    ("channel-inode", "tools/stage8b-readonly-preflight/src/r2a5.rs", "receipt.terminal_channel_device != terminal_stat.st_dev", "false"),
    ("evidence-dir-mode-code", "tools/stage8b-readonly-preflight/src/r2a5.rs", "R2B_EVIDENCE_DIRECTORY_MODE: u32 = 0o700", "R2B_EVIDENCE_DIRECTORY_MODE: u32 = 0o730"),
    ("evidence-file-mode-code", "tools/stage8b-readonly-preflight/src/r2a5.rs", "R2B_EVIDENCE_FILE_MODE: u32 = 0o400", "R2B_EVIDENCE_FILE_MODE: u32 = 0o640"),
    ("root-record-no-pid", "tools/stage8b-readonly-preflight/src/r2a5.rs", "pub child_pid: Option<i32>", "pub child_pid_removed: Option<i32>"),
    ("root-record-no-exit", "tools/stage8b-readonly-preflight/src/r2a5.rs", "pub child_exit_code: Option<i32>", "pub child_exit_code_removed: Option<i32>"),
    ("response-no-status", "tools/stage8b-readonly-preflight/src/r2a3.rs", "pub status: Option<u16>", "pub status_removed: Option<u16>"),
    ("response-no-length", "tools/stage8b-readonly-preflight/src/r2a3.rs", "pub observed_body_length: Option<usize>", "pub observed_body_length_removed: Option<usize>"),
    ("query-limit-code", "tools/stage8b-readonly-preflight/src/lib.rs", "pub const TRADES_LIMIT: usize = 1_000;", "pub const TRADES_LIMIT: usize = 999;"),
    ("status-stale", "docs/current-status.md", "Stage 8B-P R2B Proposal R3", "Stage 8B-P R2B Proposal R2"),
    ("matrix-weakened", "docs/stage-8/STAGE8B_P_R2B_PROPOSAL_ACCEPTANCE_MATRIX_2026-08-27.csv", "R2B-P-052,evidence,UID8301 cannot write truncate chmod unlink rename or recreate terminal evidence,adversarial Linux rehearsal,PASS", "R2B-P-052,evidence,terminal exists,declaration only,PASS"),
)


def run_checker(root: Path) -> int:
    return subprocess.run(
        ["python3", str(root / CHECKER)], cwd=root,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    ).returncode


def main() -> None:
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2b-r3-negative-") as temporary:
        base = Path(temporary) / "base"
        for relative in FILES:
            destination = base / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        if run_checker(base) != 0:
            raise SystemExit("stage8b-p-r2b-proposal-negative: FAIL baseline")

        for name, dotted, replacement in JSON_CASES:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            target = case / AUTHORITY
            document = json.loads(target.read_text())
            set_path(document, dotted, replacement)
            target.write_text(json.dumps(document, indent=2) + "\n")
            if run_checker(case) == 0:
                raise SystemExit(f"stage8b-p-r2b-proposal-negative: FAIL accepted {name}")
            passed += 1

        for name, dotted, replacement in BUILD_CASES:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            target = case / BUILD
            document = json.loads(target.read_text())
            set_path(document, dotted, replacement)
            target.write_text(json.dumps(document, indent=2) + "\n")
            if run_checker(case) == 0:
                raise SystemExit(f"stage8b-p-r2b-proposal-negative: FAIL accepted {name}")
            passed += 1

        for name, relative, old, new in TEXT_CASES:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            target = case / relative
            source = target.read_text()
            if source.count(old) < 1:
                raise SystemExit(f"stage8b-p-r2b-proposal-negative: FAIL setup {name}")
            target.write_text(source.replace(old, new, 1))
            if run_checker(case) == 0:
                raise SystemExit(f"stage8b-p-r2b-proposal-negative: FAIL accepted {name}")
            passed += 1

    expected = len(JSON_CASES) + len(BUILD_CASES) + len(TEXT_CASES)
    if passed != expected or expected < 125:
        raise SystemExit(f"stage8b-p-r2b-proposal-negative: FAIL matrix cardinality {passed}/{expected}")
    print(f"stage8b-p-r2b-proposal-negative: PASS {passed}/{expected}")


if __name__ == "__main__":
    main()
