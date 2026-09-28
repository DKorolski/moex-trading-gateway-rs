#!/usr/bin/env python3
"""Fail-closed source checker for Stage 8B-P1-f Ic fixed producers."""

from __future__ import annotations

import csv
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
BASE = "7c481bc60699b514b016e8dffe62eb9ca462a100"
REVIEW_SHA256 = "098a24fc87871968bc6e7b0a77deefffd403bc56d997d21af18e1869750acf7e"
CANDIDATE = "33c82e6b3beff400a187d3d17c8d97e6151c2495"
CORRECTION_REVIEW_SHA256 = "42dee90288167fa9969edd0b5c244ab0445b048841468fccf84e5b4ad9bb3800"
PRODUCER = "crates/finam-gateway/src/stage8b_p1f_fixed_producers.rs"
PUBLISHER = "crates/finam-gateway/src/stage8b_p1e_schedule_publisher.rs"
LIB = "crates/finam-gateway/src/lib.rs"
DOCUMENT = "docs/stage-8/stage8b-p1f-ic-fixed-producers.md"
INVENTORY = "docs/stage-8/stage8b-p1f-ic-fixed-producers.json"
MATRIX = "docs/stage-8/stage8b-p1f-ic-fixed-producers-matrix.csv"
STATUS = "docs/current-status.md"
ROADMAP = "docs/roadmap.md"
CHECKER = "scripts/stage8b_p1f_ic_check.py"
NEGATIVE = "scripts/stage8b_p1f_ic_negative_harness.py"
GATE = "scripts/stage8b_p1f_ic_gate.sh"
SAFETY = "scripts/stage8b_p1f_ic_handoff_safety_check.py"
BUILDER = "scripts/make_stage8b_p1f_ic_handoff.py"
ALLOWED_CHANGES = {
    PRODUCER,
    PUBLISHER,
    LIB,
    DOCUMENT,
    INVENTORY,
    MATRIX,
    STATUS,
    ROADMAP,
    CHECKER,
    NEGATIVE,
    GATE,
    SAFETY,
    BUILDER,
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(root: Path, relative: str) -> dict[str, Any]:
    try:
        value = json.loads(
            (root / relative).read_text(), object_pairs_hook=strict_object
        )
    except (OSError, json.JSONDecodeError) as error:
        raise CheckFailure(f"cannot read {relative}: {error}") from error
    require(type(value) is dict, f"{relative} must be an object")
    return value


def validate_lineage(root: Path) -> None:
    try:
        subprocess.run(
            ["git", "merge-base", "--is-ancestor", BASE, "HEAD"],
            cwd=root,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        changed = set(
            subprocess.check_output(
                ["git", "diff", "--name-only", BASE, "--"], cwd=root, text=True
            ).splitlines()
        )
        changed |= set(
            subprocess.check_output(
                ["git", "ls-files", "--others", "--exclude-standard"],
                cwd=root,
                text=True,
            ).splitlines()
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise CheckFailure(f"cannot verify Ic lineage: {error}") from error
    require(
        changed == ALLOWED_CHANGES,
        f"Ic changed-path drift: {sorted(changed ^ ALLOWED_CHANGES)}",
    )


def validate_inventory(root: Path) -> None:
    value = read_json(root, INVENTORY)
    require(
        set(value)
        == {
            "schema_version",
            "stage",
            "status",
            "accepted_ib_commit",
            "accepted_ib_review_sha256",
            "production_module",
            "o2_contract",
            "schedule_contract",
            "m10_contract",
            "restart_contract",
            "evidence",
            "closed_surfaces",
            "next_after_acceptance",
        },
        "Ic inventory key set drift",
    )
    require(value["schema_version"] == 1 and type(value["schema_version"]) is int, "schema drift")
    require(value["stage"] == "Stage 8B-P1-f Ic fixed producers and retained high-water", "stage drift")
    require(value["status"] == "REVIEW_CANDIDATE_FIXED_PRODUCERS_ONLY", "Ic self-accepted")
    require(value["accepted_ib_commit"] == BASE, "accepted Ib binding drift")
    require(value["accepted_ib_review_sha256"] == REVIEW_SHA256, "accepted review binding drift")
    require(value["production_module"] == PRODUCER, "production module drift")
    require("materialize_o2" in value["o2_contract"] and "no duplicate" in value["o2_contract"], "O2 reuse drift")
    schedule = value["schedule_contract"]
    require(type(schedule) is dict and set(schedule) == {"o3", "o4", "publisher", "signed_schedule_max_age_ms_at_m10"}, "schedule inventory drift")
    require(schedule["signed_schedule_max_age_ms_at_m10"] == 2000, "schedule freshness drift")
    m10 = value["m10_contract"]
    require(type(m10) is dict and m10["source_generation"] == "1", "M10 source generation drift")
    require(m10["publication_phases"] == ["Prepared", "Published"], "M10 phase drift")
    restart = value["restart_contract"]
    require(type(restart) is dict and len(restart) == 7, "restart inventory drift")
    require(restart["same_close_conflict"] == "fail closed" and restart["stale_candidate"] == "fail closed", "conflict policy drift")
    require(
        value["evidence"]
        == {
            "acceptance_scenarios": 24,
            "checker": CHECKER,
            "gate": GATE,
            "negative_harness": NEGATIVE,
            "targeted_rust_tests": 6,
        },
        "evidence inventory drift",
    )
    closed = value["closed_surfaces"]
    require(type(closed) is dict and len(closed) == 8, "closed surface inventory drift")
    require(all(flag is False for flag in closed.values()), "operational surface opened")
    require(
        value["next_after_acceptance"]
        == "P1F-Id fixed Redis roles, resource polling and command audit; P1F-O0 remains closed",
        "next boundary drift",
    )


def validate_matrix(root: Path) -> None:
    try:
        with (root / MATRIX).open(newline="") as handle:
            rows = list(csv.DictReader(handle))
    except OSError as error:
        raise CheckFailure(f"cannot read Ic matrix: {error}") from error
    require(rows and list(rows[0]) == ["id", "area", "requirement", "status"], "matrix header drift")
    require([row["id"] for row in rows] == [f"P1FIC-{index:03}" for index in range(1, 25)], "matrix rows drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "optional Ic row")


def validate_source(root: Path) -> None:
    source = (root / PRODUCER).read_text()
    publisher = (root / PUBLISHER).read_text()
    library = (root / LIB).read_text()
    for fragment in (
        'const M10_STATE_DOMAIN: &str = "moex.stage8b.p1f.fixed-m10-producer-state.v1";',
        "const SCHEDULE_REFRESH_MAX_MS: i64 = 2_000;",
        "const CROSS_SOURCE_SKEW_MAX_MS: i64 = 5_000;",
        "const O3_OBSERVATION_MAX_AGE_MS: i64 = 2_000;",
        "const O4_OBSERVATION_MAX_AGE_MS: i64 = 30_000;",
        "const M10_TIMEFRAME_SECONDS: u32 = 600;",
        'self.source_generation != "1"',
        "adapt_stage8b_p1e_readonly_schedule(input)?",
        "Stage8bP1eSchedulePublisherLineage::Resume(retained)",
        "batch.observations.len() != 10",
        "MarketDataSourceKind::ReadOnlyPoll",
        "MarketDataSourceKind::LiveStream",
        "CanonicalBarAggregator::new(M10_TIMEFRAME_SECONDS)",
        "state.verify_fresh_envelope(identity, trusted_now)",
        "let schedule_envelope_sha256 = sha256_hex(&verified_schedule);",
        "observation.observed_at_utc < previous",
        "nonnegative_age_ms(batch.trusted_now_utc, latest)? > O4_OBSERVATION_MAX_AGE_MS",
        "build_stage8b_p1_canonical_m10(",
        "parse_stage8b_p1_canonical_m10(",
        "Stage8bP1fM10ProducerLineageV1::First(_) =>",
        "batch.phase != Stage8bP1fProducerPhaseV1::O3Synthetic",
        "return Err(Stage8bP1fProducerErrorV1::PreparedPublicationPending);",
        "Stage8bP1fM10PrepareOutcomeV1::ReplayPrepared(prior.clone())",
        "Stage8bP1fM10PrepareOutcomeV1::IdempotentPublished(prior.clone())",
        ".create_new(true)",
        ".mode(0o600)",
        "file.sync_all()?;",
        "File::open(parent)?.sync_all()?;",
        "let reread = load_stage8b_p1f_m10_producer_state(path)?;",
        "exact_retained_bytes != prepared.exact_canonical_m10_bytes()?",
        "fixed_o3_o4_schedule_uses_one_retained_sequence_and_revision",
        "prepared_and_published_m10_restart_preserve_exact_high_water",
        "o3_to_o4_m10_continuity_rejects_reset_stale_and_conflict",
        "newer_candidate_waits_for_exact_prepared_publication",
        "o4_streaming_receipts_use_completed_m10_freshness",
        "m10_admission_requires_fresh_signed_schedule_authority",
    ):
        require(fragment in source, f"Ic source contract missing: {fragment}")
    for forbidden in (
        "reqwest::",
        "redis::Client",
        "FinamClient::",
        "place_order(",
        "cancel_order(",
        "BrokerCommand::",
    ):
        require(forbidden not in source, f"Ic forbidden operational surface: {forbidden}")
    require(
        "mod stage8b_p1f_fixed_producers;" in library.splitlines(),
        "Ic module not private",
    )
    require("pub use stage8b_p1f_fixed_producers::{" in library, "Ic typed API not exported")
    require("#[cfg(test)]\npub(crate) fn test_prepare_stage8b_p1e_schedule_publication_with_key" in publisher, "test-only publisher seam missing")
    require("verify_stage8b_p1e_schedule_envelope_v3(&envelope, &context)" in publisher, "fresh production verifier missing")
    require("pub(crate) fn test_verify_fresh_envelope_with_key" in publisher, "fresh fixture verifier seam missing")
    require("#[cfg(test)]\npub(crate) mod tests" in publisher, "publisher test fixture visibility drift")


def validate_docs(root: Path) -> None:
    document = (root / DOCUMENT).read_text()
    status = (root / STATUS).read_text()
    roadmap = (root / ROADMAP).read_text()
    for text, name in ((document, "document"), (status, "status"), (roadmap, "roadmap")):
        require("7c481bc60699b514b016e8dffe62eb9ca462a100" in text, f"{name}: accepted Ib ref missing")
        require("P1F-Ic" in text and "P1F-Id" in text and "P1F-Ie" in text, f"{name}: source sequence missing")
        require("P1F-O0" in text and "closed" in text.lower(), f"{name}: operational boundary missing")
    require("REVIEW_CANDIDATE_FIXED_PRODUCERS_ONLY" in document, "candidate status missing")
    require("does not create a\nsecond first-boot source path" in document, "O2 non-duplication boundary missing")
    require(
        "The active source candidate is P1F-Ic fixed producers and retained high-water."
        in status,
        "current-status active Ic boundary missing",
    )
    require(
        "P1F-Ic is the active source\ncandidate" in roadmap,
        "roadmap active Ic boundary missing",
    )


def validate(root: Path, *, check_lineage: bool = True) -> None:
    if check_lineage:
        validate_lineage(root)
    validate_inventory(root)
    validate_matrix(root)
    validate_source(root)
    validate_docs(root)


def main() -> None:
    try:
        validate(ROOT)
    except (CheckFailure, OSError, UnicodeDecodeError, KeyError, TypeError) as error:
        print(f"stage8b-p1f-ic-check: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1f-ic-check scenarios=24 operational=false")


if __name__ == "__main__":
    main()
