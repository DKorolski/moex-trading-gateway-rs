#!/usr/bin/env python3
"""Mutation matrix for the R2A4 static closure checker."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MUTATIONS = [
    ("production", "production-issued", '"authorization_status": "NOT_ISSUED"', '"authorization_status": "ISSUED"'),
    ("status", "r2b-open", '"r2b_real_credentials_allowed": false', '"r2b_real_credentials_allowed": true'),
    ("status", "shared-receipts", '"source_specific_receipt_directories": true', '"source_specific_receipt_directories": false'),
    ("status", "producer-count", '"producer_count": 11', '"producer_count": 10'),
    ("status", "issuer-count", '"issuer_count": 11', '"issuer_count": 10'),
    ("status", "cancel-not-qualified", '"cancel_full_tls_sequence": true', '"cancel_full_tls_sequence": false'),
    ("controlled", "controlled-key-zero", '"2b61f0ec7a2b23523274ba3f5b0a4a9e66f75a1165e11ced25862d8a1692761d"', "0" * 64),
    ("production-trust", "public-key-replacement", '"public_key_ed25519_hex": "7decc95d909fda9caba2e6392dbd037c48ef69df18df14e63e2b24393aaae803"', '"public_key_ed25519_hex": "0000000000000000000000000000000000000000000000000000000000000000"'),
    ("production-account", "account-generation-replacement", '"generation_id": "1"', '"generation_id": "2"'),
    ("helper", "helper-unfrozen", None, "0" * 64),
    ("r2a4", "manifest-unbound", "package.manifest_sha256", "legacy_manifest_sha256"),
    ("r2a4", "account-hmac-unbound", "pub keyed_account_binding_hmac_sha256: String", "pub legacy_account_hmac: String"),
    ("r2a4", "trust-unbound", "pub public_key_set_sha256: String", "pub legacy_key_set: String"),
    ("r2a4", "generation-unbound", "pub source_generation_commitment_sha256: String", "pub legacy_generation_set: String"),
    ("r2a4", "operator-unbound", "pub operator_decision_sha256: String", "pub legacy_operator_decision: String"),
    ("r2a4", "follow-symlink", "libc::O_CLOEXEC | libc::O_NOFOLLOW", "libc::O_CLOEXEC"),
    ("r2a4", "nonce-replay", ".create_new(true)", ".create(true)"),
    ("r2a4", "whitespace-normalization", "fn strict_single_line", "fn forbidden() { let _ = value.trim(); }\nfn strict_single_line"),
    ("r2a4", "generation-replay", 'source_directory.join("generations")', 'source_directory.join("legacy")'),
    ("r2a4", "issuer-identity-unbound", "issuer_executable_sha256", "legacy_issuer_sha256"),
    ("r2a2", "executed-quantity-mismatch", "&& exact.executed_quantity == listed.executed_quantity", "&& true"),
    ("r2a2", "remaining-quantity-mismatch", "&& exact.remaining_quantity == listed.remaining_quantity", "&& true"),
    ("r2a2", "execution-id-mismatch", "exact.exec_id == listed.exec_id", "true"),
    ("r2a2", "timestamp-mismatch", "&& exact.accept_at == listed.accept_at", "&& true"),
    ("r2a3", "issuer-field-removed", "issuer_executable_sha256: String", "legacy_issuer_sha256: String"),
    ("main", "fixed-layout-entry-removed", 'mode == "--r2a4-qualify-fixed-layout"', 'mode == "--removed"'),
    ("launcher", "fd-launch-removed", "verified_exec(", "legacy_exec("),
    ("launcher", "ambient-environment", "let environment: Vec<CString> = Vec::new();", "let environment = vec![CString::new(\"X=1\").unwrap()];"),
    ("rehearsal", "cancel-rehearsal-removed", "PLACE CANCEL", "PLACE"),
    ("rehearsal", "uid-separation-removed", "setpriv --reuid", "true # removed uid switch"),
    ("rehearsal", "package-issuer-bypass", '  "$PACKAGE_ISSUER"\n  "$SERVER"', '  true # package issuer bypassed\n  "$SERVER"'),
    ("issuer-service", "shared-write-path", "ReadWritePaths=/run/moex-trading/stage8b/r2a4/receipts", "ReadWritePaths=/run/moex-trading/stage8b/r2a4"),
]


def main() -> None:
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2a4-negative-") as temp:
        repo = Path(temp) / "repo"
        shutil.copytree(ROOT / "docs/stage-8", repo / "docs/stage-8")
        shutil.copytree(ROOT / "deploy/stage8b-r2a4", repo / "deploy/stage8b-r2a4")
        shutil.copytree(ROOT / "tools/stage8b-readonly-preflight", repo / "tools/stage8b-readonly-preflight", ignore=shutil.ignore_patterns("target"))
        (repo / "scripts").mkdir()
        for name in ("stage8b_p_r2a4_review_closure_check.py", "stage8b_p_r2a4_linux_rehearsal.sh"):
            shutil.copy2(ROOT / "scripts" / name, repo / "scripts" / name)
        targets = {
            "production": repo / "docs/stage-8/stage8b-p-r2a4-authority.json",
            "status": repo / "docs/stage-8/stage8b-p-r2a4-status.json",
            "controlled": repo / "docs/stage-8/stage8b-p-r2a4-controlled-authority.json",
            "production-trust": repo / "docs/stage-8/stage8b-p-r2a4-production-trust-manifest.json",
            "production-account": repo / "docs/stage-8/stage8b-p-r2a4-production-account-key-manifest.json",
            "helper": repo / "docs/stage-8/stage8b-p-r2a4-accepted-helper-sha256.txt",
            "r2a2": repo / "tools/stage8b-readonly-preflight/src/r2a2.rs",
            "r2a3": repo / "tools/stage8b-readonly-preflight/src/r2a3.rs",
            "r2a4": repo / "tools/stage8b-readonly-preflight/src/r2a4.rs",
            "main": repo / "tools/stage8b-readonly-preflight/src/main.rs",
            "launcher": repo / "tools/stage8b-readonly-preflight/src/bin/stage8b-r2a4-launcher.rs",
            "rehearsal": repo / "scripts/stage8b_p_r2a4_linux_rehearsal.sh",
            "issuer-service": repo / "deploy/stage8b-r2a4/stage8b-r2a4-issuer@.service",
        }
        checker = repo / "scripts/stage8b_p_r2a4_review_closure_check.py"
        for target_name, name, old, new in MUTATIONS:
            target = targets[target_name]
            original = target.read_text()
            if old is None:
                target.write_text(new + "\n")
            else:
                if old not in original:
                    raise SystemExit(f"stage8b-p-r2a4-negative: FAIL setup {name}")
                target.write_text(original.replace(old, new, 1))
            result = subprocess.run(["python3", str(checker), "--root", str(repo)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            target.write_text(original)
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r2a4-negative: FAIL accepted {name}")
            passed += 1
            print(f"PASS {name}")
    print(f"stage8b-p-r2a4-negative: PASS {passed}/{len(MUTATIONS)}")


if __name__ == "__main__":
    main()
