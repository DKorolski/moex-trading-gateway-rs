#!/usr/bin/env python3
"""Fail-closed Stage 8A-1 R2 operational-authority checker."""

from __future__ import annotations

import csv
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "ef6b9ac70aa8a3cdd6bfaf93f1c1339b030eb75d"
STAGE8A0 = "c949d7f83aa87cf990204a5b8ae66e5ca37c9f1d"
BRANCH = "stage8a1-protected-capability"
R1_REVIEW_SHA = "6c98f034f2456a3004ca4fd162ff632372dc22d0199240cd990784025d89a9a6"
STAGE8A0_REVIEW_SHA = "574876211e0c896cc9d61f9f2d078059e54fd471a9b97e94a3c9c8c81930879b"
MODULE = Path("crates/finam-gateway/src/stage8a1_execution_capability.rs")
BLACK_BOX_TEST = Path("crates/finam-gateway/tests/stage8a1_r2_authority_boundary.rs")
LIB = Path("crates/finam-gateway/src/lib.rs")
RUNTIME_RECOVERY = Path("crates/runtime-durable-service/src/recovery.rs")
RUNTIME_LIB = Path("crates/runtime-durable-service/src/lib.rs")
STAGE6_CORE = Path("crates/strategy-runtime-core/src/stage6d_live_core.rs")
STAGE6_LIB = Path("crates/strategy-runtime-core/src/lib.rs")
FINAM_CARGO = Path("crates/finam-gateway/Cargo.toml")
DESCRIPTOR = Path("docs/stage-8/stage8a1-descriptor.json")
DESIGN = Path("docs/stage-8/stage8a1-protected-capability.md")
MATRIX = Path("docs/stage-8/STAGE8A_1_R2_ACCEPTANCE_MATRIX_2026-08-15.csv")
INVENTORY = Path("docs/stage-8/STAGE8A_1_R2_NEGATIVE_INVENTORY_2026-08-15.md")
TZ = Path("docs/stage-8/TZ_STAGE8A_1_R2_OPERATIONAL_AUTHORITY_CONTINUITY_2026-08-15.md")

PINNED_RUST_SHA256 = {
    LIB: "7d12e635730e4a8f6c6f9875298b67bc55c12d7c163f32849ffe2d476cf581a0",
    MODULE: "0950f2b99f50fe2b786759aba8e5684bb04ea4391fed3f7c67a407cc97848197",
    BLACK_BOX_TEST: "b3259f17944f0b2574916e3693e9012eafa286570bd60edf26cb7d881674b8ac",
    RUNTIME_LIB: "6cf2ab07fb70f05c682cdbf9b8882660f08e006f43b397da8d83539e34033211",
    RUNTIME_RECOVERY: "400536c3d48b83f1f41754fb5d4b6757d56bc354ad53185ade156b1114081d35",
    STAGE6_LIB: "120f6c4f5bb838e44b5ae5310bf1f4547b77abede0a73d63d74c58a8d2ad3967",
    STAGE6_CORE: "530797b793281fe2cd7c58d3bfab0408a81c5d57810378bf508aa2d93c198e7e",
}

