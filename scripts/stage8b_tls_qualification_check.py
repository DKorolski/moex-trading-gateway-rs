#!/usr/bin/env python3
"""Static authority gate for Stage 8B-IT-TLS R1."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
A = ROOT / "docs/stage-8/stage8b-tls-qualification-authority.json"
M = ROOT / "crates/finam-gateway/src/stage8b_no_send/stage8b_adapter.rs"
H = ROOT / "crates/finam-gateway/src/stage8b_no_send/stage8b_tls_qualification.rs"
P = ROOT / "crates/finam-gateway/src/stage8b_no_send.rs"
WM = ROOT / "Cargo.toml"
FM = ROOT / "crates/finam-gateway/Cargo.toml"
LOCK = ROOT / "Cargo.lock"
MATRIX = ROOT / "docs/stage-8/STAGE8B_TLS_ACCEPTANCE_MATRIX_2026-08-23.csv"
NEGATIVE = ROOT / "docs/stage-8/STAGE8B_TLS_NEGATIVE_INVENTORY_2026-08-23.md"
DESIGN = ROOT / "docs/stage-8/STAGE8B_TLS_QUALIFICATION_2026-08-23.md"
GATE = ROOT / "scripts/stage8b_tls_gate.sh"
GRAPH = ROOT / "scripts/stage8b_tls_graph_evidence.py"
REPLAY = ROOT / "scripts/stage8b_tls_predecessor_replay.sh"
MAKER = ROOT / "scripts/make_stage8b_tls_handoff.py"
SAFETY = ROOT / "scripts/stage8b_tls_handoff_safety_check.py"


def require(value: bool, message: str) -> None:
    if not value:
        raise SystemExit(f"stage8b-tls-check: FAIL {message}")


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def lower_sha(value: object) -> bool:
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def exact_package_version(lock: str, package: str, version: str) -> bool:
    pattern = rf'\[\[package\]\]\nname = "{re.escape(package)}"\nversion = "{re.escape(version)}"\n'
    return re.search(pattern, lock) is not None


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--no-git", action="store_true")
    args = parser.parse_args()

    for path in (A, M, H, P, WM, FM, LOCK, MATRIX, NEGATIVE, DESIGN, GATE, GRAPH, REPLAY, MAKER, SAFETY):
        require(path.is_file(), f"missing {path.relative_to(ROOT)}")

    authority = json.loads(A.read_text())
    adapter = M.read_text()
    harness = H.read_text()
    parent = P.read_text()
    workspace = WM.read_text()
    manifest = FM.read_text()
    lock = LOCK.read_text()
    negative = NEGATIVE.read_text()
    design = DESIGN.read_text()
    gate = GATE.read_text()

    require(authority.get("stage") == "8B-IT-TLS", "stage drift")
    require(authority.get("revision") == "R1", "revision drift")
    require(authority.get("status") == "controlled_qualification_candidate", "status drift")
    require(authority.get("branch") == "stage8b-tls-qualification", "branch drift")
    require(
        authority.get("accepted_predecessor_ref")
        == "14e01a9f838080e196ece5945a7796f2bd2600bc",
        "accepted predecessor drift",
    )
    require(
        authority.get("accepted_it_r3_adapter_sha256")
        == "087856c8e170ddf318a124453987f7e5d85052acde3a260dd53eaed479e4cf87",
        "accepted adapter hash drift",
    )
    expected_hashes = {
        "tls_successor_adapter_sha256": sha(M),
        "tls_harness_sha256": sha(H),
        "stage8b_parent_sha256": sha(P),
        "workspace_manifest_sha256": sha(WM),
        "finam_gateway_manifest_sha256": sha(FM),
        "cargo_lock_sha256": sha(LOCK),
    }
    for field, expected in expected_hashes.items():
        require(authority.get(field) == expected, f"{field} drift")
    require(lower_sha(authority.get("production_graph_sha256")), "production graph hash invalid")
    require(lower_sha(authority.get("qualification_graph_sha256")), "qualification graph hash invalid")

    client = authority.get("client", {})
    require(client.get("reqwest") == "0.12.24", "reqwest identity drift")
    require(client.get("hyper_rustls") == "0.27.9", "hyper-rustls identity drift")
    require(client.get("rustls") == "0.23.41", "rustls identity drift")
    require(client.get("tokio_rustls") == "0.26.4", "tokio-rustls identity drift")
    require(client.get("crypto_provider") == "ring", "crypto provider drift")
    require(client.get("native_tls_present") is False, "native-tls opened")
    require(client.get("automatic_retry") == "never", "retry authority drift")
    require(client.get("redirects") == "disabled", "redirect authority drift")
    require(client.get("proxy") == "disabled", "proxy authority drift")
    require(client.get("connect_timeout_seconds") == 2, "connect timeout drift")
    require(client.get("request_timeout_seconds") == 3, "request timeout drift")
    require(client.get("built_in_roots_for_controlled_qualification") is False, "root authority drift")

    require(adapter.count("reqwest::Client::builder()") == 1, "client builder count drift")
    require(adapter.count("reqwest::retry::never()") == 1, "retry policy count drift")
    require(adapter.count("redirect(Policy::none())") == 1, "redirect policy count drift")
    require(adapter.count(".no_proxy()") == 1, "proxy policy count drift")
    require(adapter.count(".connect_timeout(CONNECT_TIMEOUT)") == 1, "connect timeout policy drift")
    require(adapter.count(".timeout(REQUEST_TIMEOUT)") == 1, "request timeout policy drift")
    require(adapter.count(".pool_max_idle_per_host(0)") == 1, "pool policy drift")
    require(adapter.count(".send()") == 1, "transport send count drift")
    require(adapter.count(".post(") == 1 and adapter.count(".delete(") == 1, "method count drift")
    require(adapter.count(".tls_built_in_root_certs(false)") == 1, "controlled root policy drift")
    require(adapter.count(".add_root_certificate(") == 1, "local root count drift")
    require(adapter.count(".resolve(TLS_QUALIFICATION_HOST, authority.resolve)") == 1, "loopback resolver drift")
    require('TLS_QUALIFICATION_HOST: &str = "stage8b-it.invalid"' in adapter, "controlled host drift")
    require("pub(super) struct Stage8bItTlsQualificationAuthority" in adapter, "TLS authority missing")
    require("if !resolve.ip().is_loopback()" in adapter, "loopback validation missing")
    require("root_certificate_der.is_empty()" in adapter, "root validation missing")
    require("#[cfg(test)]\npub(super) struct Stage8bItTlsQualificationAuthority" in adapter, "TLS authority escaped test cfg")
    require("#[cfg(test)]\n    pub(super) fn qualified_tls" in adapter, "TLS constructor escaped test cfg")

    forbidden = (
        "danger_accept_invalid_certs",
        "danger_accept_invalid_hostnames",
        "native_tls",
        "native-tls",
    )
    source_surface = adapter + harness + parent
    for token in forbidden:
        require(token not in source_surface, f"forbidden source token {token}")
    require("fn production_tls_endpoint" not in source_surface, "production TLS constructor opened")

    require('TcpListener::bind("127.0.0.1:0")' in harness, "loopback listener drift")
    require('server_config.alpn_protocols = vec![b"h2".to_vec()]' in harness, "ALPN h2 drift")
    require("Stage8bTlsCertificateProfile::WrongHostname" in harness, "wrong hostname profile missing")
    require("Stage8bTlsCertificateProfile::Expired" in harness, "expired profile missing")
    require("Stage8bTlsCertificateProfile::NotYetValid" in harness, "not-yet-valid profile missing")
    require("http2::Builder::new" in harness, "HTTP2 server missing")
    require("authorization_present" in harness, "redacted authorization observation missing")
    require("LOCAL_TLS_QUALIFICATION_TOKEN" in parent, "synthetic qualification credential missing")
    for test_name in (
        "it_tls_valid_ca_hostname_and_h2_preserve_exact_place_request",
        "it_tls_valid_ca_hostname_and_h2_preserve_exact_cancel_request",
        "it_tls_rejects_wrong_ca_hostname_and_validity_without_http_request",
        "it_tls_timeout_and_response_loss_remain_classified_without_retry",
        "it_tls_authority_rejects_non_loopback_resolution_and_invalid_root",
    ):
        require(f"fn {test_name}" in parent, f"focused test missing: {test_name}")

    require(
        'reqwest = { version = "=0.12.24", default-features = false, features = ["http2", "json", "rustls-tls"] }'
        in workspace,
        "workspace reqwest feature drift",
    )
    for expected in (
        'rcgen = "=0.14.7"',
        'rustls = { version = "=0.23.41", default-features = false, features = ["ring", "std", "tls12"] }',
        'tokio-rustls = { version = "=0.26.4", default-features = false, features = ["ring", "tls12"] }',
    ):
        require(expected in manifest, f"qualification dependency drift: {expected}")
    require("native-tls" not in workspace + manifest, "native-tls manifest dependency opened")
    for package, version in (
        ("reqwest", "0.12.24"),
        ("hyper-rustls", "0.27.9"),
        ("rustls", "0.23.41"),
        ("tokio-rustls", "0.26.4"),
        ("rcgen", "0.14.7"),
    ):
        require(exact_package_version(lock, package, version), f"locked {package} version drift")
    require('\nname = "native-tls"\n' not in lock, "native-tls locked graph opened")

    with MATRIX.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 50, "acceptance row count drift")
    require([row["id"] for row in rows] == [f"TLS-{i:03d}" for i in range(1, 51)], "acceptance IDs drift")
    require(all(row["status"] == "PASS" for row in rows), "acceptance matrix not all PASS")
    require(len(re.findall(r"^\d+\. ", negative, flags=re.MULTILINE)) == 40, "negative inventory drift")
    require("Stage 8B-P" in design and "remain closed" in design, "phase boundary documentation drift")
    for command in (
        "python3 scripts/current_tree_authority_check.py",
        "python3 scripts/current_tree_authority_negative_harness.py",
        "bash scripts/stage8b_tls_predecessor_replay.sh",
        "python3 scripts/stage8b_tls_qualification_check.py",
        "python3 scripts/stage8b_tls_negative_harness.py",
        "python3 scripts/stage8b_tls_graph_evidence.py",
        "cargo test -p finam-gateway stage8b_no_send::tests::it_tls",
        "cargo clippy -p finam-gateway --all-targets --all-features -- -D warnings",
        "bash scripts/stage8b_i_full_regression.sh",
    ):
        require(gate.count(command) == 1, f"TLS gate command drift: {command}")

    closed = authority.get("closed_surfaces", {})
    require(closed and all(value is True for value in closed.values()), "closed surface opened")
    require(authority.get("acceptance_rows") == 50, "authority acceptance count drift")
    require(authority.get("negative_mutations") == 40, "authority negative count drift")
    require(authority.get("focused_tls_tests") == 5, "focused test count drift")
    require(authority.get("canonical_full_regression_required") is True, "full regression weakened")
    require(
        authority.get("next_if_accepted") == "8B-P_preconditions_refresh_and_design_only_package",
        "next stage drift",
    )

    if not args.no_git:
        branch = subprocess.run(
            ["git", "branch", "--show-current"], cwd=ROOT, check=True, text=True, capture_output=True
        ).stdout.strip()
        require(branch == authority["branch"], "current branch drift")

    print("stage8b-tls-check: PASS revision=R1 rows=50 negatives=40 focused_tls_tests=5 h2=true native_tls=false stage8b_p=false")


if __name__ == "__main__":
    main()
