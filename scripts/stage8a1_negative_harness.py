#!/usr/bin/env python3
"""Exact 36 Stage 8A-1 fail-closed mutations."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path
from typing import Callable

import stage8a1_check as checker

ROOT = Path(__file__).resolve().parents[1]
FILES = {
    checker.MODULE,
    checker.LIB,
    checker.DESCRIPTOR,
    checker.DESIGN,
    checker.MATRIX,
    checker.INVENTORY,
    *checker.PREDECESSOR_HASHES.keys(),
}


def copy_contract(destination: Path) -> None:
    for relative in FILES:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)


def replace(root: Path, path: Path, old: str, new: str) -> None:
    target = root / path
    text = target.read_text()
    if old not in text:
        raise RuntimeError(f"mutation source missing: {path}: {old}")
    target.write_text(text.replace(old, new, 1))


def append(root: Path, path: Path, value: str) -> None:
    target = root / path
    target.write_text(target.read_text() + value)


def descriptor(root: Path, mutate: Callable[[dict], None]) -> None:
    path = root / checker.DESCRIPTOR
    value = json.loads(path.read_text())
    mutate(value)
    path.write_text(json.dumps(value, indent=2) + "\n")


def drop_last_matrix_row(root: Path) -> None:
    path = root / checker.MATRIX
    lines = path.read_text().splitlines()
    path.write_text("\n".join(lines[:-1]) + "\n")


CASES: list[tuple[str, Callable[[Path], None]]] = [
    ("self-accept", lambda root: descriptor(root, lambda value: value.__setitem__("status", "ACCEPTED"))),
    ("forge-predecessor", lambda root: descriptor(root, lambda value: value.__setitem__("accepted_stage8a0_ref", "0" * 40))),
    ("forge-review-hash", lambda root: descriptor(root, lambda value: value.__setitem__("accepted_stage8a0_review_sha256", "0" * 64))),
    ("open-stage8a2", lambda root: descriptor(root, lambda value: value["closed_surfaces"].__setitem__("stage8a2", False))),
    ("open-finam-post-delete", lambda root: descriptor(root, lambda value: value["closed_surfaces"].__setitem__("finam_post_delete", False))),
    ("capability-clone", lambda root: replace(root, checker.MODULE, "pub struct Stage8ExecutionCapability {", "#[derive(Clone)]\npub struct Stage8ExecutionCapability {")),
    ("capability-debug", lambda root: replace(root, checker.MODULE, "pub struct Stage8ExecutionCapability {", "#[derive(Debug)]\npub struct Stage8ExecutionCapability {")),
    ("capability-serialize", lambda root: replace(root, checker.MODULE, "pub struct Stage8ExecutionCapability {", "#[derive(Serialize)]\npub struct Stage8ExecutionCapability {")),
    ("public-capability-field", lambda root: replace(root, checker.MODULE, "    approved: Stage8ApprovedCommand,", "    pub approved: Stage8ApprovedCommand,")),
    ("request-extraction", lambda root: append(root, checker.MODULE, "\nimpl Stage8ExecutionCapability { pub fn into_request(self) {} }\n")),
    ("place-builder", lambda root: append(root, checker.MODULE, "\nfn forged_place_builder() { build_place_order_request(); }\n")),
    ("cancel-builder", lambda root: append(root, checker.MODULE, "\nfn forged_cancel_builder() { build_cancel_order_request(); }\n")),
    ("reqwest", lambda root: append(root, checker.MODULE, "\nuse reqwest as forged_transport;\n")),
    ("send-call", lambda root: append(root, checker.MODULE, "\nfn forged_send<T>(x: T) { x.send(); }\n")),
    ("post-call", lambda root: append(root, checker.MODULE, "\nfn forged_post<T>(x: T) { x.post(); }\n")),
    ("redis-command", lambda root: append(root, checker.MODULE, "\nfn forged_redis() { redis::cmd(\"XREAD\"); }\n")),
    ("remove-one-shot", lambda root: replace(root, checker.MODULE, "|| !arm.one_shot", "|| false")),
    ("remove-account-allowlist", lambda root: replace(root, checker.MODULE, "allowlist.accounts.contains(account_id)", "true")),
    ("remove-instrument-allowlist", lambda root: replace(root, checker.MODULE, "allowlist.instruments.contains(instrument)", "true")),
    ("remove-strategy-allowlist", lambda root: replace(root, checker.MODULE, ".any(|value| value == strategy_id)", ".any(|_| true)")),
    ("remove-day-only", lambda root: replace(root, checker.MODULE, "input.order.time_in_force != TimeInForce::Day", "false")),
    ("remove-runallowed", lambda root: replace(root, checker.MODULE, "evidence.state != Stage8KillSwitchState::RunAllowed", "false")),
    ("remove-durable-revision", lambda root: replace(root, checker.MODULE, "evidence.durable_revision == 0", "false")),
    ("remove-finam-owner", lambda root: replace(root, checker.MODULE, "evidence.broker != BrokerKind::Finam", "false")),
    ("remove-single-owner", lambda root: replace(root, checker.MODULE, "evidence.active_broker_owner_count != 1", "false")),
    ("remove-unresolved-orders", lambda root: replace(root, checker.MODULE, "evidence.unresolved_order_count != 0", "false")),
    ("remove-unresolved-delivery", lambda root: replace(root, checker.MODULE, "evidence.unresolved_delivery_count != 0", "false")),
    ("remove-reconciliation-required", lambda root: replace(root, checker.MODULE, "evidence.reconciliation_required_count != 0", "false")),
    ("remove-restart-binding", lambda root: replace(root, checker.MODULE, "arm.restart_generation != restart_generation", "false")),
    ("remove-config-binding", lambda root: replace(root, checker.MODULE, "arm.config_fingerprint != config_fingerprint", "false")),
    ("remove-cancel-mapping", lambda root: replace(root, checker.MODULE, "return Err(Stage8ExecutionPreflightError::CancelMappingRequired);", "return Err(Stage8ExecutionPreflightError::AccountNotAllowed);")),
    ("allow-unmapped-cancel", lambda root: replace(root, checker.MODULE, "if input\n        .broker_preflight_policy\n        .allow_cancel_by_broker_order_id_without_mapping", "if false && input\n        .broker_preflight_policy\n        .allow_cancel_by_broker_order_id_without_mapping")),
    ("remove-clone-compile-fail", lambda root: replace(root, checker.MODULE, "require_clone::<Stage8ExecutionCapability>();", "let _ = 1;")),
    ("remove-serialize-compile-fail", lambda root: replace(root, checker.MODULE, "require_serialize::<Stage8ExecutionCapability>();", "let _ = 1;")),
    ("reduce-matrix", drop_last_matrix_row),
    ("drift-predecessor-freeze", lambda root: append(root, next(iter(checker.PREDECESSOR_HASHES)), "\n")),
]


def main() -> None:
    if len(CASES) != 36:
        raise SystemExit(f"stage8a1-negative: FAIL inventory={len(CASES)}")
    for name, mutate in CASES:
        with tempfile.TemporaryDirectory(prefix="stage8a1-negative-") as temp:
            candidate = Path(temp)
            copy_contract(candidate)
            mutate(candidate)
            try:
                checker.check(candidate, git_scope=False)
            except (checker.CheckFailure, KeyError, ValueError):
                print(f"PASS {name}")
                continue
            raise SystemExit(f"stage8a1-negative: FAIL accepted mutation {name}")
    print("stage8a1-negative: PASS cases=36/36")


if __name__ == "__main__":
    main()
