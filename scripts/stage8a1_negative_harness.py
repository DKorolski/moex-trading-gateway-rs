#!/usr/bin/env python3
"""Exact 52 Stage 8A-1 R1 fail-closed mutations."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path
from typing import Callable

import stage8a1_check as checker

ROOT = Path(__file__).resolve().parents[1]
FILES = {
    checker.DESCRIPTOR, checker.DESIGN, checker.MATRIX, checker.INVENTORY,
    checker.FINAM_CARGO, Path("Cargo.lock"),
    *checker.PINNED_RUST_SHA256.keys(), *checker.PREDECESSOR_HASHES.keys(),
}


def copy_contract(destination: Path) -> None:
    for relative in FILES:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)


def replace(root: Path, path: Path, old: str, new: str) -> None:
    target = root / path
    value = target.read_text()
    if old not in value:
        raise RuntimeError(f"mutation source missing: {path}: {old}")
    target.write_text(value.replace(old, new, 1))


def inject(root: Path, code: str, path: Path = checker.MODULE) -> None:
    target = root / path
    target.write_text(target.read_text() + f"\n{code}\n")


def descriptor(root: Path, mutate: Callable[[dict], None]) -> None:
    path = root / checker.DESCRIPTOR
    value = json.loads(path.read_text())
    mutate(value)
    path.write_text(json.dumps(value, indent=2) + "\n")


def source_case(code: str, path: Path = checker.MODULE) -> Callable[[Path], None]:
    return lambda root: inject(root, code, path)


CASES: list[tuple[str, Callable[[Path], None]]] = [
    ("self-accept", lambda r: descriptor(r, lambda v: v.__setitem__("status", "ACCEPTED"))),
    ("forge-stage8a0-ref", lambda r: descriptor(r, lambda v: v.__setitem__("accepted_stage8a0_ref", "0" * 40))),
    ("forge-stage8a0-review", lambda r: descriptor(r, lambda v: v.__setitem__("accepted_stage8a0_review_sha256", "0" * 64))),
    ("open-stage8a2", lambda r: descriptor(r, lambda v: v["closed_surfaces"].__setitem__("stage8a2", False))),
    ("capability-clone-copy", source_case("impl Clone for Stage8ExecutionCapability { fn clone(&self) -> Self { unreachable!() } }")),
    ("capability-debug", source_case("impl std::fmt::Debug for Stage8ExecutionCapability { fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { Ok(()) } }")),
    ("capability-serialize", source_case("impl serde::Serialize for Stage8ExecutionCapability { fn serialize<S>(&self, _: S) -> Result<S::Ok,S::Error> where S: serde::Serializer { unreachable!() } }")),
    ("public-capability-field", lambda r: replace(r, checker.MODULE, "    approved: Stage8ApprovedCommand,", "    pub approved: Stage8ApprovedCommand,")),
    ("request-extraction", source_case("impl Stage8ExecutionCapability { pub fn into_request(self) {} }")),
    ("builder-composition", source_case("fn bypass() { broker_finam::build_place_order_request(); }")),
    ("http-send-redis", source_case("fn bypass<T>(x:T) { x.send(); redis::cmd(\"XREAD\"); }")),
    ("stage8-journal-owner", source_case("struct Stage8JournalReducer;")),
    ("without-stage7b-owner", lambda r: replace(r, checker.MODULE, "owner: &Stage7bRecoveryReadyOwner,", "owner: &(),")),
    ("stale-seal", lambda r: replace(r, checker.RUNTIME_RECOVERY, "seal_generation: seal.seal_generation(),", "seal_generation: 1,")),
    ("raw-place-without-stage6", lambda r: replace(r, checker.STAGE6_CORE, "pub fn authorize_exact_durable_request", "pub fn bypass_exact_durable_request")),
    ("place-client-id-drift", source_case("fn allow_changed_place_client_id() {}")),
    ("cancel-target-drift", source_case("fn allow_changed_cancel_target() {}")),
    ("attribution-drift", source_case("fn allow_changed_attribution() {}")),
    ("without-readiness", lambda r: replace(r, checker.MODULE, "input.readiness,", "test_missing_readiness(),")),
    ("public-proof-literal", lambda r: replace(r, checker.MODULE, "    state: Stage8KillSwitchState,", "    pub state: Stage8KillSwitchState,")),
    ("public-arm-fields", lambda r: replace(r, checker.MODULE, "    nonce_sha256: String,", "    pub nonce_sha256: String,")),
    ("clone-arm", source_case("impl Clone for Stage8a1OperatorArmAuthority { fn clone(&self)->Self { unreachable!() } }")),
    ("reuse-arm-twice", source_case("fn reuse_arm(_: &Stage8a1OperatorArmAuthority) {}")),
    ("duplicate-arm-nonce", source_case("fn accept_duplicate_nonce() -> bool { true }")),
    ("arm-request-drift", source_case("fn arm_request_drift() {}")),
    ("arm-client-id-drift", source_case("fn arm_client_id_drift() {}")),
    ("arm-scope-drift", source_case("fn arm_scope_drift() {}")),
    ("arm-attribution-drift", source_case("fn arm_attribution_drift() {}")),
    ("arm-side-drift", source_case("fn arm_side_drift() {}")),
    ("arm-qty-drift", source_case("fn arm_qty_drift() {}")),
    ("arm-limit-price-drift", source_case("fn arm_limit_price_drift() {}")),
    ("arm-market-guard-drift", source_case("fn arm_market_guard_drift() {}")),
    ("arm-risk-guard-drift", source_case("fn arm_risk_guard_drift() {}")),
    ("arm-build-config-endpoint-drift", source_case("fn arm_digest_drift() {}")),
    ("unbounded-arm-ttl", lambda r: replace(r, checker.MODULE, "> policy.max_arm_ttl_ms as i64", "> u64::MAX as i64")),
    ("reuse-after-restart", source_case("fn reuse_after_restart() {}")),
    ("reuse-after-config-drift", source_case("fn reuse_after_config_drift() {}")),
    ("arbitrary-frozen-policy", lambda r: replace(r, checker.MODULE, "    broker_policy: OrderPreflightPolicy,", "    pub broker_policy: OrderPreflightPolicy,")),
    ("unsupported-type-or-tif", source_case("fn allow_gtc_stop() {}")),
    ("widen-quantity-policy", source_case("fn widen_max_qty() {}")),
    ("remove-notional-limits", lambda r: replace(r, checker.MODULE, "|| broker.max_notional_per_order.is_none()", "|| false")),
    ("disable-slippage-age", lambda r: replace(r, checker.MODULE, "|| broker.max_reference_age_ms == 0", "|| false")),
    ("ignore-schedule", lambda r: replace(r, checker.MODULE, "schedule.state != Stage8ScheduleState::Eligible", "false")),
    ("forge-runallowed", lambda r: replace(r, checker.MODULE, "kill_switch.state != Stage8KillSwitchState::RunAllowed", "false")),
    ("ownership-other-strategy", source_case("fn ownership_other_strategy() {}")),
    ("ambiguity-other-scope", source_case("fn ambiguity_other_scope() {}")),
    ("omit-stale-truth", lambda r: replace(r, checker.MODULE, "if !truth.account_truth_fresh", "if false && !truth.account_truth_fresh")),
    ("exhaust-micro-budget", lambda r: replace(r, checker.MODULE, "if budget.max_orders != 1", "if false && budget.max_orders != 1")),
    ("caller-time-or-age", lambda r: replace(r, checker.MODULE, "age as u64 > max_age_ms", "false")),
    ("mixed-proof-scope", lambda r: replace(r, checker.MODULE, "observed_scope != expected_scope", "false")),
    ("terminal-unmapped-cancel", source_case("fn mint_terminal_cancel() {}")),
    ("hidden-builder-in-lib", source_case("pub fn hidden() { broker_finam::build_place_order_request(); }", checker.LIB)),
]


def main() -> None:
    if len(CASES) != 52:
        raise SystemExit(f"stage8a1-r1-negative: FAIL inventory={len(CASES)}")
    for name, mutate in CASES:
        with tempfile.TemporaryDirectory(prefix="stage8a1-r1-negative-") as temp:
            candidate = Path(temp)
            copy_contract(candidate)
            mutate(candidate)
            try:
                checker.check(candidate, git_scope=False, pin_hashes=True)
            except (checker.CheckFailure, KeyError, ValueError):
                print(f"PASS {name}")
                continue
            raise SystemExit(f"stage8a1-r1-negative: FAIL accepted mutation {name}")
    print("stage8a1-r1-negative: PASS cases=52/52")


if __name__ == "__main__":
    main()
