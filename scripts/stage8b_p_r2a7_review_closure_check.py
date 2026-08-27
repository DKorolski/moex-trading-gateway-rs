#!/usr/bin/env python3
"""Fail-closed source/deployment/evidence checker for Stage 8B-P R2A7."""

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
    status = json.loads((DOCS / "stage8b-p-r2a7-status.json").read_text())
    build = json.loads((DOCS / "stage8b-p-r2a7-build-evidence.json").read_text())
    cargo = (ROOT / "crates/finam-gateway/Cargo.toml").read_text()
    binary = (ROOT / "crates/finam-gateway/src/bin/stage8b-r2a7-source-adapter.rs").read_text()
    adapter = (ROOT / "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs").read_text()
    composition = (ROOT / "crates/finam-gateway/src/stage8a1_execution_capability.rs").read_text()
    runtime = (ROOT / "crates/strategy-runtime-core/src/stage6d_live_core.rs").read_text()
    service = (ROOT / "deploy/stage8b-r2a5/stage8b-r2a7-source-adapter.service").read_text()
    rehearsal = (ROOT / "scripts/stage8b_p_r2a7_linux_rehearsal.sh").read_text()

    require(status["revision"] == "R2A7" and status["authorization_status"] == "NOT_ISSUED", "status drift")
    require(all(value is False for value in status["closed_surfaces"].values()), "closed surface opened")
    require(status["typed_r2b_operator_decision_required"] is True, "R2B decision lost")
    require(status["source_adapter"]["fixture_dependencies"] is False, "production fixture graph opened")
    require(status["accepted_effect_executable_sha256"] == "677f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06", "effect identity drift")

    require('stage8b-r2a7-source-adapter = []' in cargo, "production feature drift")
    require('required-features = ["stage8b-r2a7-source-adapter"]' in cargo, "production binary feature drift")
    production_block = cargo.split('stage8b-r2a7-source-adapter = []', 1)[0][-80:]
    require("test-fixtures" not in production_block, "fixture leaked into production feature")
    for marker in (
        '"--one-shot-production"', "run_stage8b_r2a7_source_adapter(mode)",
        "evidence.finam_credential_accessed", "evidence.network_accessed",
    ):
        require(marker in binary, f"production entry drift: {marker}")
    for forbidden in ("reqwest", "redis::", "AuthService", ".post(", ".delete(", "OperatorArm", "dispatch_attempt"):
        require(forbidden not in binary and forbidden not in adapter, f"effect surface opened: {forbidden}")

    for marker in (
        "const PRODUCTION_WORK_ROOT", "const PRODUCTION_STAGE7B_PARENT",
        "Stage7bRecoveryReadyOwner::restart(", ".single_exact_dispatch_ready_request()",
        "publish_stage8b_r2a7_operational_sources_from_owner(",
        "stage8b_r2a7_verify_reader_manifest_hmac_sha256(",
        "verify_published_domain(", '"controlled_qualification"',
        "execution_authority_granted: false", "network_accessed: false",
        "finam_credential_accessed: false",
    ):
        require(marker in adapter, f"reader invariant absent: {marker}")
    require("candidates.next().is_some()" in runtime, "duplicate candidate check absent")
    require("request.final_disposition().is_none()" in runtime, "terminal request filter absent")
    require("request.dispatch_attempt_count() == 1" in runtime, "dispatch-count selection drift")
    require("attach_stage8b_r2a7_record_provenance" in composition, "record provenance absent")

    for marker in (
        "ExecStart=/opt/moex-trading/stage8b-r2a7/bin/stage8b-r2a7-source-adapter --one-shot-production",
        "User=m8a8095", "Group=m8a8095", "RestrictAddressFamilies=AF_UNIX",
        "IPAddressDeny=any", "Type=oneshot",
    ):
        require(marker in service, f"production service drift: {marker}")
    require("controlled" not in service, "controlled mode entered production service")
    require("AF_INET" not in service and "AF_INET6" not in service, "production network opened")
    for marker in (
        '"$SEEDER_BIN" "--seed-controlled-$operation"',
        '"$ADAPTER_BIN" "--one-shot-controlled-$operation"',
        '"adapter_domain":"controlled_qualification"',
        '"network_accessed":false', '"finam_credential_accessed":false',
        "stage8b-r2a7-linux-rehearsal: PASS",
    ):
        require(marker in rehearsal, f"qualification assertion absent: {marker}")

    require(build["revision"] == "R2A7", "build revision drift")
    require(build["source_ref"] == status["causal_source_ref"], "source ref drift")
    require(build["production_feature"] == "stage8b-r2a7-source-adapter", "accepted feature drift")
    require(build["fixture_dependencies"] is False, "fixture build accepted")
    require(build["reproducible"] is True, "adapter build not reproducible")
    require(len(build["source_ref"]) == 40 and "PENDING" not in build["source_ref"], "source ref pending")
    require(len(build["build_a_sha256"]) == 64 and "PENDING" not in build["build_a_sha256"], "adapter SHA pending")
    require(build["build_a_sha256"] == build["build_b_sha256"], "non-reproducible adapter")
    require(build["build_a_sha256"] == status["source_adapter"]["executable_sha256"], "adapter SHA drift")
    require(build["controlled_rehearsal"] == {"place": "PASS", "cancel": "PASS", "real_finam": False}, "qualification drift")
    require(build["source_sha256"]["binary"] == sha(ROOT / "crates/finam-gateway/src/bin/stage8b-r2a7-source-adapter.rs"), "binary source drift")
    require(build["source_sha256"]["adapter"] == sha(ROOT / "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs"), "adapter source drift")
    require(build["source_sha256"]["composition"] == sha(ROOT / "crates/finam-gateway/src/stage8a1_execution_capability.rs"), "composition source drift")
    print("stage8b-p-r2a7-check: PASS production_reader=true fixture_graph=false exact_request=true sources=10 place=true cancel=true authorization=NOT_ISSUED real_finam=false")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        raise SystemExit(f"stage8b-p-r2a7-check: FAIL {error}")
