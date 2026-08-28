#!/usr/bin/env python3
"""Executable-aware adversarial matrix for Stage 8B-P R2B Proposal R2."""

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
BUILD = "docs/stage-8/stage8b-p-r2b-r2-build-evidence.json"

FILES = (
    AUTHORITY,
    BUILD,
    "docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json",
    "docs/stage-8/stage8b-p-r2b-helper-acceptance-authority.json",
    "docs/stage-8/stage8b-p-r2b-accepted-helper-sha256.txt",
    "docs/stage-8/STAGE8B_P_R2B_PROPOSAL_2026-08-27.md",
    "docs/stage-8/STAGE8B_P_R2A8_R1_ACCEPTANCE_CLOSURE_2026-08-27.md",
    "docs/stage-8/STAGE8B_P_R2B_PROPOSAL_ACCEPTANCE_MATRIX_2026-08-27.csv",
    "docs/current-status.md",
    "crates/finam-gateway/Cargo.toml",
    "crates/finam-gateway/src/lib.rs",
    "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs",
    "crates/finam-gateway/src/bin/stage8b-r2a8-production-intake-producer.rs",
    "crates/finam-gateway/src/bin/stage8b-r2a8-production-current-source-writer.rs",
    "tools/stage8b-readonly-preflight/src/lib.rs",
    "tools/stage8b-readonly-preflight/Cargo.toml",
    "tools/stage8b-readonly-preflight/src/main.rs",
    "tools/stage8b-readonly-preflight/src/r2a3.rs",
    "tools/stage8b-readonly-preflight/src/r2a5.rs",
    "tools/stage8b-readonly-preflight/src/bin/stage8b-r2a5-launcher.rs",
    "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs",
    "scripts/stage8b_p_r2b_r2_linux_custody_rehearsal.sh",
    "scripts/stage8b_p_r2b_proposal_gate.sh",
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
    ("wrong-predecessor", "accepted_predecessor.source_ref", "0" * 40),
    ("multi-operation-selection", "proposed_capability.selection_count", 2),
    ("background-loop-opened", "proposed_capability.background_loop", True),
    ("result-influences-execution", "proposed_capability.result_may_influence_execution", True),
    ("alternate-host", "network_contract.exact_host", "example.invalid"),
    ("redirect-opened", "network_contract.redirects_allowed", True),
    ("proxy-opened", "network_contract.proxy_allowed", True),
    ("retry-opened", "network_contract.automatic_retries_allowed", True),
    ("order-post-opened", "network_contract.order_post_allowed", True),
    ("order-delete-opened", "network_contract.order_delete_allowed", True),
    ("missing-production-writer", "production_composition.exact_executable_sequence.0", "stage8b-r2a7-controlled-seeder"),
    ("manifest-before-writer", "production_composition.exact_executable_sequence.0", "stage8b-r2a8-current-manifest-issuer"),
    ("adapter-before-manifest", "production_composition.exact_executable_sequence.1", "stage8b-r2a7-source-adapter"),
    ("duplicate-writer", "production_composition.exact_executable_sequence.2", "stage8b-r2a8-production-current-source-writer"),
    ("wrong-producer-cardinality", "production_composition.exact_invocation_cardinality.stage8b-r2a5-authority-producer", 1),
    ("wrong-issuer-cardinality", "production_composition.exact_invocation_cardinality.stage8b-r2a5-authority-issuer", 1),
    ("helper-before-launcher", "production_composition.exact_executable_sequence.6", "accepted-stage8b-readonly-preflight"),
    ("fixture-feature-in-production", "production_composition.fixture_features_allowed_in_production", True),
    ("controlled-resolution-in-production", "production_composition.production_may_resolve_controlled_hash_domain", True),
    ("caller-writer-arguments", "production_current_source_writer.caller_arguments_allowed", True),
    ("caller-writer-path", "production_current_source_writer.caller_paths_allowed", True),
    ("caller-readiness-snapshot", "production_current_source_writer.caller_snapshots_allowed", True),
    ("writer-network-access", "production_current_source_writer.network_access_allowed", True),
    ("writer-credential-access", "production_current_source_writer.finam_credential_access_allowed", True),
    ("writer-redis-access", "production_current_source_writer.redis_access_allowed", True),
    ("writer-not-atomic", "production_current_source_writer.atomic_publication", False),
    ("writer-no-owner-restart", "production_current_source_writer.stage7b_owner_restart_required", False),
    ("missing-intake-producer", "production_composition.exact_executable_sequence.0", "stage8b-r2a8-production-current-source-writer"),
    ("caller-intake-json", "production_intake_producer.caller_json_allowed", True),
    ("caller-intake-readiness", "production_intake_producer.caller_readiness_allowed", True),
    ("caller-intake-truth", "production_intake_producer.caller_broker_truth_allowed", True),
    ("caller-intake-broker-readiness", "production_intake_producer.caller_broker_readiness_allowed", True),
    ("caller-intake-timestamp", "production_intake_producer.caller_timestamps_allowed", True),
    ("intake-producer-network", "production_intake_producer.network_access_allowed", True),
    ("intake-producer-credential", "production_intake_producer.finam_credential_access_allowed", True),
    ("intake-producer-not-atomic", "production_intake_producer.atomic_write", False),
    ("intake-upstream-owner-drift", "production_intake_producer.upstream_owner_component", "manual-signer"),
    ("launcher-not-root", "r2b_launcher_and_admission.launcher_uid", 8301),
    ("nonce-registry-group-writable", "r2b_launcher_and_admission.nonce_registry_mode", "0770"),
    ("helper-dac-override", "r2b_launcher_and_admission.helper_capabilities", ["CAP_DAC_OVERRIDE"]),
    ("helper-writes-nonce", "r2b_launcher_and_admission.helper_can_write_nonce_registry", True),
    ("helper-deletes-nonce", "r2b_launcher_and_admission.helper_can_delete_nonce_marker", True),
    ("wrong-receipt-fd", "r2b_launcher_and_admission.sealed_receipt_fd", 4),
    ("wrong-helper-uid", "r2b_launcher_and_admission.privilege_drop_uid", 0),
    ("admission-replay-open", "r2b_launcher_and_admission.automatic_replay_allowed", True),
    ("helper-execution-path-rebindable", "r2b_launcher_and_admission.helper_execution", "verify-then-exec-path"),
    ("evidence-root-drift", "evidence_contract.fixed_root", "/tmp/r2b"),
    ("evidence-owner-drift", "evidence_contract.file_uid", 0),
    ("evidence-group-drift", "evidence_contract.file_gid", 0),
    ("evidence-mode-drift", "evidence_contract.file_mode", "0666"),
    ("evidence-overwrite-opened", "evidence_contract.create_new", False),
    ("evidence-symlink-opened", "evidence_contract.no_follow", False),
    ("evidence-multilink-opened", "evidence_contract.single_link_required", False),
    ("evidence-file-fsync-removed", "evidence_contract.file_fsync", False),
    ("evidence-directory-fsync-removed", "evidence_contract.directory_fsync", False),
    ("duplicate-terminal-record", "evidence_contract.one_terminal_record_per_nonce", False),
    ("partial-attempts-disabled", "evidence_contract.partial_attempts_preserved_on_failure", False),
    ("raw-body-export", "evidence_contract.raw_body_recorded", True),
    ("token-export", "evidence_contract.token_recorded", True),
    ("account-export", "evidence_contract.account_id_recorded", True),
    ("query-method-drift", "freshness_and_validation.query_policy.method", "POST"),
    ("query-route-drift", "freshness_and_validation.query_policy.route_template", "/v2/trades"),
    ("query-parameter-drift", "freshness_and_validation.query_policy.trades_query_parameter_names.1", "from"),
    ("trades-limit-drift", "freshness_and_validation.query_policy.trades_limit", 999),
    ("trades-window-drift", "freshness_and_validation.query_policy.trades_window_ms", 1),
    ("query-time-basis-drift", "freshness_and_validation.query_policy.time_basis", "wall_clock_now"),
    ("trades-start-exclusive", "freshness_and_validation.query_policy.window_start_semantics", "request_requested_at_minus_window_exclusive"),
    ("trades-end-inclusive", "freshness_and_validation.query_policy.window_end_semantics", "request_requested_at_inclusive"),
    ("pagination-opened", "freshness_and_validation.query_policy.pagination", "cursor"),
    ("full-page-rule-removed", "freshness_and_validation.query_policy.full_page_means_incomplete", False),
    ("caller-query-override", "freshness_and_validation.query_policy.caller_override_allowed", True),
    ("broker-dispatch-opened", "closed_surfaces.broker_dispatch", True),
    ("redis-live-opened", "closed_surfaces.redis_live_consumer", True),
    ("runtime-live-opened", "closed_surfaces.runtime_live", True),
    ("real-orders-opened", "closed_surfaces.real_orders", True),
)

