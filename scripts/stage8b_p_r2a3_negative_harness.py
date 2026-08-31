#!/usr/bin/env python3
"""Mutation harness for the R2A3 review-closure checker."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

# The first 26 cases retain the accepted R2A2 semantic categories. Remaining
# cases cover the focused R2A3 review findings.
MUTATIONS = [
    ("authority", "authorization", '"authorization_status": "NOT_ISSUED"', '"authorization_status": "ISSUED"'),
    ("authority", "source-count", '"source_count": 11', '"source_count": 10'),
    ("authority", "verifier-can-mint", '"verifier_private_key_access": false', '"verifier_private_key_access": true'),
    ("authority", "claim-inventory", '"closed_claim_inventory_per_source": true', '"closed_claim_inventory_per_source": false'),
    ("authority", "control-skew", '"control_source_max_skew_ms": 1000', '"control_source_max_skew_ms": 10000'),
    ("authority", "runtime-skew", '"runtime_source_max_skew_ms": 5000', '"runtime_source_max_skew_ms": 50000'),
    ("authority", "pacing", '"minimum_broker_get_interval_ms": 250', '"minimum_broker_get_interval_ms": 0'),
    ("authority", "place-prior-trade", '"place_prior_matching_trade": "BLOCK"', '"place_prior_matching_trade": "ALLOW"'),
    ("authority", "cancel-link", '"cancel_trade_must_match_exact_order": true', '"cancel_trade_must_match_exact_order": false'),
    ("authority", "exact-list", '"exact_and_list_full_immutable_equality": true', '"exact_and_list_full_immutable_equality": false'),
    ("authority", "helper-digest", '"accepted_helper_sha256_status": "FROZEN"', '"accepted_helper_sha256_status": "PENDING"'),
    ("r2a2", "deny-unknown", "#[serde(deny_unknown_fields)]", "#[serde(default)]"),
    ("r2a2", "constant-time", "mac.verify_slice(&asserted)", "let _ = asserted"),
    ("r2a2", "bounded-stream", "while let Some(chunk) = response.chunk().await", "while let None = response.chunk().await"),
    ("r2a2", "raw-body", "raw_body_sha256_exported: false", "raw_body_sha256_exported: true"),
    ("r2a2", "position-baseline", "if target_position != manifest.approved_pre_run_position", "if false"),
    ("r2a2", "prior-effect", "|| target_trade_count != 0", "|| false"),
    ("r2a2", "cancel-trade-order", "trade.order_id.as_deref() != manifest.broker_order_id.as_deref()", "false"),
    ("r2a2", "order-id-equality", "exact.order_id == listed.order_id", "true"),
    ("r2a2", "order-body-equality", "&& exact.order == listed.order", "&& true"),
    ("r2a2", "trigger-field", "triggered_order_id: Option<String>", "legacy_trigger: Option<String>"),
    ("r2a2", "account-field", "maintenance_margin: Option<StrictDecimal>", "legacy_margin: Option<StrictDecimal>"),
    ("r2a2", "trade-field", "accrued_interest: Option<StrictDecimal>", "legacy_interest: Option<StrictDecimal>"),
    ("main", "credential-main", "fn main()", 'const FINAM_SECRET_TOKEN: &str = "forbidden";\nfn main()'),
    ("main", "remove-controlled", 'mode == "--qualify-controlled"', 'mode == "--removed"'),
    ("main", "remove-oneshot", 'mode == "--r2b-one-shot"', 'mode == "--removed"'),
    ("r2a3", "signature-domain", "stage8b-p-r2a3-source-receipt-ed25519-v1", "stage8b-p-r2a3-source-receipt-ed25519-v2"),
    ("r2a3", "producer-generation", "source_generation: u64", "legacy_generation: u64"),
    ("r2a3", "producer-nonce", "pub source_generation: u64,\n    pub run_nonce_sha256: String", "pub source_generation: u64,\n    pub legacy_nonce: String"),
    ("r2a3", "arbitrary-claims", "actual_claims != expected_claims", "false"),
    ("r2a3", "producer-owner", "require_owned_file(&source_path, source_producer_uid(source_name)?, false)?;", "require_owned_file(&source_path, 0, false)?;"),
    ("r2a3", "issuer-owner", "require_owned_file(&path, source_issuer_uid(source)?, false)?;", "require_owned_file(&path, 0, false)?;"),
    ("r2a3", "signature-verify", "key.verify(&receipt_signing_preimage(&signed)?", "key.verify(b\"forged\""),
    ("r2a3", "nonce-registry", "claim_run_nonce_once(\n        Path::new(PRODUCTION_NONCE_REGISTRY)", "legacy_claim_once(\n        Path::new(PRODUCTION_NONCE_REGISTRY)"),
    ("r2a3", "nonce-create-new", ".write(true)\n        .create_new(true)\n        .mode(0o600)", ".write(true)\n        .create(true)\n        .mode(0o600)"),
    ("r2a3", "fd-exec", "libc::execveat(", "libc::execve("),
    ("r2a3", "fd-empty-path", "libc::AT_EMPTY_PATH", "0"),
    ("r2a3", "fd-close-on-exec", "descriptor_flags & !libc::FD_CLOEXEC", "descriptor_flags | libc::FD_CLOEXEC"),
    ("r2a3", "self-digest", "current_linux_executable_sha256()?", '"0".repeat(64)'),
    ("r2a3", "package-issued", 'package.authorization_status != "ISSUED"', 'package.authorization_status != "NOT_ISSUED"'),
    ("r2a3", "operation-cross-bind", "manifest_operation != package_operation", "false"),
    ("r2a3", "remove-pacing", "tokio::time::sleep(minimum - elapsed).await", "tokio::task::yield_now().await"),
    ("r2a3", "remove-final-freshness", "let (final_manifest, _) = revalidate(&input)?;", "let final_manifest = manifest.clone();"),
    ("r2a3", "separate-controlled-builder", "crate::hardened_client_builder(true, Duration::from_secs(2))", "reqwest::Client::builder()"),
    ("launcher", "ambient-environment", "let environment: Vec<CString> = Vec::new();", "let environment = std::env::vars_os().map(|_| CString::new(\"X=1\").unwrap()).collect();"),
    ("r2a3", "order-delete", "pub async fn run_r2b_one_shot", "fn forbidden() { let _ = client.delete(url); }\npub async fn run_r2b_one_shot"),
    ("snapshot", "contract-credential", '"real_credential_used": false', '"real_credential_used": true'),
]


def main() -> None:
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2a3-negative-") as temp:
        repo = Path(temp) / "repo"
        (repo / "scripts").mkdir(parents=True)
        shutil.copytree(ROOT / "docs/stage-8", repo / "docs/stage-8")
        shutil.copytree(
            ROOT / "tools/stage8b-readonly-preflight",
            repo / "tools/stage8b-readonly-preflight",
            ignore=shutil.ignore_patterns("target"),
        )
        shutil.copy2(ROOT / "scripts/stage8b_p_r2a3_review_closure_check.py", repo / "scripts")
        targets = {
            "authority": repo / "docs/stage-8/stage8b-p-r2a3-authority.json",
            "snapshot": repo / "docs/stage-8/stage8b-p-r2a3-finam-read-contract-snapshot.json",
            "r2a2": repo / "tools/stage8b-readonly-preflight/src/r2a2.rs",
            "r2a3": repo / "tools/stage8b-readonly-preflight/src/r2a3.rs",
            "main": repo / "tools/stage8b-readonly-preflight/src/main.rs",
            "launcher": repo / "tools/stage8b-readonly-preflight/src/bin/stage8b-r2a3-launcher.rs",
        }
        checker = repo / "scripts/stage8b_p_r2a3_review_closure_check.py"
        for target_name, name, old, new in MUTATIONS:
            target = targets[target_name]
            original = target.read_text()
            if old not in original:
                raise SystemExit(f"stage8b-p-r2a3-negative: FAIL setup marker {name}")
            target.write_text(original.replace(old, new, 1))
            result = subprocess.run(
                ["python3", str(checker), "--root", str(repo)],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            target.write_text(original)
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r2a3-negative: FAIL accepted {name}")
            passed += 1
            print(f"PASS {name}")
    print(f"stage8b-p-r2a3-negative: PASS {passed}/{len(MUTATIONS)}")


if __name__ == "__main__":
    main()
