#!/usr/bin/env python3
"""Mutation tests for the I1 first-boot governance closure checker."""

from __future__ import annotations

import json
import shutil
import sys
import tempfile
from pathlib import Path
from typing import Callable

import stage8b_p1e_i1_first_boot_governance_check as checker


ROOT = Path(__file__).resolve().parents[1]
Mutation = Callable[[Path], None]


def edit_json(root: Path, relative: Path, edit: Callable[[dict], None]) -> None:
    path = root / relative
    value = json.loads(path.read_text(encoding="utf-8"))
    edit(value)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def replace_text(root: Path, relative: Path, old: str, new: str) -> None:
    path = root / relative
    value = path.read_text(encoding="utf-8")
    if old not in value:
        raise RuntimeError(f"mutation source not found in {relative}: {old}")
    path.write_text(value.replace(old, new, 1), encoding="utf-8")


def copy_fixture(destination: Path) -> None:
    paths = (
        checker.PLAN_PATH,
        checker.CLOSURE_PATH,
        checker.SCHEMA_V2_PATH,
        checker.SCHEMA_V1_PATH,
        checker.PLAN_V1_PATH,
        checker.CONTRACT_PATH,
        checker.SOURCE_GATE_PATH,
        checker.STATUS_PATH,
        checker.SOURCE_PATH,
        checker.SUPERVISOR_PATH,
    )
    for relative in paths:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)


def mutations() -> list[tuple[str, Mutation]]:
    return [
        (
            "active-plan-domain",
            lambda root: edit_json(
                root, checker.PLAN_PATH, lambda value: value.__setitem__("domain", "forged")
            ),
        ),
        (
            "accepted-source-ref",
            lambda root: edit_json(
                root,
                checker.CLOSURE_PATH,
                lambda value: value["accepted_source"].__setitem__("source_ref", "0" * 40),
            ),
        ),
        (
            "active-schema-hash",
            lambda root: edit_json(
                root,
                checker.CLOSURE_PATH,
                lambda value: value["active_contracts"].__setitem__(
                    "wire_schema_sha256", "0" * 64
                ),
            ),
        ),
        (
            "historical-plan-hash",
            lambda root: edit_json(
                root,
                checker.CLOSURE_PATH,
                lambda value: value["historical_contracts"].__setitem__(
                    "source_plan_v1_sha256", "0" * 64
                ),
            ),
        ),
        (
            "legacy-v1-reopened",
            lambda root: edit_json(
                root,
                checker.PLAN_PATH,
                lambda value: value["wire_contract"].__setitem__(
                    "legacy_v1_wire_accepted", True
                ),
            ),
        ),
        (
            "history-count-only",
            lambda root: edit_json(
                root,
                checker.PLAN_PATH,
                lambda value: value["history"].__setitem__(
                    "coverage_authority", "session-date-count-only"
                ),
            ),
        ),
        (
            "candidate-builder-bypass",
            lambda root: edit_json(
                root,
                checker.PLAN_PATH,
                lambda value: value["candidate"].__setitem__(
                    "canonical_builder", "build_unchecked_candidate"
                ),
            ),
        ),
        (
            "candidate-m1-cardinality",
            lambda root: edit_json(
                root,
                checker.PLAN_PATH,
                lambda value: value["candidate"].__setitem__("exact_source_m1_records", 9),
            ),
        ),
        (
            "review-hash-rebind",
            lambda root: edit_json(
                root,
                checker.CLOSURE_PATH,
                lambda value: value["accepted_source"].__setitem__(
                    "review_sha256", "f" * 64
                ),
            ),
        ),
        (
            "transaction-v5-premature-acceptance",
            lambda root: edit_json(
                root,
                checker.CLOSURE_PATH,
                lambda value: value["source_acceptance"].__setitem__(
                    "transaction_v5_accepted", True
                ),
            ),
        ),
        (
            "deployable-owner-loop-opened",
            lambda root: edit_json(
                root,
                checker.CLOSURE_PATH,
                lambda value: value["next_authorized_slice"].__setitem__(
                    "deployable_owner_loop_authorized", True
                ),
            ),
        ),
        (
            "finam-write-opened",
            lambda root: edit_json(
                root,
                checker.CLOSURE_PATH,
                lambda value: value["closed_surfaces"].__setitem__(
                    "finam_post_delete", True
                ),
            ),
        ),
        (
            "source-wire-version-downgrade",
            lambda root: replace_text(
                root,
                checker.SOURCE_PATH,
                "STAGE8B_P1E_FIRST_BOOT_SOURCE_SCHEMA_VERSION: u16 = 2",
                "STAGE8B_P1E_FIRST_BOOT_SOURCE_SCHEMA_VERSION: u16 = 1",
            ),
        ),
        (
            "supervisor-path-override",
            lambda root: replace_text(
                root,
                checker.SUPERVISOR_PATH,
                checker.FIXED_SOURCE_PATH,
                "/tmp/untrusted-first-boot-source.json",
            ),
        ),
        (
            "status-premature-completion",
            lambda root: replace_text(
                root,
                checker.STATUS_PATH,
                "complete deployable I1 is not yet accepted",
                "complete deployable I1 is accepted",
            ),
        ),
    ]


def main() -> int:
    passed = 0
    cases = mutations()
    for name, mutate in cases:
        with tempfile.TemporaryDirectory(prefix="stage8b-p1e-i1-governance-") as temporary:
            root = Path(temporary)
            copy_fixture(root)
            mutate(root)
            try:
                checker.check(root)
            except checker.GovernanceCheckError:
                passed += 1
                print(f"PASS {name}")
            else:
                print(f"FAIL {name}: mutation was accepted", file=sys.stderr)
                return 1
    print(f"PASS stage8b-p1e-i1-first-boot-governance-negative-harness {passed}/{len(cases)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
