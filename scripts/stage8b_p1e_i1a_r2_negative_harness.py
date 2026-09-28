#!/usr/bin/env python3
"""Repinned semantic mutations for the I1A R2 design correction."""

from __future__ import annotations

import csv
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = Path("scripts/stage8b_p1e_i1a_r2_design_check.py")
MODEL = Path("scripts/stage8b_p1e_i1a_r2_semantic_model.py")
BASE_MODEL = Path("scripts/stage8b_p1e_i1a_r1_semantic_model.py")
IDENTITY_SCHEMA = Path("docs/stage-8/stage8b-p1e-i1a-semantic-identity-v1.schema.json")
IDENTITY_CONTRACT = Path("docs/stage-8/stage8b-p1e-i1a-semantic-identity-contract-v1.json")
ENVELOPE = Path("docs/stage-8/stage8b-p1e-i1a-schedule-envelope-v3.schema.json")
POLICY = Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-policy-v3.json")
PROGRESSION = Path("docs/stage-8/stage8b-p1e-i1a-source-progression-v2.json")
MATRIX = Path("docs/stage-8/stage8b-p1e-i1a-r2-acceptance-matrix-v1.csv")
FIXTURES = Path("docs/stage-8/stage8b-p1e-i1a-r2-semantic-fixtures-v1.json")
DESIGN = Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v3.json")
MARKDOWN = Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v3.md")
BASE_FILES = (
    Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-policy-v2.json"),
    Path("docs/stage-8/stage8b-p1e-i1a-schedule-envelope-v2.schema.json"),
    Path("docs/stage-8/stage8b-p1e-i1a-source-progression-v1.json"),
    Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v2.json"),
    Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v2.md"),
    Path("docs/stage-8/stage8b-p1e-i1a-r1-model-fixtures-v1.json"),
    Path("docs/stage-8/stage8b-p1e-i1a-implementation-scope-v2.json"),
)
FILES = (
    CHECKER,
    MODEL,
    BASE_MODEL,
    IDENTITY_SCHEMA,
    IDENTITY_CONTRACT,
    ENVELOPE,
    POLICY,
    PROGRESSION,
    MATRIX,
    FIXTURES,
    DESIGN,
    MARKDOWN,
    *BASE_FILES,
)


def copy_fixture(target: Path) -> None:
    for relative in FILES:
        destination = target / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, destination)


def run(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            "/bin/bash",
            "-c",
            f"{sys.executable} {CHECKER} && PYTHONPATH=scripts {sys.executable} {MODEL}",
        ],
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )


def repin(root: Path, relative: Path, old_hash: str) -> None:
    checker = root / CHECKER
    source = checker.read_text()
    if old_hash not in source:
        raise RuntimeError(f"checker hash anchor missing for {relative}: {old_hash}")
    new_hash = hashlib.sha256((root / relative).read_bytes()).hexdigest()
    checker.write_text(source.replace(old_hash, new_hash, 1))


def mutate_json(root: Path, relative: Path, change: Callable[[dict[str, Any]], None]) -> None:
    path = root / relative
    old_hash = hashlib.sha256(path.read_bytes()).hexdigest()
    value = json.loads(path.read_text())
    change(value)
    path.write_text(json.dumps(value, indent=2) + "\n")
    repin(root, relative, old_hash)


def mutate_csv(root: Path, change: Callable[[list[dict[str, str]]], None]) -> None:
    path = root / MATRIX
    old_hash = hashlib.sha256(path.read_bytes()).hexdigest()
    with path.open(newline="") as handle:
        reader = csv.DictReader(handle)
        fields = reader.fieldnames
        rows = list(reader)
    if fields is None:
        raise RuntimeError("missing matrix header")
    change(rows)
    with path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)
    repin(root, MATRIX, old_hash)


def mutate_text(root: Path, old: str, new: str) -> None:
    path = root / MARKDOWN
    old_hash = hashlib.sha256(path.read_bytes()).hexdigest()
    source = path.read_text()
    if old not in source:
        raise RuntimeError(f"markdown anchor missing: {old}")
    path.write_text(source.replace(old, new, 1))
    repin(root, MARKDOWN, old_hash)