PREDECESSOR_HASHES = {
    Path("docs/stage-8/stage8a0-descriptor.json"): "fc59a64f00338078ca84e85098d7d18b50e3e719ebf01c4dae521acdbacf9560",
    Path("docs/stage-8/stage8a0-finam-contract-snapshot-2026-08-14.json"): "11062063c5f1f4f83f645af6b3a2e2716af363dca0bafdbdf3ee2b00da5d572e",
    Path("docs/stage-8/stage8a0-contract-parity-evidence-2026-08-14.json"): "d7247d3a8802cc2600bdf3a9eda20fd5075cadf313ff81ad44217b826b431d6f",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def struct_body(source: str, name: str) -> str:
    match = re.search(rf"pub struct {name}\s*\{{(?P<body>.*?)\n\}}", source, re.S)
    require(match is not None, f"missing type: {name}")
    return match.group("body")


def check(root: Path = ROOT, *, git_scope: bool = True, pin_hashes: bool = True) -> None:
    descriptor = json.loads((root / DESCRIPTOR).read_text())
    require(descriptor["stage"] == "8A-1-R2", "stage drift")
    require(descriptor["status"] == "operational_authority_continuity_candidate", "self acceptance")
    require(descriptor["base_candidate"] == BASE, "R1 base drift")
    require(descriptor["base_review_sha256"] == R1_REVIEW_SHA, "R1 review drift")
    require(descriptor["accepted_stage8a0_ref"] == STAGE8A0, "predecessor drift")
    require(descriptor["accepted_stage8a0_review_sha256"] == STAGE8A0_REVIEW_SHA, "review hash drift")
    require(descriptor["acceptance_rows"] == 68, "row count drift")
    require(descriptor["negative_cases"] == 62, "negative count drift")
    require(all(descriptor["requirements"].values()), "required authority disabled")
    require(all(descriptor["closed_surfaces"].values()), "closed surface opened")
    require(descriptor["next_after_independent_acceptance"] == "Stage 8A-2 only", "scope drift")
    for key in ("clone", "copy", "debug", "serialize", "deserialize", "request_extraction_available", "transport_consumer_available"):
        require(descriptor["capability"][key] is False, f"capability opened: {key}")
    for path, expected in PREDECESSOR_HASHES.items():
        require(sha256(root / path) == expected, f"predecessor artifact drift: {path}")

    source = (root / MODULE).read_text()
    runtime = (root / RUNTIME_RECOVERY).read_text()
    stage6 = (root / STAGE6_CORE).read_text()
    lib_source = (root / LIB).read_text()
    black_box = (root / BLACK_BOX_TEST).read_text()
    opaque = [
        "Stage8ExecutionCapability", "Stage8a1CurrentlyAuthorizedCapability",
        "Stage8a1DurableRequestAuthority", "Stage8a1OperatorArmAuthority",
        "Stage8a1FrozenExecutionPolicy", "Stage8a1TrustedClockAuthority",
        "Stage8a1ReadinessAuthority", "Stage8a1KillSwitchAuthority",
        "Stage8a1BrokerOwnershipAuthority", "Stage8a1ZeroAmbiguityAuthority",
        "Stage8a1FreshBrokerTruthAuthority", "Stage8a1ScheduleAuthority",
        "Stage8a1MicroBudgetAuthority",
    ]
    for name in opaque:
        require(not re.search(r"^\s*pub\s+", struct_body(source, name), re.M), f"public field: {name}")

    for token in (
        "build_place_order_request", "build_cancel_order_request", "FinamRestClient",
        "reqwest", ".send(", ".post(", ".delete(", "redis::cmd", "pub fn into_",
        "pub(crate) fn into_",
    ):
        require(token not in source, f"forbidden R2 surface: {token}")

    required_source = (
        "Stage8a1OperationalAuthorityIssuer", "load_accepted_config", "load_current_control",
        "register_arm_nonce", ".create_new(true)", "sync_directory", "Utc::now()",
        "Stage8a1CurrentOperationalSources", "revalidate_place_capability",
        "Stage8a1CurrentlyAuthorizedCapability", "current_state_from_sources",
        "Stage8KillSwitchState::RunAllowed", "BrokerMarketSessionState::Open",
        "broker_truth_is_fresh", "account_orphan_order_count", "max_orders != 1",
    )
    for token in required_source:
        require(token in source, f"R2 issuer/revalidation guard missing: {token}")
    for token in (
        "&mut self", "revalidate_cached_committed_seal(commitment_key)",
        "refresh_stage7b_durable_frontier", "advance_recovery_seal(commitment_key)",
        "final disk/HMAC barrier", "seal_commit_uncertain",
    ):
        require(token in runtime, f"current-seal owner guard missing: {token}")
    for token in (
        "dispatch_attempt_count() != 1", "DispatchAttemptRecorded", "dispatch_record_id",
        "dispatch_sequence", "durable_frontier_sha256", "runtime_config_fingerprint_sha256",
        "stage8a1_exact_durable_authority_rejects_accepted_only_request",
    ):
        require(token in stage6, f"dispatch-ready Stage6 guard missing: {token}")
    require("pub mod stage8a1_execution_capability;" in lib_source, "module export missing")
    require("issuer.authorize_place" in black_box, "black-box production issuer witness missing")

    with (root / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 68, "acceptance matrix must contain 68 rows")
    require([row["id"] for row in rows] == [f"S8A1R2-{i:03d}" for i in range(1, 69)], "acceptance ids drift")
    inventory = (root / INVENTORY).read_text()
    require(len(re.findall(r"^\d+\. ", inventory, re.M)) == 62, "negative inventory must contain 62 cases")

    if pin_hashes:
        for path, expected in PINNED_RUST_SHA256.items():
            require(sha256(root / path) == expected, f"pinned Rust surface drift: {path}")

    if git_scope:
        subprocess.run(["git", "merge-base", "--is-ancestor", BASE, "HEAD"], cwd=root, check=True)
        committed = subprocess.check_output(["git", "diff", "--name-only", BASE], cwd=root, text=True).splitlines()
        untracked = subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True).splitlines()
        changed = set(committed + untracked)
        allowed = {
            str(MODULE), str(BLACK_BOX_TEST), str(LIB), str(RUNTIME_RECOVERY), str(RUNTIME_LIB),
            str(STAGE6_CORE), str(STAGE6_LIB), str(FINAM_CARGO), "Cargo.lock",
            str(DESCRIPTOR), str(DESIGN), str(MATRIX), str(INVENTORY), str(TZ),
            "docs/current-status.md", "docs/roadmap.md", "docs/stage-8/stage8-slice-plan.md",
            "scripts/stage8a1_check.py", "scripts/stage8a1_negative_harness.py",
            "scripts/stage8a1_closed_surface_check.py", "scripts/stage8a1_proof_map.py",
            "scripts/stage8a1_gate.sh", "scripts/make_stage8a1_handoff_archive.py",
            "scripts/stage8a1_handoff_safety_check.py",
        }
        require(changed <= allowed, f"R2 scope violation: {sorted(changed - allowed)}")
        require(not any(path.startswith(".github/") for path in changed), "CI drift")


def main() -> None:
    try:
        check()
    except (CheckFailure, KeyError, ValueError, subprocess.CalledProcessError) as error:
        print(f"stage8a1-r2-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a1-r2-check: PASS rows=68 issuers=production durable=dispatch-ready no-send=true next=8A-2-pending")


if __name__ == "__main__":
    main()
