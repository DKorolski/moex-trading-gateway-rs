#!/usr/bin/env python3
"""Fail-closed Stage 8A-1 R1 authority/provenance contract checker."""

from __future__ import annotations

import csv
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "c949d7f83aa87cf990204a5b8ae66e5ca37c9f1d"
R0 = "29b868621361c5beff16e3008864e66d2efdafef"
BRANCH = "stage8a1-protected-capability"
REVIEW_SHA = "574876211e0c896cc9d61f9f2d078059e54fd471a9b97e94a3c9c8c81930879b"
R0_REVIEW_SHA = "a36c9cc254c0ff7ca22c4b1c89484a20adb359f27e0b5db607668282d6dcf82d"
MODULE = Path("crates/finam-gateway/src/stage8a1_execution_capability.rs")
LIB = Path("crates/finam-gateway/src/lib.rs")
RUNTIME_RECOVERY = Path("crates/runtime-durable-service/src/recovery.rs")
RUNTIME_LIB = Path("crates/runtime-durable-service/src/lib.rs")
STAGE6_CORE = Path("crates/strategy-runtime-core/src/stage6d_live_core.rs")
STAGE6_LIB = Path("crates/strategy-runtime-core/src/lib.rs")
FINAM_CARGO = Path("crates/finam-gateway/Cargo.toml")
DESCRIPTOR = Path("docs/stage-8/stage8a1-descriptor.json")
DESIGN = Path("docs/stage-8/stage8a1-protected-capability.md")
MATRIX = Path("docs/stage-8/STAGE8A_1_ACCEPTANCE_MATRIX_2026-08-15.csv")
INVENTORY = Path("docs/stage-8/STAGE8A_1_NEGATIVE_INVENTORY_2026-08-15.md")

PINNED_RUST_SHA256 = {
    LIB: "872d3f38b74931720573be52d2a6799f881e02963d239c621343a062146f29f2",
    MODULE: "7c6aaa667090d1bc682dfa38f43dfef93d1c89427f5f755da68d72a6d30ea338",
    RUNTIME_LIB: "6cf2ab07fb70f05c682cdbf9b8882660f08e006f43b397da8d83539e34033211",
    RUNTIME_RECOVERY: "49f5dd0350e012acef94f77d08f1e16d1b64021994009dfda98b1c2651713859",
    STAGE6_LIB: "120f6c4f5bb838e44b5ae5310bf1f4547b77abede0a73d63d74c58a8d2ad3967",
    STAGE6_CORE: "1757352c431d5c59a160f5d687ae3f35e4c1a47e3a8b328073bbce2fd682a39f",
}

