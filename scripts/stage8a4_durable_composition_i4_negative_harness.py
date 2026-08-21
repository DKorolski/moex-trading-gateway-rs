#!/usr/bin/env python3
"""Mutation harness for the I4 implementation checker."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

CASES = [
    ("clone-authority", "crates/runtime-durable-service/src/recovery.rs", "pub struct Stage7bStage8a4TerminalAuthority", "#[derive(Clone)]\npub struct Stage7bStage8a4TerminalAuthority"),
    ("remove-s1-reread", "crates/runtime-durable-service/src/recovery.rs", "self.revalidate_cached_committed_seal(commitment_key)?;", "let _ = commitment_key;"),
    ("enable-seal-advance", "crates/runtime-durable-service/src/recovery.rs", "return Err(Stage7bRecoveryError::SealInvalid);\n        }\n        self.revalidate_cached_committed_seal", "self.advance_recovery_seal(commitment_key)?;\n        }\n        self.revalidate_cached_committed_seal"),
    ("remove-complete", "crates/strategy-runtime-core/src/stage6d_live_core.rs", "|| batch.completion() != Stage6ReconciliationBatchCompletionV2::Complete", "|| false"),
    ("remove-exact", "crates/strategy-runtime-core/src/stage6d_live_core.rs", "Stage6ReconciliationTransitionKindV2::Exact", "Stage6ReconciliationTransitionKindV2::ReconciliationConflictHold"),
    ("timestamp-ack", "crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i4.rs", "strategy_request_id: StrategyRequestId,", "received_ts: DateTime<Utc>,\n    strategy_request_id: StrategyRequestId,"),
    ("remove-runallowed", "crates/finam-gateway/src/stage8a1_execution_capability.rs", "control.kill_switch != Stage8KillSwitchState::RunAllowed", "false"),
    ("remove-account-active", "crates/finam-gateway/src/stage8a1_execution_capability.rs", "summary.account_active_orders_count != 0", "false"),
    ("remove-target-active", "crates/finam-gateway/src/stage8a1_execution_capability.rs", "summary.target_active_orders_count != 0", "false"),
    ("export-i4-module", "crates/finam-gateway/src/stage8a4_reconciliation.rs", "mod durable_composition_i4;", "pub mod durable_composition_i4;"),
    ("add-http", "crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i4.rs", "//! Everything in this module", "use reqwest as _;\n//! Everything in this module"),
    ("remove-terminal-identity", "crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i4.rs", ".terminal_request_ack_identity_sha256()\n            .to_string()", ".canonical_command_sha256()\n            .to_string()"),
    ("raw-snapshot-mint", "crates/finam-gateway/src/stage8a1_execution_capability.rs", "sources: &Stage8a1TrustedCurrentSources,\n) -> Result<Stage8a4I4CurrentReadinessEvidence", "sources: &BrokerTruthSnapshot,\n) -> Result<Stage8a4I4CurrentReadinessEvidence"),
    ("caller-clock", "crates/finam-gateway/src/stage8a1_execution_capability.rs", "sources: &Stage8a1TrustedCurrentSources,\n) -> Result<Stage8a4I4CurrentReadinessEvidence", "sources: &Stage8a1TrustedCurrentSources,\n    now: DateTime<Utc>,\n) -> Result<Stage8a4I4CurrentReadinessEvidence"),
    ("remove-max-orders", "crates/finam-gateway/src/stage8a1_execution_capability.rs", "|| control.max_orders != 1", "|| false"),
    ("remove-consumed-orders", "crates/finam-gateway/src/stage8a1_execution_capability.rs", "|| control.consumed_orders != 0", "|| false"),
    ("remove-strategy-instance-binding", "crates/finam-gateway/src/stage8a1_execution_capability.rs", "|| !stage8a4_i4_strategy_instance_scope_matches(", "|| false && !stage8a4_i4_strategy_instance_scope_matches("),
    ("remove-current-source-validation", "crates/finam-gateway/src/stage8a1_execution_capability.rs", "sources.validate(authority_root)?;", "let _ = sources;"),
    ("public-checkpoint-getter", "crates/runtime-durable-service/src/recovery.rs", "pub fn terminal_request_ack_identity_sha256", "pub fn stage6_checkpoint_sha256(&self) -> &str { \"0\" }\n    pub fn terminal_request_ack_identity_sha256"),
    ("public-seal-getters", "crates/runtime-durable-service/src/recovery.rs", "pub fn terminal_request_ack_identity_sha256", "pub fn seal_generation(&self) -> u64 { 1 }\n    pub fn seal_commitment_sha256(&self) -> &str { \"0\" }\n    pub fn terminal_request_ack_identity_sha256"),
    ("public-settlement-getter", "crates/runtime-durable-service/src/recovery.rs", "pub fn terminal_request_ack_identity_sha256", "pub fn settlement_authority_fingerprint_sha256(&self) -> &str { \"0\" }\n    pub fn terminal_request_ack_identity_sha256"),
    ("pre-finalization-issuer-in-composer", "crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i4.rs", "&mut Stage8a4I4ReadOnlyAuthorityIssuer,", "&mut Stage8a1OperationalAuthorityIssuer,"),
    ("dispatch-ready-authority-in-i4-reopen", "crates/finam-gateway/src/stage8a1_execution_capability.rs", "let root = root.as_ref();", "let _forbidden = \"authorize_stage8a1_durable_request\";\n        let root = root.as_ref();"),
    ("remove-i4-read-only-issuer", "crates/finam-gateway/src/stage8a1_execution_capability.rs", "pub(crate) struct Stage8a4I4ReadOnlyAuthorityIssuer", "pub(crate) struct RemovedStage8a4I4ReadOnlyAuthorityIssuer"),
    ("remove-fresh-process-witness", "crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i2/durable_writer_i3.rs", "stage8a4_i4_fresh_process_post_s1_readonly_facade_and_ack_fallback", "removed_fresh_process_i4_witness"),
    ("retain-process-a-issuer", "crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i2/durable_writer_i3.rs", "drop(capability);\n        drop(issuer);\n        let seal_path", "drop(capability);\n        let _process_b_retains_process_a_issuer = &issuer;\n        let seal_path"),
    ("readiness-failure-drops-ack", "crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i2/durable_writer_i3.rs", "readiness-unavailable restart preserves historical ACK", "readiness-unavailable restart drops historical ACK"),
    ("restart-trace-loses-witness", "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_DESIGN_TO_IMPLEMENTATION_TRACEABILITY_2026-08-21.csv", "I4D-008,stage8a4_i4_fresh_process_post_s1_readonly_facade_and_ack_fallback", "I4D-008,generic restart proof"),
]

with tempfile.TemporaryDirectory(prefix="stage8a4-i4-negative-") as temp:
    work = Path(temp) / "repo"
    shutil.copytree(ROOT, work, ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports"))
    for name, relative, old, new in CASES:
        path = work / relative
        original = path.read_text(encoding="utf-8")
        if old not in original:
            raise SystemExit(f"mutation anchor missing: {name}")
        path.write_text(original.replace(old, new, 1), encoding="utf-8")
        result = subprocess.run(
            ["python3", str(ROOT / "scripts/stage8a4_durable_composition_i4_check.py"), "--root", str(work)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        path.write_text(original, encoding="utf-8")
        if result.returncode == 0:
            raise SystemExit(f"negative mutation escaped: {name}")
        print(f"PASS {name}")
print(f"stage8a4-durable-composition-i4-negative-harness: PASS {len(CASES)}/{len(CASES)}")
