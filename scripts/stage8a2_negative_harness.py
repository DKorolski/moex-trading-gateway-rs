#!/usr/bin/env python3
"""Run the exact 37 Stage 8A-2 semantic escape mutations."""

from __future__ import annotations

import json
import shutil
import tempfile
from dataclasses import dataclass
from pathlib import Path

import stage8a2_check as checker

ROOT = Path(__file__).resolve().parents[1]
MODULE = checker.MODULE
PARENT = checker.PARENT
DESCRIPTOR = checker.DESCRIPTOR


@dataclass(frozen=True)
class Mutation:
    label: str
    path: Path
    old: str | None = None
    new: str | None = None
    append: str | None = None


def mutation(label: str, token: str) -> Mutation:
    return Mutation(label, MODULE, append=f"\nfn {token}() {{}}\n")


MUTATIONS = [
    Mutation(
        "accept an unrevalidated Stage8ExecutionCapability",
        MODULE,
        "pub fn compose_stage8a2_no_send(\n        self,",
        "pub fn compose_stage8a2_no_send(\n        capability: Stage8ExecutionCapability,",
    ),
    Mutation(
        "accept a diagnostic instead of the continuation",
        MODULE,
        "pub fn compose_stage8a2_no_send(\n        self,",
        "pub fn compose_stage8a2_no_send(\n        diagnostic: Stage8a1ContinuationDiagnostic,",
    ),
    mutation("reconstruct a capability", "reconstruct_capability"),
    mutation("reconstruct an approved command", "reconstruct_approved_command"),
    Mutation("expose an approved-command getter", MODULE, append="\npub fn approved() {}\n"),
    Mutation(
        "borrow rather than consume the continuation",
        MODULE,
        "pub fn compose_stage8a2_no_send(\n        self,",
        "pub fn compose_stage8a2_no_send(\n        &self,",
    ),
    Mutation(
        "clone or reuse continuation authority",
        PARENT,
        "pub struct Stage8a1CurrentlyAuthorizedCapability {",
        "#[derive(Clone)]\npub struct Stage8a1CurrentlyAuthorizedCapability {",
    ),
    mutation("hand-build a PLACE DTO", "hand_build_place"),
    mutation("hand-build a CANCEL request/path", "hand_build_cancel"),
    mutation("add a second PLACE/CANCEL serializer", "serialize_place_order"),
    Mutation(
        "bypass the accepted PLACE builder",
        MODULE,
        "build_place_order_request(&approved, None)?",
        "hand_build_place(&approved)?",
    ),
    Mutation(
        "bypass the accepted CANCEL builder",
        MODULE,
        "build_cancel_order_request(&approved)?",
        "hand_build_cancel(&approved)?",
    ),
    Mutation(
        "change PLACE outgoing comment from None to Some",
        MODULE,
        "build_place_order_request(&approved, None)?",
        "build_place_order_request(&approved, Some(&comment))?",
    ),
    Mutation(
        "export FinamPlaceOrderRequestSpec",
        MODULE,
        append="\npub fn leak_place() -> FinamPlaceOrderRequestSpec { loop {} }\n",
    ),
    Mutation(
        "export FinamCancelOrderRequestSpec",
        MODULE,
        append="\npub fn leak_cancel() -> FinamCancelOrderRequestSpec { loop {} }\n",
    ),
    Mutation("export raw body or serialized JSON", MODULE, append="\npub fn raw_body() {}\n"),
    Mutation("export raw path or path segments", MODULE, append="\npub fn raw_path() {}\n"),
    Mutation(
        "export raw account/order/client identifiers",
        MODULE,
        "pub account_id_present: bool,",
        "pub account_id: String,\n    pub account_id_present: bool,",
    ),
    mutation("export a transport-ready URL/request builder", "transport_ready_url"),
    Mutation(
        "import M3d2RealOrderEndpointTransport",
        MODULE,
        append="\nuse crate::m3d2_real_order_transport::M3d2RealOrderEndpointTransport;\n",
    ),
    mutation("invoke historical PLACE execution", "place_order_execution"),
    mutation("invoke historical CANCEL execution", "cancel_order_execution"),
    Mutation(
        "import EndpointGateApproved",
        MODULE,
        append="\nuse crate::real_order_endpoint::EndpointGateApproved;\n",
    ),
    Mutation("add reqwest to the Stage8A2 path", MODULE, append="\nuse reqwest::Client;\n"),
    Mutation("add post", MODULE, append="\nfn escape(c: Client) { c.post(\"x\"); }\n"),
    Mutation("add delete", MODULE, append="\nfn escape(c: Client) { c.delete(\"x\"); }\n"),
    Mutation("add send", MODULE, append="\nfn escape(r: Request) { r.send(); }\n"),
    Mutation(
        "reach m3j16 actual one shot",
        MODULE,
        append="\nconst FEATURE: &str = \"m3j16-actual-one-shot\";\n",
    ),
    Mutation("add external FINAM base URL", MODULE, append="\nfn base_url() {}\n"),
    Mutation("add HTTP outcome classification", MODULE, append="\nfn http_status() {}\n"),
    Mutation("add automatic retry", MODULE, append="\nfn automatic_retry() {}\n"),
    Mutation("construct ProvenNoMatch", MODULE, append="\nstruct ProvenNoMatch;\n"),
    Mutation("invoke broker reconciliation", MODULE, append="\nfn reconcile_order() {}\n"),
    Mutation("attach Redis live consumption", MODULE, append="\nuse redis::Commands;\n"),
    Mutation("attach broker dispatch or runtime-live", MODULE, append="\nfn runtime_live() {}\n"),
    mutation("issue real strategy orders", "real_strategy_order"),
    Mutation("add protective/multi-leg behavior", MODULE, append="\nfn stop_sltp_bracket_multi_leg() {}\n"),
]


REQUIRED_PATHS = {
    MODULE,
    PARENT,
    checker.LIB,
    DESCRIPTOR,
    checker.AUTHORITY,
    checker.MATRIX,
    checker.INVENTORY,
    checker.ENTRY,
}


def apply_mutation(root: Path, item: Mutation) -> None:
    path = root / item.path
    source = path.read_text()
    if item.append is not None:
        marker = "\n#[cfg(test)]\nmod tests"
        if item.path == MODULE and marker in source:
            path.write_text(source.replace(marker, item.append + marker, 1))
        else:
            path.write_text(source + item.append)
        return
    if item.old is None or item.new is None or item.old not in source:
        raise RuntimeError(f"mutation anchor missing: {item.label}")
    path.write_text(source.replace(item.old, item.new, 1))


def main() -> None:
    if len(MUTATIONS) != 37:
        raise SystemExit(f"stage8a2-r1-negative: FAIL expected 37 cases, got {len(MUTATIONS)}")
    passed = 0
    for index, item in enumerate(MUTATIONS, start=1):
        with tempfile.TemporaryDirectory(prefix="stage8a2-negative-") as raw:
            root = Path(raw)
            for relative in REQUIRED_PATHS:
                destination = root / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / relative, destination)
            apply_mutation(root, item)
            try:
                checker.check(root, git_scope=False, pin_hashes=False, exact_parent_delta=True)
            except (checker.CheckFailure, KeyError, ValueError, json.JSONDecodeError):
                passed += 1
                print(f"PASS {index:02d} {item.label}")
            else:
                raise SystemExit(f"stage8a2-r1-negative: FAIL mutation survived: {item.label}")
    print(f"stage8a2-r1-negative: PASS cases={passed}/37")


if __name__ == "__main__":
    main()
