#!/usr/bin/env python3
"""Exact negative mutation harness for Stage 8B-IT."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage8b_it_check.py"

FILES = (
    "docs/stage-8/stage8b-it-authority.json",
    "docs/stage-8/STAGE8B_IT_IMPLEMENTATION_2026-08-23.md",
    "docs/stage-8/STAGE8B_IT_ACCEPTANCE_MATRIX_2026-08-23.csv",
    "docs/stage-8/STAGE8B_IT_NEGATIVE_INVENTORY_2026-08-23.md",
    "crates/finam-gateway/src/stage8b_adapter.rs",
    "crates/finam-gateway/src/stage8b_no_send.rs",
    "crates/finam-gateway/src/stage8a1_execution_capability.rs",
    "crates/finam-gateway/src/stage8a1_execution_capability/stage8a2_builder_composition.rs",
    "crates/finam-gateway/src/stage8a3_endpoint_classifier.rs",
    "crates/finam-gateway/src/lib.rs",
    "crates/broker-finam/src/order_request.rs",
    "scripts/stage8b_it_external_compile_fail.sh",
    "scripts/stage8b_it_gate.sh",
    "scripts/make_stage8b_it_handoff.py",
    "scripts/stage8b_it_handoff_safety_check.py",
)


def mutation(path: str, old: str, new: str) -> tuple[str, str, str]:
    return path, old, new


A = "docs/stage-8/stage8b-it-authority.json"
D = "docs/stage-8/STAGE8B_IT_IMPLEMENTATION_2026-08-23.md"
M = "crates/finam-gateway/src/stage8b_adapter.rs"
P = "crates/finam-gateway/src/stage8b_no_send.rs"
L = "crates/finam-gateway/src/lib.rs"

CASES = [
    ("predecessor", mutation(A, "0af222f252cdc2b4c763c9e04935a5cb5f0c6d65", "1" * 40)),
    ("a2-ref", mutation(A, "16180ac4f8eab761b3b055c1f5515f62cd94bfb9", "2" * 40)),
    ("a2-digest", mutation(A, "1026a24962bf45de8653c80ba095f892af35523da58f4fa4fccad706fb023653", "3" * 64)),
    ("a3-ref", mutation(A, "012c9bfa51c1d6206fbd9a7e1f06f1fc90fdf30d", "4" * 40)),
    ("a3-digest", mutation(A, "f34c9fef5e219dad15b0a00ce1eaf63311ec9f77d1997e422b977e5c8ffe47b3", "5" * 64)),
    ("adapter-count", mutation(A, '"adapter_count": 1', '"adapter_count": 2')),
    ("attempt-count", mutation(A, '"transport_attempt_count": 1', '"transport_attempt_count": 2')),
    ("place-method", mutation(A, '"place_method": "POST"', '"place_method": "PUT"')),
    ("place-route", mutation(A, '"place_route_template": "/v1/accounts/{account_id}/orders"', '"place_route_template": "/v2/orders"')),
    ("cancel-method", mutation(A, '"cancel_method": "DELETE"', '"cancel_method": "POST"')),
    ("cancel-route", mutation(A, '"cancel_route_template": "/v1/accounts/{account_id}/orders/{order_id}"', '"cancel_route_template": "/v2/orders/{order_id}"')),
    ("production-scheme", mutation(A, '"production_scheme": "https"', '"production_scheme": "http"')),
    ("production-host", mutation(A, '"production_host": "api.finam.ru"', '"production_host": "mirror.invalid"')),
    ("redirect-enabled", mutation(A, '"redirects_disabled": true', '"redirects_disabled": false')),
    ("proxy-enabled", mutation(A, '"proxy_disabled": true', '"proxy_disabled": false')),
    ("retry-enabled", mutation(A, '"automatic_transport_retry_disabled": true', '"automatic_transport_retry_disabled": false')),
    ("generic-builder", mutation(A, '"generic_request_builder_forbidden": true', '"generic_request_builder_forbidden": false')),
    ("alternate-host", mutation(A, '"alternate_host_forbidden": true', '"alternate_host_forbidden": false')),
    ("response-bound", mutation(A, '"bounded_response_bytes": 65536', '"bounded_response_bytes": 1048576')),
    ("connect-timeout", mutation(A, '"connect_timeout_seconds": 2', '"connect_timeout_seconds": 20')),
    ("request-timeout", mutation(A, '"request_timeout_seconds": 3', '"request_timeout_seconds": 30')),
    ("place-builder-inventory", mutation(A, '"build_place_order_request"', '"build_other_place_request"')),
    ("cancel-builder-inventory", mutation(A, '"build_cancel_order_request"', '"build_other_cancel_request"')),
    ("classifier-inventory", mutation(A, '"classify_stage8b_transport_observation_with_stage8a3"', '"classify_second_model"')),
    ("public-module", mutation(L, "mod stage8b_adapter;", "pub mod stage8b_adapter;")),
    ("public-adapter", mutation(M, "pub(crate) struct Stage8bItAdapter {", "pub struct Stage8bItAdapter {")),
    ("public-endpoint", mutation(M, "pub(crate) struct Stage8bItQualificationEndpoint {", "pub struct Stage8bItQualificationEndpoint {")),
    ("public-token", mutation(M, "pub(crate) struct Stage8bItQualificationToken", "pub struct Stage8bItQualificationToken")),
    ("public-parts", mutation(P, "pub(crate) struct Stage8bApprovedRequestParts {", "pub struct Stage8bApprovedRequestParts {")),
    ("permit-bypass", mutation(P, "permit.continuation.compose_stage8a2_no_send(&mut sink)", "Stage8a2InMemoryNoSendSink::new().consumed")),
    ("generic-request", mutation(M, ".post(url)", ".request(reqwest::Method::POST, url)")),
    ("second-post", mutation(M, ".post(url)", ".post(url).post(url)")),
    ("second-delete", mutation(M, ".delete(url)", ".delete(url).delete(url)")),
    ("second-send", mutation(M, "let result = request.send().await;", "let _extra = request.send();\n        let result = request.send().await;")),
    ("redirect-limited", mutation(M, "redirect(Policy::none())", "redirect(Policy::limited(10))")),
    ("proxy-removed", mutation(M, ".no_proxy()", ".user_agent(\"proxy-allowed\")")),
    ("alternate-host-literal", mutation(M, 'const FINAM_PRODUCTION_HOST: &str = "api.finam.ru";', 'const FINAM_PRODUCTION_HOST: &str = "alternate.invalid";')),
    ("production-constructor", mutation(M, "pub(crate) fn production_policy_accepts", "pub(crate) fn production_endpoint")),
    ("dns-loopback", mutation(M, ".parse::<std::net::IpAddr>()", ".parse::<String>()")),
    ("port-optional", mutation(M, "|| url.port().is_none()", "|| false")),
    ("route-segment", mutation(M, 'segment == "." || segment == ".." || segment.contains(\'/\')', "segment.is_empty()")),
    ("response-unbounded", mutation(M, "<= MAX_RESPONSE_BYTES", "<= usize::MAX")),
    ("classifier-is-truth", mutation(D, "It is not broker truth", "It is broker truth")),
    ("redis-import", mutation(M, "use reqwest::{redirect::Policy, Url};", "use reqwest::{redirect::Policy, Url};\nuse redis::Commands;")),
    ("retry-authority", mutation(M, "pub(crate) struct Stage8bItQualifiedObservation", "pub(crate) struct retry_authority;\npub(crate) struct Stage8bItQualifiedObservation")),
    ("open-p", mutation(A, '"stage8b_p": true', '"stage8b_p": false')),
    ("open-xe", mutation(A, '"stage8b_xe": true', '"stage8b_xe": false')),
    ("open-production", mutation(A, '"production_endpoint_authority": true', '"production_endpoint_authority": false')),
]


def main() -> None:
    if len(CASES) != 48:
        raise SystemExit(f"negative case inventory drift: {len(CASES)}")
    with tempfile.TemporaryDirectory(prefix="stage8b-it-negative-") as raw:
        base = Path(raw) / "base"
        for relative in FILES:
            destination = base / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        passed = 0
        for index, (name, (relative, old, new)) in enumerate(CASES, start=1):
            candidate = Path(raw) / f"case-{index:02d}"
            shutil.copytree(base, candidate)
            path = candidate / relative
            source = path.read_text(encoding="utf-8")
            if source.count(old) != 1:
                raise SystemExit(f"mutation source count drift for {name}: {source.count(old)}")
            path.write_text(source.replace(old, new, 1), encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(CHECKER), "--root", str(candidate), "--no-git"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if result.returncode == 0:
                raise SystemExit(f"negative mutation unexpectedly passed: {index} {name}")
            print(f"PASS {index:02d} {name}")
            passed += 1
    print(f"stage8b-it-negative-harness: PASS {passed}/48")


if __name__ == "__main__":
    main()
