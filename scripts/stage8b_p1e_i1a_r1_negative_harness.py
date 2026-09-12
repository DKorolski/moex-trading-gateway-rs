#!/usr/bin/env python3
"""Repinned semantic mutations for the Stage 8B-P1-e I1A R1 design."""

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
CHECKER = Path("scripts/stage8b_p1e_i1a_r1_design_check.py")
MODEL = Path("scripts/stage8b_p1e_i1a_r1_semantic_model.py")
POLICY = Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-policy-v2.json")
BINDING = Path("docs/stage-8/stage8b-p1e-i1a-schedule-binding-record-v1.schema.json")
PROGRESSION = Path("docs/stage-8/stage8b-p1e-i1a-source-progression-v1.json")
DAY = Path("docs/stage-8/stage8b-p1e-i1a-day-boundary-proof-v1.json")
TIMER = Path("docs/stage-8/stage8b-p1e-source-timer-precedence-v5.json")
ENVELOPE = Path("docs/stage-8/stage8b-p1e-i1a-schedule-envelope-v2.schema.json")
SCOPE = Path("docs/stage-8/stage8b-p1e-i1a-implementation-scope-v2.json")
DESIGN = Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v2.json")
MARKDOWN = Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v2.md")
PHASES = Path("docs/stage-8/stage8b-p1e-i1a-schedule-binding-phase-matrix-v1.csv")
MATRIX = Path("docs/stage-8/stage8b-p1e-i1a-r1-acceptance-matrix-v1.csv")
FIXTURES = Path("docs/stage-8/stage8b-p1e-i1a-r1-model-fixtures-v1.json")
IMMUTABLE = (
    Path("docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json"),
    Path("docs/stage-8/stage8b-p1e-source-timer-precedence-v4.json"),
    Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-policy-v1.json"),
    Path("docs/stage-8/stage8b-p1e-i1a-schedule-envelope-v1.schema.json"),
    Path("docs/stage-8/stage8b-p1e-i1a-implementation-scope-v1.json"),
    Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v1.json"),
    Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v1.md"),
    Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-acceptance-matrix-v1.csv"),
)
FILES = (CHECKER, MODEL, POLICY, BINDING, PROGRESSION, DAY, TIMER, ENVELOPE, SCOPE, DESIGN, MARKDOWN, PHASES, MATRIX, FIXTURES, *IMMUTABLE)


def copy_fixture(target: Path) -> None:
    for relative in FILES:
        destination = target / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, destination)


def run(root: Path) -> subprocess.CompletedProcess[str]:
    command = (
        f"{sys.executable} {CHECKER} && "
        f"{sys.executable} {MODEL}"
    )
    return subprocess.run(
        ["/bin/bash", "-c", command],
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )


def repin(root: Path, relative: Path, old_hash: str) -> None:
    checker = root / CHECKER
    text = checker.read_text()
    if old_hash not in text:
        raise RuntimeError(f"checker hash anchor missing for {relative}: {old_hash}")
    new_hash = hashlib.sha256((root / relative).read_bytes()).hexdigest()
    checker.write_text(text.replace(old_hash, new_hash, 1))


def mutate_json(root: Path, relative: Path, change: Callable[[dict[str, Any]], None]) -> None:
    path = root / relative
    old_hash = hashlib.sha256(path.read_bytes()).hexdigest()
    value = json.loads(path.read_text())
    change(value)
    path.write_text(json.dumps(value, indent=2) + "\n")
    repin(root, relative, old_hash)


def mutate_csv(root: Path, relative: Path, change: Callable[[list[dict[str, str]]], None]) -> None:
    path = root / relative
    old_hash = hashlib.sha256(path.read_bytes()).hexdigest()
    with path.open(newline="") as handle:
        reader = csv.DictReader(handle)
        fields = reader.fieldnames
        rows = list(reader)
    if fields is None:
        raise RuntimeError(f"missing CSV header: {relative}")
    change(rows)
    with path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)
    repin(root, relative, old_hash)