PREDECESSOR_HASHES = {
    Path("docs/stage-8/stage8a0-descriptor.json"): "fc59a64f00338078ca84e85098d7d18b50e3e719ebf01c4dae521acdbacf9560",
    Path("docs/stage-8/stage8a0-finam-contract-snapshot-2026-08-14.json"): "11062063c5f1f4f83f645af6b3a2e2716af363dca0bafdbdf3ee2b00da5d572e",
    Path("docs/stage-8/stage8a0-contract-parity-evidence-2026-08-14.json"): "d7247d3a8802cc2600bdf3a9eda20fd5075cadf313ff81ad44217b826b431d6f",
    Path("docs/stage-8/STAGE8A_0_R1_ACCEPTANCE_MATRIX_2026-08-14.csv"): "2f3692a26df9dfa4d8d5bb14ef36f5ca0a86ada7c85179ebf9d9263fb8be41b6",
    Path("docs/stage-8/STAGE8A_0_R1_NEGATIVE_INVENTORY_2026-08-14.md"): "ec67a816a8a1dfa061d09178342d0864369dd55b4ea5e517cf2823392ef926b0",
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
    require(descriptor["stage"] == "8A-1-R1", "stage drift")
    require(descriptor["status"] == "authority_provenance_hardening_candidate", "self acceptance")
    require(descriptor["base_candidate"] == R0, "R0 candidate drift")
    require(descriptor["base_review_sha256"] == R0_REVIEW_SHA, "R0 review drift")
    require(descriptor["accepted_stage8a0_ref"] == BASE, "predecessor drift")
    require(descriptor["accepted_stage8a0_review_sha256"] == REVIEW_SHA, "review hash drift")
    require(descriptor["acceptance_rows"] == 58, "row count descriptor drift")
    require(descriptor["negative_cases"] == 52, "negative count descriptor drift")
    require(all(descriptor["requirements"].values()), "required authority disabled")
    require(all(descriptor["closed_surfaces"].values()), "closed surface opened")
    require(descriptor["next_after_acceptance"] == "Stage 8A-2 only", "template scope drift")
    require(descriptor["next_after_independent_acceptance"] == "Stage 8A-2 only", "scope drift")
    for key in ("clone", "copy", "debug", "serialize", "deserialize", "request_extraction_available", "transport_consumer_available"):
        require(descriptor["capability"][key] is False, f"capability opened: {key}")
    for path, expected in PREDECESSOR_HASHES.items():
        require(sha256(root / path) == expected, f"predecessor artifact drift: {path}")

    source = (root / MODULE).read_text()
    runtime = (root / RUNTIME_RECOVERY).read_text()
    stage6 = (root / STAGE6_CORE).read_text()
    lib_source = (root / LIB).read_text()
    cargo = (root / FINAM_CARGO).read_text()

    opaque = [
        "Stage8ExecutionCapability", "Stage8a1DurableRequestAuthority",
        "Stage8a1OperatorArmAuthority", "Stage8a1FrozenExecutionPolicy",
        "Stage8a1TrustedClockAuthority", "Stage8a1ReadinessAuthority",
        "Stage8a1KillSwitchAuthority", "Stage8a1BrokerOwnershipAuthority",
        "Stage8a1ZeroAmbiguityAuthority", "Stage8a1FreshBrokerTruthAuthority",
        "Stage8a1ScheduleAuthority", "Stage8a1MicroBudgetAuthority",
    ]
    for name in opaque:
        require(not re.search(r"^\s*pub\s+", struct_body(source, name), re.M), f"public field: {name}")
        prefix = source[: source.index(f"pub struct {name}")][-160:]
        require("#[derive" not in prefix, f"authority derives traits: {name}")
    for token in (
        "impl Clone for Stage8", "impl Copy for Stage8", "impl Serialize for Stage8a1",
        "impl Deserialize for Stage8a1", "pub fn into_", "pub(crate) fn into_",
        "build_place_order_request", "build_cancel_order_request", "reqwest", ".send(",
        ".post(", ".delete(", "redis::cmd", "FinamRestClient",
    ):
        require(token not in source, f"forbidden Stage8A-1 surface: {token}")
    for removed in (
        "Stage8ExecutionAllowlist", "Stage8OperatorArmInput",
        "Stage8PersistentKillSwitchEvidenceInput", "pub max_arm_ttl_ms: u64,",
        "pub max_evidence_age_ms: u64,", "pub now: DateTime<Utc>",
        "pub broker_preflight_policy: &'a OrderPreflightPolicy",
    ):
        require(removed not in source, f"forgeable R0 input retained: {removed}")

    required_source = (
        "from_stage7b_owner", "authorize_stage8a1_durable_request",
        "Stage8a1OperatorArmAuthority",
        "exact_command_sha256", "durable_command_sha256", "max_arm_ttl_ms",
        "max_evidence_age_ms", "Stage8ScheduleState::Eligible",
        "Stage8KillSwitchState::RunAllowed", "active_owner_count != 1",
        "unresolved_order_count != 0", "account_truth_fresh",
        "max_orders != 1", "authority_scope_sha256",
        "CancelPreflightApproval::AlreadyTerminal", "diagnostic(&self)",
    )
    for token in required_source:
        require(token in source, f"required authority guard missing: {token}")
    require("Stage7bRecoveryReadyOwner" in source, "Stage7B owner bridge missing")
    require("Stage6DurableRequestIdentityV1" in source, "Stage6 identity binding missing")
    require("pub struct Stage7bStage8a1DurableRequestAuthority" in runtime, "Stage7B wrapper missing")
    require("pub fn authorize_stage8a1_durable_request" in runtime, "Stage7B issuer missing")
    require("pub struct Stage6DurableRequestAuthorityV1" in stage6, "Stage6 authority missing")
    require("pub fn authorize_exact_durable_request" in stage6, "Stage6 exact issuer missing")
    require("runtime-durable-service" in cargo and "strategy-runtime-core" in cargo, "bridge deps missing")
    require("pub mod stage8a1_execution_capability;" in lib_source, "module export missing")

    with (root / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 58, "acceptance matrix must contain 58 rows")
    require([row["id"] for row in rows] == [f"S8A1R1-{index:03d}" for index in range(1, 59)], "acceptance ids drift")
    inventory = (root / INVENTORY).read_text()
    require(len(re.findall(r"^\d+\. ", inventory, re.M)) == 52, "negative inventory must contain 52 cases")

    if pin_hashes:
        for path, expected in PINNED_RUST_SHA256.items():
            require(sha256(root / path) == expected, f"pinned Rust surface drift: {path}")

    if git_scope:
        subprocess.run(["git", "merge-base", "--is-ancestor", R0, "HEAD"], cwd=root, check=True)
        committed = subprocess.check_output(["git", "diff", "--name-only", R0], cwd=root, text=True).splitlines()
        untracked = subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True).splitlines()
        changed = set(committed + untracked)
        allowed = {
            str(MODULE), str(LIB), str(RUNTIME_RECOVERY), str(RUNTIME_LIB),
            str(STAGE6_CORE), str(STAGE6_LIB), str(FINAM_CARGO), "Cargo.lock",
            str(DESCRIPTOR), str(DESIGN), str(MATRIX), str(INVENTORY),
            "docs/current-status.md", "docs/roadmap.md", "docs/stage-8/stage8-slice-plan.md",
            "scripts/stage8a1_check.py", "scripts/stage8a1_negative_harness.py",
            "scripts/stage8a1_closed_surface_check.py", "scripts/stage8a1_proof_map.py",
            "scripts/stage8a1_gate.sh", "scripts/make_stage8a1_handoff_archive.py",
            "scripts/stage8a1_handoff_safety_check.py",
        }
        require(changed <= allowed, f"R1 scope violation: {sorted(changed - allowed)}")
        require(not any(path.startswith(".github/") for path in changed), "CI drift")


def main() -> None:
    try:
        check()
    except (CheckFailure, KeyError, ValueError, subprocess.CalledProcessError) as error:
        print(f"stage8a1-r1-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a1-r1-check: PASS rows=58 opaque-authorities=true no-send=true next=8A-2-pending")


if __name__ == "__main__":
    main()