def remove_matrix_row(rows: list[dict[str, str]]) -> None:
    rows.pop()


def change_case(root: Path, case_id: str, key: str, value: object) -> None:
    def mutation(document: dict[str, Any]) -> None:
        case = next(item for item in document["cases"] if item["id"] == case_id)
        case[key] = value

    mutate_json(root, FIXTURES, mutation)


def create_early_source(root: Path) -> None:
    path = root / "crates/runtime-durable-service/src/stage8b_p1e_schedule_source.rs"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("// production source is not authorized by I1A R2 design\n")


def main() -> None:
    cases: list[tuple[str, Callable[[Path], None]]] = [
        (
            "identity-stage4-field-removed",
            lambda root: mutate_json(
                root,
                IDENTITY_SCHEMA,
                lambda value: value["required"].remove("stage4_semantic_state"),
            ),
        ),
        (
            "instrument-tick-field-removed",
            lambda root: mutate_json(
                root,
                IDENTITY_SCHEMA,
                lambda value: value["properties"]["instrument"]["required"].remove("tick_size"),
            ),
        ),
        (
            "closed-state-kind-forged",
            lambda root: mutate_json(
                root,
                IDENTITY_SCHEMA,
                lambda value: value["$defs"]["stage4SemanticState"]["oneOf"][1]
                ["properties"]["evidence_kind"].update(const="tradability"),
            ),
        ),
        (
            "freshness-added-to-semantic-projection",
            lambda root: mutate_json(
                root,
                IDENTITY_CONTRACT,
                lambda value: value["included_projection"].append("published_at_utc"),
            ),
        ),
        (
            "publication-sequence-not-explicitly-excluded",
            lambda root: mutate_json(
                root,
                IDENTITY_CONTRACT,
                lambda value: value["excluded_transport_and_freshness_fields"].remove(
                    "publication_sequence"
                ),
            ),
        ),
        (
            "semantic-hash-domain-drift",
            lambda root: mutate_json(
                root,
                IDENTITY_CONTRACT,
                lambda value: value["hash"].update(domain="unscoped-sha256"),
            ),
        ),
        (
            "canonical-floating-point-opened",
            lambda root: mutate_json(
                root,
                IDENTITY_CONTRACT,
                lambda value: value["canonical_encoding"].update(
                    floating_point_numbers_allowed=True
                ),
            ),
        ),
        (
            "changed-hash-retains-revision",
            lambda root: mutate_json(
                root,
                IDENTITY_CONTRACT,
                lambda value: value["producer_revision"].update(
                    different_hash_from_immediately_prior_publication="retain-exact-prior-semantic-revision"
                ),
            ),
        ),
        (
            "heartbeat-increments-revision",
            lambda root: mutate_json(
                root,
                IDENTITY_CONTRACT,
                lambda value: value["producer_revision"].update(
                    same_hash_as_immediately_prior_publication="exact-prior-semantic-revision-plus-one"
                ),
            ),
        ),
        (
            "publisher-restart-reset-opened",
            lambda root: mutate_json(
                root,
                IDENTITY_CONTRACT,
                lambda value: value["producer_revision"].update(
                    reset_or_guess_after_restart=True
                ),
            ),
        ),
        (
            "same-revision-conflict-weakened",
            lambda root: mutate_json(
                root,
                IDENTITY_CONTRACT,
                lambda value: value["consumer_progression"].update(
                    semantic_revision_equal_and_hash_different="allowed"
                ),
            ),
        ),
        (
            "intermediate-revisions-required",
            lambda root: mutate_json(
                root,
                IDENTITY_CONTRACT,
                lambda value: value["consumer_progression"].update(
                    require_observation_of_intermediate_semantic_revisions=True
                ),
            ),
        ),
        (
            "envelope-semantic-schema-detached",
            lambda root: mutate_json(
                root,
                ENVELOPE,
                lambda value: value["properties"]["semantic_identity"].update(
                    **{"$ref": "different-schema.json"}
                ),
            ),
        ),
        (
            "envelope-open-close-rule-removed",
            lambda root: mutate_json(
                root,
                ENVELOPE,
                lambda value: value["x_semantic_constraints"].remove(
                    "Open-to-Closed and trading-day transitions change semantic identity and revision even when sessions are unchanged"
                ),
            ),
        ),
        (
            "policy-production-implementation-opened",
            lambda root: mutate_json(
                root,
                POLICY,
                lambda value: value.update(production_implementation_authorized=True),
            ),
        ),
        (
            "policy-open-close-retains-revision",
            lambda root: mutate_json(
                root,
                POLICY,
                lambda value: value["producer_revision"].update(
                    open_to_closed_same_sessions="hash-changes-revision-unchanged"
                ),
            ),
        ),
        (
            "progression-changed-hash-retains-revision",
            lambda root: mutate_json(
                root,
                PROGRESSION,
                lambda value: value["producer_rules"][2].update(
                    decision="retain-prior-semantic-revision"
                ),
            ),
        ),
        (
            "progression-day-route-link-removed",
            lambda root: mutate_json(
                root,
                PROGRESSION,
                lambda value: value["required_transitions"].update(
                    open_to_closed_same_sessions="terminal-semantic-conflict"
                ),
            ),
        ),
        ("acceptance-case-removed", lambda root: mutate_csv(root, remove_matrix_row)),
        (
            "open-closed-sessions-no-longer-identical",
            lambda root: mutate_json(
                root,
                FIXTURES,
                lambda value: value["identities"]["closed_day_1"]["sessions"][0].update(
                    start_utc="2026-09-14T06:01:00.000000Z"
                ),
            ),
        ),
        (
            "open-close-positive-expectation-forged",
            lambda root: change_case(
                root,
                "SI-open-to-closed",
                "expected_progression",
                "terminal-semantic-conflict",
            ),
        ),
        (
            "heartbeat-revision-forged",
            lambda root: change_case(root, "SI-heartbeat", "expected_revision", 8),
        ),
        (
            "table-lookup-disclosure-reduced",
            lambda root: mutate_json(
                root,
                DESIGN,
                lambda value: value["coverage_classification"].update(
                    r1_table_lookup_cases=13
                ),
            ),
        ),
        (
            "design-model-misrepresented-as-source",
            lambda root: mutate_json(
                root,
                DESIGN,
                lambda value: value["coverage_classification"].update(
                    design_model_is_source_execution_evidence=True
                ),
            ),
        ),
        (
            "implementation-test-obligation-removed",
            lambda root: mutate_json(
                root,
                DESIGN,
                lambda value: value["coverage_classification"].update(
                    implementation_requires_real_state_transition_counter_crash_and_latch_tests=False
                ),
            ),
        ),
        (
            "design-production-facade-opened",
            lambda root: mutate_json(
                root,
                DESIGN,
                lambda value: value["implementation_authorization"].update(
                    production_stage5e_facade=True
                ),
            ),
        ),
        (
            "markdown-table-lookup-disclosure-removed",
            lambda root: mutate_text(root, "14/41", "some inherited cases"),
        ),
        ("production-source-before-r2-acceptance", create_early_source),
    ]

    with tempfile.TemporaryDirectory(prefix="stage8b-p1e-i1a-r2-baseline-") as directory:
        baseline = Path(directory)
        copy_fixture(baseline)
        result = run(baseline)
        if result.returncode != 0:
            print(result.stdout, end="")
            raise SystemExit(
                "stage8b-p1e-i1a-r2-negative-harness: FAIL: baseline did not pass"
            )

    passed = 0
    for name, mutation in cases:
        with tempfile.TemporaryDirectory(
            prefix=f"stage8b-p1e-i1a-r2-{name}-"
        ) as directory:
            root = Path(directory)
            copy_fixture(root)
            mutation(root)
            result = run(root)
            if result.returncode == 0:
                print(result.stdout, end="")
                raise SystemExit(
                    f"stage8b-p1e-i1a-r2-negative-harness: FAIL: mutation passed: {name}"
                )
            print(f"PASS {name}")
            passed += 1

    print(f"stage8b-p1e-i1a-r2-negative-harness: PASS {passed}/{len(cases)}")


if __name__ == "__main__":
    main()
