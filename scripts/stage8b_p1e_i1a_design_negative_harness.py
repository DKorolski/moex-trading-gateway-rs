#!/usr/bin/env python3
"""Semantic mutation harness for the Stage 8B-P1-e I1A design gate."""

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
CHECKER = Path("scripts/stage8b_p1e_i1a_design_check.py")
POLICY = Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-policy-v1.json")
SCHEMA = Path("docs/stage-8/stage8b-p1e-i1a-schedule-envelope-v1.schema.json")
DESIGN = Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v1.json")
SCOPE = Path("docs/stage-8/stage8b-p1e-i1a-implementation-scope-v1.json")
MARKDOWN = Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v1.md")
MATRIX = Path("docs/stage-8/stage8b-p1e-i1a-schedule-source-acceptance-matrix-v1.csv")
TRUST = Path("docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json")
FILES = (CHECKER, POLICY, SCHEMA, DESIGN, SCOPE, MARKDOWN, MATRIX, TRUST)


def copy_fixture(target: Path) -> None:
    for relative in FILES:
        destination = target / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, destination)


def run(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(root / CHECKER)],
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )


def replace(path: Path, old: str, new: str) -> None:
    text = path.read_text()
    if old not in text:
        raise RuntimeError(f"mutation anchor missing in {path}: {old}")
    path.write_text(text.replace(old, new, 1))


def repin(root: Path, relative: Path, previous_hash: str | None = None) -> None:
    path = root / relative
    new_hash = hashlib.sha256(path.read_bytes()).hexdigest()
    checker = root / CHECKER
    checker_text = checker.read_text()
    old_hash = previous_hash or hashlib.sha256((ROOT / relative).read_bytes()).hexdigest()
    if old_hash not in checker_text:
        raise RuntimeError(f"checker hash anchor missing for {relative}: {old_hash}")
    checker.write_text(checker_text.replace(old_hash, new_hash, 1))


def mutate_json(root: Path, relative: Path, change: Callable[[dict[str, Any]], None]) -> None:
    path = root / relative
    value = json.loads(path.read_text())
    change(value)
    path.write_text(json.dumps(value, indent=2) + "\n")
    repin(root, relative)


def mutate_text(root: Path, relative: Path, old: str, new: str) -> None:
    path = root / relative
    replace(path, old, new)
    repin(root, relative)


def remove_matrix_row(root: Path, row_id: str) -> None:
    path = root / MATRIX
    with path.open(newline="") as handle:
        reader = csv.DictReader(handle)
        fieldnames = reader.fieldnames
        rows = [row for row in reader if row["id"] != row_id]
    if fieldnames is None or len(rows) != 80:
        raise RuntimeError(f"matrix mutation failed for {row_id}")
    with path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)
    repin(root, MATRIX)


def create_early_implementation(root: Path) -> None:
    path = root / "crates/runtime-durable-service/src/stage8b_p1e_schedule_source.rs"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("// unauthorized pre-acceptance implementation\n")


