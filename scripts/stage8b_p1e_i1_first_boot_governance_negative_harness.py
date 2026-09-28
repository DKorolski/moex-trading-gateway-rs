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


class HarnessError(RuntimeError):
    """Raised when a harness control or mutation does not behave as required."""


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
        checker.GOVERNANCE_CHECKER_PATH,
        checker.GOVERNANCE_NEGATIVE_PATH,
        checker.GOVERNANCE_GATE_PATH,
        checker.STATUS_PATH,
        checker.SOURCE_PATH,
        checker.SUPERVISOR_PATH,
    )
    for relative in paths:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)


def mutations() -> list[tuple[str, str, Mutation]]:
    return [
        (
            "active-plan-domain",
            "immutable_hash",
            lambda root: edit_json(
                root, checker.PLAN_PATH, lambda value: value.__setitem__("domain", "forged")
            ),
        ),
        (
            "accepted-source-ref",
            "contract_violation",
            lambda root: edit_json(
                root,
                checker.CLOSURE_PATH,
                lambda value: value["accepted_source"].__setitem__("source_ref", "0" * 40),
            ),
        ),
        (
            "active-schema-hash",
            "contract_violation",
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
            "contract_violation",
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
            "immutable_hash",
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
            "immutable_hash",
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
            "immutable_hash",
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
            "immutable_hash",
            lambda root: edit_json(
                root,
                checker.PLAN_PATH,
                lambda value: value["candidate"].__setitem__("exact_source_m1_records", 9),
            ),
        ),
        (
            "review-hash-rebind",
            "contract_violation",
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
            "contract_violation",
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
            "contract_violation",
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
            "contract_violation",
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
            "contract_violation",
            lambda root: replace_text(
                root,
                checker.SOURCE_PATH,
                "STAGE8B_P1E_FIRST_BOOT_SOURCE_SCHEMA_VERSION: u16 = 2",
                "STAGE8B_P1E_FIRST_BOOT_SOURCE_SCHEMA_VERSION: u16 = 1",
            ),
        ),
        (
            "supervisor-path-override",
            "contract_violation",
            lambda root: replace_text(
                root,
                checker.SUPERVISOR_PATH,
                checker.FIXED_SOURCE_PATH,
                "/tmp/untrusted-first-boot-source.json",
            ),
        ),
        (
            "status-premature-completion",
            "contract_violation",
            lambda root: replace_text(
                root,
                checker.STATUS_PATH,
                "complete deployable I1 is not yet accepted",
                "complete deployable I1 is accepted",
            ),
        ),
    ]


def make_fixture() -> tempfile.TemporaryDirectory[str]:
    return tempfile.TemporaryDirectory(prefix="stage8b-p1e-i1-governance-")


def require_positive_control() -> None:
    with make_fixture() as temporary:
        root = Path(temporary)
        copy_fixture(root)
        try:
            checker.check(root, repository_root=ROOT)
        except checker.GovernanceCheckError as error:
            raise HarnessError(
                f"positive control failed code={error.code}: {error}"
            ) from error
    print("PASS positive-control-same-check-path")


def mutation_rejection_code(root: Path, mutate: Mutation) -> str | None:
    mutate(root)
    try:
        checker.check(root, repository_root=ROOT)
    except checker.GovernanceCheckError as error:
        return error.code
    return None


def require_noop_control() -> None:
    try:
        run_mutation_cases(
            [("noop-control", "contract_violation", lambda _: None)],
            emit_pass=False,
        )
    except HarnessError as error:
        if str(error) != "noop-control: mutation was accepted":
            raise HarnessError(
                f"no-op control failed for an unrelated reason: {error}"
            ) from error
    else:
        raise HarnessError("no-op mutation was incorrectly counted as rejected")
    print("PASS no-op-mutation-is-not-counted")


def require_missing_git_is_infrastructure() -> None:
    with make_fixture() as temporary:
        root = Path(temporary)
        copy_fixture(root)
        try:
            checker.check(root)
        except checker.GovernanceCheckError as error:
            if error.code != "infrastructure":
                raise HarnessError(
                    "missing Git authority returned unexpected "
                    f"code={error.code}: {error}"
                ) from error
        else:
            raise HarnessError("fixture without Git authority unexpectedly passed")
    print("PASS missing-git-authority-is-infrastructure")


def run_mutation_cases(
    cases: list[tuple[str, str, Mutation]], *, emit_pass: bool = True
) -> int:
    passed = 0
    for name, expected_code, mutate in cases:
        with make_fixture() as temporary:
            root = Path(temporary)
            copy_fixture(root)
            code = mutation_rejection_code(root, mutate)
            if code is None:
                raise HarnessError(f"{name}: mutation was accepted")
            if code != expected_code:
                raise HarnessError(
                    f"{name}: expected code={expected_code}, got code={code}"
                )
            passed += 1
            if emit_pass:
                print(f"PASS {name} code={code}")
    return passed


def main() -> int:
    try:
        require_positive_control()
        require_noop_control()
        require_missing_git_is_infrastructure()

        cases = mutations()
        passed = run_mutation_cases(cases)
        print(
            "PASS stage8b-p1e-i1-first-boot-governance-negative-harness "
            f"semantic_cases={passed}/{len(cases)} controls=3/3"
        )
        return 0
    except (HarnessError, RuntimeError, ValueError) as error:
        print(
            f"FAIL stage8b-p1e-i1-first-boot-governance-negative-harness: {error}",
            file=sys.stderr,
        )
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
