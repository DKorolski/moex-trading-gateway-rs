#!/usr/bin/env python3
"""Strict-schema and refresh-exhaustiveness negatives for R0-R1A1."""

from __future__ import annotations

import copy
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage8b_p_r2b_issuance_r1_check.py"
AUTHORITY = "docs/stage-8/stage8b-p-r2b-issuance-package-r0-r1-authority.json"
ISSUANCE_EVIDENCE = "docs/stage-8/stage8b-p-r2b-issuance-package-r0-r1-evidence.json"
REFRESH_EVIDENCE = "docs/stage-8/stage8b-p-r2b-r0-r1-read-contract-refresh-evidence.json"
MATRIX = "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_R1_ACCEPTANCE_MATRIX_2026-08-29.csv"
FILES = (
    CHECKER,
    "scripts/stage8b_p_r2b_read_contract_refresh.py",
    AUTHORITY,
    ISSUANCE_EVIDENCE,
    REFRESH_EVIDENCE,
    MATRIX,
    "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_R1_2026-08-29.md",
    "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_R1A_EXACT_GOVERNANCE_FREEZE_2026-08-29.md",
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
Mutation = Callable[[Path], None]


def run(root: Path) -> int:
    return subprocess.run(
        ["python3", str(root / CHECKER)], cwd=root,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    ).returncode


def read_json(root: Path, relative: str) -> dict:
    return json.loads((root / relative).read_text(encoding="utf-8"))


def write_json(root: Path, relative: str, document: dict) -> None:
    (root / relative).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


def mutate_authority(mutator: Callable[[dict], None]) -> Mutation:
    def mutate(root: Path) -> None:
        document = read_json(root, AUTHORITY)
        mutator(document)
        write_json(root, AUTHORITY, document)
    return mutate


def mutate_issuance_evidence(mutator: Callable[[dict], None]) -> Mutation:
    def mutate(root: Path) -> None:
        document = read_json(root, ISSUANCE_EVIDENCE)
        mutator(document)
        write_json(root, ISSUANCE_EVIDENCE, document)
    return mutate


def mutate_refresh(mutator: Callable[[dict], None]) -> Mutation:
    def mutate(root: Path) -> None:
        refresh = read_json(root, REFRESH_EVIDENCE)
        mutator(refresh)
        write_json(root, REFRESH_EVIDENCE, refresh)
        digest = hashlib.sha256((root / REFRESH_EVIDENCE).read_bytes()).hexdigest()
        issuance = read_json(root, ISSUANCE_EVIDENCE)
        issuance["read_contract"]["fresh_refresh_evidence_sha256"] = digest
        write_json(root, ISSUANCE_EVIDENCE, issuance)
    return mutate


def duplicate_refresh(document: dict) -> None:
    document["documents"][5] = copy.deepcopy(document["documents"][0])


def substitute_refresh(document: dict) -> None:
    document["documents"][5] = copy.deepcopy(document["documents"][1])


def reorder_refresh(document: dict) -> None:
    document["documents"][0], document["documents"][1] = (
        document["documents"][1], document["documents"][0]
    )


def mutate_matrix(root: Path) -> None:
    path = root / MATRIX
    text = path.read_text(encoding="utf-8")
    old = "R2B remains NOT_ISSUED"
    if text.count(old) != 1:
        raise RuntimeError("matrix mutation anchor drift")
    path.write_text(text.replace(old, "R2B claim text drifted"), encoding="utf-8")


CASES: tuple[tuple[str, Mutation], ...] = (
    ("refresh-document-duplicate", mutate_refresh(duplicate_refresh)),
    ("refresh-document-substitution", mutate_refresh(substitute_refresh)),
    ("refresh-document-order-drift", mutate_refresh(reorder_refresh)),
    ("closed-surface-key-removed", mutate_authority(lambda d: d["closed_surfaces"].pop("order_delete_sent"))),
    ("closed-surface-key-extra", mutate_authority(lambda d: d["closed_surfaces"].update({"unknown_effect": False}))),
    ("implementation-state-key-removed", mutate_authority(lambda d: d["implementation_state"].pop("services_enabled"))),
    ("operator-input-key-removed", mutate_authority(lambda d: d["operator_local_inputs"].pop("operation"))),
    ("builder-unknown-field", mutate_authority(lambda d: d["package_formation"]["builder"].update({"alternate_output": "/tmp/forged"}))),
    ("barrier-contract-unknown-field", mutate_authority(lambda d: d["transaction"]["barrier_contract"].update({"downstream_may_start_on_failure": True}))),
    ("authority-stage-drift", mutate_authority(lambda d: d.update({"stage": "Forged Stage"}))),
    ("evidence-stage-drift", mutate_issuance_evidence(lambda d: d.update({"stage": "Forged Evidence Stage"}))),
    ("acceptance-row-claim-drift", mutate_matrix),
)


def main() -> None:
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2b-issuance-r1a1-negative-") as temporary:
        base = Path(temporary) / "base"
        for relative in FILES:
            destination = base / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        if run(base) != 0:
            raise SystemExit("stage8b-p-r2b-issuance-r1a1-negative: FAIL baseline")

        for name, mutate in CASES:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            mutate(case)
            if run(case) == 0:
                raise SystemExit(f"stage8b-p-r2b-issuance-r1a1-negative: FAIL accepted {name}")
            passed += 1

    if passed != 12:
        raise SystemExit(f"stage8b-p-r2b-issuance-r1a1-negative: FAIL {passed}/12")
    print("stage8b-p-r2b-issuance-r1a1-negative: PASS 12/12")


if __name__ == "__main__":
    main()
