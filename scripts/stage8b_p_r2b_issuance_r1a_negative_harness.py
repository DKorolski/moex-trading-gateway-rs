#!/usr/bin/env python3
"""Exact-governance negative matrix for the R0-R1A issuance freeze."""

from __future__ import annotations

import copy
import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage8b_p_r2b_issuance_r1_check.py"
AUTHORITY = "docs/stage-8/stage8b-p-r2b-issuance-package-r0-r1-authority.json"
FILES = (
    CHECKER,
    "scripts/stage8b_p_r2b_read_contract_refresh.py",
    AUTHORITY,
    "docs/stage-8/stage8b-p-r2b-r0-r1-read-contract-refresh-evidence.json",
    "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_R1_2026-08-29.md",
    "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_R1A_EXACT_GOVERNANCE_FREEZE_2026-08-29.md",
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
Mutation = Callable[[dict[str, Any]], None]


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


def setting(path: tuple[Any, ...], value: Any) -> Mutation:
    return lambda document: set_path(document, path, value)


def pop_at(path: tuple[Any, ...], index: int) -> Mutation:
    def mutate(document: dict[str, Any]) -> None:
        current: Any = document
        for key in path:
            current = current[key]
        current.pop(index)
    return mutate


def swap_at(path: tuple[Any, ...], left: int, right: int) -> Mutation:
    def mutate(document: dict[str, Any]) -> None:
        current: Any = document
        for key in path:
            current = current[key]
        current[left], current[right] = current[right], current[left]
    return mutate


def append_copy(path: tuple[Any, ...], index: int) -> Mutation:
    def mutate(document: dict[str, Any]) -> None:
        current: Any = document
        for key in path:
            current = current[key]
        current.append(copy.deepcopy(current[index]))
    return mutate


def duplicate_receipt_source(document: dict[str, Any]) -> None:
    sources = document["package_formation"]["builder"]["receipt_sources"]
    sources[1] = copy.deepcopy(sources[0])


def drift_both_receipt_uids(document: dict[str, Any]) -> None:
    source = document["package_formation"]["builder"]["receipt_sources"][0]
    source["producer_uid"] = 8991
    source["issuer_uid"] = 8992


def rename_phase_with_updated_successor(document: dict[str, Any]) -> None:
    phases = document["transaction"]["phases"]
    phases[0]["target"] = "moex-stage8b-r2b-phase1-renamed.target"
    phases[1]["after_target"] = phases[0]["target"]


CASES: tuple[tuple[str, Mutation], ...] = (
    ("builder-fixed-input-path-drift", setting(("package_formation", "builder", "fixed_inputs", 0, "path"), "/tmp/run-nonce")),
    ("builder-fixed-input-missing", pop_at(("package_formation", "builder", "fixed_inputs"), 3)),
    ("builder-fixed-input-reordered", swap_at(("package_formation", "builder", "fixed_inputs"), 0, 1)),
    ("builder-fixed-input-extra", append_copy(("package_formation", "builder", "fixed_inputs"), 0)),
    ("receipt-source-name-drift", setting(("package_formation", "builder", "receipt_sources", 0, "source"), "trusted_clock_alias")),
    ("receipt-source-missing", pop_at(("package_formation", "builder", "receipt_sources"), 4)),
    ("receipt-source-duplicate", duplicate_receipt_source),
    ("receipt-producer-instance-drift", setting(("package_formation", "builder", "receipt_sources", 0, "producer_service"), "stage8b-r2a5-producer@m8p8991.service")),
    ("receipt-issuer-instance-drift", setting(("package_formation", "builder", "receipt_sources", 0, "issuer_service"), "stage8b-r2a5-issuer@m8i8992.service")),
    ("receipt-uid-map-drift", drift_both_receipt_uids),
    ("receipt-path-drift", setting(("package_formation", "builder", "receipt_sources", 0, "receipt_path"), "/tmp/receipt.json")),
    ("receipt-authority-type-drift", setting(("package_formation", "builder", "receipt_sources", 0, "authority_type"), "ForgedAuthority")),
    ("phase-target-name-drift", rename_phase_with_updated_successor),
    ("phase-ordinal-drift", setting(("transaction", "phases", 1, "ordinal"), 7)),
    ("phase-service-name-drift", setting(("transaction", "phases", 0, "services", 0), "forged-current-publisher.service")),
    ("phase-service-cardinality-drift", pop_at(("transaction", "phases", 2, "services"), 0)),
    ("phase-service-order-drift", swap_at(("transaction", "phases", 0, "services"), 0, 1)),
    ("builder-service-drift", setting(("package_formation", "builder", "service"), "forged-builder.service")),
    ("builder-output-drift", setting(("package_formation", "builder", "output", "path"), "/tmp/unsigned.json")),
    ("builder-signing-key-access", setting(("package_formation", "builder", "signing_key_access"), True)),
    ("builder-network-opened", setting(("package_formation", "builder", "network_allowed"), True)),
    ("signer-service-drift", setting(("package_formation", "signer", "service"), "forged-signer.service")),
    ("signer-input-drift", setting(("package_formation", "signer", "fixed_input"), "/tmp/forged-unsigned.json")),
    ("signer-output-drift", setting(("package_formation", "signer", "output", "path"), "/tmp/forged-signed.json")),
    ("signer-order-drift", swap_at(("transaction", "phases", 4, "services"), 0, 1)),
    ("refresh-evidence-path-drift", setting(("read_contract", "refresh_evidence"), "docs/stage-8/forged-refresh.json")),
    ("refresh-snapshot-path-drift", setting(("read_contract", "snapshot_path"), "docs/stage-8/forged-snapshot.json")),
    ("refresh-document-inventory-drift", pop_at(("read_contract", "document_names"), 5)),
    ("refresh-max-age-drift", setting(("read_contract", "activation_max_age_seconds"), 1801)),
)


def main() -> None:
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2b-issuance-r1a-negative-") as temporary:
        base = Path(temporary) / "base"
        for relative in FILES:
            destination = base / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        if run(base) != 0:
            raise SystemExit("stage8b-p-r2b-issuance-r1a-negative: FAIL baseline")

        for name, mutate in CASES:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            target = case / AUTHORITY
            document = json.loads(target.read_text(encoding="utf-8"))
            mutate(document)
            target.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
            if run(case) == 0:
                raise SystemExit(f"stage8b-p-r2b-issuance-r1a-negative: FAIL accepted {name}")
            passed += 1

    if passed != 29:
        raise SystemExit(f"stage8b-p-r2b-issuance-r1a-negative: FAIL {passed}/29")
    print("stage8b-p-r2b-issuance-r1a-negative: PASS 29/29")


if __name__ == "__main__":
    main()
