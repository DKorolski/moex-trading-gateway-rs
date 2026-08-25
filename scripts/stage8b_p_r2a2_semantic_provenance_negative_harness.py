#!/usr/bin/env python3
"""Negative mutation harness for the R2A2 source-bound checker."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

MUTATIONS = [
    ("source-count", "\"count\": 11", "\"count\": 17"),
    ("broker-pre-network", "\"broker_derived_sources_allowed\": false", "\"broker_derived_sources_allowed\": true"),
    ("fixed-r1b", "\"accepted_r1b_fixed_identities_exact\": true", "\"accepted_r1b_fixed_identities_exact\": false"),
    ("dynamic-claims", "\"dynamic_receipt_claims_exact\": true", "\"dynamic_receipt_claims_exact\": false"),
    ("constant-time", "\"constant_time_verify\": true", "\"constant_time_verify\": false"),
    ("endpoint-recompute", "\"endpoint_identity_recomputed_from_accepted_r1b_formula\": true", "\"endpoint_identity_recomputed_from_accepted_r1b_formula\": false"),
    ("schema-permissive", "\"unknown_or_missing_required_shape\": \"BLOCK\"", "\"unknown_or_missing_required_shape\": \"ALLOW\""),
    ("position-baseline", "\"position_must_equal_approved_baseline\": true", "\"position_must_equal_approved_baseline\": false"),
    ("auth-cap", "\"auth\": 65536", "\"auth\": 0"),
    ("trades-cap", "\"trades\": 16777216", "\"trades\": 99999999"),
    ("wrong-ca", "\"wrong_ca_rejected_before_http\": true", "\"wrong_ca_rejected_before_http\": false"),
    ("wrong-host", "\"wrong_hostname_rejected_before_http\": true", "\"wrong_hostname_rejected_before_http\": false"),
    ("self-hash-authority", "\"self_hash_is_authority\": false", "\"self_hash_is_authority\": true"),
    ("binary-open", "\"r2a2_binary_network_entry\": \"FAIL_CLOSED\"", "\"r2a2_binary_network_entry\": \"OPEN\""),
    ("authorization", "\"authorization_status\": \"NOT_ISSUED\"", "\"authorization_status\": \"ISSUED\""),
    ("receipt-domain", "stage8b-p-r2a2-local-authority-receipt-v1", "stage8b-p-r2a2-local-authority-receipt-v2"),
    ("account-domain", "moex-stage8b-account-binding-v1", "moex-stage8b-account-binding-v2"),
    ("endpoint-domain", "stage8b-i-r2-endpoint-identity-v1", "stage8b-i-r2-endpoint-identity-v2"),
    ("run-ttl", "pub const MAX_RUN_AHEAD_MS: i64 = 60_000;", "pub const MAX_RUN_AHEAD_MS: i64 = 600_000;"),
    ("remove-deny-unknown", "#[serde(deny_unknown_fields)]", "#[serde(default)]"),
    ("remove-ct-verify", "mac.verify_slice(&asserted)", "let _ = asserted"),
    ("position-compare", "if target_position != manifest.approved_pre_run_position", "if false"),
    ("working-check", "if !working", "if false"),
    ("bounded-stream", "while let Some(chunk) = response.chunk().await", "while let None = response.chunk().await"),
    ("raw-body-export", "raw_body_sha256_exported: false", "raw_body_sha256_exported: true"),
    ("credential-main", "fn main()", "const FINAM_SECRET_TOKEN: &str = \"forbidden\";\nfn main()"),
]


def main() -> None:
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2a2-negative-") as temp:
        temp_root = Path(temp) / "repo"
        shutil.copytree(ROOT / "tools/stage8b-readonly-preflight", temp_root / "tools/stage8b-readonly-preflight")
        shutil.copytree(ROOT / "docs/stage-8", temp_root / "docs/stage-8")
        (temp_root / "scripts").mkdir(parents=True)
        shutil.copy2(ROOT / "scripts/stage8b_p_r2a2_semantic_provenance_check.py", temp_root / "scripts")
        shutil.copy2(ROOT / "scripts/launch_stage8b_p_r2a2_qualified.sh", temp_root / "scripts")
        authority = temp_root / "docs/stage-8/stage8b-p-r2a2-semantic-provenance-authority.json"
        source = temp_root / "tools/stage8b-readonly-preflight/src/r2a2.rs"
        main_source = temp_root / "tools/stage8b-readonly-preflight/src/main.rs"
        for name, old, new in MUTATIONS:
            target = main_source if name == "credential-main" else source if old in source.read_text() else authority
            original = target.read_text()
            if old not in original:
                raise SystemExit(f"negative setup missing marker: {name}")
            target.write_text(original.replace(old, new, 1))
            result = subprocess.run(
                ["python3", str(temp_root / "scripts/stage8b_p_r2a2_semantic_provenance_check.py"), "--root", str(temp_root)],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            target.write_text(original)
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r2a2-negative: FAIL accepted {name}")
            passed += 1
            print(f"PASS {name}")
    print(f"stage8b-p-r2a2-negative: PASS {passed}/{len(MUTATIONS)}")


if __name__ == "__main__":
    main()
