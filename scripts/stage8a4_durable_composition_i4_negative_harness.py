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
