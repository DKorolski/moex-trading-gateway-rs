#!/usr/bin/env python3
"""Mutation harness for the Stage 8B-P1-f Ic source checker."""

from __future__ import annotations

import shutil
import tempfile
from pathlib import Path

import stage8b_p1f_ic_check as check


def replace(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError(f"mutation anchor count for {relative}: {old!r}")
    path.write_text(text.replace(old, new, 1))


CASES = (
    ("self-accept", check.INVENTORY, "REVIEW_CANDIDATE_FIXED_PRODUCERS_ONLY", "ACCEPTED"),
    ("lineage", check.INVENTORY, check.BASE, "0" * 40),
    ("surface-open", check.INVENTORY, '"operational_redis": false', '"operational_redis": true'),
    ("generation", check.PRODUCER, 'self.source_generation != "1"', 'self.source_generation != "2"'),
    ("schedule-age", check.PRODUCER, "const SCHEDULE_REFRESH_MAX_MS: i64 = 2_000;", "const SCHEDULE_REFRESH_MAX_MS: i64 = 20_000;"),
    ("cross-skew", check.PRODUCER, "const CROSS_SOURCE_SKEW_MAX_MS: i64 = 5_000;", "const CROSS_SOURCE_SKEW_MAX_MS: i64 = 50_000;"),
    ("o4-age", check.PRODUCER, "const O4_OBSERVATION_MAX_AGE_MS: i64 = 30_000;", "const O4_OBSERVATION_MAX_AGE_MS: i64 = 300_000;"),
    ("m10-count", check.PRODUCER, "batch.observations.len() != 10", "batch.observations.is_empty()"),
    ("aggregator", check.PRODUCER, "CanonicalBarAggregator::new(M10_TIMEFRAME_SECONDS)", "CanonicalBarAggregator::new(60)"),
    ("o4-resume", check.PRODUCER, "Stage8bP1eSchedulePublisherLineage::Resume(retained)", "lineage"),
    ("o4-first", check.PRODUCER, "batch.phase != Stage8bP1fProducerPhaseV1::O3Synthetic", "false"),
    ("pending-overtake", check.PRODUCER, "return Err(Stage8bP1fProducerErrorV1::PreparedPublicationPending);", "return Err(Stage8bP1fProducerErrorV1::StaleCandidate);"),
    ("prepared-replay", check.PRODUCER, "Stage8bP1fM10PrepareOutcomeV1::ReplayPrepared(prior.clone())", "Stage8bP1fM10PrepareOutcomeV1::Prepared(prior.clone())"),
    ("published-idempotency", check.PRODUCER, "Stage8bP1fM10PrepareOutcomeV1::IdempotentPublished(prior.clone())", "Stage8bP1fM10PrepareOutcomeV1::Prepared(prior.clone())"),
    ("create-new", check.PRODUCER, ".create_new(true)", ".create(true)"),
    ("file-fsync", check.PRODUCER, "file.sync_all()?;", "file.flush()?;"),
    ("parent-fsync", check.PRODUCER, "File::open(parent)?.sync_all()?;", "// parent sync removed"),
    ("reread", check.PRODUCER, "let reread = load_stage8b_p1f_m10_producer_state(path)?;", "let reread = state.clone();"),
    ("exact-publish", check.PRODUCER, "exact_retained_bytes != prepared.exact_canonical_m10_bytes()?", "false"),
    ("matrix-row", check.MATRIX, "P1FIC-024,closed,Installation VPS operational Redis FINAM write provider dispatch runtime-live and real orders remain closed,REQUIRED\n", ""),
    ("status-boundary", check.STATUS, "The active source candidate is P1F-Ic fixed producers and retained high-water.", "The active source candidate is operational P1F-O0."),
    ("roadmap-boundary", check.ROADMAP, "P1F-Ic is the active source\ncandidate", "P1F-O0 is the active source\ncandidate"),
    ("module", check.LIB, "mod stage8b_p1f_fixed_producers;", "pub mod stage8b_p1f_fixed_producers;"),
    ("publisher-seam", check.PUBLISHER, "#[cfg(test)]\npub(crate) fn test_prepare_stage8b_p1e_schedule_publication_with_key", "pub(crate) fn test_prepare_stage8b_p1e_schedule_publication_with_key"),
)


def copy_contract(root: Path) -> None:
    for relative in check.ALLOWED_CHANGES:
        source = check.ROOT / relative
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="stage8b-p1f-ic-negative-") as raw:
        root = Path(raw)
        copy_contract(root)
        check.validate(root, check_lineage=False)
        print("PASS positive-control")
        document = root / check.DOCUMENT
        document.write_text(document.read_text() + "\n")
        check.validate(root, check_lineage=False)
        print("PASS nonsemantic-control")

    for name, relative, old, new in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-p1f-ic-{name}-") as raw:
            root = Path(raw)
            copy_contract(root)
            replace(root, relative, old, new)
            try:
                check.validate(root, check_lineage=False)
            except (check.CheckFailure, OSError, UnicodeDecodeError, KeyError, TypeError):
                print(f"PASS {name}")
            else:
                raise SystemExit(f"FAIL mutation survived: {name}")
    print(f"PASS stage8b-p1f-ic-negative-harness {len(CASES)}/{len(CASES)}")


if __name__ == "__main__":
    main()
