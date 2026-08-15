#!/usr/bin/env python3
"""Fail-closed Stage 8A-1 R3 trusted-authority checker."""

from __future__ import annotations

import csv
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "166423668b2dea3e1a9ea505f1d452a367c62b64"
STAGE8A0 = "c949d7f83aa87cf990204a5b8ae66e5ca37c9f1d"
BRANCH = "stage8a1-protected-capability"
BASE_REVIEW_SHA = "c8b20954e89031e292596c9d4a0b245960109889faf2b5ee9510d79ef62ae77e"
STAGE8A0_REVIEW_SHA = "574876211e0c896cc9d61f9f2d078059e54fd471a9b97e94a3c9c8c81930879b"
MODULE = Path("crates/finam-gateway/src/stage8a1_execution_capability.rs")
BLACK_BOX_TEST = Path("crates/finam-gateway/tests/stage8a1_r3_authority_boundary.rs")
LIB = Path("crates/finam-gateway/src/lib.rs")
RUNTIME_RECOVERY = Path("crates/runtime-durable-service/src/recovery.rs")
RUNTIME_LIB = Path("crates/runtime-durable-service/src/lib.rs")
STAGE6_CORE = Path("crates/strategy-runtime-core/src/stage6d_live_core.rs")
STAGE6_LIB = Path("crates/strategy-runtime-core/src/lib.rs")
FINAM_CARGO = Path("crates/finam-gateway/Cargo.toml")
DESCRIPTOR = Path("docs/stage-8/stage8a1-descriptor.json")
DESIGN = Path("docs/stage-8/stage8a1-protected-capability.md")
MATRIX = Path("docs/stage-8/STAGE8A_1_R3_ACCEPTANCE_MATRIX_2026-08-15.csv")
INVENTORY = Path("docs/stage-8/STAGE8A_1_R3_NEGATIVE_INVENTORY_2026-08-15.md")
TZ = Path("docs/stage-8/TZ_STAGE8A_1_R3_TRUSTED_ISSUER_ONE_ARM_CANCEL_REVALIDATION_2026-08-15.md")

