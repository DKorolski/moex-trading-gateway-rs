#!/usr/bin/env python3
"""Replay all declared Stage 8B TLS authority mutations."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage8b_tls_qualification_check.py"
A = "docs/stage-8/stage8b-tls-qualification-authority.json"
M = "crates/finam-gateway/src/stage8b_no_send/stage8b_adapter.rs"
H = "crates/finam-gateway/src/stage8b_no_send/stage8b_tls_qualification.rs"
P = "crates/finam-gateway/src/stage8b_no_send.rs"
WM = "Cargo.toml"
FM = "crates/finam-gateway/Cargo.toml"


def replace(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    value = path.read_text()
    if old not in value:
        raise RuntimeError(f"mutation anchor missing: {relative}: {old}")
    path.write_text(value.replace(old, new, 1))


MUTATIONS = [
    ("predecessor", A, "14e01a9f838080e196ece5945a7796f2bd2600bc", "04e01a9f838080e196ece5945a7796f2bd2600bc"),
    ("accepted-hash", A, "087856c8e170ddf318a124453987f7e5d85052acde3a260dd53eaed479e4cf87", "187856c8e170ddf318a124453987f7e5d85052acde3a260dd53eaed479e4cf87"),
    ("successor-hash", A, '"tls_successor_adapter_sha256": "', '"tls_successor_adapter_sha256": "0'),
    ("harness-hash", A, '"tls_harness_sha256": "', '"tls_harness_sha256": "0'),
    ("lock-hash", A, '"cargo_lock_sha256": "', '"cargo_lock_sha256": "0'),
    ("production-graph", A, '"production_graph_sha256": "', '"production_graph_sha256": "x'),
    ("qualification-graph", A, '"qualification_graph_sha256": "', '"qualification_graph_sha256": "x'),
    ("open-p", A, '"stage8b_p": true', '"stage8b_p": false'),
    ("open-xe", A, '"stage8b_xe": true', '"stage8b_xe": false'),
    ("open-finam", A, '"real_finam_post_delete": true', '"real_finam_post_delete": false'),
    ("open-effect", A, '"broker_effect": true', '"broker_effect": false'),
    ("open-redis", A, '"redis_execution_consumer": true', '"redis_execution_consumer": false'),
    ("retry", M, ".retry(reqwest::retry::never())", ".retry(reqwest::retry::Builder::new().build())"),
    ("redirect", M, ".redirect(Policy::none())", ".user_agent(\"redirect-open\")"),
    ("proxy", M, ".no_proxy()", ".user_agent(\"proxy-open\")"),
    ("connect-timeout", M, "Duration::from_secs(2)", "Duration::from_secs(20)"),
    ("request-timeout", M, "Duration::from_secs(3)", "Duration::from_secs(30)"),
    ("pool", M, ".pool_max_idle_per_host(0)", ".pool_max_idle_per_host(1)"),
    ("builtin-roots", M, ".tls_built_in_root_certs(false)", ".tls_built_in_root_certs(true)"),
    ("local-root", M, ".add_root_certificate(authority.root_certificate)", ".user_agent(\"root-removed\")"),
    ("resolver", M, ".resolve(TLS_QUALIFICATION_HOST, authority.resolve)", ".user_agent(\"resolver-removed\")"),
    ("finam-host", M, '"stage8b-it.invalid"', '"api.finam.ru"'),
    ("non-reserved-host", M, '"stage8b-it.invalid"', '"stage8b-it.example.com"'),
    ("non-loopback", M, "if !resolve.ip().is_loopback()", "if false"),
    ("danger-certs", M, ".tls_built_in_root_certs(false)", ".danger_accept_invalid_certs(true).tls_built_in_root_certs(false)"),
    ("danger-hostnames", M, ".tls_built_in_root_certs(false)", ".danger_accept_invalid_hostnames(true).tls_built_in_root_certs(false)"),
    ("native-tls", FM, 'rcgen = "=0.14.7"', 'native-tls = "0.2"\nrcgen = "=0.14.7"'),
    ("reqwest-defaults", WM, "default-features = false", "default-features = true"),
    ("provider", FM, 'features = ["ring", "std", "tls12"]', 'features = ["aws_lc_rs", "std", "tls12"]'),
    ("public-listener", H, 'TcpListener::bind("127.0.0.1:0")', 'TcpListener::bind("0.0.0.0:0")'),
    ("no-h2", H, 'vec![b"h2".to_vec()]', 'vec![b"http/1.1".to_vec()]'),
    ("remove-place-test", P, "fn it_tls_valid_ca_hostname_and_h2_preserve_exact_place_request", "fn removed_tls_place_test"),
    ("remove-cancel-test", P, "fn it_tls_valid_ca_hostname_and_h2_preserve_exact_cancel_request", "fn removed_tls_cancel_test"),
    ("remove-wrong-ca", P, "(Stage8bTlsCertificateProfile::Valid, true)", "(Stage8bTlsCertificateProfile::Valid, false)"),
    ("remove-wrong-host", H, "Stage8bTlsCertificateProfile::WrongHostname =>", "Stage8bTlsCertificateProfile::Valid =>"),
    ("remove-expired", H, "Stage8bTlsCertificateProfile::Expired =>", "Stage8bTlsCertificateProfile::Valid =>"),
    ("remove-notyet", H, "Stage8bTlsCertificateProfile::NotYetValid =>", "Stage8bTlsCertificateProfile::Valid =>"),
    ("remove-timeout", P, "Stage8bTlsServerBehavior::Timeout,", "Stage8bTlsServerBehavior::ResponseLost,"),
    ("remove-loss", P, "Stage8bTlsServerBehavior::ResponseLost,", "Stage8bTlsServerBehavior::Timeout,"),
    ("production-constructor", M, "pub(super) struct Stage8bItAdapter {", "fn production_tls_endpoint() { let _ = \"https://api.finam.ru/v1\"; }\npub(super) struct Stage8bItAdapter {"),
]


def main() -> None:
    if len(MUTATIONS) != 40:
        raise SystemExit("stage8b-tls-negative: FAIL inventory count")
    with tempfile.TemporaryDirectory(prefix="stage8b-tls-negative-") as tmp:
        base = Path(tmp) / "base"
        shutil.copytree(ROOT, base, ignore=shutil.ignore_patterns("target", ".git", "reports", "tmp"))
        for index, (name, relative, old, new) in enumerate(MUTATIONS, 1):
            case = Path(tmp) / f"case-{index:02d}"
            shutil.copytree(base, case)
            replace(case, relative, old, new)
            result = subprocess.run(
                ["python3", CHECKER, "--no-git"], cwd=case, text=True, capture_output=True
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8b-tls-negative: FAIL mutation passed: {name}")
            print(f"PASS {index:02d}/40 {name}")
    print("stage8b-tls-negative: PASS 40/40")


if __name__ == "__main__":
    main()
