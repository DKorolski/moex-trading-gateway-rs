#!/usr/bin/env python3
"""Fail-closed source/deployment checker for Stage 8B-P R2A8."""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def main() -> None:
    status = json.loads((ROOT / "docs/stage-8/stage8b-p-r2a8-status.json").read_text())
    adapter = (ROOT / "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs").read_text()
    schema = (ROOT / "tools/stage8b-readonly-preflight/src/r2a5.rs").read_text()
    producer = (ROOT / "tools/stage8b-readonly-preflight/src/bin/stage8b-r2a5-authority-producer.rs").read_text()
    issuer_bin = (ROOT / "crates/finam-gateway/src/bin/stage8b-r2a8-current-manifest-issuer.rs").read_text()
    issuer_service = (ROOT / "deploy/stage8b-r2a5/stage8b-r2a8-current-manifest-issuer.service").read_text()
    adapter_service = (ROOT / "deploy/stage8b-r2a5/stage8b-r2a7-source-adapter.service").read_text()
    rehearsal = (ROOT / "scripts/stage8b_p_r2a7_linux_rehearsal.sh").read_text()

    require(status["r2b_authorization"] == "NOT_ISSUED", "R2B opened")
    require(not status["finam_network_allowed"] and not status["real_order_allowed"], "effect opened")
    require(status["source_writer_uid"] == 8095 and status["manifest_issuer_uid"] == 8096, "UID drift")
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
    ):
        require(marker in adapter, f"trusted-source invariant absent: {marker}")
    key_parser = adapter.split("fn parse_lifecycle_key(", 1)[1].split("fn fixed_runtime_profile(", 1)[0]
    require(".trim()" not in key_parser, "lifecycle key normalization restored")
    require("strip_suffix(b\"\\n\")" in key_parser and "(b'a'..=b'f')" in key_parser, "strict key grammar absent")

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
    print("stage8b-p-r2a8-check: PASS trusted_manifest=true schema_compatible=true strict_key=true place=true cancel=true r2b=NOT_ISSUED")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        raise SystemExit(f"stage8b-p-r2a8-check: FAIL {error}")
