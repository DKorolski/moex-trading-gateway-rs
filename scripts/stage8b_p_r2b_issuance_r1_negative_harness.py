#!/usr/bin/env python3
"""Targeted negative matrix for the two R0-R1 closure findings."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage8b_p_r2b_issuance_r1_check.py"
AUTHORITY = "docs/stage-8/stage8b-p-r2b-issuance-package-r0-r1-authority.json"
EVIDENCE = "docs/stage-8/stage8b-p-r2b-r0-r1-read-contract-refresh-evidence.json"
FILES = (
    CHECKER,
    "scripts/stage8b_p_r2b_read_contract_refresh.py",
    AUTHORITY,
    EVIDENCE,
    "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_R1_2026-08-29.md",
    "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_R1_ACCEPTANCE_MATRIX_2026-08-29.csv",
    "docs/stage-8/stage8b-p-r2b-issuance-package-r0-r1-evidence.json",
    "docs/stage-8/stage8b-p-r2a3-finam-read-contract-snapshot.json",
    "docs/stage-8/finam-r2a3-read-contracts/auth.md",
    "docs/stage-8/finam-r2a3-read-contracts/token_details.md",
    "docs/stage-8/finam-r2a3-read-contracts/get_account.md",
    "docs/stage-8/finam-r2a3-read-contracts/trades.md",
    "docs/stage-8/finam-r2a3-read-contracts/get_orders.md",
    "docs/stage-8/finam-r2a3-read-contracts/get_order.md",
    "tools/stage8b-readonly-preflight/src/r2a3.rs",
    "tools/stage8b-readonly-preflight/src/r2a4.rs",
    "tools/stage8b-readonly-preflight/src/r2a5.rs",
)


def run(root: Path) -> int:
    return subprocess.run(
        ["python3", str(root / CHECKER)], cwd=root,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    ).returncode


def set_path(document: Any, path: tuple[Any, ...], value: Any) -> None:
    current = document
    for key in path[:-1]:
        current = current[key]
    current[path[-1]] = value


def main() -> None:
    authority_cases = (
        ("wrong-r2b-contract-snapshot", ("read_contract", "snapshot_sha256"), "0" * 64),
        ("effect-contract-used-as-read-contract", ("read_contract", "snapshot_path"), "docs/stage-8/stage8b-p-finam-contract-snapshot-2026-08-24.json"),
        ("helper-contract-sha-mismatch", ("read_contract", "helper_embedded_snapshot_sha256"), "1" * 64),
        ("run-package-contract-sha-mismatch", ("read_contract", "future_run_package_contract_snapshot_sha256"), "2" * 64),
        ("activation-refresh-disabled", ("read_contract", "activation_refresh_required"), False),
        ("contract-refresh-too-old", ("read_contract", "activation_max_age_seconds"), 1801),
        ("unsigned-draft-producer-missing", ("package_formation", "builder", "executable"), ""),
        ("unsigned-draft-path-drift", ("package_formation", "builder", "output", "path"), "/tmp/draft.json"),
        ("unsigned-draft-owner-drift", ("package_formation", "builder", "output", "owner_uid"), 1),
        ("unsigned-draft-mode-drift", ("package_formation", "builder", "output", "mode"), "0644"),
        ("unsigned-draft-controlled-producer", ("package_formation", "builder", "controlled_fixture_producer_allowed"), True),
        ("mixed-run-nonces-build-package", ("package_formation", "builder", "required_same_run_nonce"), False),
        ("stale-receipt-package", ("package_formation", "builder", "stale_receipts_allowed"), True),
        ("builder-signing-key-access", ("package_formation", "builder", "signing_key_access"), True),
        ("existing-unsigned-reused", ("package_formation", "builder", "output", "existing_output_reuse_allowed"), True),
        ("phase-target-missing", ("transaction", "phases", 2, "target"), ""),
        ("phase-order-edge-removed", ("transaction", "phases", 2, "after_target"), None),
        ("phase-failure-allows-downstream", ("transaction", "barrier_contract", "failed_component_blocks_downstream"), False),
        ("partial-producer-phase-allows-issuers", ("transaction", "barrier_contract", "partial_fanout_allowed"), True),
        ("package-issuer-failure-allows-supervisor", ("transaction", "barrier_contract", "package_issuer_requires_draft_builder"), False),
        ("supervisor-without-signed-package", ("transaction", "barrier_contract", "supervisor_requires_package_issuer"), False),
        ("condition-skip-allows-downstream", ("transaction", "barrier_contract", "condition_skip_semantics_allowed"), True),
    )
    evidence_cases = (
        ("read-contract-url-drift", ("documents", 0, "url"), "https://api.finam.ru/docs/rest/ordersservice_placeorder.md"),
        ("read-contract-response-hash-drift", ("documents", 0, "observed_sha256"), "3" * 64),
    )
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2b-issuance-r1-negative-") as temporary:
        base = Path(temporary) / "base"
        for relative in FILES:
            destination = base / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        if run(base) != 0:
            raise SystemExit("stage8b-p-r2b-issuance-r1-negative: FAIL baseline")

        for name, path, value in authority_cases:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            target = case / AUTHORITY
            document = json.loads(target.read_text(encoding="utf-8"))
            set_path(document, path, value)
            target.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
            if run(case) == 0:
                raise SystemExit(f"stage8b-p-r2b-issuance-r1-negative: FAIL accepted {name}")
            passed += 1

        for name, path, value in evidence_cases:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            target = case / EVIDENCE
            document = json.loads(target.read_text(encoding="utf-8"))
            set_path(document, path, value)
            target.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
            if run(case) == 0:
                raise SystemExit(f"stage8b-p-r2b-issuance-r1-negative: FAIL accepted {name}")
            passed += 1

        case = Path(temporary) / "read-contract-document-missing"
        shutil.copytree(base, case)
        target = case / EVIDENCE
        document = json.loads(target.read_text(encoding="utf-8"))
        document["documents"].pop()
        target.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
        if run(case) == 0:
            raise SystemExit("stage8b-p-r2b-issuance-r1-negative: FAIL accepted read-contract-document-missing")
        passed += 1

    if passed != 25:
        raise SystemExit(f"stage8b-p-r2b-issuance-r1-negative: FAIL {passed}/25")
    print("stage8b-p-r2b-issuance-r1-negative: PASS 25/25")


if __name__ == "__main__":
    main()
