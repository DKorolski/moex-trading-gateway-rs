#!/usr/bin/env python3
"""Negative mutations for the Stage 5G-g immutable lifecycle artifact."""

from __future__ import annotations

import argparse
import copy
import json
import tempfile
from pathlib import Path

import stage5g_g_matrix_check as checker


def mutate_and_require_rejection(
    root: Path,
    rows: list[dict[str, object]],
    name: str,
    mutation,
) -> None:
    candidate = copy.deepcopy(rows)
    mutation(candidate)
    with tempfile.TemporaryDirectory(prefix="stage5g-g-negative-") as directory:
        artifact = Path(directory) / f"{name}.json"
        artifact.write_text(json.dumps(candidate, sort_keys=True))
        try:
            checker.check(root, artifact)
        except SystemExit:
            print(f"PASS {name}")
            return
    raise SystemExit(f"stage5g-g-negative-harness: FAIL: mutation accepted: {name}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--artifact", type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve()
    rows = json.loads(args.artifact.resolve().read_text())

    cases = [
        ("missing-row", lambda value: value.pop()),
        ("duplicate-row", lambda value: value.append(copy.deepcopy(value[-1]))),
        ("ordering-drift", lambda value: value.__setitem__(slice(0, 2), reversed(value[:2]))),
        (
            "predecessor-drift",
            lambda value: value[0].__setitem__("accepted_predecessor", "0" * 40),
        ),
        (
            "closed-surface-opened",
            lambda value: value[0]["closed_surfaces"].__setitem__("real_orders_attached", True),
        ),
        (
            "bad-row-fingerprint",
            lambda value: value[0].__setitem__("canonical_row_fingerprint_sha256", "00"),
        ),
        (
            "witness-fabricated-runtime",
            lambda value: next(
                row for row in value if row["evidence_kind"] == "executable_accepted_witness"
            ).__setitem__("pre_runtime_fingerprint_sha256", "a" * 64),
        ),
        (
            "ack-evidence-downgrade",
            lambda value: next(row for row in value if row["family"] == "ACK").__setitem__(
                "evidence_kind", "executable_accepted_witness"
            ),
        ),
        (
            "protective-evidence-downgrade",
            lambda value: next(
                row for row in value if row["family"] == "PROTECTIVE"
            ).__setitem__("evidence_kind", "source_produced_lifecycle_artifact"),
        ),
        (
            "bad-lifecycle-fingerprint",
            lambda value: value[0].__setitem__(
                "lifecycle_checkpoint_fingerprint_sha256", "b" * 63
            ),
        ),
    ]
    for name, mutation in cases:
        mutate_and_require_rejection(root, rows, name, mutation)
    print(f"stage5g-g-negative-harness: PASS {len(cases)}/{len(cases)}")


if __name__ == "__main__":
    main()
