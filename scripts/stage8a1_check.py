#!/usr/bin/env python3
"""Fail-closed Stage 8A-1 protected-capability contract checker."""

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
BRANCH = "stage8a1-protected-capability"
REVIEW_SHA = "574876211e0c896cc9d61f9f2d078059e54fd471a9b97e94a3c9c8c81930879b"
MODULE = Path("crates/finam-gateway/src/stage8a1_execution_capability.rs")
LIB = Path("crates/finam-gateway/src/lib.rs")
DESCRIPTOR = Path("docs/stage-8/stage8a1-descriptor.json")
DESIGN = Path("docs/stage-8/stage8a1-protected-capability.md")
MATRIX = Path("docs/stage-8/STAGE8A_1_ACCEPTANCE_MATRIX_2026-08-15.csv")
INVENTORY = Path("docs/stage-8/STAGE8A_1_NEGATIVE_INVENTORY_2026-08-15.md")
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


def load_json(root: Path, path: Path) -> dict:
    return json.loads((root / path).read_text())


def capability_block(source: str) -> str:
    match = re.search(r"pub struct Stage8ExecutionCapability\s*\{(?P<body>.*?)\n\}", source, re.S)
    require(match is not None, "opaque capability type missing")
    return match.group("body")


def check(root: Path = ROOT, *, git_scope: bool = True) -> None:
    descriptor = load_json(root, DESCRIPTOR)
    require(descriptor["stage"] == "8A-1", "stage drift")
    require(
        descriptor["status"] == "implementation_candidate_independent_acceptance_pending",
        "candidate self-accepted or status drifted",
    )
    require(descriptor["accepted_stage8a0_ref"] == BASE, "accepted predecessor drift")
    require(
        descriptor["accepted_stage8a0_review_sha256"] == REVIEW_SHA,
        "accepted predecessor review drift",
    )
    capability = descriptor["capability"]
    for key in ("clone", "copy", "debug", "serialize", "deserialize", "request_extraction_available", "transport_consumer_available"):
        require(capability[key] is False, f"capability boundary opened: {key}")
    require(capability["opaque_private_fields"] is True, "capability fields not opaque")
    for key, value in descriptor["closed_surfaces"].items():
        require(value is True, f"closed surface opened: {key}")
    require(
        descriptor["next_after_independent_acceptance"] == "Stage 8A-2 only",
        "next-stage scope drift",
    )

    for path, expected in PREDECESSOR_HASHES.items():
        require(sha256(root / path) == expected, f"accepted predecessor artifact drift: {path}")

    source = (root / MODULE).read_text()
    lib_source = (root / LIB).read_text()
    block = capability_block(source)
    require(not re.search(r"^\s*pub\s+", block, re.M), "public capability field")
    prefix = source[: source.index("pub struct Stage8ExecutionCapability")][-180:]
    require("#[derive" not in prefix, "capability derives traits")
    for token in (
        "impl Clone for Stage8ExecutionCapability",
        "impl Copy for Stage8ExecutionCapability",
        "impl std::fmt::Debug for Stage8ExecutionCapability",
        "impl Serialize for Stage8ExecutionCapability",
        "impl Deserialize for Stage8ExecutionCapability",
    ):
        require(token not in source, f"forbidden capability trait: {token}")
    for token in (
        "build_place_order_request",
        "build_cancel_order_request",
        "reqwest",
        ".send(",
        ".post(",
        ".delete(",
        "redis::cmd",
        "FinamRestClient",
        "into_request",
        "into_approved_command",
        "extract_request",
    ):
        require(token not in source, f"forbidden Stage 8A-1 surface: {token}")
    require("pub fn diagnostic(&self)" in source, "redacted diagnostic missing")
    require("pub fn into_" not in source, "public consuming/extraction method added")
    require("pub(crate) fn into_" not in source, "crate-private extraction method added")
    require("enum Stage8ApprovedCommand" in source and "pub enum Stage8ApprovedCommand" not in source, "approved command exposed")
    require("#[derive(Debug, PartialEq, Eq)]\npub struct Stage8OperatorArmInput" in source, "operator arm became cloneable")
    require("|| !arm.one_shot" in source, "one-shot arm check missing")
    require("arm.request_id != request_id" in source, "request binding missing")
    require("allowlist.accounts.contains(account_id)" in source, "account allowlist missing")
    require("allowlist.instruments.contains(instrument)" in source, "instrument allowlist missing")
    require(".any(|value| value == strategy_id)" in source, "strategy allowlist missing")
    require("input.order.time_in_force != TimeInForce::Day" in source, "DAY-only check missing")
    require("evidence.state != Stage8KillSwitchState::RunAllowed" in source, "RunAllowed check missing")
    require("evidence.durable_revision == 0" in source, "durable kill-switch revision missing")
    require("evidence.broker != BrokerKind::Finam" in source, "FINAM ownership missing")
    require("evidence.active_broker_owner_count != 1" in source, "single-owner check missing")
    require("evidence.unresolved_order_count != 0" in source, "unresolved-order check missing")
    require("evidence.unresolved_delivery_count != 0" in source, "unknown-delivery check missing")
    require("evidence.reconciliation_required_count != 0" in source, "reconciliation check missing")
    require("arm.restart_generation != restart_generation" in source, "restart binding missing")
    require("arm.config_fingerprint != config_fingerprint" in source, "config binding missing")
    require("return Err(Stage8ExecutionPreflightError::CancelMappingRequired);" in source, "cancel mapping guard missing")
    require(
        re.search(
            r"if\s+input\s*\.broker_preflight_policy\s*\.allow_cancel_by_broker_order_id_without_mapping\s*\{\s*return Err\(Stage8ExecutionPreflightError::BrokerPolicyTooWide\);",
            source,
            re.S,
        ) is not None,
        "mapping bypass guard missing",
    )
    require("CancelPreflightApproval::AlreadyTerminal" in source, "terminal cancel guard missing")
    require("require_clone::<Stage8ExecutionCapability>()" in source, "Clone compile-fail witness missing")
    require("require_serialize::<Stage8ExecutionCapability>()" in source, "Serialize compile-fail witness missing")
    require("require_debug::<Stage8ExecutionCapability>()" in source, "Debug compile-fail witness missing")
    require("let _ = Stage8ExecutionCapability {};" in source, "privacy compile-fail witness missing")
    require("pub mod stage8a1_execution_capability;" in lib_source, "module export missing")

    with (root / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 36, "acceptance matrix must contain 36 rows")
    require([row["id"] for row in rows] == [f"S8A1-{index:03d}" for index in range(1, 37)], "acceptance ids drift")
    inventory = (root / INVENTORY).read_text()
    require(len(re.findall(r"^\d+\. ", inventory, re.M)) == 36, "negative inventory must contain 36 cases")
    design = (root / DESIGN).read_text()
    require(BASE in design and REVIEW_SHA in design, "design predecessor binding missing")
    require("Only independent acceptance of this exact slice may open Stage 8A-2" in design, "independent gate missing")

    if git_scope:
        subprocess.run(["git", "merge-base", "--is-ancestor", BASE, "HEAD"], cwd=root, check=True)
        committed = subprocess.check_output(["git", "diff", "--name-only", BASE], cwd=root, text=True).splitlines()
        untracked = subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True).splitlines()
        changed = set(committed + untracked)
        allowed_exact = {
            str(MODULE), str(LIB), str(DESCRIPTOR), str(DESIGN), str(MATRIX), str(INVENTORY),
            "docs/current-status.md", "docs/roadmap.md", "docs/stage-8/stage8-slice-plan.md",
            "scripts/stage8a1_check.py", "scripts/stage8a1_negative_harness.py",
            "scripts/stage8a1_closed_surface_check.py", "scripts/stage8a1_proof_map.py",
            "scripts/stage8a1_gate.sh",
            "scripts/make_stage8a1_handoff_archive.py", "scripts/stage8a1_handoff_safety_check.py",
        }
        require(changed <= allowed_exact, f"Stage 8A-1 scope violation: {sorted(changed - allowed_exact)}")
        require(not any(path in changed for path in ("Cargo.toml", "Cargo.lock")), "Cargo drift")
        require(not any(path.startswith(".github/") for path in changed), "GitHub workflow drift")


def main() -> None:
    try:
        check()
    except (CheckFailure, KeyError, ValueError, subprocess.CalledProcessError) as error:
        print(f"stage8a1-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a1-check: PASS rows=36 opaque=true no-send=true next=8A-2-pending")


if __name__ == "__main__":
    main()
