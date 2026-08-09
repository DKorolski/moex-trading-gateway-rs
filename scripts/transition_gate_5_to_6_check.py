#!/usr/bin/env python3
"""Planning/authority checks for Transition Gate 5->6."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

BASE = "013e63bbee57c4f2d00a0587e9343ab623efba0d"
BRANCH = "stage5g-lifecycle"
CLOSURE_SHA = "0ab4997d9fb62f390bb4d7e789ebc12a9b206007046ca2871bd6776031f37743"
STAGE5_INVENTORY_SHA = "546552301c26fe80cd4106221e25aa2ec35c378708fc208cd3b9a46aa6ce2fd0"
ARTIFACT_SHA = "0f6698a7256537596071eef762f7d623050d1a1ec3023ecafc9b3799e9ba8bf0"
TRANSITION_INVENTORY_SHA = "e9a927bdd623fec3632862177e48c6792c7a362aad1900469ab8d25fd0fc91c0"
ACCEPTANCE_SHA = "beb42e87f558078f1b8362291799739baf4129ef216ff633040581a424be3e86"
DESCRIPTOR = Path("docs/stage-6/transition-5-to-6-descriptor.json")
INVENTORY = Path("docs/stage-6/transition-5-to-6-authority-inventory.json")
ACCEPTANCE = Path("docs/stage-6/stage5g-h-acceptance-reference.json")
CONTRACTS = {
    "identity": Path("docs/stage-6/stage6-durable-identity-contract.md"),
    "ownership": Path("docs/stage-6/stage6-persistence-ownership.md"),
    "crash": Path("docs/stage-6/stage6-crash-window-matrix.md"),
    "slices": Path("docs/stage-6/stage6-slice-plan.md"),
}


class CheckFailure(ValueError):
    pass


def require(value: bool, message: str) -> None:
    if not value:
        raise CheckFailure(message)


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_acceptance(value: dict) -> None:
    require(value.get("verdict") == "ACCEPTED_CLOSED", "Stage 5G-h acceptance missing")
    require(value.get("stage5g_status") == "ACCEPTED_CLOSED", "Stage 5G not closed")
    require(value.get("accepted_commit") == BASE, "accepted closure commit drift")
    require(value.get("accepted_archive_sha256") == "2d37e701f732b531f7a7c599c2d09c6f3526aae8e6b86988cd87371a4fe03644", "accepted archive drift")
    require(value.get("transition_gate_5_to_6") == "OPEN", "transition gate not opened")
    require(value.get("stage6") == "CLOSED_PENDING_TRANSITION_ACCEPTANCE", "Stage 6 opened early")


def validate_descriptor(value: dict) -> None:
    require(value.get("schema_version") == 1, "descriptor schema drift")
    require(value.get("gate") == "Transition Gate 5->6", "gate identity drift")
    binding = value.get("source_ref_binding", {})
    require(binding.get("required_parent") == BASE, "source parent drift")
    require(binding.get("required_branch") == BRANCH, "source branch drift")
    accepted = value.get("accepted_stage5", {})
    require(accepted.get("closure_commit") == BASE, "closure commit drift")
    require(accepted.get("closure_descriptor_sha256") == CLOSURE_SHA, "closure descriptor drift")
    require(accepted.get("authority_inventory_sha256") == STAGE5_INVENTORY_SHA, "Stage 5 inventory drift")
    require(accepted.get("lifecycle_artifact_sha256") == ARTIFACT_SHA, "54-row artifact drift")
    require(accepted.get("lifecycle_row_count") == 54, "lifecycle row count drift")
    require(accepted.get("acceptance_reference_sha256") == ACCEPTANCE_SHA, "acceptance reference drift")
    require(value.get("transition_authority_inventory_sha256") == TRANSITION_INVENTORY_SHA, "transition inventory drift")
    require(value.get("stage6_slices") == ["6A", "6B", "6C", "6D", "6E"], "Stage 6 slice drift")
    require(value.get("stage6_status") == "closed_pending_transition_gate_acceptance", "Stage 6 opened early")
    require(not any(value.get("closed_surfaces", {}).values()), "closed execution surface opened")


def validate_inventory(root: Path, value: dict) -> None:
    require(value.get("accepted_stage5_closure") == BASE, "inventory closure drift")
    entries = value.get("authorities")
    require(isinstance(entries, list) and len(entries) >= 17, "macro authority inventory incomplete")
    seen: set[str] = set()
    for entry in entries:
        path = entry.get("path", "")
        roles = entry.get("classifications", [])
        require(path not in seen, f"duplicate authority: {path}")
        seen.add(path)
        require(isinstance(roles, list) and roles, f"classification missing: {path}")
        target = root / path
        require(target.is_file(), f"authority missing: {path}")
        require(sha(target) == entry.get("sha256"), f"authority hash drift: {path}")
    required = {
        "crates/broker-core/src/ids.rs",
        "crates/broker-core/src/command.rs",
        "crates/broker-core/src/operational_snapshot.rs",
        "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs",
        "source-oracles/alor-stage5/hybrid_intraday_runtime.rs",
        "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "docs/stage-5/accepted-stage5g-g-lifecycle-artifact.json",
    }
    require(required <= seen, "required macro authority missing")
    by_path = {entry["path"]: entry["classifications"] for entry in entries}
    for path in ("crates/strategy-runtime-core/src/stage5g_mock_ack.rs", "crates/strategy-runtime-core/src/stage5g_order_position.rs"):
        require("semantic_authority" in by_path.get(path, []), f"semantic classification missing: {path}")
        require("artifact_fixture_adapter" in by_path.get(path, []), f"fixture role missing: {path}")


REQUIRED_TOKENS = {
    "identity": [
        "StrategyRequestId", "ClientOrderId", "BrokerOrderId", "BrokerTradeId",
        "BrokerAccountId", "InstrumentId", "strategy ID", "owner", "cycle ID", "role",
        "action", "causal_parent_id", "lifecycle_sequence", "append-only",
        "Wall-clock time", "blind redispatch", "requires a new",
    ],
    "ownership": [
        "Stage 5D persistence envelope", "Protective cleanup ledger", "future Stage 6 journal",
        "do not reuse", "validated journal frontier", "No second restart authority",
    ],
    "crash": [
        "Intent accepted before journal write", "Journal write before broker dispatch",
        "Dispatch attempted before broker ID known", "Broker accepted but response lost",
        "Broker ID known before local commit", "Fill observed before request finalization",
        "Cancel requested before cancel result", "Restart with unresolved request",
        "Duplicate command after restart", "Conflicting command with same idempotency key",
    ],
    "slices": ["6A", "6B", "6C", "6D", "6E", "No runtime attachment", "Stage 7"],
}


def validate_contracts(values: dict[str, str]) -> None:
    for name, tokens in REQUIRED_TOKENS.items():
        text = values.get(name, "")
        for token in tokens:
            require(token in text, f"{name} contract missing: {token}")
    crash_rows = [line for line in values["crash"].splitlines() if line.startswith("|") and not line.startswith("|---")]
    require(len(crash_rows) == 11, "crash matrix is not exact ten windows")
    identity_words = " ".join(values["identity"].split())
    require("numeric surrogates" in identity_words.lower(), "numeric surrogate prohibition missing")
    require("FINAM-specific IDs are forbidden" in identity_words, "broker-neutral identity rule missing")


def check(root: Path) -> None:
    require(sha(root / "docs/stage-5/stage5g-closure-descriptor.json") == CLOSURE_SHA, "accepted closure descriptor changed")
    require(sha(root / "docs/stage-5/stage5g-authority-inventory.json") == STAGE5_INVENTORY_SHA, "accepted Stage 5 inventory changed")
    require(sha(root / "docs/stage-5/accepted-stage5g-g-lifecycle-artifact.json") == ARTIFACT_SHA, "accepted artifact changed")
    require(sha(root / ACCEPTANCE) == ACCEPTANCE_SHA, "acceptance marker changed")
    require(sha(root / INVENTORY) == TRANSITION_INVENTORY_SHA, "transition inventory bytes changed")
    validate_acceptance(json.loads((root / ACCEPTANCE).read_text()))
    validate_descriptor(json.loads((root / DESCRIPTOR).read_text()))
    validate_inventory(root, json.loads((root / INVENTORY).read_text()))
    validate_contracts({name: (root / path).read_text() for name, path in CONTRACTS.items()})
    print("transition-gate-5-to-6-check: PASS authorities=17 crash_windows=10 slices=5")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except CheckFailure as error:
        raise SystemExit(f"transition-gate-5-to-6-check: FAIL: {error}") from error


if __name__ == "__main__":
    main()