BUILD_CASES: tuple[tuple[str, str, Any], ...] = (
    ("build-run-count-reduced", "run_count", 1),
    ("production-fixture-dependency", "fixture_dependencies_in_production", True),
    ("production-writer-hash-drift", "production_binaries.stage8b-r2a8-production-current-source-writer.build_a_sha256", "0" * 64),
    ("production-producer-hash-drift", "production_binaries.stage8b-r2a8-production-intake-producer.build_a_sha256", "0" * 64),
    ("production-launcher-hash-drift", "production_binaries.stage8b-r2b-launcher.build_a_sha256", "0" * 64),
    ("controlled-adapter-hash-drift", "controlled_qualification_binaries.stage8b-r2a7-source-adapter.build_b_sha256", "0" * 64),
    ("place-regression-not-pass", "controlled_place_regression", "PENDING"),
    ("linux-terminal-test-not-pass", "linux_terminal_evidence_test", "FAIL"),
    ("build-evidence-authorized", "authorization_status", "ISSUED"),
    ("build-evidence-network-used", "finam_network_accessed", True),
)

TEXT_CASES: tuple[tuple[str, str, str, str], ...] = (
    ("runtime-contract-authorized", "docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json", '"authorization_status": "NOT_ISSUED"', '"authorization_status": "ISSUED"'),
    ("runtime-contract-live-opened", "docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json", '"runtime_live": false', '"runtime_live": true'),
    ("runtime-contract-end-inclusive", "docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json", '"window_end_semantics": "request_requested_at_exclusive"', '"window_end_semantics": "request_requested_at_inclusive"'),
    ("runtime-contract-not-embedded", "tools/stage8b-readonly-preflight/src/r2a5.rs", "stage8b-p-r2b-runtime-composition-contract.json", "stage8b-p-r2b-proposal-authority.json"),
    ("writer-accepts-cli", "crates/finam-gateway/src/bin/stage8b-r2a8-production-current-source-writer.rs", "std::env::args_os().len() != 1", "false"),
    ("intake-producer-accepts-cli", "crates/finam-gateway/src/bin/stage8b-r2a8-production-intake-producer.rs", "std::env::args_os().len() != 1", "false"),
    ("intake-producer-skips-signature-validation", "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs", "validate_production_writer_intake(&intake, &config, config_sha)?;", "drop(intake.clone());"),
    ("intake-producer-skips-atomic-write", "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs", "atomic_write_fixed(&output, &bytes, STAGE8B_R2A8_CURRENT_SOURCE_INPUT_UID)?;", "drop(output);"),
    ("owner-seam-public", "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs", "pub(crate) fn publish_stage8b_r2a8_trusted_current_source_from_owner(", "pub fn publish_stage8b_r2a8_trusted_current_source_from_owner("),
    ("writer-skips-owner-restart", "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs", "Stage7bRecoveryReadyOwner::restart(", "Stage7bRecoveryReadyOwner::restart_removed("),
    ("writer-skips-exact-request", "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs", "identity != intake.durable_request_identity", "false"),
    ("failure-drops-current-attempt", "tools/stage8b-readonly-preflight/src/r2a3.rs", "failed_request = Some(evidence);", "drop(evidence);"),
    ("helper-skips-preserving-pipeline", "tools/stage8b-readonly-preflight/src/r2a5.rs", "execute_r2a3_pipeline_preserving_attempts(", "execute_r2a3_pipeline("),
    ("success-without-terminal-evidence", "tools/stage8b-readonly-preflight/src/r2a5.rs", "let terminal = terminal_evidence(", "let terminal = terminal_evidence_removed("),
    ("terminal-not-persisted", "tools/stage8b-readonly-preflight/src/r2a5.rs", "persist_terminal_evidence(&terminal)", "drop(&terminal)"),
    ("helper-claims-nonce-after-drop", "tools/stage8b-readonly-preflight/src/r2a5.rs", "consume_sealed_r2b_admission_receipt(&prepared, &executable)?;", "claim_nonce(&Path::new(PRODUCTION_ROOT).join(\"used-run-nonces\"), &prepared.package.run_nonce_sha256)?;"),
    ("launcher-skips-admission", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "prepare_r2b_privileged_admission(&accepted)?", "Vec::new()"),
    ("launcher-opens-helper-after-nonce", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "let helper = open_accepted_helper(&accepted)?;", "let helper = std::fs::File::open(\"/dev/null\")?;"),
    ("launcher-path-exec-instead-of-fd", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "libc::fexecve", "libc::execve"),
    ("launcher-reuses-receipt-fd-for-helper", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "libc::F_DUPFD_CLOEXEC", "libc::F_DUPFD"),
    ("launcher-omits-helper-started", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "record_r2b_helper_started(&receipt)?;", "drop(receipt.clone());"),
    ("launcher-skips-sealing", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "libc::F_ADD_SEALS", "libc::F_GET_SEALS"),
    ("launcher-skips-groups-drop", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "libc::setgroups", "libc::getgroups"),
    ("launcher-skips-uid-drop", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "libc::setuid", "libc::geteuid"),
    ("acceptance-executable-signature-test-removed", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs", "embedded_helper_acceptance_signature_and_hash_are_valid", "embedded_helper_hash_only"),
    ("response-failure-drops-status", "tools/stage8b-readonly-preflight/src/r2a3.rs", "pub status: Option<u16>", "pub status_removed: Option<u16>"),
    ("response-failure-drops-length", "tools/stage8b-readonly-preflight/src/r2a3.rs", "pub observed_body_length: Option<usize>", "pub observed_body_length_removed: Option<usize>"),
    ("response-failure-drops-cap", "tools/stage8b-readonly-preflight/src/r2a3.rs", "pub configured_body_cap: usize", "pub configured_body_cap_removed: usize"),
    ("response-failure-drops-overflow", "tools/stage8b-readonly-preflight/src/r2a3.rs", "pub body_overflow: bool", "pub body_overflow_removed: bool"),
    ("evidence-create-new-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", "let mut pending = OpenOptions::new()\n        .write(true)\n        .create_new(true)", "let mut pending = OpenOptions::new()\n        .write(true)\n        .create(true)"),
    ("evidence-nofollow-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", ".mode(R2B_EVIDENCE_FILE_MODE)\n        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)", ".mode(R2B_EVIDENCE_FILE_MODE)\n        .custom_flags(libc::O_CLOEXEC)"),
    ("evidence-link-check-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", "if !metadata.file_type().is_file()\n        || metadata.nlink() != 1", "if !metadata.file_type().is_file()\n        || false"),
    ("evidence-file-fsync-code-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", ".and_then(|_| pending.sync_all())", ".and_then(|_| Ok(()))"),
    ("evidence-directory-fsync-code-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", "File::open(root)\n        .and_then(|directory| directory.sync_all())", "File::open(root)\n        .and_then(|_| Ok(()))"),
    ("evidence-atomic-publish-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", "std::fs::hard_link(&pending_path, &final_path)", "std::fs::rename(&pending_path, &final_path)"),
    ("query-limit-code-drift", "tools/stage8b-readonly-preflight/src/lib.rs", "pub const TRADES_LIMIT: usize = 1_000;", "pub const TRADES_LIMIT: usize = 999;"),
    ("query-window-code-drift", "tools/stage8b-readonly-preflight/src/lib.rs", "pub const TRADES_WINDOW_MS: i64 = 24 * 60 * 60 * 1_000;", "pub const TRADES_WINDOW_MS: i64 = 1;"),
    ("current-status-stale", "docs/current-status.md", "Stage 8B-P R2B Proposal R2", "Stage 8B-P R2B old proposal"),
    ("matrix-evidence-declaration-only", "docs/stage-8/STAGE8B_P_R2B_PROPOSAL_ACCEPTANCE_MATRIX_2026-08-27.csv", "R2B-P-021,evidence,success and failure preserve attempts and publish one durable terminal record,helper code plus Linux tests plus checker,PASS", "R2B-P-021,evidence,durable redacted evidence required,proposal authority,PASS"),
)


def run_checker(root: Path) -> int:
    return subprocess.run(
        ["python3", str(root / CHECKER)], cwd=root,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    ).returncode


def main() -> None:
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2b-r2-negative-") as temporary:
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
    if passed != expected or expected < 100:
        raise SystemExit("stage8b-p-r2b-proposal-negative: FAIL matrix cardinality")
    print(f"stage8b-p-r2b-proposal-negative: PASS {passed}/{expected}")


if __name__ == "__main__":
    main()
