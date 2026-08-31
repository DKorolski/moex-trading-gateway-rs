#!/usr/bin/env python3
"""Fail-closed source/build/deployment checker for Stage 8B-P R2A6."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs/stage-8"


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    status = json.loads((DOCS / "stage8b-p-r2a6-status.json").read_text())
    build = json.loads((DOCS / "stage8b-p-r2a6-build-evidence.json").read_text())
    adapter = (ROOT / "crates/finam-gateway/src/stage8a1_execution_capability.rs").read_text()
    binary = (ROOT / "crates/finam-gateway/src/bin/stage8b-r2a6-source-adapter.rs").read_text()
    cargo = (ROOT / "crates/finam-gateway/Cargo.toml").read_text()
    runtime = (ROOT / "crates/runtime-durable-service/src/recovery.rs").read_text()
    producer = (ROOT / "tools/stage8b-readonly-preflight/src/r2a5.rs").read_text()
    layout = (ROOT / "tools/stage8b-readonly-preflight/src/bin/stage8b-r2a5-controlled-layout.rs").read_text()
    service = (ROOT / "deploy/stage8b-r2a5/stage8b-r2a6-source-adapter@.service").read_text()
    tmpfiles = (ROOT / "deploy/stage8b-r2a5/stage8b-r2a6.tmpfiles").read_text()
    sysusers = (ROOT / "deploy/stage8b-r2a5/stage8b-r2a5.sysusers").read_text()
    rehearsal = (ROOT / "scripts/stage8b_p_r2a6_linux_rehearsal.sh").read_text()

    require(status["revision"] == "R2A6" and status["status"] == "review_candidate", "status drift")
    require(status["authorization_status"] == "NOT_ISSUED", "production authorization opened")
    require(status["accepted_effect_build_identity_sha256"] == "ca330934df540de69b52d0463a665a1ab0ff89fa13eeb663b162763cc6bc83a0", "effect build drift")
    require(status["accepted_effect_executable_sha256"] == "677f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06", "effect executable drift")
    require(all(value is False for value in status["closed_surfaces"].values()), "closed surface opened")
    require(status["typed_r2b_operator_decision_required"] is True, "typed decision lost")

    for marker in (
        "pub fn publish_stage8b_r2a6_operational_sources_from_owner(",
        "Stage8a1DurableRequestAuthority::from_stage7b_owner(",
        "issuer.issue_current_sources(",
        "issuer.publish_stage8b_r2a5_operational_sources_at(",
        "STAGE8B_R2A6_SOURCE_ADAPTER_UID: u32 = 8095",
        "validate_stage8b_r2a6_output_ownership()?",
        "execution_authority_granted: false",
        "STAGE8B_R2A6_ACCEPTED_EFFECT_CONFIG_SHA256",
        "STAGE8B_R2A6_ACCEPTED_EFFECT_POLICY_SHA256",
    ):
        require(marker in adapter, f"adapter control absent: {marker}")
    for marker in (
        "run_stage8b_r2a6_controlled_source_adapter(&value)",
        '"--controlled-rehearsal"',
        "evidence.execution_authority_granted",
    ):
        require(marker in binary, f"runnable caller absent: {marker}")
    for forbidden in ("reqwest", "redis::", "AuthService", ".post(", ".delete(", "operator_arm", "dispatch_attempt"):
        require(forbidden not in binary, f"adapter binary effect surface: {forbidden}")
    require("required-features = [\"stage8b-r2a6-controlled-rehearsal\"]" in cargo, "adapter build feature drift")
    require("stage8b_r2a6_cancel_production_test_setup_in" in runtime, "durable CANCEL fixture absent")
    require("stage7b_test_authenticated_cancel_restart_fixture" in runtime, "source-authenticated CANCEL source absent")
    require("R2A6_SOURCE_ADAPTER_UID: u32 = 8095" in producer, "producer owner binding absent")
    require("R2A6_SOURCE_ADAPTER_UID" in producer and "read_owned_fd(" in producer, "producer read binding absent")
    require(
        "read_owned_fd(&store_path, 128 * 1024, R2A6_SOURCE_ADAPTER_UID, false)?" in producer,
        "producer operational-record owner check drift",
    )
    require(
        "read_owned_fd(&source_path, 128 * 1024, R2A6_SOURCE_ADAPTER_UID, false)?" in producer,
        "controlled manifest owner check drift",
    )
    require('command == "seed-r2a6"' in layout, "R2A6 layout entry absent")
    require('command == "bind-r2a6"' in layout, "R2A6 manifest binding entry absent")
    require("recompute_manifest_run_identity" in producer, "actual-source run identity binding absent")

    for marker in (
        "User=m8a8095", "Group=m8a8095", "RestrictAddressFamilies=AF_UNIX",
        "IPAddressDeny=any", "NoNewPrivileges=yes", "ProtectSystem=strict",
    ):
        require(marker in service, f"service sandbox drift: {marker}")
    require("AF_INET" not in service and "AF_INET6" not in service, "adapter network family opened")
    require("8095" in sysusers and "m8a8095" in sysusers, "adapter sysuser absent")
    require("0755 m8a8095 m8a8095" in tmpfiles, "adapter output ownership absent")
    require("0700 m8a8095 m8a8095" in tmpfiles, "adapter work ownership absent")

    adapter_call = rehearsal.index('"$ADAPTER_BIN" --controlled-rehearsal')
    binding_call = rehearsal.index('"$LAYOUT" bind-r2a6')
    producer_call = rehearsal.index('"$PRODUCER" "$source"')
    require(adapter_call < binding_call < producer_call, "adapter/manifest/producer order drift")
    for marker in (
        'test -z "$(find /var/lib/moex-trading/operational-authorities',
        'stat -c %u /var/lib/moex-trading/operational-authorities',
        '"source_count":10', '"execution_authority_granted":false',
        'stage8b-r2a6-fixed-layout-$operation: PASS',
        'ACCEPTED_R2A5_BIN_DIR=',
        'HELPER="$ACCEPTED_R2A5_BIN_DIR/stage8b-readonly-preflight"',
        'LAUNCHER="$ACCEPTED_R2A5_BIN_DIR/stage8b-r2a5-launcher"',
    ):
        require(marker in rehearsal, f"rehearsal assertion absent: {marker}")

    require(build["revision"] == "R2A6", "build revision drift")
    require(build["source_ref"] == status["source_adapter"]["build_identity"], "source/build binding drift")
    require(build["target"] == "x86_64-unknown-linux-gnu", "target drift")
    require(build["adapter"]["reproducible"] is True, "adapter build not reproducible")
    require(build["adapter"]["build_a_sha256"] == build["adapter"]["build_b_sha256"], "adapter digest mismatch")
    require(build["adapter"]["build_a_sha256"] == status["source_adapter"]["executable_sha256"], "status adapter digest drift")
    require(build["accepted_effect_executable_sha256"] == status["accepted_effect_executable_sha256"], "effect/adapter identity conflation")
    require(build["controlled_rehearsal"] == {"place": "PASS", "cancel": "PASS", "real_finam": False}, "rehearsal evidence drift")
    require(build["source_sha256"]["adapter_binary_rs"] == sha(ROOT / "crates/finam-gateway/src/bin/stage8b-r2a6-source-adapter.rs"), "adapter source digest drift")
    require(build["source_sha256"]["composition_rs"] == sha(ROOT / "crates/finam-gateway/src/stage8a1_execution_capability.rs"), "composition source digest drift")

    print(
        "stage8b-p-r2a6-check: PASS adapter_uid=8095 sources=10 "
        "place=true cancel=true effect_unchanged=true authorization=NOT_ISSUED real_finam=false"
    )


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        raise SystemExit(f"stage8b-p-r2a6-check: FAIL {error}")