def main() -> None:
    cases: list[tuple[str, Callable[[Path], None]]] = [
        (
            "supervisor-becomes-producer",
            lambda root: mutate_json(root, POLICY, lambda value: value["producer"].update(manual_or_supervisor_publication_allowed=True)),
        ),
        (
            "finam-write-opened",
            lambda root: mutate_json(root, POLICY, lambda value: value["producer"].update(finam_post_delete_allowed=True)),
        ),
        (
            "redis-becomes-trust-anchor",
            lambda root: mutate_json(root, POLICY, lambda value: value["trust"].update(redis_is_trust_anchor=True)),
        ),
        (
            "generation2-execution-activated",
            lambda root: mutate_json(root, POLICY, lambda value: value["trust"].update(generation_2_execution_activated_by_this_policy=True)),
        ),
        (
            "private-key-install-authorized",
            lambda root: mutate_json(root, POLICY, lambda value: value["trust"].update(operational_private_key_installation_authorized=True)),
        ),
        (
            "schedule-consumer-group-added",
            lambda root: mutate_json(root, POLICY, lambda value: value["transport"].update(consumer_group="schedule-workers")),
        ),
        (
            "supervisor-stream-write-opened",
            lambda root: mutate_json(root, POLICY, lambda value: value["transport"].update(supervisor_may_create_repair_trim_xadd_xdel_xack=True)),
        ),
        (
            "older-row-fallback-opened",
            lambda root: mutate_json(root, POLICY, lambda value: value["transport"].update(invalid_or_untrusted_newest_entry="scan-for-older-valid")),
        ),
        (
            "freshness-ttl-weakened",
            lambda root: mutate_json(root, POLICY, lambda value: value["freshness"].update(transport_envelope_max_age_ms=60_000)),
        ),
        (
            "reserialization-refreshes-source",
            lambda root: mutate_json(root, POLICY, lambda value: value["freshness"].update(reserialization_may_refresh_observation=True)),
        ),
        (
            "unknown-session-type-opened",
            lambda root: mutate_json(root, POLICY, lambda value: value["normalized_schedule"].update(unknown_session_type_allowed=True)),
        ),
        (
            "stage4-live-authorization-opened",
            lambda root: mutate_json(root, POLICY, lambda value: value["stage4_evidence"].update(no_live_authorization=False)),
        ),
        (
            "historical-binding-new-work",
            lambda root: mutate_json(
                root,
                POLICY,
                lambda value: value["durable_replay"].__setitem__(
                    "new_transition_from_expired_or_historical_binding", True
                ),
            ),
        ),
        (
            "raw-accepted-flags-returned",
            lambda root: mutate_json(root, POLICY, lambda value: value["authority_issuers"].update(raw_accepted_flags_returned=True)),
        ),
        (
            "timer-before-source",
            lambda root: mutate_json(root, POLICY, lambda value: value["timer_precedence"].update(source_scan_before_timer=False)),
        ),
        (
            "runtime-live-surface-opened",
            lambda root: mutate_json(root, POLICY, lambda value: value["closed_surfaces"].update(runtime_live=True)),
        ),
        (
            "production-facade-authorized",
            lambda root: mutate_json(root, DESIGN, lambda value: value["implementation_authorization"].update(production_facade=True)),
        ),
        (
            "stage4-independent-expiry-removed",
            lambda root: mutate_json(root, SCHEMA, lambda value: value["$defs"]["stage4Evidence"]["required"].remove("source_expires_at_utc")),
        ),
        (
            "constructor-bypass-not-forbidden",
            lambda root: mutate_json(root, SCOPE, lambda value: value["forbidden_implementation_shortcuts"].remove("public-or-raw authority constructor")),
        ),
        (
            "replay-case-removed",
            lambda root: remove_matrix_row(root, "I1A-R06"),
        ),
        (
            "design-authorization-warning-removed",
            lambda root: mutate_text(root, MARKDOWN, "DESIGN REVIEW CANDIDATE — IMPLEMENTATION NOT AUTHORIZED", "IMPLEMENTATION AUTHORIZED"),
        ),
        (
            "trust-manifest-schedule-key-drift",
            lambda root: mutate_json(root, TRUST, lambda value: value["source_keys"]["schedule"].update(generation=3)),
        ),
        (
            "production-source-added-before-acceptance",
            create_early_implementation,
        ),
    ]

    with tempfile.TemporaryDirectory(prefix="stage8b-p1e-i1a-design-") as temporary:
        base = Path(temporary) / "base"
        copy_fixture(base)
        baseline = run(base)
        if baseline.returncode != 0:
            raise SystemExit(f"baseline checker failed:\n{baseline.stdout}")

        for index, (name, mutate) in enumerate(cases, start=1):
            case = Path(temporary) / f"case-{index:02d}"
            copy_fixture(case)
            mutate(case)
            completed = run(case)
            if completed.returncode == 0:
                raise SystemExit(f"mutation unexpectedly passed: {name}")
            print(f"PASS {name}")

    print(f"stage8b-p1e-i1a-design-negative-harness: ok cases={len(cases)}")


if __name__ == "__main__":
    main()
