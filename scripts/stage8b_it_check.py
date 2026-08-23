#!/usr/bin/env python3
"""Static authority checker for Stage 8B-IT no-effect adapter qualification."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

ACCEPTED_PREDECESSOR = "0af222f252cdc2b4c763c9e04935a5cb5f0c6d65"
A2_SHA256 = "1026a24962bf45de8653c80ba095f892af35523da58f4fa4fccad706fb023653"
A3_SHA256 = "f34c9fef5e219dad15b0a00ce1eaf63311ec9f77d1997e422b977e5c8ffe47b3"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_paths(root: Path) -> dict[str, Path]:
    return {
        "authority": root / "docs/stage-8/stage8b-it-authority.json",
        "doc": root / "docs/stage-8/STAGE8B_IT_IMPLEMENTATION_2026-08-23.md",
        "matrix": root / "docs/stage-8/STAGE8B_IT_ACCEPTANCE_MATRIX_2026-08-23.csv",
        "negative": root / "docs/stage-8/STAGE8B_IT_NEGATIVE_INVENTORY_2026-08-23.md",
        "adapter": root / "crates/finam-gateway/src/stage8b_adapter.rs",
        "permit": root / "crates/finam-gateway/src/stage8b_no_send.rs",
        "a1": root / "crates/finam-gateway/src/stage8a1_execution_capability.rs",
        "a2": root / "crates/finam-gateway/src/stage8a1_execution_capability/stage8a2_builder_composition.rs",
        "a3": root / "crates/finam-gateway/src/stage8a3_endpoint_classifier.rs",
        "lib": root / "crates/finam-gateway/src/lib.rs",
        "surface": root / "crates/broker-finam/src/order_request.rs",
        "compile": root / "scripts/stage8b_it_external_compile_fail.sh",
        "gate": root / "scripts/stage8b_it_gate.sh",
        "maker": root / "scripts/make_stage8b_it_handoff.py",
        "safety": root / "scripts/stage8b_it_handoff_safety_check.py",
    }


def check(root: Path, git_scope: bool) -> None:
    paths = load_paths(root)
    for label, path in paths.items():
        require(path.is_file(), f"missing {label}: {path}")

    authority = json.loads(paths["authority"].read_text(encoding="utf-8"))
    doc = paths["doc"].read_text(encoding="utf-8")
    negative = paths["negative"].read_text(encoding="utf-8")
    adapter = paths["adapter"].read_text(encoding="utf-8")
    permit = paths["permit"].read_text(encoding="utf-8")
    a1 = paths["a1"].read_text(encoding="utf-8")
    lib = paths["lib"].read_text(encoding="utf-8")
    surface = paths["surface"].read_text(encoding="utf-8")
    compile_script = paths["compile"].read_text(encoding="utf-8")
    gate_script = paths["gate"].read_text(encoding="utf-8")
    maker = paths["maker"].read_text(encoding="utf-8")
    safety = paths["safety"].read_text(encoding="utf-8")

    require(authority.get("schema_version") == 1, "authority schema drift")
    require(authority.get("stage") == "8B-IT", "stage drift")
    require(authority.get("status") == "implementation_candidate", "status drift")
    require(authority.get("branch") == "stage8b-it", "branch drift")
    require(authority.get("accepted_predecessor_ref") == ACCEPTED_PREDECESSOR, "predecessor drift")
    require(authority.get("accepted_stage8a2_ref") == "16180ac4f8eab761b3b055c1f5515f62cd94bfb9", "A2 ref drift")
    require(authority.get("accepted_stage8a2_source_sha256") == A2_SHA256, "A2 authority digest drift")
    require(authority.get("accepted_stage8a3_ref") == "012c9bfa51c1d6206fbd9a7e1f06f1fc90fdf30d", "A3 ref drift")
    require(authority.get("accepted_stage8a3_source_sha256") == A3_SHA256, "A3 authority digest drift")
    require(sha256(paths["a2"]) == A2_SHA256, "accepted A2 source changed")
    require(sha256(paths["a3"]) == A3_SHA256, "accepted A3 source changed")

    require(authority.get("adapter_module") == "crates/finam-gateway/src/stage8b_adapter.rs", "adapter path drift")
    require(authority.get("permit_module") == "crates/finam-gateway/src/stage8b_no_send.rs", "permit path drift")
    require(authority.get("adapter_count") == 1, "adapter count drift")
    require(authority.get("transport_attempt_count") == 1, "attempt count drift")
    require(authority.get("place_method") == "POST", "PLACE method drift")
    require(authority.get("place_route_template") == "/v1/accounts/{account_id}/orders", "PLACE route drift")
    require(authority.get("cancel_method") == "DELETE", "CANCEL method drift")
    require(authority.get("cancel_route_template") == "/v1/accounts/{account_id}/orders/{order_id}", "CANCEL route drift")
    require(authority.get("production_scheme") == "https", "production scheme drift")
    require(authority.get("production_host") == "api.finam.ru", "production host drift")
    require(authority.get("qualification_endpoint_kind") == "explicit_ip_loopback_with_port", "qualification endpoint drift")
    for key in (
        "redirects_disabled", "proxy_disabled", "automatic_transport_retry_disabled",
        "generic_request_builder_forbidden", "alternate_host_forbidden",
    ):
        require(authority.get(key) is True, f"authority weakened: {key}")
    require(authority.get("bounded_response_bytes") == 65536, "response bound drift")
    require(authority.get("connect_timeout_seconds") == 2, "connect timeout drift")
    require(authority.get("request_timeout_seconds") == 3, "request timeout drift")
    require(authority.get("accepted_builder_bridge") == "compose_stage8b_private_request_parts_from_stage8a2", "builder bridge drift")
    require(authority.get("accepted_builders_only") == ["build_place_order_request", "build_cancel_order_request"], "builder inventory drift")
    require(authority.get("accepted_classifier_bridge") == "classify_stage8b_transport_observation_with_stage8a3", "classifier bridge drift")
    require(authority.get("controlled_scenarios") == [
        "place_response", "cancel_response", "redirect_not_followed",
        "response_lost", "timeout", "connection_failure",
    ], "controlled scenario drift")
    require(len(authority.get("adapter_identity_fields", [])) == 13, "adapter identity inventory drift")
    closed = authority.get("closed_surfaces", {})
    require(len(closed) == 14 and all(value is True for value in closed.values()), "closed surface opened")
    require(authority.get("acceptance_rows") == 60, "acceptance count drift")
    require(authority.get("negative_cases") == 48, "negative count drift")
    require(authority.get("external_compile_fail_cases") == 12, "compile-fail count drift")
    require(authority.get("next_if_accepted") == "8B-P_exact_adapter_qualified_build_authorization", "next stage drift")

    with paths["matrix"].open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream))
    require([row.get("id") for row in rows] == [f"IT-{number:03d}" for number in range(1, 61)], "matrix ID/count drift")
    require(all(row.get("area") and row.get("requirement") and row.get("evidence") and row.get("status") == "pending" for row in rows), "matrix row incomplete")
    numbers = [int(value) for value in re.findall(r"^(\d+)\.", negative, flags=re.MULTILINE)]
    require(numbers == list(range(1, 49)), "negative inventory must be exact 1..48")

    require("mod stage8b_adapter;" in lib, "private adapter module missing")
    require("pub mod stage8b_adapter" not in lib and "pub use stage8b_adapter" not in lib, "adapter module escaped publicly")
    require(adapter.count("pub(crate) struct Stage8bItAdapter {") == 1, "adapter type drift")
    require("pub struct Stage8bItAdapter" not in adapter, "adapter became public")
    require(adapter.count("pub(crate) struct Stage8bItQualificationEndpoint {") == 1, "qualification endpoint type drift")
    require("pub struct Stage8bItQualificationEndpoint" not in adapter, "qualification endpoint became public")
    require(adapter.count("pub(crate) struct Stage8bItQualificationToken(") == 1, "qualification token type drift")
    require("pub struct Stage8bItQualificationToken" not in adapter, "qualification token became public")
    require(adapter.count(".post(") == 1, "POST surface count drift")
    require(adapter.count(".delete(") == 1, "DELETE surface count drift")
    require(adapter.count(".send()") == 1, "send surface count drift")
    for forbidden in (
        ".request(", "reqwest::Method::", ".header(", "Proxy::", "redis::", ".xadd(",
        ".xack(", "resend_authority", "retry_authority", "production_endpoint(",
    ):
        require(forbidden not in adapter, f"forbidden adapter surface: {forbidden}")
    for marker in (
        'const FINAM_PRODUCTION_SCHEME: &str = "https";',
        'const FINAM_PRODUCTION_HOST: &str = "api.finam.ru";',
        'const PLACE_ROUTE_TEMPLATE: &str = "/v1/accounts/{account_id}/orders";',
        'const CANCEL_ROUTE_TEMPLATE: &str = "/v1/accounts/{account_id}/orders/{order_id}";',
        "redirect(Policy::none())", ".no_proxy()", ".connect_timeout(CONNECT_TIMEOUT)",
        ".timeout(REQUEST_TIMEOUT)", ".pool_max_idle_per_host(0)",
        "parse::<std::net::IpAddr>()", "!ip.is_loopback()", "url.port().is_none()",
        "segment == \".\"", "segment == \"..\"", "segment.contains('/')",
        "MAX_RESPONSE_BYTES",
        "body.len().saturating_add(chunk.len()) <= MAX_RESPONSE_BYTES",
        "transport_attempts = 1", "possible_write = true",
        "Stage8a3LocalHttpObservation::timeout()", "Stage8a3LocalHttpObservation::disconnected()",
    ):
        require(marker in adapter, f"adapter marker missing: {marker}")
    require("#[cfg(test)]\n    pub(crate) fn controlled_loopback" in adapter, "loopback authority escaped tests")
    require("#[cfg(test)]\n    pub(crate) fn controlled" in adapter, "qualification token escaped tests")

    require("pub(crate) struct Stage8bApprovedRequestParts" in permit, "private request parts missing")
    require("pub struct Stage8bApprovedRequestParts" not in permit, "request parts became public")
    require(permit.count("build_place_order_request(&approved, None)") == 1, "place builder bridge drift")
    require(permit.count("build_cancel_order_request(&approved)") == 1, "cancel builder bridge drift")
    require("permit.continuation.compose_stage8a2_no_send(&mut sink)" in permit, "accepted A2 diagnostic seam removed")
    require("clone_approved_command_for_stage8b()?" in permit, "exact approved command bridge removed")
    require("context.classify(observation)" in permit, "accepted classifier seam removed")
    for test_name in (
        "it_place_uses_permit_only_adapter_and_accepted_classifier",
        "it_cancel_uses_exact_delete_route_without_body_or_retry",
        "it_redirect_is_observed_locally_and_never_followed",
        "it_response_loss_is_reconciliation_only_without_retry",
        "it_timeout_is_reconciliation_only_without_retry",
        "it_connection_failure_is_single_attempt_and_reconciliation_only",
    ):
        require(test_name in permit, f"controlled test missing: {test_name}")
    require("pub(crate) enum Stage8a1Stage8bApprovedCommand" in a1, "approved command bridge type missing")
    require("pub(crate) fn clone_approved_command_for_stage8b(" in a1, "approved command clone seam missing")
    require(
        'path.ends_with("crates/finam-gateway/src/stage8b_adapter.rs")' in surface,
        "broker-finam source-surface allowlist missing",
    )
    for marker in (
        'source.matches(&[".", "post("].concat()).count(), 1',
        'source.matches(&[".", "delete("].concat()).count(), 1',
        'source.matches(&[".", "send()"].concat()).count(), 1',
        'assert!(!source.contains("production_endpoint("))',
    ):
        require(marker in surface, f"source-surface exact allowance drift: {marker}")
    require(
        "stage8b-it-external-compile-fail: PASS positive=1 negative=12" in compile_script,
        "external compile-fail count marker drift",
    )
    require(compile_script.count("check_fail ") == 12, "external compile-fail inventory drift")
    require("python3 scripts/stage8b_it_negative_harness.py" in gate_script, "negative harness omitted from gate")
    require("bash scripts/stage8b_it_external_compile_fail.sh" in gate_script, "compile-fail omitted from gate")
    require("stage8b-it-gate: PASS rows=60 negatives=48 compile_fail=12" in gate_script, "gate marker drift")
    require('BRANCH = "stage8b-it"' in maker and f'BASE = "{ACCEPTED_PREDECESSOR}"' in maker, "handoff lineage drift")
    require('"stage": "8B-IT"' in maker, "handoff evidence stage drift")
    require('evidence.get("stage") != "8B-IT"' in safety, "handoff safety stage drift")

    for marker in (
        "adapter-qualified no-effect integration", "owns the single IT transport",
        "redirect(Policy::none())", "no automatic retry or resend loop",
        "explicit numeric loopback IP", "candidate/diagnostic evidence only",
        "It is not broker truth",
        "Stage 8B-P", "Stage 8B-XE", "Stage 12",
    ):
        require(marker in doc, f"documentation marker missing: {marker}")

    if git_scope:
        branch = subprocess.check_output(["git", "branch", "--show-current"], cwd=root, text=True).strip()
        require(branch == "stage8b-it", "git branch drift")
        subprocess.run(["git", "merge-base", "--is-ancestor", ACCEPTED_PREDECESSOR, "HEAD"], cwd=root, check=True)

    print(
        "stage8b-it-check: PASS rows=60 negatives=48 compile_fail=12 "
        "adapter=1 post=1 delete=1 send=1 controlled_only=true broker_effect=false "
        "stage8b_p=false stage8b_xe=false stage12=false"
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--no-git", action="store_true")
    args = parser.parse_args()
    try:
        check(args.root.resolve(), not args.no_git)
    except (ValueError, OSError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"stage8b-it-check: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)


if __name__ == "__main__":
    main()