PINNED_RUST_SHA256 = {
    LIB: "7335e64973d0ea48f5c75f91a7e2cb8c46a504a52aff35eae9dbefbb20084555",
    MODULE: "bc6cf03ea72f367793a148dc9cda3b8476b0bf31ba838ddb6dcbd040df3ffade",
    BLACK_BOX_TEST: "0213b6729f88259e22acf5925ec82c80be02525987f9eaae49685a02a1f337b5",
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

R3_CONTRACT_HASHES = {
    MATRIX: "6f1b7410308ae0b74099330eb28090748f85d6fc40c54f5f0ef5bd69b0928203",
    INVENTORY: "a979521901adbf292fdd2957808b1b453ab70d79baa7d262465ef5ec60b46707",
    TZ: "a0ff9603bb566b208ebc012123d5e565de6d04401cf79e06926355558099449e",
    DESCRIPTOR: "0ed3f1376f8c40a035109fcc31974c311ba97cb659524107c75a81a737a9892d",
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
    require(descriptor["stage"] == "8A-1-R3", "stage drift")
    require(descriptor["status"] == "trusted_issuer_one_arm_cancel_revalidation_candidate", "self acceptance")
    require(descriptor["base_candidate"] == BASE, "R1 base drift")
    require(descriptor["base_review_sha256"] == BASE_REVIEW_SHA, "R2 review drift")
    require(descriptor["accepted_stage8a0_ref"] == STAGE8A0, "predecessor drift")
    require(descriptor["acceptance_rows"] == 76, "row count drift")
    require(descriptor["negative_cases"] == 70, "negative count drift")
    require(all(descriptor["required"].values()), "required authority disabled")
    require(all(descriptor["closed"].values()), "closed surface opened")
    require(descriptor["next_after_acceptance"] == "Stage 8A-2 only", "scope drift")
    for path, expected in PREDECESSOR_HASHES.items():
        require(sha256(root / path) == expected, f"predecessor artifact drift: {path}")
    for path, expected in R3_CONTRACT_HASHES.items():
        require(sha256(root / path) == expected, f"R3 contract artifact drift: {path}")

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
        "Stage8a1MicroBudgetAuthority", "Stage8a1AuthorityRoot",
        "Stage8a1TrustedCurrentSources",
    ]
    for name in opaque:
        require(not re.search(r"^\s*pub\s+", struct_body(source, name), re.M), f"public field: {name}")

    for token in (
        "build_place_order_request", "build_cancel_order_request", "FinamRestClient",
        "reqwest", ".send(", ".post(", ".delete(", "redis::cmd", "pub fn into_",
        "pub(crate) fn into_",
    ):
        require(token not in source, f"forbidden R3 surface: {token}")
    require("pub fn open(" not in source, "caller-selected issuer opener restored")
    require("Stage8a1CurrentOperationalSources" not in source, "raw current snapshots restored")
    require("logical_arm_nonce" not in source, "caller-chosen arm token restored")

    required_source = (
        "Stage8a1OperationalAuthorityIssuer", "load_accepted_config", "load_current_control",
        "register_arm_nonce", ".create_new(true)", "sync_directory", "Utc::now()",
        "Stage8a1AuthorityRoot", "from_stage7b_owner", "authority_root_sha256",
        "Stage8a1TrustedCurrentSources", "issue_current_sources",
        "one-arm-per-durable-request", "revalidate_place_capability",
        "revalidate_cancel_capability", "regular_file_identity", "directory_identity",
        "persist_current_control", "libc::O_NOFOLLOW", "libc::openat",
        "Stage8a1CurrentlyAuthorizedCapability", "current_state_from_sources",
        "Stage8KillSwitchState::RunAllowed", "BrokerMarketSessionState::Open",
        "broker_truth_is_fresh", "account_orphan_order_count", "max_orders != 1",
    )
    for token in required_source:
        require(token in source, f"R3 issuer/revalidation guard missing: {token}")
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
    require("opaque_authority_boundary" in black_box, "black-box opaque authority witness missing")

    with (root / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 76, "acceptance matrix must contain 76 rows")
    require([row["id"] for row in rows] == [f"S8A1R3-{i:03d}" for i in range(1, 77)], "acceptance ids drift")
    inventory = (root / INVENTORY).read_text()
    require(len(re.findall(r"^\d+\. ", inventory, re.M)) == 70, "negative inventory must contain 70 cases")

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
            "crates/finam-gateway/tests/stage8a1_r2_authority_boundary.rs",
            str(STAGE6_CORE), str(STAGE6_LIB), str(FINAM_CARGO), "Cargo.lock",
            str(DESCRIPTOR), str(DESIGN), str(MATRIX), str(INVENTORY), str(TZ),
            "docs/current-status.md", "docs/roadmap.md", "docs/stage-8/stage8-slice-plan.md",
            "scripts/stage8a1_check.py", "scripts/stage8a1_negative_harness.py",
            "scripts/stage8a1_closed_surface_check.py", "scripts/stage8a1_proof_map.py",
            "scripts/stage8a1_gate.sh", "scripts/make_stage8a1_handoff_archive.py",
            "scripts/stage8a1_handoff_safety_check.py",
        }
        require(changed <= allowed, f"R3 scope violation: {sorted(changed - allowed)}")
        require(not any(path.startswith(".github/") for path in changed), "CI drift")


def main() -> None:
    try:
        check()
    except (CheckFailure, KeyError, ValueError, subprocess.CalledProcessError) as error:
        print(f"stage8a1-r3-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a1-r3-check: PASS rows=76 trusted-root=true one-arm=true cancel-revalidation=true no-send=true next=8A-2-pending")


if __name__ == "__main__":
    main()
