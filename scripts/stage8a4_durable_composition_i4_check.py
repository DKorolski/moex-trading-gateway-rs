#!/usr/bin/env python3
"""Fail-closed source/evidence checker for Stage 8A-4 I4 implementation."""

from __future__ import annotations

import argparse
import csv
import json
import re
from pathlib import Path


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root = args.root.resolve()
    paths = {
        "authority": root / "docs/stage-8/stage8a4-durable-composition-i4-authority.json",
        "contract": root / "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_IMPLEMENTATION_2026-08-20.md",
        "matrix": root / "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_ACCEPTANCE_MATRIX_2026-08-20.csv",
        "negative": root / "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_NEGATIVE_INVENTORY_2026-08-20.md",
        "core": root / "crates/strategy-runtime-core/src/stage6d_live_core.rs",
        "runtime": root / "crates/runtime-durable-service/src/recovery.rs",
        "runtime_lib": root / "crates/runtime-durable-service/src/lib.rs",
        "stage8a1": root / "crates/finam-gateway/src/stage8a1_execution_capability.rs",
        "reconciliation": root / "crates/finam-gateway/src/stage8a4_reconciliation.rs",
        "i4": root / "crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i4.rs",
        "compile": root / "scripts/stage8a4_durable_composition_i4_external_compile_fail.sh",
    }
    for label, path in paths.items():
        require(path.is_file(), f"missing {label}: {path}")
    text = {label: path.read_text(encoding="utf-8") for label, path in paths.items() if label != "matrix"}
    authority = json.loads(text["authority"])
    require(authority["accepted_design_ref"] == "81727aae1f648f17961177fc9541e2483cbf07f2", "design lineage drift")
    for key in (
        "terminal_authority_public_opaque", "complete_exact_mixed_replay_required",
        "request_finalized_required", "already_covering_authenticated_s1_required",
        "current_readiness_independent", "account_active_orders_required_zero",
        "target_active_orders_required_zero", "external_compile_fail_proof",
    ):
        require(authority.get(key) is True, f"required authority property drift: {key}")
    for key in (
        "terminal_authority_publicly_constructible", "seal_repair_or_advancement_allowed",
        "ack_readiness_publication_enabled", "redis_mutation_enabled",
        "finam_post_delete_enabled", "broker_dispatch_enabled", "execution_capability_minted",
        "operator_arm_minted", "runtime_live_enabled", "real_orders_enabled",
    ):
        require(authority.get(key) is False, f"closed surface opened: {key}")
    with paths["matrix"].open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 40, "I4 acceptance matrix must contain 40 rows")
    require(len({row["id"] for row in rows}) == 40, "duplicate I4 acceptance IDs")

    core = text["core"]
    for marker in (
        "pub struct Stage8a4CompletedTransitionFacts",
        "pub fn stage8a4_completed_transition_facts(",
    ):
        require(marker in core, f"complete-transition marker missing: {marker}")
    completed_start = core.index("pub fn stage8a4_completed_transition_facts(")
    completed_end = core.index("fn finalized_request_cancel_outcome(", completed_start)
    completed = core[completed_start:completed_end]
    for marker in (
        "Stage6ReconciliationBatchCompletionV2::Complete",
        "batch.missing_suffix_entries().is_empty()",
        "Stage6ReconciliationTransitionKindV2::Exact",
        "Stage6ReconciliationLifecycleV2::Working",
        "finalized.final_record_id() != batch.last_mixed_record_id()",
    ):
        require(marker in completed, f"complete-transition marker missing: {marker}")

    runtime = text["runtime"]
    authority_match = re.search(r"pub struct Stage7bStage8a4TerminalAuthority\s*\{(?P<body>.*?)\n\}", runtime, re.S)
    require(authority_match is not None, "public-opaque terminal authority missing")
    prefix = runtime[max(0, authority_match.start() - 160):authority_match.start()]
    require("derive(" not in prefix, "terminal authority gained a derive")
    require("pub " not in authority_match.group("body"), "terminal authority fields became public")
    issue_start = runtime.index("pub fn issue_stage8a4_terminal_authority(")
    issue_end = runtime.index("/// Issues a no-send Stage 8A-1 authority", issue_start)
    issue = runtime[issue_start:issue_end]
    require(issue.count("revalidate_cached_committed_seal") >= 2, "terminal issue lacks two S1 disk barriers")
    require("refresh_stage7b_durable_frontier" in issue, "mixed replay refresh missing")
    require("self.committed_seal.stage6_checkpoint() != self.recovered.authenticated_checkpoint()" in issue, "already-covering S1 guard missing")
    require("advance_recovery_seal" not in issue, "I4 terminal issue can advance S1")
    require("stage8a4_completed_transition_facts" in issue, "complete transition not owner-bound")
    require("durable_ack_authority" in issue, "existing Stage7B terminal identity not reused")
    require("Stage7bStage8a4TerminalAuthority" in text["runtime_lib"], "terminal authority not re-exported by owner crate")

    stage8a1 = text["stage8a1"]
    readiness_start = stage8a1.index("pub(crate) fn issue_stage8a4_i4_current_readiness(")
    readiness_end = stage8a1.index("#[allow(clippy::too_many_arguments)]\nfn derive_current_authorities", readiness_start)
    readiness = stage8a1[readiness_start:readiness_end]
    for marker in (
        "load_accepted_config_pinned", "load_current_control_pinned",
        "Stage8KillSwitchState::RunAllowed", "Stage7bPaperReadinessPhase::PaperReady",
        "BrokerMarketSessionState::Open", "broker_truth_is_fresh(now)",
        "summary.account_active_orders_count != 0", "summary.target_active_orders_count != 0",
        "authority_root_sha256", "accepted_config_sha256", "current_source_evidence_sha256",
        "valid_until",
    ):
        require(marker in readiness, f"readiness marker missing: {marker}")
    require("Stage8ExecutionCapability" not in readiness, "I4 readiness mints execution capability")
    require("OperatorArm" not in readiness, "I4 readiness uses operator arm")

    i4 = text["i4"]
    for marker in (
        "struct Stage8a4I4TerminalAckFacts", "struct Stage8a4I4CurrentReadinessEvidence",
        "struct Stage8a4I4DerivedAckReadinessFacade", "fn canonical_ack_mapping(",
        "CommandAckStatus::Recovered", "CommandAckStatus::Rejected",
        "CommandAckReasonCode::RecoveredByBrokerTruth", "CommandAckReasonCode::BrokerRejected",
        ".terminal_request_ack_identity_sha256()",
        "Stage8a4I4ReadinessState::Blocked",
    ):
        require(marker in i4 + stage8a1, f"I4 facade marker missing: {marker}")
    require("Utc::now" not in i4, "timestamp entered I4 ACK derivation")
    require("received_ts" not in i4, "I4 ACK facts gained publication timestamp")
    require("pub mod durable_composition_i4" not in text["reconciliation"], "I4 private module exported")
    for token in ("reqwest", ".post(", ".delete(", "XACK", "xack(", "xadd("):
        require(token not in i4, f"effect surface entered I4 facade: {token}")
    require("positive=1 negative=7" in text["compile"], "external compile matrix drift")
    print("stage8a4-durable-composition-i4-check: PASS rows=40 read_only=true ack_publish=false")


if __name__ == "__main__":
    main()