def mutate_text(root: Path, relative: Path, old: str, new: str) -> None:
    path = root / relative
    old_hash = hashlib.sha256(path.read_bytes()).hexdigest()
    text = path.read_text()
    if old not in text:
        raise RuntimeError(f"text mutation anchor missing: {relative}: {old}")
    path.write_text(text.replace(old, new, 1))
    repin(root, relative, old_hash)


def create_early_source(root: Path) -> None:
    path = root / "crates/runtime-durable-service/src/stage8b_p1e_schedule_source.rs"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("// unauthorized production source before design acceptance\n")


def set_fixture_expected(value: dict[str, Any]) -> None:
    value["progression_cases"][1]["expected"] = "blocked-unexplained-gap"


def remove_row(rows: list[dict[str, str]]) -> None:
    rows.pop()


def add_wrong_supersedes(rows: list[dict[str, str]]) -> None:
    rows[0]["supersedes"] = "I1A-A01"


def weaken_phase(rows: list[dict[str, str]]) -> None:
    rows[3]["forbidden_effects"] = rows[3]["forbidden_effects"].replace("|business_xack", "")


def main() -> None:
    cases: list[tuple[str, Callable[[Path], None]]] = [
        ("publisher-command-without-nomkstream", lambda root: mutate_json(root, POLICY, lambda value: value["transport"].update(publisher_command="XADD <stream> MAXLEN = 4096 * payload <canonical-signed-envelope>"))),
        ("publisher-create-opened", lambda root: mutate_json(root, POLICY, lambda value: value["transport"].update(publisher_may_create_stream=True))),
        ("type-treated-as-write-fence", lambda root: mutate_json(root, POLICY, lambda value: value["transport"].update(publisher_preflight_type_is_not_write_fence=False))),
        ("snapshot-becomes-event-log", lambda root: mutate_json(root, POLICY, lambda value: value["progression"].update(semantics="authenticated-continuous-event-log"))),
        ("ordinary-cadence-requires-continuity", lambda root: mutate_json(root, POLICY, lambda value: value["progression"].update(ordinary_m10_cadence="requires-every-publication"))),
        ("invalid-newest-fallback-opened", lambda root: mutate_json(root, POLICY, lambda value: value["progression"].update(invalid_newest_fallback=True))),
        ("per-publication-durable-commit", lambda root: mutate_json(root, POLICY, lambda value: value["progression"].update(durable_commit_each_publication=True))),
        ("market-closed-authorized", lambda root: mutate_json(root, POLICY, lambda value: value["route_evidence"]["market_execution"].update(closed_state_authorizes=True))),
        ("working-closed-authorized", lambda root: mutate_json(root, POLICY, lambda value: value["route_evidence"]["working_limit_evaluation"].update(closed_state_authorizes=True))),
        ("day-open-state", lambda root: mutate_json(root, POLICY, lambda value: value["route_evidence"]["day_expiry"].update(required_stage4_state="open"))),
        ("day-last-m10-proof-removed", lambda root: mutate_json(root, POLICY, lambda value: value["route_evidence"]["day_expiry"].update(requires_last_eligible_m10_evaluated=False))),
        ("v4-wrong-sequence-rule", lambda root: mutate_json(root, POLICY, lambda value: value["durable_binding"].update(journal_sequence="any-next-sequence"))),
        ("v4-wrong-seal-generation", lambda root: mutate_json(root, POLICY, lambda value: value["durable_binding"].update(covering_seal_generation="any-new-generation"))),
        ("v4-authority-before-reread", lambda root: mutate_json(root, POLICY, lambda value: value["durable_binding"].__setitem__("authority_before_covering_seal_and-reread", True))),
        ("v4-business-terminal", lambda root: mutate_json(root, POLICY, lambda value: value["durable_binding"].update(binding_seal_is_business_terminal_boundary=True))),
        ("v4-gains-xack", lambda root: mutate_json(root, POLICY, lambda value: value["durable_binding"].update(source_m10_xack_effect="xack"))),
        ("binding-consumes-two-sequences", lambda root: mutate_json(root, POLICY, lambda value: value["binding_counter_effects"].update(stage6_lifecycle_sequence_increment=2))),
        ("binding-increments-m10-xack", lambda root: mutate_json(root, POLICY, lambda value: value["binding_counter_effects"].update(m10_xack_count_increment=1))),
        ("source-latch-removed", lambda root: mutate_json(root, POLICY, lambda value: value["timer_precedence"].update(latch_after_source_before_reclassification=False))),
        ("timer-steps-collapsed", lambda root: mutate_json(root, POLICY, lambda value: value["timer_precedence"].update(timer_execution_same_step_as_reclassification_or_binding=True))),
        ("envelope-progression-field-removed", lambda root: mutate_json(root, ENVELOPE, lambda value: value["required"].remove("semantic_revision"))),
        ("envelope-day-becomes-open", lambda root: mutate_json(root, ENVELOPE, lambda value: value["$defs"]["dayBoundaryEvidence"]["allOf"][1]["properties"]["schedule_state"].update(const="open"))),
        ("binding-sequence-constraint-removed", lambda root: mutate_json(root, BINDING, lambda value: value["x_semantic_constraints"].remove("lifecycle_sequence equals prior journal lifecycle sequence plus one"))),
        ("progression-jump-rejected", lambda root: mutate_json(root, PROGRESSION, lambda value: value["rules"][4].update(decision="blocked-unexplained-gap"))),
        ("restart-before-binding-allows-stale", lambda root: mutate_json(root, DAY, lambda value: value.update(restart_before_binding="reuse-stale-closed-source"))),
        ("timer-step1-allows-seal", lambda root: mutate_json(root, TIMER, lambda value: value["owner_loop_steps"][0].update(new_schedule_binding_seal_allowed=True))),
        ("timer-step2-allows-execution", lambda root: mutate_json(root, TIMER, lambda value: value["owner_loop_steps"][1].update(timer_execution_allowed=True))),
        ("stage6-allowlist-incomplete", lambda root: mutate_json(root, SCOPE, lambda value: value["modifiable_rust_paths"].remove("crates/strategy-runtime-core/src/stage6_reconciliation_v2.rs"))),
        ("old-stage6-bytes-opened", lambda root: mutate_json(root, SCOPE, lambda value: value["stage6_v4_contract"].update(old_v1_v2_v3_wire_bytes_change=True))),
        ("binding-phase-early-xack", lambda root: mutate_csv(root, PHASES, weaken_phase)),
        ("correction-row-removed", lambda root: mutate_csv(root, MATRIX, remove_row)),
        ("supersedes-inventory-widened", lambda root: mutate_csv(root, MATRIX, add_wrong_supersedes)),
        ("model-ordinary-jump-expectation-forged", lambda root: mutate_json(root, FIXTURES, set_fixture_expected)),
        ("design-implementation-authorized", lambda root: mutate_json(root, DESIGN, lambda value: value["implementation_authorization"].update(production_stage5e_facade=True))),
        ("markdown-removes-nomkstream", lambda root: mutate_text(root, MARKDOWN, "`NOMKSTREAM` is the atomic no-create fence", "A prior TYPE check is the write fence")),
        ("production-source-before-acceptance", create_early_source),
    ]

    with tempfile.TemporaryDirectory(prefix="stage8b-p1e-i1a-r1-negative-") as directory:
        baseline = Path(directory) / "baseline"
        baseline.mkdir()
        copy_fixture(baseline)
        baseline_result = run(baseline)
        if baseline_result.returncode != 0:
            print(baseline_result.stdout, end="")
            raise SystemExit("stage8b-p1e-i1a-r1-negative-harness: FAIL: baseline did not pass")

    passed = 0
    for name, mutation in cases:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-p1e-i1a-r1-{name}-") as directory:
            root = Path(directory)
            copy_fixture(root)
            mutation(root)
            result = run(root)
            if result.returncode == 0:
                print(result.stdout, end="")
                raise SystemExit(f"stage8b-p1e-i1a-r1-negative-harness: FAIL: mutation passed: {name}")
            print(f"PASS {name}")
            passed += 1

    print(f"stage8b-p1e-i1a-r1-negative-harness: PASS {passed}/{len(cases)}")


if __name__ == "__main__":
    main()
