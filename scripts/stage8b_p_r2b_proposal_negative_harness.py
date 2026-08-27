#!/usr/bin/env python3
"""Adversarial mutations for the design-only Stage 8B-P R2B proposal."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FILES = (
    "docs/stage-8/stage8b-p-r2b-proposal-authority.json",
    "docs/stage-8/STAGE8B_P_R2B_PROPOSAL_2026-08-27.md",
    "docs/stage-8/STAGE8B_P_R2A8_R1_ACCEPTANCE_CLOSURE_2026-08-27.md",
    "docs/stage-8/STAGE8B_P_R2B_PROPOSAL_ACCEPTANCE_MATRIX_2026-08-27.csv",
    "docs/stage-8/stage8b-p-r2a8-r1-causal-build-evidence.json",
    "tools/stage8b-readonly-preflight/src/r2a3.rs",
    "tools/stage8b-readonly-preflight/src/r2a5.rs",
    "scripts/stage8b_p_r2b_proposal_check.py",
)
MUTATIONS = (
    (0, "issue-r2b", '"authorization_status": "NOT_ISSUED"', '"authorization_status": "ISSUED"'),
    (0, "authorize-proposal", '"status": "PROPOSAL_ONLY_NOT_AUTHORIZED"', '"status": "AUTHORIZED"'),
    (0, "wrong-predecessor", '"source_ref": "5b2079d7d524d2fa6f084f44f961c4b5958c042a"', '"source_ref": "0000000000000000000000000000000000000000"'),
    (0, "multi-selection", '"selection_count": 1', '"selection_count": 2'),
    (0, "background-loop", '"background_loop": false', '"background_loop": true'),
    (0, "execution-influence", '"result_may_influence_execution": false', '"result_may_influence_execution": true'),
    (0, "alternate-host", '"exact_host": "api.finam.ru"', '"exact_host": "example.invalid"'),
    (0, "redirect", '"redirects_allowed": false', '"redirects_allowed": true'),
    (0, "proxy", '"proxy_allowed": false', '"proxy_allowed": true'),
    (0, "retry", '"automatic_retries_allowed": false', '"automatic_retries_allowed": true'),
    (0, "order-post", '"order_post_allowed": false', '"order_post_allowed": true'),
    (0, "order-delete", '"order_delete_allowed": false', '"order_delete_allowed": true'),
    (0, "arbitrary-request", '"arbitrary_request_allowed": false', '"arbitrary_request_allowed": true'),
    (0, "timeout-drift", '"request_timeout_seconds": 10', '"request_timeout_seconds": 30'),
    (0, "rate-drift", '"minimum_broker_get_interval_ms": 250', '"minimum_broker_get_interval_ms": 0'),
    (0, "place-route-drift", '"/v1/accounts/{account_id}/trades"', '"/v2/accounts/{account_id}/trades"'),
    (0, "fixture-production", '"fixture_features_allowed": false', '"fixture_features_allowed": true'),
    (0, "binary-hash-drift", '"b407b8997f0c5bcf299e7e0c25192ae7a0535680f75ed2d4b4bd941f1af945f5"', '"0407b8997f0c5bcf299e7e0c25192ae7a0535680f75ed2d4b4bd941f1af945f5"'),
    (0, "credential-early", '"credential_read_before_signed_package_validation": false', '"credential_read_before_signed_package_validation": true'),
    (0, "token-export", '"token_export_allowed": false', '"token_export_allowed": true'),
    (0, "caller-path", '"caller_supplied_path_allowed": false', '"caller_supplied_path_allowed": true'),
    (0, "redis-access", '"redis_access_allowed": false', '"redis_access_allowed": true'),
    (0, "drop-contract-refresh", '"fresh_public_contract_refresh_required": true', '"fresh_public_contract_refresh_required": false'),
    (0, "drop-readiness", '"full_readiness_semantics_required": true', '"full_readiness_semantics_required": false'),
    (0, "allow-unknown", '"unknown_status_fails_closed": true', '"unknown_status_fails_closed": false'),
    (0, "record-raw-body", '"raw_body_recorded": false', '"raw_body_recorded": true'),
    (0, "open-dispatch", '"broker_dispatch": false', '"broker_dispatch": true'),
    (0, "open-runtime", '"runtime_live": false', '"runtime_live": true'),
    (0, "fake-operator-selection", '"operator_selection": "ABSENT"', '"operator_selection": "PRESENT"'),
    (3, "matrix-row-removed", "R2B-P-030,issuance,R2B authorization is NOT_ISSUED,proposal authority,PASS\n", ""),
)


def main() -> None:
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2b-proposal-negative-") as temporary:
        base = Path(temporary) / "base"
        for relative in FILES:
            destination = base / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        baseline = subprocess.run(["python3", str(base / FILES[7])], cwd=base, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
        if baseline.returncode != 0:
            raise SystemExit("stage8b-p-r2b-proposal-negative: FAIL baseline")
        for index, name, old, new in MUTATIONS:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            target = case / FILES[index]
            text = target.read_text()
            if text.count(old) < 1:
                raise SystemExit(f"stage8b-p-r2b-proposal-negative: FAIL setup {name}")
            target.write_text(text.replace(old, new))
            result = subprocess.run(["python3", str(case / FILES[7])], cwd=case, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r2b-proposal-negative: FAIL accepted {name}")
            passed += 1
    print(f"stage8b-p-r2b-proposal-negative: PASS {passed}/{len(MUTATIONS)}")


if __name__ == "__main__":
    main()
