#!/usr/bin/env python3
"""Stage 5G-f negative mutation harness."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

import stage5g_f_check as checker

ROOT = Path(__file__).resolve().parents[1]
SOURCE = "crates/strategy-runtime-core/src/stage5g_protective_completion.rs"
RESTART = "crates/strategy-runtime-core/src/stage5g_clean_restart.rs"
STAGE5C = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
CONTRACT = "docs/stage-5/stage5g-f-protective-completion-contract.json"
DESIGN = "docs/stage-5/stage5g-f-protective-completion-contract.md"
LIB = "crates/strategy-runtime-core/src/lib.rs"
CARGO = "crates/strategy-runtime-core/Cargo.toml"
STAGE5D = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
GATE = "scripts/stage5g_f_r7_gate.sh"
PRESEAL = "scripts/stage5g_f_preseal_check.py"
HANDOFF = "scripts/make_stage5g_f_handoff_archive.py"


def fail(message: str) -> None:
    raise SystemExit(f"stage5g-f-negative: FAIL: {message}")


def mutate(root: Path, relative: str, old: str, new: str, count: int | None = 1) -> None:
    path = root / relative
    text = path.read_text()
    if old not in text:
        fail(f"mutation target missing in {relative}: {old}")
    path.write_text(text.replace(old, new) if count is None else text.replace(old, new, count))


def mutate_between(
    root: Path,
    relative: str,
    begin: str,
    end: str,
    old: str,
    new: str,
    count: int | None = 1,
) -> None:
    path = root / relative
    text = path.read_text()
    start = text.find(begin)
    if start == -1:
        fail(f"mutation section begin missing in {relative}: {begin}")
    stop = text.find(end, start + len(begin))
    if stop == -1:
        fail(f"mutation section end missing in {relative}: {end}")
    section = text[start:stop]
    if old not in section:
        fail(f"mutation target missing in {relative} section: {old}")
    mutated = section.replace(old, new) if count is None else section.replace(old, new, count)
    path.write_text(text[:start] + mutated + text[stop:])


def run_checker(root: Path) -> bool:
    result = subprocess.run(
        ["python3", "scripts/stage5g_f_check.py", "--root", str(root), "--skip-git"],
        cwd=root,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        text=True,
        check=False,
    )
    return result.returncode == 0


def source_marker_cases() -> list[tuple[str, callable]]:
    cases: list[tuple[str, callable]] = []
    for idx, marker in enumerate(checker.REQUIRED_SOURCE_MARKERS, start=1):
        cases.append((
            f"source-guard-marker-{idx:02d}",
            lambda root, marker=marker: mutate(root, SOURCE, marker, marker[::-1], count=None),
        ))
    return cases


def restart_marker_cases() -> list[tuple[str, callable]]:
    cases: list[tuple[str, callable]] = []
    for idx, marker in enumerate(checker.REQUIRED_RESTART_MARKERS, start=1):
        cases.append((
            f"restart-guard-marker-{idx:02d}",
            lambda root, marker=marker: mutate(root, RESTART, marker, marker[::-1], count=None),
        ))
    return cases


def focused_test_cases() -> list[tuple[str, callable]]:
    cases: list[tuple[str, callable]] = []
    for test in checker.REQUIRED_TESTS:
        cases.append((
            f"remove-{test}",
            lambda root, test=test: mutate(root, SOURCE, f"fn {test}(", f"fn removed_{test}("),
        ))
    return cases


def contract_cases() -> list[tuple[str, callable]]:
    cases: list[tuple[str, callable]] = []
    for scenario in checker.EXPECTED_SCENARIOS:
        cases.append((
            f"contract-rename-{scenario.lower()}",
            lambda root, scenario=scenario: mutate(root, CONTRACT, scenario, f"{scenario}_DRIFT"),
        ))
    for surface in [
        "finam_native_stop_endpoint",
        "finam_sltp_bracket_endpoint",
        "http_post_delete",
        "redis_live_consumer",
        "broker_dispatch",
        "second_callback_path",
        "runtime_live",
        "real_orders",
        "stage5g_g",
        "stage5g_h",
        "stage6",
    ]:
        cases.append((
            f"open-{surface}",
            lambda root, surface=surface: mutate(root, CONTRACT, f'"{surface}": false', f'"{surface}": true'),
        ))
    cases.extend([
        ("lower-negative-floor", lambda root: mutate(root, CONTRACT, '"current_stage5g_f_minimum": 460', '"current_stage5g_f_minimum": 1')),
        ("open-production-raw-evidence", lambda root: mutate(root, CONTRACT, '"production_apply_accepts_raw_evidence": false', '"production_apply_accepts_raw_evidence": true')),
        ("wrong-validated-evidence-type", lambda root: mutate(root, CONTRACT, '"validated_evidence_type": "Stage5gValidatedProtectiveEvidence"', '"validated_evidence_type": "Stage5gProtectiveCompletionEvidence"')),
        ("wrong-callback-bridge-file", lambda root: mutate(root, CONTRACT, '"canonical_callback_bridge_file": "crates/strategy-runtime-core/src/stage5c_paper_host.rs"', '"canonical_callback_bridge_file": "crates/strategy-runtime-core/src/stage5g_protective_completion.rs"')),
        ("open-stage5g-direct-raw-callback", lambda root: mutate(root, CONTRACT, '"stage5g_direct_raw_broker_callback_boundary": false', '"stage5g_direct_raw_broker_callback_boundary": true')),
        ("drop-completed-post-runtime-contract", lambda root: mutate(root, CONTRACT, '"successful_completion_owns_post_runtime": true', '"successful_completion_owns_post_runtime": false')),
        ("drop-flat-cleanup-batch-contract", lambda root: mutate(root, CONTRACT, '"flat_cleanup_pending_owns_generated_batch": true', '"flat_cleanup_pending_owns_generated_batch": false')),
        ("drop-generated-cleanup-retention-contract", lambda root: mutate(root, CONTRACT, '"generated_cleanup_intents_retained": true', '"generated_cleanup_intents_retained": false')),
        ("hide-restart-extension-status", lambda root: mutate(root, CONTRACT, '"restart_extension_status": "protective_restart_production_oracle_r7"', '"restart_extension_status": "pending_next_slice"')),
        ("disable-authenticated-protective-restart", lambda root: mutate(root, CONTRACT, '"authenticated_protective_restart": true', '"authenticated_protective_restart": false')),
        ("detach-protective-projection-package", lambda root: mutate(root, CONTRACT, '"protective_projection_in_clean_restart_package": true', '"protective_projection_in_clean_restart_package": false')),
        ("wrong-canonical-protective-issuer", lambda root: mutate(root, CONTRACT, '"canonical_protective_evidence_issuer": "issue_stage5g_canonical_protective_evidence"', '"canonical_protective_evidence_issuer": "validate_stage5g_protective_completion_evidence"')),
        ("wrong-canonical-broker-truth-acceptor-scope", lambda root: mutate(root, CONTRACT, '"canonical_broker_truth_acceptor_scope": "crate_private_production_issuer"', '"canonical_broker_truth_acceptor_scope": "public_raw_acceptor"')),
        ("export-production-raw-validator-contract", lambda root: mutate(root, CONTRACT, '"production_raw_evidence_validator_exported": false', '"production_raw_evidence_validator_exported": true')),
        ("change-completed-policy", lambda root: mutate(root, CONTRACT, '"completed_policy": "not_immediate_when_sibling_cleanup_pending"', '"completed_policy": "immediate_when_flat"')),
        ("open-bar-ohlc-authority", lambda root: mutate(root, CONTRACT, '"bar_ohlc_completion_authority": false', '"bar_ohlc_completion_authority": true')),
        ("wrong-base-ref", lambda root: mutate(root, CONTRACT, checker.BASE, "0" * 40)),
        ("wrong-entry-function", lambda root: mutate(root, CONTRACT, '"apply_stage5g_protective_completion"', '"apply_stage5g_protective_completion_bypass"')),
        ("wrong-authority-issuer", lambda root: mutate(root, CONTRACT, '"authority_issuer": "prepare_stage5g_protective_completion"', '"authority_issuer": "admit_stage5g_protective_completion_authority"')),
        ("wrong-authority-source", lambda root: mutate(root, CONTRACT, '"authority_source": "Stage5gCleanRestartedCapability"', '"authority_source": "caller_fields"')),
        ("open-public-raw-authority-input", lambda root: mutate(root, CONTRACT, '"production_public_raw_authority_input": false', '"production_public_raw_authority_input": true')),
        ("open-standalone-json-restart-codec", lambda root: mutate(root, CONTRACT, '"production_standalone_json_restart_codec": false', '"production_standalone_json_restart_codec": true')),
        ("disable-canonical-callback-bridge", lambda root: mutate(root, CONTRACT, '"canonical_callback_bridge": true', '"canonical_callback_bridge": false')),
        ("attach-callback-transport", lambda root: mutate(root, CONTRACT, '"callback_bridge_transport_attached": false', '"callback_bridge_transport_attached": true')),
        ("open-cleanup-caller-bool-proof", lambda root: mutate(root, CONTRACT, '"cleanup_caller_boolean_proof": false', '"cleanup_caller_boolean_proof": true')),
        ("disable-cleanup-proof-requirement", lambda root: mutate(root, CONTRACT, '"cleanup_requires_escrow_or_terminal_proof": true', '"cleanup_requires_escrow_or_terminal_proof": false')),
        ("allow-exact-replay-append", lambda root: mutate(root, CONTRACT, '"exact_replay_appends_receipt": false', '"exact_replay_appends_receipt": true')),
        ("allow-position-row-summing", lambda root: mutate(root, CONTRACT, '"position_rows_are_summed_to_flat": false', '"position_rows_are_summed_to_flat": true')),
        ("wrong-predecessor-verification-mode", lambda root: mutate(root, CONTRACT, '"mode": "bounded_detached_stage5g_edc_r3"', '"mode": "recursive_stage5g_edc_r3_gate"')),
        ("wrong-predecessor-verification-commit", lambda root: mutate(root, CONTRACT, '"commit": "' + checker.ACCEPTED_EDC_R3 + '"', '"commit": "' + ("1" * 40) + '"')),
        ("open-recursive-historical-lineage", lambda root: mutate(root, CONTRACT, '"runs_recursive_historical_lineage": false', '"runs_recursive_historical_lineage": true')),
        ("remove-predecessor-check-command", lambda root: mutate(root, CONTRACT, '"python3 scripts/stage5g_edc_r3_check.py"', '"python3 scripts/stage5g_edc_r3_check_removed.py"')),
    ])
    return cases


def governance_cases() -> list[tuple[str, callable]]:
    return [
        ("remove-module-link", lambda root: mutate(root, LIB, "mod stage5g_protective_completion;", "// removed stage5g_f module")),
        ("remove-public-facade", lambda root: mutate(root, LIB, "pub use stage5g_protective_completion::", "pub use stage5g_protective_completion_removed::", count=None)),
        ("design-loses-f12-f15", lambda root: mutate(root, DESIGN, "Stage 5F F12–F15 remain no-bar-exit", "Stage 5F F12-F15 drift")),
        ("design-opens-stage5g-g", lambda root: mutate(root, DESIGN, "Only after independent Stage 5G-f acceptance may Stage 5G-g begin", "Stage 5G-g may begin immediately")),
        ("gate-removes-checker", lambda root: mutate(root, GATE, "python3 scripts/stage5g_f_check.py", "# checker removed", count=None)),
        ("gate-removes-negative", lambda root: mutate(root, GATE, "python3 scripts/stage5g_f_negative_harness.py", "# negative removed", count=None)),
        ("gate-removes-preseal", lambda root: mutate(root, GATE, "python3 scripts/stage5g_f_preseal_check.py", "# preseal removed", count=None)),
        ("gate-removes-debug-tests", lambda root: mutate(root, GATE, "cargo test -p strategy-runtime-core --lib stage5g_f_", "# focused debug removed", count=None)),
        ("gate-removes-release-tests", lambda root: mutate(root, GATE, "cargo test --release -p strategy-runtime-core --lib stage5g_f_", "# focused release removed", count=None)),
        ("gate-removes-predecessor-checker", lambda root: mutate(root, GATE, "python3 scripts/stage5g_edc_r3_check.py", "# predecessor checker removed")),
        ("gate-removes-predecessor-negative", lambda root: mutate(root, GATE, "python3 scripts/stage5g_edc_r3_negative_harness.py", "# predecessor negative removed")),
        ("gate-removes-predecessor-release-tests", lambda root: mutate(root, GATE, "cargo test --release -p strategy-runtime-core --lib stage5g_edc_r3_", "# predecessor release tests removed")),
        ("gate-removes-r1-lineage", lambda root: mutate(root, GATE, "a28cedd984d41bd2db4aeb7fd8c125c62ded4b28", "a28cedd984d41bd2db4aeb7fd8c125c62ded4b20")),
        ("preseal-loses-allowlist", lambda root: mutate(root, PRESEAL, "EXPECTED = sorted([", "EXPECTED_DISABLED = sorted([")),
        ("handoff-removes-gate", lambda root: mutate(root, HANDOFF, '["bash", "scripts/stage5g_f_r7_gate.sh"]', '["bash", "scripts/stage5g_f_check.py"]')),
    ]


def forbidden_surface_cases() -> list[tuple[str, callable]]:
    return [
        ("inject-reqwest", lambda root: mutate(root, SOURCE, "use broker_core::{", "use reqwest as forbidden_reqwest;\nuse broker_core::{")),
        ("inject-method-post", lambda root: mutate(root, SOURCE, "use broker_core::{", "use http::Method::POST;\nuse broker_core::{")),
        ("inject-method-delete", lambda root: mutate(root, SOURCE, "use broker_core::{", "use http::Method::DELETE;\nuse broker_core::{")),
        ("inject-finam-namespace", lambda root: mutate(root, SOURCE, "//! Stage 5G-f paper/mock", "//! finam::orders Stage 5G-f paper/mock")),
        ("inject-finam-client", lambda root: mutate(root, SOURCE, "//! Stage 5G-f paper/mock", "//! FinamRestClient Stage 5G-f paper/mock")),
        ("inject-finam-transport", lambda root: mutate(root, SOURCE, "//! Stage 5G-f paper/mock", "//! FinamTransport Stage 5G-f paper/mock")),
        ("inject-redis", lambda root: mutate(root, SOURCE, "use broker_core::{", "use redis::Commands;\nuse broker_core::{")),
        ("inject-runtime-live", lambda root: mutate(root, SOURCE, "//! Stage 5G-f paper/mock", "//! runtime_live_enabled: true Stage 5G-f paper/mock")),
        ("inject-bar-event", lambda root: mutate(root, SOURCE, "//! Stage 5G-f paper/mock", "//! BarEvent Stage 5G-f paper/mock")),
        ("inject-bar-high", lambda root: mutate(root, SOURCE, "//! Stage 5G-f paper/mock", "//! .high Stage 5G-f paper/mock")),
        ("inject-bar-low", lambda root: mutate(root, SOURCE, "//! Stage 5G-f paper/mock", "//! .low Stage 5G-f paper/mock")),
        ("inject-wall-clock", lambda root: mutate(root, SOURCE, "//! Stage 5G-f paper/mock", "//! Utc::now Stage 5G-f paper/mock")),
        ("inject-sleep", lambda root: mutate(root, SOURCE, "//! Stage 5G-f paper/mock", "//! thread::sleep Stage 5G-f paper/mock")),
    ]


def r2_lifecycle_cases() -> list[tuple[str, callable]]:
    return [
        ("drop-validated-evidence-type", lambda root: mutate(root, SOURCE, "pub struct Stage5gValidatedProtectiveEvidence", "pub struct RemovedStage5gValidatedProtectiveEvidence")),
        ("raw-apply-signature", lambda root: mutate(root, SOURCE, "validated: Stage5gValidatedProtectiveEvidence", "evidence: Stage5gProtectiveCompletionEvidence")),
        ("remove-validated-evidence-consume", lambda root: mutate(root, SOURCE, "let evidence = validated.evidence;", "let evidence = Stage5gProtectiveCompletionEvidence")),
        ("drop-committed-state-type", lambda root: mutate(root, SOURCE, "pub struct Stage5gProtectiveCommittedState", "pub struct RemovedStage5gProtectiveCommittedState")),
        ("drop-post-state-summary-type", lambda root: mutate(root, SOURCE, "pub struct Stage5gProtectivePostStateSummary", "pub struct RemovedStage5gProtectivePostStateSummary")),
        ("drop-flat-cleanup-pending-type", lambda root: mutate(root, SOURCE, "pub struct Stage5gProtectiveFlatCleanupPending", "pub struct RemovedStage5gProtectiveFlatCleanupPending")),
        ("drop-flat-cleanup-disposition", lambda root: mutate(root, SOURCE, "Stage5gProtectiveDisposition::FlatCleanupPending", "Stage5gProtectiveDisposition::Completed", count=None)),
        ("drop-completed-owned-post-state", lambda root: mutate(root, SOURCE, "post_state: Stage5gProtectiveCommittedState", "post_state_removed: Stage5gProtectiveCommittedState", count=None)),
        ("drop-cleanup-owned-post-state", lambda root: mutate(root, SOURCE, "post_state: Stage5gProtectiveCommittedState", "post_state_removed: Stage5gProtectiveCommittedState", count=None)),
        ("drop-generated-cleanup-batch-field", lambda root: mutate(root, SOURCE, "generated_cleanup_batch: crate::Stage5cPaperIntentBatch", "generated_cleanup_batch_removed: crate::Stage5cPaperIntentBatch", count=None)),
        ("drop-generated-cleanup-summary-field", lambda root: mutate(root, SOURCE, "generated_cleanup_batch_summary: crate::Stage5cPaperIntentBatchSummary", "generated_cleanup_batch_summary_removed: crate::Stage5cPaperIntentBatchSummary", count=None)),
        ("drop-settled-batch-history-field", lambda root: mutate(root, SOURCE, "settled_batch_history: Vec<crate::Stage5cPaperIntentBatchSummary>", "settled_batch_history_removed: Vec<crate::Stage5cPaperIntentBatchSummary>", count=None)),
        ("drop-stage5c-owned-bridge-call", lambda root: mutate(root, SOURCE, "fn apply_stage5c_owned_protective_lifecycle_bridge(", "fn removed_apply_stage5c_owned_protective_lifecycle_bridge(")),
        ("drop-stage5c-resolver-call", lambda root: mutate(root, SOURCE, "resolve_stage5g_protective_broker_lifecycle_bridge(", "disabled_stage5g_protective_broker_lifecycle_bridge(")),
        ("drop-order-execution-routing", lambda root: mutate(root, SOURCE, "Stage5gProtectiveBrokerLifecycleExecution::Order", "Stage5gProtectiveBrokerLifecycleExecution::RemovedOrder")),
        ("drop-stop-execution-routing", lambda root: mutate(root, SOURCE, "Stage5gProtectiveBrokerLifecycleExecution::StopOrder", "Stage5gProtectiveBrokerLifecycleExecution::RemovedStopOrder")),
        ("drop-bridge-post-state-fingerprint", lambda root: mutate(root, SOURCE, "bridge_post_state_fingerprint_sha256", "bridge_post_state_fingerprint_removed", count=None)),
        ("raw-order-callback-in-stage5g", lambda root: mutate(root, SOURCE, "fn apply_stage5c_owned_protective_lifecycle_bridge(", "crate::BrokerNeutralHybridStrategy::on_broker_order;\nfn apply_stage5c_owned_protective_lifecycle_bridge(")),
        ("raw-stop-callback-in-stage5g", lambda root: mutate(root, SOURCE, "fn apply_stage5c_owned_protective_lifecycle_bridge(", "crate::BrokerNeutralHybridStrategy::on_broker_stop_order;\nfn apply_stage5c_owned_protective_lifecycle_bridge(")),
        ("raw-position-callback-in-stage5g", lambda root: mutate(root, SOURCE, "fn apply_stage5c_owned_protective_lifecycle_bridge(", "crate::BrokerNeutralHybridStrategy::on_broker_position;\nfn apply_stage5c_owned_protective_lifecycle_bridge(")),
        ("reintroduce-cleanup-bool", lambda root: mutate(root, SOURCE, "pub struct Stage5gProtectiveCompleted {", "pub struct Stage5gProtectiveCompleted {\n    pub cleanup_pending: bool,")),
        ("count-and-discard-generated-intents", lambda root: mutate(root, SOURCE, "pub struct Stage5gProtectiveCompleted {", "let generated_cleanup_intents += intents.len();\npub struct Stage5gProtectiveCompleted {")),
        ("drop-stage5c-bridge-enum", lambda root: mutate(root, STAGE5C, "pub(crate) enum Stage5gProtectiveBrokerLifecycleExecution", "pub(crate) enum RemovedStage5gProtectiveBrokerLifecycleExecution")),
        ("drop-stage5c-bridge-input", lambda root: mutate(root, STAGE5C, "pub(crate) struct Stage5gProtectiveBrokerLifecycleBridgeInput", "pub(crate) struct RemovedStage5gProtectiveBrokerLifecycleBridgeInput")),
        ("drop-stage5c-bridge-output", lambda root: mutate(root, STAGE5C, "pub(crate) struct Stage5gProtectiveBrokerLifecycleBridgeOutput", "pub(crate) struct RemovedStage5gProtectiveBrokerLifecycleBridgeOutput")),
        ("drop-stage5c-bridge-function", lambda root: mutate(root, STAGE5C, "pub(crate) fn resolve_stage5g_protective_broker_lifecycle_bridge(", "pub(crate) fn removed_resolve_stage5g_protective_broker_lifecycle_bridge(")),
        ("drop-stage5c-order-callback", lambda root: mutate(root, STAGE5C, "crate::BrokerNeutralHybridStrategy::on_broker_order", "crate::BrokerNeutralHybridStrategy::removed_on_broker_order")),
        ("drop-stage5c-stop-callback", lambda root: mutate(root, STAGE5C, "crate::BrokerNeutralHybridStrategy::on_broker_stop_order", "crate::BrokerNeutralHybridStrategy::removed_on_broker_stop_order")),
        ("drop-stage5c-position-callback", lambda root: mutate(root, STAGE5C, "crate::BrokerNeutralHybridStrategy::on_broker_position", "crate::BrokerNeutralHybridStrategy::removed_on_broker_position")),
        ("drop-stage5c-intent-merge", lambda root: mutate(root, STAGE5C, "stage5g_protective_merge_generated_intents(", "disabled_protective_merge_generated_intents(")),
        ("drop-stage5c-terminal-consistency", lambda root: mutate(root, STAGE5C, "stage5cj_verify_generated_batch_final_pending_consistency(", "disabled_generated_batch_final_pending_consistency(")),
        ("drop-stage5c-batch-summary", lambda root: mutate(root, STAGE5C, "stage5ch_batch_summary(generated_batch)", "disabled_batch_summary(generated_batch)")),
        ("drop-stage5c-post-state-fingerprint", lambda root: mutate(root, STAGE5C, "post_state_fingerprint_sha256", "post_state_fingerprint_removed", count=None)),
        ("drop-callback-generated-cleanup-test", lambda root: mutate(root, SOURCE, "stage5g_f_callback_generated_cleanup_is_retained_and_raw_cleanup_is_blocked", "removed_stage5g_f_callback_generated_cleanup_is_retained_and_raw_cleanup_is_blocked", count=None)),
    ]


def r3_restart_and_issuer_cases() -> list[tuple[str, callable]]:
    return [
        ("drop-restart-projection-schema", lambda root: mutate(root, SOURCE, "pub const STAGE5G_PROTECTIVE_RESTART_PROJECTION_SCHEMA_VERSION: u16 = 1;", "pub const STAGE5G_PROTECTIVE_RESTART_PROJECTION_SCHEMA_VERSION: u16 = 2;")),
        ("drop-canonical-evidence-schema", lambda root: mutate(root, SOURCE, "pub const STAGE5G_PROTECTIVE_CANONICAL_EVIDENCE_SCHEMA_VERSION: u16 = 1;", "pub const STAGE5G_PROTECTIVE_CANONICAL_EVIDENCE_SCHEMA_VERSION: u16 = 2;")),
        ("drop-accepted-broker-truth", lambda root: mutate(root, SOURCE, "pub struct Stage5gAcceptedProtectiveBrokerTruth", "pub struct RemovedStage5gAcceptedProtectiveBrokerTruth")),
        ("drop-canonical-issuer", lambda root: mutate(root, SOURCE, "pub fn issue_stage5g_canonical_protective_evidence(", "pub fn removed_issue_stage5g_canonical_protective_evidence(")),
        ("drop-protective-restart-source-exporter", lambda root: mutate(root, SOURCE, "pub fn stage5g_protective_restart_source_from_transition(", "pub fn removed_stage5g_protective_restart_source_from_transition(")),
        ("drop-protective-continuation-restore", lambda root: mutate(root, SOURCE, "pub fn restore_stage5g_protective_completion_continuation(", "pub fn removed_restore_stage5g_protective_completion_continuation(")),
        ("drop-clean-restart-protective-source", lambda root: mutate(root, RESTART, "ProtectiveLifecycle(crate::stage5g_protective_completion::Stage5gProtectiveRestartSource)", "LifecycleSourceRemoved(crate::stage5g_protective_completion::Stage5gProtectiveRestartSource)")),
        ("drop-clean-restart-protective-kind", lambda root: mutate(root, RESTART, "ProtectiveLifecycleCommitted", "LifecycleKindRemoved", count=None)),
        ("drop-protective-projection-field", lambda root: mutate(root, RESTART, "protective_lifecycle_projection:", "protective_lifecycle_projection_removed:")),
        ("drop-authority-parts", lambda root: mutate(root, RESTART, "into_stage5g_protective_completion_authority_parts", "authority_parts_removed", count=None)),
        ("drop-projection-replay-fingerprint-check", lambda root: mutate(root, SOURCE, "projection.replay_protection_fingerprint_sha256\n            != protective_projection_fingerprint(projection)", "projection.replay_protection_fingerprint_sha256\n            == protective_projection_fingerprint(projection)")),
        ("drop-runtime-fingerprint-check", lambda root: mutate(root, SOURCE, "projection.post_runtime_stage5c_state_fingerprint_sha256\n            != runtime_stage5c_state_fingerprint_sha256", "projection.post_runtime_stage5c_state_fingerprint_sha256\n            == runtime_stage5c_state_fingerprint_sha256")),
        ("drop-canonical-authority-fingerprint-check", lambda root: mutate(root, SOURCE, "accepted.canonical_authority_fingerprint_sha256\n        != authority.summary().authority_fingerprint_sha256", "accepted.canonical_authority_fingerprint_sha256\n        == authority.summary().authority_fingerprint_sha256")),
        ("drop-canonical-issuer-validation", lambda root: mutate(root, SOURCE, "validate_evidence(authority, &accepted.evidence)?;", "// validate_evidence removed", count=1)),
        ("drop-sibling-terminal-validation", lambda root: mutate(root, SOURCE, "validate_preexisting_sibling_terminal(authority, &accepted.evidence)?;", "// validate_preexisting_sibling_terminal removed", count=1)),
        ("exact-replay-appends-receipt", lambda root: mutate(root, SOURCE, "let replay_should_append = matches!(replay, Stage5gProtectiveReplayClassification::New);", "let replay_should_append = true;")),
        ("conflict-replay-not-blocked", lambda root: mutate(root, SOURCE, "if replay == Stage5gProtectiveReplayClassification::FingerprintConflict {", "if false && replay == Stage5gProtectiveReplayClassification::FingerprintConflict {")),
        ("awaiting-does-not-append-receipt", lambda root: mutate(root, SOURCE, "authority.accepted_receipts.push(execution_receipt.clone());", "drop(execution_receipt.clone());", count=1)),
        ("cleanup-batch-cleared", lambda root: mutate(root, SOURCE, "generated_cleanup_batch: bridge.generated_intent_batch,", "generated_cleanup_batch: None,")),
        ("cleanup-history-dropped", lambda root: mutate(root, SOURCE, "settled_batch_history: bridge.settled_batch_history,", "settled_batch_history: Vec::new(),")),
        ("post-runtime-dropped", lambda root: mutate(root, SOURCE, "post_state: callback.post_state,", "post_state: Stage5gProtectiveCommittedState::new(crate::HybridIntradayRuntimeStrategy::default()),", count=None)),
        ("unsupported-flat-prepare-opened", lambda root: mutate(root, SOURCE, "return Err(Stage5gProtectiveBlockReason::UnsupportedCleanRestartLifecycleKind);", "continue;", count=1)),
        ("restore-ignores-protective-projection", lambda root: mutate(root, SOURCE, "let Some(projection) = parts.protective_projection.clone() else {", "let None = parts.protective_projection.clone() else {")),
        ("completed-forged-as-cleanup-pending", lambda root: mutate(root, SOURCE, "Stage5gProtectiveRestartProjectionKind::Completed => {", "Stage5gProtectiveRestartProjectionKind::Completed | Stage5gProtectiveRestartProjectionKind::FlatCleanupPending => {", count=1)),
        ("flat-cleanup-forged-completed", lambda root: mutate(root, SOURCE, "Stage5gProtectiveRestartProjectionKind::FlatCleanupPending => {", "Stage5gProtectiveRestartProjectionKind::FlatCleanupPending | Stage5gProtectiveRestartProjectionKind::Completed => {", count=1)),
        ("remove-r3-authenticated-restart-test", lambda root: mutate(root, SOURCE, "stage5g_f_r3_authenticated_restart_prepares_protective_authority_and_canonical_issuer", "removed_stage5g_f_r3_authenticated_restart_prepares_protective_authority_and_canonical_issuer", count=None)),
        ("remove-r3-awaiting-restore-test", lambda root: mutate(root, SOURCE, "stage5g_f_r3_awaiting_position_truth_survives_authenticated_restart", "removed_stage5g_f_r3_awaiting_position_truth_survives_authenticated_restart", count=None)),
        ("remove-r3-flat-cleanup-restore-test", lambda root: mutate(root, SOURCE, "stage5g_f_r3_flat_cleanup_pending_survives_authenticated_restart", "removed_stage5g_f_r3_flat_cleanup_pending_survives_authenticated_restart", count=None)),
        ("remove-r3-completed-policy-test", lambda root: mutate(root, SOURCE, "stage5g_f_r3_completed_is_not_immediate_when_sibling_cleanup_is_pending", "removed_stage5g_f_r3_completed_is_not_immediate_when_sibling_cleanup_is_pending", count=None)),
    ]


def r4_cleanup_closure_cases() -> list[tuple[str, callable]]:
    stage5c_cleanup_begin = "pub(crate) fn stage5g_protective_cleanup_batch_restart_projection("
    stage5c_cleanup_end = "pub(crate) fn stage5g_protective_cleanup_batch_projection_fingerprint("
    return [
        ("remove-r4-flat-cleanup-settlement-test", lambda root: mutate(root, SOURCE, "stage5g_f_r5_multi_request_cleanup_settles_only_after_all_requests", "removed_stage5g_f_r5_multi_request_cleanup_settles_only_after_all_requests", count=None)),
        ("remove-r4-non-terminal-cleanup-test", lambda root: mutate(root, SOURCE, "stage5g_f_r4_non_terminal_cleanup_truth_keeps_flat_cleanup_pending", "removed_stage5g_f_r4_non_terminal_cleanup_truth_keeps_flat_cleanup_pending", count=None)),
        ("drop-production-canonical-truth-issuer", lambda root: mutate(root, SOURCE, "pub(crate) fn accept_stage5g_canonical_protective_broker_truth", "fn removed_accept_stage5g_canonical_protective_broker_truth")),
        ("make-canonical-truth-issuer-test-only", lambda root: mutate(root, SOURCE, "pub(crate) fn accept_stage5g_canonical_protective_broker_truth", "#[cfg(test)]\npub(crate) fn accept_stage5g_canonical_protective_broker_truth")),
        ("drop-cleanup-batch-projection-field", lambda root: mutate(root, SOURCE, "cleanup_batch_restart_projection:", "cleanup_batch_restart_projection_removed:", count=None)),
        ("drop-cleanup-settlement-fingerprint-field", lambda root: mutate(root, SOURCE, "cleanup_settlement_fingerprint_sha256", "cleanup_settlement_fingerprint_removed", count=None)),
        ("drop-restored-pending-owning-batch", lambda root: mutate(root, SOURCE, "generated_cleanup_batch: crate::Stage5cPaperIntentBatch", "generated_cleanup_batch_removed: crate::Stage5cPaperIntentBatch", count=1)),
        ("drop-restored-pending-summary", lambda root: mutate(root, SOURCE, "generated_cleanup_batch_summary: crate::Stage5cPaperIntentBatchSummary", "generated_cleanup_batch_summary_removed: crate::Stage5cPaperIntentBatchSummary", count=2)),
        ("drop-restored-pending-history", lambda root: mutate(root, SOURCE, "settled_batch_history: Vec<crate::Stage5cPaperIntentBatchSummary>", "settled_batch_history_removed: Vec<crate::Stage5cPaperIntentBatchSummary>", count=2)),
        ("drop-restored-pending-restart-seed", lambda root: mutate(root, SOURCE, "restart_seed: Some(parts.restart_seed)", "restart_seed: None")),
        ("restore-cleanup-from-summary-only", lambda root: mutate(root, SOURCE, "restore_stage5g_protective_cleanup_batch_from_projection", "restore_stage5g_protective_cleanup_batch_from_summary", count=None)),
        ("drop-cleanup-projection-fingerprint-check", lambda root: mutate(root, SOURCE, "cleanup_projection.batch_fingerprint\n                    != crate::stage5c_paper_host::stage5g_protective_cleanup_batch_projection_fingerprint", "cleanup_projection.batch_fingerprint\n                    == crate::stage5c_paper_host::stage5g_protective_cleanup_batch_projection_fingerprint")),
        ("drop-cleanup-truth-boundary", lambda root: mutate(root, SOURCE, "pub fn apply_stage5g_protective_cleanup_completion(", "pub fn removed_apply_stage5g_protective_cleanup_completion(")),
        ("drop-cleanup-truth-acceptor", lambda root: mutate(root, SOURCE, "pub fn accept_stage5g_protective_cleanup_truth(", "pub fn removed_accept_stage5g_protective_cleanup_truth(")),
        ("cleanup-terminal-always-completed", lambda root: mutate(root, SOURCE, "if !stage5g_cleanup_ledger_all_terminal_non_execution(&pending.cleanup_settlement_ledger) {", "if false && !stage5g_cleanup_ledger_all_terminal_non_execution(&pending.cleanup_settlement_ledger) {")),
        ("cleanup-working-forged-terminal", lambda root: mutate(root, SOURCE, "Stage5gProtectiveCleanupOutcome::Pending => {", "Stage5gProtectiveCleanupOutcome::Canceled => {", count=1)),
        ("cleanup-request-id-check-bypassed", lambda root: mutate(root, SOURCE, ".find(|record| record.request_id == request_id)", ".find(|_record| true)")),
        ("cleanup-target-id-check-bypassed", lambda root: mutate(root, SOURCE, "if target_protective_id != record.target_protective_id", "if false && target_protective_id != record.target_protective_id")),
        ("cleanup-chronology-check-bypassed", lambda root: mutate(root, SOURCE, "|| received_ts_utc < record.source_event_ts", "|| false && received_ts_utc < record.source_event_ts")),
        ("completed-drops-cleanup-settlement-fingerprint", lambda root: mutate(root, SOURCE, "cleanup_settlement_fingerprint_sha256: Some(cleanup_settlement_fingerprint_sha256)", "cleanup_settlement_fingerprint_sha256: None")),
        ("completed-restart-drops-cleanup-settlement-fingerprint", lambda root: mutate(root, SOURCE, "cleanup_settlement_fingerprint_sha256: completed.cleanup_settlement_fingerprint_sha256", "cleanup_settlement_fingerprint_sha256: None")),
        ("drop-stage5c-cleanup-projection-type", lambda root: mutate(root, STAGE5C, "pub struct Stage5gProtectiveCleanupBatchRestartProjectionV1", "pub struct RemovedStage5gProtectiveCleanupBatchRestartProjectionV1")),
        ("drop-stage5c-cleanup-record-type", lambda root: mutate(root, STAGE5C, "pub struct Stage5gProtectiveCleanupBatchRestartRecordV1", "pub struct RemovedStage5gProtectiveCleanupBatchRestartRecordV1")),
        ("drop-stage5c-cleanup-projector", lambda root: mutate(root, STAGE5C, "pub(crate) fn stage5g_protective_cleanup_batch_restart_projection(", "pub(crate) fn removed_stage5g_protective_cleanup_batch_restart_projection(")),
        ("drop-stage5c-cleanup-reconstructor", lambda root: mutate(root, STAGE5C, "pub(crate) fn restore_stage5g_protective_cleanup_batch_from_projection(", "pub(crate) fn removed_restore_stage5g_protective_cleanup_batch_from_projection(")),
        ("drop-stage5c-cleanup-fingerprint", lambda root: mutate(root, STAGE5C, "pub(crate) fn stage5g_protective_cleanup_batch_projection_fingerprint(", "pub(crate) fn removed_stage5g_protective_cleanup_batch_projection_fingerprint(")),
        ("stage5c-cleanup-accepts-entry", lambda root: mutate(root, STAGE5C, "if intent_class != crate::BrokerNeutralHybridIntentClass::CancelCleanup", "if false && intent_class != crate::BrokerNeutralHybridIntentClass::CancelCleanup")),
        ("stage5c-cleanup-allows-non-cleanup-action", lambda root: mutate(root, STAGE5C, "_ => return Err(Stage5cIntentSettlementError::UnsupportedIntentAction),", "_ => crate::BrokerNeutralHybridIntent::Cancel { order_id: BrokerOrderId::new(record.target_protective_id.clone()) },", count=1)),
        ("stage5c-cleanup-drops-request-order-check", lambda root: mutate(root, STAGE5C, "projection.request_ids.get(index) != Some(&record.request_id)", "false && projection.request_ids.get(index) != Some(&record.request_id)")),
        ("stage5c-cleanup-drops-fingerprint-check", lambda root: mutate(root, STAGE5C, "projection.batch_fingerprint\n            != stage5g_protective_cleanup_batch_projection_fingerprint(projection)", "projection.batch_fingerprint\n            == stage5g_protective_cleanup_batch_projection_fingerprint(projection)")),
        ("stage5c-cleanup-record-drops-target-id", lambda root: mutate_between(root, STAGE5C, stage5c_cleanup_begin, stage5c_cleanup_end, "target_protective_id,", "target_protective_id: String::new(),", count=1)),
        ("stage5c-cleanup-record-drops-attribution", lambda root: mutate_between(root, STAGE5C, stage5c_cleanup_begin, stage5c_cleanup_end, "expected_attribution: record.expected_attribution.clone()", "expected_attribution: None", count=1)),
        ("stage5c-cleanup-record-drops-source-ts", lambda root: mutate_between(root, STAGE5C, stage5c_cleanup_begin, stage5c_cleanup_end, "source_event_ts: record.source_event_ts", "source_event_ts: 0", count=1)),
        ("drop-all-eight-witness-gprt05", lambda root: mutate(root, SOURCE, "Gprt05WrongOwnerOrCycleBlocks", "RemovedGprt05WrongOwnerOrCycleBlocks", count=1)),
        ("drop-all-eight-witness-gprt06", lambda root: mutate(root, SOURCE, "TP_OTHER", "TP_STAGE5G_F", count=None)),
        ("drop-all-eight-witness-gprt07", lambda root: mutate(root, SOURCE, "\"Triggered\",\n                    nonflat_position_truth()", "\"Triggered\",\n                    flat_position_truth()", count=1)),
        ("drop-all-eight-witness-gprt08", lambda root: mutate(root, SOURCE, "\"Canceled\",\n                    flat_position_truth()", "\"Filled\",\n                    flat_position_truth()", count=1)),
        ("open-stage5g-g-contract", lambda root: mutate(root, CONTRACT, '"stage5g_g": false', '"stage5g_g": true')),
    ]



def r5_cleanup_ledger_cases() -> list[tuple[str, callable]]:
    return [
        ("drop-cleanup-settlement-ledger-type", lambda root: mutate(root, SOURCE, "pub struct Stage5gProtectiveCleanupSettlementLedgerV1", "pub struct RemovedStage5gProtectiveCleanupSettlementLedgerV1")),
        ("drop-cleanup-request-settlement-type", lambda root: mutate(root, SOURCE, "pub struct Stage5gProtectiveCleanupRequestSettlementV1", "pub struct RemovedStage5gProtectiveCleanupRequestSettlementV1")),
        ("drop-cleanup-outcome-type", lambda root: mutate(root, SOURCE, "pub enum Stage5gProtectiveCleanupOutcome", "pub enum RemovedStage5gProtectiveCleanupOutcome")),
        ("drop-cleanup-state-type", lambda root: mutate(root, SOURCE, "pub enum Stage5gProtectiveCleanupSettlementState", "pub enum RemovedStage5gProtectiveCleanupSettlementState")),
        ("drop-pending-authority-helper", lambda root: mutate(root, SOURCE, "fn stage5g_pending_cleanup_authority_sha256", "fn removed_stage5g_pending_cleanup_authority_sha256")),
        ("drop-pending-authority-field", lambda root: mutate(root, SOURCE, "pending_cleanup_authority_sha256", "removed_pending_cleanup_authority_sha256", count=None)),
        ("drop-ledger-before-field", lambda root: mutate(root, SOURCE, "cleanup_ledger_fingerprint_before_sha256", "removed_cleanup_ledger_fingerprint_before_sha256", count=None)),
        ("drop-authority-mismatch-reason", lambda root: mutate(root, SOURCE, "CleanupAuthorityMismatch", "RemovedCleanupAuthorityMismatch", count=None)),
        ("drop-cleanup-conflict-reason", lambda root: mutate(root, SOURCE, "CleanupConflict", "RemovedCleanupConflict", count=None)),
        ("drop-position-truth-required-reason", lambda root: mutate(root, SOURCE, "CleanupPositionTruthRequired", "RemovedCleanupPositionTruthRequired", count=None)),
        ("drop-position-truth-required-transition", lambda root: mutate(root, SOURCE, "CleanupPositionTruthRequired(Box<Stage5gProtectiveRestoredFlatCleanupPending>)", "RemovedCleanupPositionTruthRequired(Box<Stage5gProtectiveRestoredFlatCleanupPending>)")),
        ("drop-ledger-validity-check", lambda root: mutate(root, SOURCE, "if !stage5g_cleanup_ledger_is_valid(&pending.cleanup_settlement_ledger, cleanup_projection)", "if false && !stage5g_cleanup_ledger_is_valid(&pending.cleanup_settlement_ledger, cleanup_projection)", count=1)),
        ("drop-apply-pending-authority-compare", lambda root: mutate(root, SOURCE, "pending_authority != accepted.evidence.pending_cleanup_authority_sha256", "false && pending_authority != accepted.evidence.pending_cleanup_authority_sha256")),
        ("drop-ledger-before-compare", lambda root: mutate(root, SOURCE, "!= accepted.evidence.cleanup_ledger_fingerprint_before_sha256", "== accepted.evidence.cleanup_ledger_fingerprint_before_sha256", count=1)),
        ("drop-batch-fingerprint-compare", lambda root: mutate(root, SOURCE, "!= accepted.evidence.batch_fingerprint_sha256", "== accepted.evidence.batch_fingerprint_sha256", count=1)),
        ("drop-entry-target-compare", lambda root: mutate(root, SOURCE, "entry.target_protective_id != accepted.evidence.target_protective_id", "false && entry.target_protective_id != accepted.evidence.target_protective_id")),
        ("drop-entry-action-compare", lambda root: mutate(root, SOURCE, "entry.base_action != accepted.evidence.base_action", "false && entry.base_action != accepted.evidence.base_action")),
        ("drop-entry-attribution-compare", lambda root: mutate(root, SOURCE, "entry.expected_attribution != accepted.evidence.expected_attribution", "false && entry.expected_attribution != accepted.evidence.expected_attribution")),
        ("force-all-cleanup-terminal", lambda root: mutate(root, SOURCE, "stage5g_cleanup_ledger_all_terminal_non_execution(&pending.cleanup_settlement_ledger)", "true", count=1)),
        ("execution-observed-completes", lambda root: mutate(root, SOURCE, "Stage5gProtectiveCleanupOutcome::ExecutionObserved => {", "Stage5gProtectiveCleanupOutcome::Canceled => {", count=1)),
        ("filled-status-not-execution-race", lambda root: mutate(root, SOURCE, "\"filled\" | \"executed\" | \"triggered\" | \"completed-as-execution\"", "\"executed\" | \"triggered\" | \"completed-as-execution\"")),
        ("delete-action-name-drift", lambda root: mutate(root, SOURCE, "\"delete_stop_limit\"", "\"delete\"", count=1)),
        ("ledger-fingerprint-domain-drift", lambda root: mutate(root, SOURCE, "stage5g_cleanup_ledger_fingerprint", "removed_stage5g_cleanup_ledger_fingerprint", count=1)),
        ("drop-r5-multi-request-test", lambda root: mutate(root, SOURCE, "stage5g_f_r5_multi_request_cleanup_settles_only_after_all_requests", "removed_stage5g_f_r5_multi_request_cleanup_settles_only_after_all_requests", count=None)),
        ("drop-r5-cross-pending-test", lambda root: mutate(root, SOURCE, "stage5g_f_r5_cleanup_token_is_bound_to_exact_pending_authority", "removed_stage5g_f_r5_cleanup_token_is_bound_to_exact_pending_authority", count=None)),
        ("drop-r5-execution-race-test", lambda root: mutate(root, SOURCE, "stage5g_f_r5_cleanup_execution_race_requires_position_truth", "removed_stage5g_f_r5_cleanup_execution_race_requires_position_truth", count=None)),
        ("drop-gprt-artifact-api", lambda root: mutate(root, SOURCE, "pub fn stage5g_f_gprt_artifact_json_pretty", "pub fn removed_stage5g_f_gprt_artifact_json_pretty")),
        ("drop-gprt-artifact-row-type", lambda root: mutate(root, SOURCE, "pub struct Stage5gProtectiveGprtArtifactRow", "pub struct RemovedStage5gProtectiveGprtArtifactRow")),
        ("omit-gprt01-phase-b", lambda root: mutate(root, SOURCE, "row.phase_b_disposition = Some(\"completed\".to_string())", "row.phase_b_disposition = None", count=None)),
        ("open-runtime-live-in-artifact", lambda root: mutate(root, SOURCE, "runtime_live_attached: false", "runtime_live_attached: true", count=1)),
        ("open-redis-in-artifact", lambda root: mutate(root, SOURCE, "redis_command_stream_attached: false", "redis_command_stream_attached: true", count=1)),
        ("open-finam-in-artifact", lambda root: mutate(root, SOURCE, "finam_transport_attached: false", "finam_transport_attached: true", count=1)),
        ("remove-debug-artifact-emit", lambda root: mutate(root, GATE, "stage5g-f-gprt-artifact.debug.json", "removed-stage5g-f-gprt-artifact.debug.json", count=None)),
        ("remove-release-artifact-emit", lambda root: mutate(root, GATE, "stage5g-f-gprt-artifact.release.json", "removed-stage5g-f-gprt-artifact.release.json", count=None)),
        ("remove-artifact-cmp", lambda root: mutate(root, GATE, "cmp \"$artifact_dir/stage5g-f-gprt-artifact.debug.json\" \"$artifact_dir/stage5g-f-gprt-artifact.release.json\"", "true # cmp removed")),
        ("remove-artifact-sha", lambda root: mutate(root, HANDOFF, "stage5g-f-gprt-artifact.sha256", "removed-stage5g-f-gprt-artifact.sha256", count=None)),
        ("drop-r5-gate-pass-marker", lambda root: mutate(root, GATE, "stage5g-f-r7-gate: PASS", "stage5g-f-r7-gate: REMOVED")),
        ("r5-contract-ledger-disabled", lambda root: mutate(root, CONTRACT, '"cleanup_settlement_ledger": "per_request_terminal_nonexecution_required"', '"cleanup_settlement_ledger": "disabled"')),
        ("r5-contract-authority-binding-disabled", lambda root: mutate(root, CONTRACT, '"cleanup_truth_pending_authority_binding": true', '"cleanup_truth_pending_authority_binding": false')),
        ("r5-contract-execution-race-disabled", lambda root: mutate(root, CONTRACT, '"cleanup_execution_race_policy": "position_truth_required"', '"cleanup_execution_race_policy": "completed"')),
        ("r5-contract-artifact-disabled", lambda root: mutate(root, CONTRACT, '"debug_release_gprt_artifact": "stage5g-f-gprt-artifact.json"', '"debug_release_gprt_artifact": "none"')),
        ("remove-bin-artifact-source", lambda root: mutate(root, Path("crates/strategy-runtime-core/src/bin/stage5g_f_gprt_artifact.rs"), "stage5g_f_gprt_artifact_json_pretty", "removed_stage5g_f_gprt_artifact_json_pretty")),
        ("remove-preseal-fixture-source-allowlist", lambda root: mutate(root, PRESEAL, "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs", "removed/hybrid_intraday_runtime.rs")),
    ]

def r6_stable_observation_and_artifact_cases() -> list[tuple[str, callable]]:
    return [
        ("drop-r6-stable-observation-field", lambda root: mutate(root, SOURCE, "cleanup_observation_fingerprint_sha256", "removed_observation_fp", count=None)),
        ("drop-r6-stable-observation-helper", lambda root: mutate(root, SOURCE, "fn stage5g_cleanup_observation_fingerprint(", "fn removed_stage5g_cleanup_observation_fingerprint(")),
        ("observation-fingerprint-includes-ledger-before", lambda root: mutate(root, SOURCE, "evidence.received_ts_utc,\n    ))", "evidence.received_ts_utc,\n        evidence.cleanup_ledger_fingerprint_before_sha256.as_str(),\n    ))", count=1)),
        ("observation-fingerprint-includes-pending-authority", lambda root: mutate(root, SOURCE, "evidence.received_ts_utc,\n    ))", "evidence.received_ts_utc,\n        evidence.pending_cleanup_authority_sha256.as_str(),\n    ))", count=1)),
        ("observation-fingerprint-includes-settlement", lambda root: mutate(root, SOURCE, "evidence.received_ts_utc,\n    ))", "evidence.received_ts_utc,\n        evidence.settlement_fingerprint_sha256.as_str(),\n    ))", count=1)),
        ("exact-cleanup-replay-test-removed", lambda root: mutate(root, SOURCE, "stage5g_f_r6_cleanup_exact_replay_after_partial_settlement_is_idempotent", "removed_stage5g_f_r6_cleanup_exact_replay_after_partial_settlement_is_idempotent", count=None)),
        ("partial-restart-exact-replay-test-removed", lambda root: mutate(root, SOURCE, "stage5g_f_r6_cleanup_exact_replay_after_restart_is_idempotent", "removed_stage5g_f_r6_cleanup_exact_replay_after_restart_is_idempotent", count=None)),
        ("conflicting-replay-test-removed", lambda root: mutate(root, SOURCE, "stage5g_f_r6_cleanup_changed_observation_after_settlement_conflicts", "removed_stage5g_f_r6_cleanup_changed_observation_after_settlement_conflicts", count=None)),
        ("reverse-ack-order-test-removed", lambda root: mutate(root, SOURCE, "stage5g_f_r6_reverse_cleanup_ack_order_is_deterministic", "removed_stage5g_f_r6_reverse_cleanup_ack_order_is_deterministic", count=None)),
        ("source-produced-artifact-runner-removed", lambda root: mutate(root, SOURCE, "stage5g_f_source_produced_gprt_artifact_row", "removed_stage5g_f_source_produced_gprt_artifact_row", count=None)),
        ("artifact-transition-runner-removed", lambda root: mutate(root, SOURCE, "stage5g_f_artifact_transition", "removed_stage5g_f_artifact_transition", count=None)),
        ("artifact-flat-cleanup-runner-removed", lambda root: mutate(root, SOURCE, "stage5g_f_artifact_row_from_flat_cleanup_pending", "removed_stage5g_f_artifact_row_from_flat_cleanup_pending", count=None)),
        ("artifact-authenticated-roundtrip-runner-removed", lambda root: mutate(root, SOURCE, "stage5g_f_authenticated_package_roundtrip", "removed_stage5g_f_authenticated_package_roundtrip", count=None)),
        ("parallel-artifact-verifier-removed", lambda root: mutate(root, SOURCE, "stage5g_f_gprt_artifact_rows_parallel_verified", "removed_stage5g_f_gprt_artifact_rows_parallel_verified", count=None)),
        ("artifact-no-source-map", lambda root: mutate(root, SOURCE, ".map(stage5g_f_source_produced_gprt_artifact_row)", ".map(|scenario| stage5g_f_artifact_base_row(scenario, None, \"static\", \"static\", 0))")),
        ("artifact-omits-phase-a-runtime", lambda root: mutate(root, SOURCE, "phase_a_runtime_semantic_fingerprint_sha256", "removed_phase_a_runtime_semantic_fingerprint_sha256", count=None)),
        ("artifact-omits-phase-a-receipt", lambda root: mutate(root, SOURCE, "phase_a_execution_receipt_fingerprint_sha256", "removed_phase_a_execution_receipt_fingerprint_sha256", count=None)),
        ("artifact-omits-cleanup-request-ids", lambda root: mutate(root, SOURCE, "phase_a_cleanup_request_ids", "removed_phase_a_cleanup_request_ids", count=None)),
        ("artifact-omits-cleanup-batch-fingerprint", lambda root: mutate(root, SOURCE, "phase_a_cleanup_batch_fingerprint_sha256", "removed_phase_a_cleanup_batch_fingerprint_sha256", count=None)),
        ("artifact-omits-cleanup-ledger-fingerprint", lambda root: mutate(root, SOURCE, "phase_a_cleanup_ledger_fingerprint_sha256", "removed_phase_a_cleanup_ledger_fingerprint_sha256", count=None)),
        ("artifact-omits-phase-a-restart-projection", lambda root: mutate(root, SOURCE, "phase_a_protective_restart_projection_fingerprint_sha256", "removed_phase_a_protective_restart_projection_fingerprint_sha256", count=None)),
        ("artifact-omits-phase-b-states", lambda root: mutate(root, SOURCE, "phase_b_cleanup_request_states", "removed_phase_b_cleanup_request_states", count=None)),
        ("artifact-omits-observation-fingerprints", lambda root: mutate(root, SOURCE, "phase_b_cleanup_observation_fingerprints_sha256", "removed_phase_b_cleanup_observation_fingerprints_sha256", count=None)),
        ("artifact-omits-final-ledger-fingerprint", lambda root: mutate(root, SOURCE, "phase_b_final_cleanup_ledger_fingerprint_sha256", "removed_phase_b_final_cleanup_ledger_fingerprint_sha256", count=None)),
        ("artifact-omits-completion-fingerprint", lambda root: mutate(root, SOURCE, "phase_b_completion_fingerprint_sha256", "removed_phase_b_completion_fingerprint_sha256", count=None)),
        ("artifact-omits-final-runtime", lambda root: mutate(root, SOURCE, "phase_b_final_runtime_fingerprint_sha256", "removed_phase_b_final_runtime_fingerprint_sha256", count=None)),
        ("artifact-omits-final-owner", lambda root: mutate(root, SOURCE, "phase_b_final_owner", "removed_phase_b_final_owner", count=None)),
        ("artifact-omits-final-cycle", lambda root: mutate(root, SOURCE, "phase_b_final_cycle_id", "removed_phase_b_final_cycle_id", count=None)),
        ("artifact-omits-final-position", lambda root: mutate(root, SOURCE, "phase_b_final_position_qty", "removed_phase_b_final_position_qty", count=None)),
        ("artifact-omits-completed-restart-projection", lambda root: mutate(root, SOURCE, "phase_b_completed_restart_projection_fingerprint_sha256", "removed_phase_b_completed_restart_projection_fingerprint_sha256", count=None)),
        ("artifact-schema-version-drift", lambda root: mutate(root, SOURCE, "schema_version: 3", "schema_version: 2", count=1)),
        ("artifact-drops-parallel-byte-compare-sequential", lambda root: mutate(root, SOURCE, "serde_json::to_vec(&sequential)", "serde_json::to_vec(&Vec::<Stage5gProtectiveGprtArtifactRow>::new())", count=1)),
        ("artifact-drops-parallel-byte-compare-parallel", lambda root: mutate(root, SOURCE, "serde_json::to_vec(&parallel)", "serde_json::to_vec(&Vec::<Stage5gProtectiveGprtArtifactRow>::new())", count=1)),
        ("gate-drops-submitted-r6", lambda root: mutate(root, GATE, "git checkout --quiet -B stage5g-lifecycle 79c544352a0a5f8c0fc61da33c314a17df5d0e3b", "git checkout --quiet -B stage5g-lifecycle 1f8d7f3d14aa9cd2cb0f522679cf66787d5dd8a8", count=1)),
        ("gate-drops-r6-gate", lambda root: mutate(root, GATE, "bash scripts/stage5g_f_r6_gate.sh", "python3 scripts/stage5g_f_check.py")),
        ("gate-drops-submitted-r6-pass-segment", lambda root: mutate(root, GATE, "submitted-79c5443=PASS", "submitted-79c5443=REMOVED")),
        ("gate-drops-r7-pass-marker", lambda root: mutate(root, GATE, "stage5g-f-r7-gate: PASS", "stage5g-f-r7-gate: SKIPPED")),
        ("contract-r7-status-hidden", lambda root: mutate(root, CONTRACT, '"restart_extension_status": "protective_restart_production_oracle_r7"', '"restart_extension_status": "protective_restart_cleanup_completion_r6"')),
        ("contract-r7-floor-lowered", lambda root: mutate(root, CONTRACT, '"current_stage5g_f_minimum": 460', '"current_stage5g_f_minimum": 430')),
        ("lib-drops-parallel-artifact-export", lambda root: mutate(root, LIB, "stage5g_f_gprt_artifact_rows_parallel_verified", "removed_stage5g_f_gprt_artifact_rows_parallel_verified", count=None)),
    ]


def r7_authenticated_production_oracle_cases() -> list[tuple[str, callable]]:
    return [
        ("r7-contract-schema-v2", lambda root: mutate(root, CONTRACT, '"gprt_artifact_schema_version": 3', '"gprt_artifact_schema_version": 2')),
        ("r7-contract-direct-authority", lambda root: mutate(root, CONTRACT, '"artifact_authority_path": "authenticated_clean_restart_then_prepare"', '"artifact_authority_path": "direct_admission"')),
        ("r7-contract-fixture-feature-drift", lambda root: mutate(root, CONTRACT, '"artifact_fixture_feature": "stage5g-artifact-fixtures"', '"artifact_fixture_feature": "default"')),
        ("r7-contract-phase-roundtrip-disabled", lambda root: mutate(root, CONTRACT, '"canonical_package_roundtrip_per_persistable_phase": true', '"canonical_package_roundtrip_per_persistable_phase": false')),
        ("r7-contract-completed-ledger-disabled", lambda root: mutate(root, CONTRACT, '"completed_retains_final_cleanup_ledger": true', '"completed_retains_final_cleanup_ledger": false')),
        ("r7-contract-gprt07-receipt-zero", lambda root: mutate(root, CONTRACT, '"gprt07_receipt_count": 1', '"gprt07_receipt_count": 0')),
        ("r7-contract-gprt07-callback-one", lambda root: mutate(root, CONTRACT, '"gprt07_callback_count": 0', '"gprt07_callback_count": 1')),
        ("r7-contract-blocked-runtime-mutates", lambda root: mutate(root, CONTRACT, '"blocked_runtime_mutation": false', '"blocked_runtime_mutation": true')),
        ("r7-cargo-feature-default-enabled", lambda root: mutate(root, CARGO, "default = []", 'default = ["stage5g-artifact-fixtures"]')),
        ("r7-cargo-feature-declaration-removed", lambda root: mutate(root, CARGO, "stage5g-artifact-fixtures = []", "artifact-fixtures-disabled = []")),
        ("r7-cargo-bin-feature-gate-removed", lambda root: mutate(root, CARGO, 'required-features = ["stage5g-artifact-fixtures"]', 'required-features = []')),
        ("r7-stage5d-prepare-authority-removed", lambda root: mutate(root, STAGE5D, "stage5g_artifact_prepare_clean_restart_authority", "artifact_prepare_authority_disabled", count=None)),
        ("r7-stage5d-clean-authority-feature-open", lambda root: mutate(root, STAGE5D, '#[cfg(all(feature = "stage5g-artifact-fixtures", not(test)))]\npub(crate) fn stage5g_artifact_clean_restart_authority', "pub(crate) fn stage5g_artifact_clean_restart_authority")),
        ("r7-hybrid-artifact-fixture-removed", lambda root: mutate(root, "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs", "stage5g_artifact_mr_protective_runtime_fixture", "artifact_runtime_fixture_disabled", count=None)),
        ("r7-hybrid-test-fixture-name-restored", lambda root: mutate(root, "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs", "stage5g_artifact_mr_protective_runtime_fixture", "stage5g_test_mr_protective_runtime_fixture", count=None)),
        ("r7-artifact-production-prepare-removed", lambda root: mutate(root, SOURCE, "prepare_stage5g_protective_completion(restored)", "protective_prepare_disabled(restored)", count=None)),
        ("r7-artifact-export-removed", lambda root: mutate(root, SOURCE, "crate::export_stage5g_clean_restart(", "crate::removed_export_stage5g_clean_restart(", count=None)),
        ("r7-artifact-restore-removed", lambda root: mutate(root, SOURCE, "crate::restore_stage5g_clean_restart(", "crate::removed_restore_stage5g_clean_restart(", count=None)),
        ("r7-artifact-hmac-key-removed", lambda root: mutate(root, SOURCE, "Stage5gLifecycleCommitmentKey::from_secret_bytes", "ArtifactCommitmentKeyDisabled::from_secret_bytes", count=None)),
        ("r7-artifact-package-input-removed", lambda root: mutate(root, SOURCE, "Stage5gCleanRestartExportInput", "ArtifactExportInputDisabled", count=None)),
        ("r7-artifact-pre-package-fingerprint-removed", lambda root: mutate(root, SOURCE, "pre_authenticated_restart_package_fingerprint_sha256", "removed_pre_authenticated_restart_package_fingerprint_sha256", count=None)),
        ("r7-artifact-phase-a-package-fingerprint-removed", lambda root: mutate(root, SOURCE, "phase_a_canonical_restart_package_fingerprint_sha256", "removed_phase_a_canonical_restart_package_fingerprint_sha256", count=None)),
        ("r7-artifact-phase-a-restore-flag-removed", lambda root: mutate(root, SOURCE, "phase_a_authenticated_restore_succeeded", "removed_phase_a_authenticated_restore_succeeded", count=None)),
        ("r7-artifact-phase-b-package-fingerprint-removed", lambda root: mutate(root, SOURCE, "phase_b_completed_canonical_restart_package_fingerprint_sha256", "removed_phase_b_completed_canonical_restart_package_fingerprint_sha256", count=None)),
        ("r7-artifact-phase-b-restore-flag-removed", lambda root: mutate(root, SOURCE, "phase_b_authenticated_restore_succeeded", "removed_phase_b_authenticated_restore_succeeded", count=None)),
        ("r7-artifact-callback-count-removed", lambda root: mutate(root, SOURCE, "phase_a_callback_count", "removed_phase_a_callback_count", count=None)),
        ("r7-completed-final-ledger-field-removed", lambda root: mutate(root, SOURCE, "pub final_cleanup_settlement_ledger: Option<Stage5gProtectiveCleanupSettlementLedgerV1>", "pub removed_final_cleanup_settlement_ledger: Option<Stage5gProtectiveCleanupSettlementLedgerV1>")),
        ("r7-completed-final-batch-field-removed", lambda root: mutate(root, SOURCE, "pub final_cleanup_batch_restart_projection:", "pub removed_final_cleanup_batch_restart_projection:", count=None)),
        ("r7-completed-ledger-projection-removed", lambda root: mutate(root, SOURCE, "cleanup_settlement_ledger: completed.final_cleanup_settlement_ledger", "cleanup_settlement_ledger: None", count=None)),
        ("r7-completed-batch-projection-removed", lambda root: mutate(root, SOURCE, "cleanup_batch_restart_projection: final_cleanup_batch_restart_projection", "cleanup_batch_restart_projection: None", count=None)),
        ("r7-completed-ledger-validity-removed", lambda root: mutate(root, SOURCE, "stage5g_cleanup_ledger_is_valid(ledger, batch)", "true", count=None)),
        ("r7-gprt07-receipt-witness-removed", lambda root: mutate(root, SOURCE, "assert_eq!(row.phase_a_execution_receipt_count, 1);", "assert_eq!(row.phase_a_execution_receipt_count, 0);", count=None)),
        ("r7-gprt07-callback-witness-removed", lambda root: mutate(root, SOURCE, "assert_eq!(row.phase_a_callback_count, 0);", "assert_eq!(row.phase_a_callback_count, 1);", count=None)),
        ("r7-blocked-runtime-witness-removed", lambda root: mutate(root, SOURCE, "Some(row.pre_runtime_semantic_fingerprint_sha256.as_str())", "None", count=None)),
        ("r7-gate-feature-debug-removed", lambda root: mutate(root, GATE, "--features stage5g-artifact-fixtures --bin stage5g_f_gprt_artifact", "--bin stage5g_f_gprt_artifact", count=1)),
        ("r7-gate-r6-checkout-drift", lambda root: mutate(root, GATE, "79c544352a0a5f8c0fc61da33c314a17df5d0e3b", "1f8d7f3d14aa9cd2cb0f522679cf66787d5dd8a8", count=1)),
        ("r7-gate-r6-execution-removed", lambda root: mutate(root, GATE, "bash scripts/stage5g_f_r6_gate.sh", "python3 scripts/stage5g_f_check.py")),
        ("r7-handoff-feature-removed", lambda root: mutate(root, HANDOFF, '"stage5g-artifact-fixtures"', '"removed-stage5g-artifact-fixtures"', count=None)),
        ("r7-handoff-schema-v2", lambda root: mutate(root, HANDOFF, '"gprt_artifact_schema_version": 3', '"gprt_artifact_schema_version": 2')),
        ("r7-handoff-floor-lowered", lambda root: mutate(root, HANDOFF, '"negative_floor": 460', '"negative_floor": 430')),
    ]

def cases() -> list[tuple[str, callable]]:
    all_cases = (
        source_marker_cases()
        + restart_marker_cases()
        + focused_test_cases()
        + contract_cases()
        + governance_cases()
        + forbidden_surface_cases()
        + r2_lifecycle_cases()
        + r3_restart_and_issuer_cases()
        + r4_cleanup_closure_cases()
        + r5_cleanup_ledger_cases()
        + r6_stable_observation_and_artifact_cases()
        + r7_authenticated_production_oracle_cases()
    )
    if len(all_cases) < 460:
        fail(f"negative floor not met: {len(all_cases)} < 460")
    names = [name for name, _ in all_cases]
    if len(names) != len(set(names)):
        fail("duplicate mutation names")
    return all_cases


def main() -> None:
    all_cases = cases()
    passed = 0
    for name, action in all_cases:
        with tempfile.TemporaryDirectory(prefix=f"stage5g-f-negative-{name}-") as raw:
            root = Path(raw) / "repo"
            ignore = shutil.ignore_patterns("target", ".git", "reports", "tmp", "__MACOSX", "*.log")
            shutil.copytree(ROOT, root, ignore=ignore)
            action(root)
            if run_checker(root):
                fail(f"mutation survived: {name}")
            print(f"PASS {name}")
            passed += 1
    print(f"stage5g-f-negative: PASS {passed}/{len(all_cases)}")


if __name__ == "__main__":
    main()
