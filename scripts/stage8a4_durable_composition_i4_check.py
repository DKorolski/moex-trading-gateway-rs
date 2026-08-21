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
        "trace": root / "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_DESIGN_TO_IMPLEMENTATION_TRACEABILITY_2026-08-21.csv",
        "negative": root / "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_NEGATIVE_INVENTORY_2026-08-20.md",
        "core": root / "crates/strategy-runtime-core/src/stage6d_live_core.rs",
        "runtime": root / "crates/runtime-durable-service/src/recovery.rs",
        "runtime_lib": root / "crates/runtime-durable-service/src/lib.rs",
        "stage8a1": root / "crates/finam-gateway/src/stage8a1_execution_capability.rs",
        "reconciliation": root / "crates/finam-gateway/src/stage8a4_reconciliation.rs",
        "i4": root / "crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i4.rs",
        "i3": root / "crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i2/durable_writer_i3.rs",
        "compile": root / "scripts/stage8a4_durable_composition_i4_external_compile_fail.sh",
    }
    for label, path in paths.items():
        require(path.is_file(), f"missing {label}: {path}")
    text = {label: path.read_text(encoding="utf-8") for label, path in paths.items() if label not in ("matrix", "trace")}
    authority = json.loads(text["authority"])
    require(authority["accepted_design_ref"] == "81727aae1f648f17961177fc9541e2483cbf07f2", "design lineage drift")
    for key in (
        "terminal_authority_public_opaque", "complete_exact_mixed_replay_required",
        "request_finalized_required", "already_covering_authenticated_s1_required",
        "current_readiness_independent", "account_active_orders_required_zero",
        "target_active_orders_required_zero", "external_compile_fail_proof",
        "trusted_current_sources_opaque",
        "fresh_process_i4_reconstructible",
        "historical_ack_survives_readiness_unavailable",
        "i4_read_only_issuer_has_no_execution_authority",
    ):
        require(authority.get(key) is True, f"required authority property drift: {key}")
    for key in (
        "terminal_authority_publicly_constructible", "seal_repair_or_advancement_allowed",
        "ack_readiness_publication_enabled", "redis_mutation_enabled",
        "finam_post_delete_enabled", "broker_dispatch_enabled", "execution_capability_minted",
        "operator_arm_minted", "runtime_live_enabled", "real_orders_enabled",
        "caller_supplied_clock_allowed", "terminal_settlement_getters_public",
    ):
        require(authority.get(key) is False, f"closed surface opened: {key}")
    with paths["matrix"].open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 60, "I4 R3 acceptance matrix must contain 60 rows")
    require(len({row["id"] for row in rows}) == 60, "duplicate I4 acceptance IDs")
    require([row["id"] for row in rows] == [f"I4I-{index:03d}" for index in range(1, 61)], "I4 implementation IDs must be exact I4I-001..I4I-060")
    with paths["trace"].open(newline="", encoding="utf-8") as handle:
        trace = list(csv.DictReader(handle))
    require(len(trace) == 64, "I4 Design R3 traceability must contain 64 rows")
    require([row["design_id"] for row in trace] == [f"I4D-{index:03d}" for index in range(1, 65)], "I4 design trace IDs must be exact I4D-001..I4D-064")
    require(all(row["implementation_proof"].strip() for row in trace), "empty I4 implementation proof")
    require(authority.get("accepted_design_traceability_rows") == 64, "authority traceability count drift")
    require(authority.get("inherited_design_negative_cases") == 46, "inherited design negatives drift")
    require(authority.get("micro_budget_max_orders") == 1, "I4 max-orders barrier drift")
    require(authority.get("micro_budget_consumed_orders") == 0, "I4 consumed-orders barrier drift")

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
    authority_impl_start = runtime.index("impl Stage7bStage8a4TerminalAuthority")
    authority_impl_end = runtime.index("impl Stage7bStage8a4DurableBatchReceipt", authority_impl_start)
    authority_impl = runtime[authority_impl_start:authority_impl_end]
    for getter in (
        "stage6_checkpoint_sha256", "seal_generation", "seal_commitment_sha256",
        "settlement_authority_fingerprint_sha256",
    ):
        require(getter not in authority_impl, f"settlement-only terminal getter exposed: {getter}")

    stage8a1 = text["stage8a1"]
    readonly_match = re.search(r"pub\(crate\) struct Stage8a4I4ReadOnlyAuthorityIssuer\s*\{(?P<body>.*?)\n\}", stage8a1, re.S)
    require(readonly_match is not None, "I4-only read-only issuer missing")
    readonly_body = readonly_match.group("body")
    for marker in ("authority_root", "last_control_revision", "current_control_sha256", "terminal_scope_sha256"):
        require(marker in readonly_body, f"I4 read-only issuer binding missing: {marker}")
    for forbidden in ("SigningKey", "Stage8ExecutionCapability", "OperatorArm", "builder", "continuation"):
        require(forbidden not in readonly_body, f"effect authority entered I4 read-only issuer: {forbidden}")
    readonly_impl_start = stage8a1.index("impl Stage8a4I4ReadOnlyAuthorityIssuer")
    readonly_impl_end = stage8a1.index("struct Stage8a1DerivedCurrentAuthorities", readonly_impl_start)
    readonly_impl = stage8a1[readonly_impl_start:readonly_impl_end]
    for marker in (
        "pub(crate) fn from_terminal_authority(", "validate_stage8a4_i4_config_binding",
        "load_current_control_pinned", "allow_arm_registry_create", "issue_current_sources(",
        "stage8a4_i4_terminal_scope_sha256", "authority_root.validate()",
    ):
        if marker == "allow_arm_registry_create":
            require(marker not in readonly_impl, "I4 read-only issuer can create arm registry")
        else:
            require(marker in readonly_impl, f"I4 read-only issuer marker missing: {marker}")
    for forbidden in (
        "Stage8a1OperationalAuthorityIssuer::from_stage7b_owner",
        "authorize_stage8a1_durable_request", "authorize_exact_durable_request",
        "fs::create_dir", "authorize_place(", "authorize_cancel(",
    ):
        require(forbidden not in readonly_impl, f"pre-effect authority entered I4 restart issuer: {forbidden}")
    readiness_start = stage8a1.index("pub(crate) fn issue_stage8a4_i4_current_readiness(")
    readiness_end = stage8a1.index("#[allow(clippy::too_many_arguments)]\nfn derive_current_authorities", readiness_start)
    readiness = stage8a1[readiness_start:readiness_end]
    readiness_signature = readiness[:readiness.index(") -> Result")]
    for marker in (
        "issuer.validate_control_revision()", "sources.validate(authority_root)",
        "authority_root.load_accepted_config()", "authority_root.load_current_control()",
        "Stage8KillSwitchState::RunAllowed", "Stage7bPaperReadinessPhase::PaperReady",
        "BrokerMarketSessionState::Open", "broker_truth_is_fresh(now)",
        "summary.account_active_orders_count != 0", "summary.target_active_orders_count != 0",
        "control.max_orders != 1", "control.consumed_orders != 0",
        "validate_stage8a4_i4_config_binding", "let now = Utc::now();",
        "authority_root_sha256", "accepted_config_sha256", "current_source_evidence_sha256",
        "valid_until",
    ):
        require(marker in readiness, f"readiness marker missing: {marker}")
    require("|| !stage8a4_i4_strategy_instance_scope_matches(" in stage8a1, "strategy-instance equality/mapping guard missing")
    require("Stage8a4I4ReadOnlyAuthorityIssuer" in readiness_signature, "I4-only read-only issuer missing from final mint")
    require("Stage8a1OperationalAuthorityIssuer" not in readiness_signature, "pre-finalization operational issuer returned to I4 mint")
    require("Stage8a1TrustedCurrentSources" in readiness_signature, "opaque current-source authority missing from I4 mint")
    for forbidden in ("BrokerTruthSnapshot", "BrokerReadinessSnapshot", "Stage7bCompositeReadinessSnapshot", "now: DateTime", "root: &Path", "accepted_config_sha256: &str"):
        require(forbidden not in readiness_signature, f"caller-controlled readiness input returned: {forbidden}")
    require("Stage8ExecutionCapability" not in readiness, "I4 readiness mints execution capability")
    require("OperatorArm" not in readiness, "I4 readiness uses operator arm")

    i4 = text["i4"]
    compose_start = i4.index("pub(crate) fn compose_stage8a4_i4_readonly(")
    compose_signature = i4[compose_start:i4.index(") -> Result", compose_start)]
    require("Stage8a4I4ReadOnlyAuthorityIssuer" in compose_signature, "I4-only trusted issuer missing")
    require("Stage8a1OperationalAuthorityIssuer" not in compose_signature, "pre-finalization issuer required by I4 composer")
    require("Stage8a1TrustedCurrentSources" in compose_signature, "opaque I4 sources missing")
    require("Option<(" in compose_signature, "readiness-unavailable ACK-only composition missing")
    for forbidden in ("BrokerTruthSnapshot", "BrokerReadinessSnapshot", "Stage7bCompositeReadinessSnapshot", "now: DateTime", "authority_root", "accepted_config_sha256"):
        require(forbidden not in compose_signature, f"raw I4 composer input returned: {forbidden}")
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
    ack_position = i4.index("let ack = terminal_ack_facts(terminal)?;")
    readiness_position = i4.index("let readiness = current.and_then", ack_position)
    require(ack_position < readiness_position, "current readiness can suppress historical ACK")
    terminal_ack_start = i4.index("fn terminal_ack_facts(")
    terminal_ack_end = i4.index("fn canonical_ack_mapping(", terminal_ack_start)
    terminal_ack = i4[terminal_ack_start:terminal_ack_end]
    require(
        ".terminal_request_ack_identity_sha256()" in terminal_ack,
        "terminal ACK stopped reusing exact Stage7B ACK identity",
    )
    i3 = text["i3"]
    restart_marker = "stage8a4_i4_fresh_process_post_s1_readonly_facade_and_ack_fallback"
    for marker in (
        restart_marker, "drop(capability);", "drop(issuer);", "drop(i4_issuer);",
        "drop(owner);", "process B reopens I4-only read-only authority",
        "readiness-unavailable restart preserves historical ACK",
        "fresh-process I4 must not mutate the journal",
        "fresh-process I4 must not mutate S1",
        "fresh-process I4 must not create an operator arm",
    ):
        require(marker in i3, f"fresh-process I4 witness missing: {marker}")
    require(
        "drop(capability);\n        drop(issuer);\n        let seal_path" in i3,
        "process-A execution issuer/capability survives into I4 restart phase",
    )
    trace_by_id = {row["design_id"]: row["implementation_proof"] for row in trace}
    for design_id in ("I4D-008", "I4D-033", "I4D-048"):
        require(restart_marker in trace_by_id[design_id], f"restart-critical trace row lacks exact witness: {design_id}")
    require("positive=1 negative=12" in text["compile"], "external compile matrix drift")
    for getter in ("checkpoint_getter", "seal_generation_getter", "seal_commitment_getter", "settlement_fingerprint_getter", "trusted_sources_literal"):
        require(f"check_fail {getter}" in text["compile"], f"external negative missing: {getter}")
    print("stage8a4-durable-composition-i4-check: PASS rows=60 trace=64 fresh_process=true read_only=true ack_publish=false")


if __name__ == "__main__":
    main()
