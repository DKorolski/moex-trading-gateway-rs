#!/usr/bin/env python3
"""Fail-closed source/deployment checker for Stage 8B-P R2A8."""

from __future__ import annotations

import json
import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    status = json.loads((ROOT / "docs/stage-8/stage8b-p-r2a8-status.json").read_text())
    build = json.loads(
        (ROOT / "docs/stage-8/stage8b-p-r2a8-r1-causal-build-evidence.json").read_text()
    )
    adapter = (ROOT / "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs").read_text()
    schema = (ROOT / "tools/stage8b-readonly-preflight/src/r2a5.rs").read_text()
    producer = (ROOT / "tools/stage8b-readonly-preflight/src/bin/stage8b-r2a5-authority-producer.rs").read_text()
    issuer_bin = (ROOT / "crates/finam-gateway/src/bin/stage8b-r2a8-current-manifest-issuer.rs").read_text()
    issuer_service = (ROOT / "deploy/stage8b-r2a5/stage8b-r2a8-current-manifest-issuer.service").read_text()
    adapter_service = (ROOT / "deploy/stage8b-r2a5/stage8b-r2a7-source-adapter.service").read_text()
    rehearsal = (ROOT / "scripts/stage8b_p_r2a7_linux_rehearsal.sh").read_text()

    require(status["r2b_authorization"] == "NOT_ISSUED", "R2B opened")
    require(status["stage"] == "Stage 8B-P R2A8-R1", "corrective stage drift")
    require(not status["finam_network_allowed"] and not status["real_order_allowed"], "effect opened")
    require(status["source_writer_uid"] == 8095 and status["manifest_issuer_uid"] == 8096, "UID drift")
    require(
        status["composite_readiness_semantics_persisted"] is True
        and status["composite_readiness_semantics_signed"] is True
        and status["writer_readiness_fail_closed"] is True
        and status["reader_readiness_fail_closed"] is True
        and status["synthetic_paper_ready_forbidden"] is True,
        "readiness semantic closure absent",
    )
    require(
        status["lifecycle_key_exact_uid"] == 8096
        and status["lifecycle_key_exact_gid"] == 8095
        and status["lifecycle_key_exact_mode"] == "0640",
        "lifecycle key custody drift",
    )
    require(
        status["controlled_place_full_chain"] == "PASS"
        and status["controlled_cancel_full_chain"] == "PASS"
        and status["production_binaries_reproducible"] is True,
        "full-chain/reproducibility evidence absent",
    )
    for marker in (
        "publish_stage8b_r2a8_trusted_current_source_from_owner(",
        "Stage8a1OperationalAuthorityIssuer::from_stage7b_owner(",
        ".sign_stage8b_r2a8_current_source_commitment(",
        "validate_trusted_current_source(&source, mode)?",
        "current_source_issuer_public_key_hex",
        "current_source_signature_ed25519_hex",
        "source.expires_at <= now",
        "STAGE8B_R2A8_CURRENT_MANIFEST_ISSUER_UID: u32 = 8096",
        "atomic_write_fixed(",
        "Stage8bR2a8TrustedCurrentSourceV2",
        "Stage8bR2a7ReaderManifestV2",
        "pub composite_readiness: Stage8bCompositeReadinessAuthorityV1",
        "pub phase: Stage8bCompositeReadinessPhaseV1",
        "pub reasons: Vec<Stage8bCompositeReadinessReasonV1>",
        "pub blocked_entry_ids: Vec<String>",
        "pub blocked_request_ids: Vec<StrategyRequestId>",
        "Stage8bCompositeReadinessPhaseV1::Degraded",
        "Stage8bCompositeReadinessPhaseV1::Stopped",
        "Stage8bCompositeReadinessReasonV1::ConsumerNotAlive",
        "Stage8bCompositeReadinessReasonV1::StorageUnavailable",
        "Stage8bCompositeReadinessReasonV1::SourcePollStale",
        "Stage8bCompositeReadinessReasonV1::ClaimScanStale",
        "Stage8bCompositeReadinessReasonV1::SettlementUnavailable",
        "Stage8bCompositeReadinessReasonV1::DurablePendingEntries",
        "Stage8bCompositeReadinessReasonV1::CommandLifecycleBlocked",
        "stage8b-r2a8-r1-trusted-current-source-commitment-v2",
        "stage8b-r2a8-r1-reader-manifest-commitment-v2",
        "let readiness = manifest.composite_readiness.to_snapshot();",
        "manifest.composite_readiness.validate_ready()?;",
        "readiness_authority_rejects_every_degraded_or_blocked_semantic",
        "signed_readiness_mutations_change_commitment_and_fail_verification",
        "manifest_binds_readiness_and_preserves_exact_semantics",
        "cross_source_staleness_is_fail_closed",
        "lifecycle_key_custody_requires_exact_uid_gid_mode_link_and_size",
    ):
        require(marker in adapter, f"trusted-source invariant absent: {marker}")
    writer = adapter.split("pub fn publish_stage8b_r2a8_trusted_current_source_from_owner(", 1)[1].split(
        "pub fn issue_stage8b_r2a8_reader_manifest(", 1
    )[0]
    reader = adapter.split("pub fn run_stage8b_r2a7_source_adapter(", 1)[1].split(
        "fn verify_published_domain(", 1
    )[0]
    require("composite_readiness.validate_ready()?;" in writer, "writer readiness admission absent")
    require("Stage7bCompositeReadinessSnapshot {" not in reader, "synthetic readiness restored")
    require("composite_checked_at" not in adapter, "timestamp-only readiness restored")
    require(adapter.count("read_lifecycle_key_file(") == 3, "lifecycle key specific reader bypassed")
    key_parser = adapter.split("fn parse_lifecycle_key(", 1)[1].split("fn fixed_runtime_profile(", 1)[0]
    require(".trim()" not in key_parser, "lifecycle key normalization restored")
    require("strip_suffix(b\"\\n\")" in key_parser and "(b'a'..=b'f')" in key_parser, "strict key grammar absent")
    key_reader = adapter.split("fn lifecycle_key_properties_are_exact(", 1)[1].split(
        "fn atomic_write_fixed(", 1
    )[0]
    for marker in (
        "metadata.uid()",
        "metadata.gid()",
        "metadata.mode()",
        "metadata.nlink()",
        "metadata.len()",
        "O_NOFOLLOW",
        "STAGE8B_R2A8_LIFECYCLE_KEY_GID",
        "STAGE8B_R2A8_LIFECYCLE_KEY_MODE",
    ):
        require(marker in key_reader, f"exact lifecycle custody absent: {marker}")

    for marker in (
        "pub adapter_domain: OperationalAdapterDomain",
        "pub adapter_mode: OperationalAdapterMode",
        "record.adapter_mode != OperationalAdapterMode::OneShotRecoveryReader",
        "OperationalAdapterDomain::Production",
        "OperationalAdapterDomain::ControlledQualification",
        "serde_json::from_value(value).map_err(serde::de::Error::custom)?",
    ):
        require(marker in schema, f"closed downstream schema absent: {marker}")
    require("--controlled-r2a8-place" in producer and "--controlled-r2a8-cancel" in producer, "controlled producer entry absent")

    require("--one-shot-production" in issuer_bin and "usage:" in issuer_bin, "fixed issuer CLI absent")
    for marker in (
        "User=m8m8096",
        "SupplementaryGroups=m8a8095",
        "Type=oneshot",
        "RestrictAddressFamilies=AF_UNIX",
        "IPAddressDeny=any",
    ):
        require(marker in issuer_service, f"issuer service drift: {marker}")
    require("AF_INET" not in issuer_service and "AF_INET6" not in issuer_service, "issuer network opened")
    require("Requires=stage8b-r2a8-current-manifest-issuer.service" in adapter_service, "issuer ordering absent")
    for marker in (
        '"$ISSUER_BIN" "--one-shot-controlled-$operation"',
        '"$LAYOUT" bind-r2a8 "$operation_upper"',
        '"--controlled-r2a8-$operation" "$source"',
        "stage8b-r2a8-full-chain-$operation: PASS",
    ):
        require(marker in rehearsal, f"full-chain witness absent: {marker}")
    require(build["authorization_status"] == "NOT_ISSUED", "build evidence opened R2B")
    require(
        build["revision"] == "R2A8-R1"
        and build["evidence_role"] == "causal_build_and_native_full_chain"
        and build["final_candidate_binding"] == "generated_by_immutable_handoff",
        "causal/final evidence roles conflated",
    )
    require(
        build["controlled_full_chain"]["place"] == "PASS"
        and build["controlled_full_chain"]["cancel"] == "PASS"
        and build["controlled_full_chain"]["finam_network_accessed"] is False,
        "full-chain evidence invalid",
    )
    for binary in build["production_binaries"].values():
        require(
            binary["reproducible"] is True
            and binary["build_a_sha256"] == binary["build_b_sha256"],
            "production reproducibility invalid",
        )
    for name, relative in (
        ("adapter_binary_source", "crates/finam-gateway/src/bin/stage8b-r2a7-source-adapter.rs"),
        ("issuer_binary_source", "crates/finam-gateway/src/bin/stage8b-r2a8-current-manifest-issuer.rs"),
        ("adapter_module", "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs"),
        ("owner_composition", "crates/finam-gateway/src/stage8a1_execution_capability.rs"),
        ("downstream_schema", "tools/stage8b-readonly-preflight/src/r2a5.rs"),
        ("linux_rehearsal", "scripts/stage8b_p_r2a7_linux_rehearsal.sh"),
    ):
        require(build["source_sha256"][name] == sha(ROOT / relative), f"source drift: {name}")
    print("stage8b-p-r2a8-check: PASS trusted_manifest=true schema_compatible=true strict_key=true place=true cancel=true r2b=NOT_ISSUED")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        raise SystemExit(f"stage8b-p-r2a8-check: FAIL {error}")
