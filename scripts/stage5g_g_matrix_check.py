#!/usr/bin/env python3
"""Static and artifact checks for the in-progress Stage 5G-g matrix freeze."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

PREDECESSOR = "12af52d23218c67bc15b7b79835790e40834dfbb"
INVENTORY = Path("docs/stage-5/stage5g-lifecycle-entry-inventory.json")
SOURCE = Path("crates/strategy-runtime-core/src/stage5g_lifecycle_freeze.rs")
PROTECTIVE_SOURCE = Path("crates/strategy-runtime-core/src/stage5g_protective_completion.rs")
TIMER_SOURCE = Path("crates/strategy-runtime-core/src/stage5g_timer.rs")


def require(value: bool, message: str) -> None:
    if not value:
        raise SystemExit(f"stage5g-g-matrix-check: FAIL: {message}")


def expected_ids(root: Path) -> list[str]:
    value = json.loads((root / INVENTORY).read_text())
    return [case for family in value["scenario_families"] for case in family["case_ids"]]


def check(root: Path, artifact: Path | None) -> None:
    source = (root / SOURCE).read_text()
    scenario_sources = source + (root / PROTECTIVE_SOURCE).read_text()
    timer_source = (root / TIMER_SOURCE).read_text()
    ids = expected_ids(root)
    require(len(ids) == 54 and len(set(ids)) == 54, "entry inventory is not exact 54")
    require(PREDECESSOR in source, "accepted predecessor is not bound")
    for scenario in ids:
        require(f'"{scenario}"' in scenario_sources, f"scenario binding drift: {scenario}")
    forbidden = ["reqwest", ".post(", ".delete(", "redis::", "tokio::spawn", "std::time::SystemTime"]
    lowered = source.lower()
    for token in forbidden:
        require(token.lower() not in lowered, f"forbidden surface in aggregate source: {token}")
    for token in (
        "std::thread",
        "thread::spawn",
        "tokio::spawn",
        "tokio::time::sleep",
        "std::thread::sleep",
        "Utc::now",
        "redis::",
        "reqwest",
        "Method::POST",
        "Method::DELETE",
        ".post(",
        ".delete(",
    ):
        require(token not in timer_source, f"GTMR08 autonomous/IO surface opened: {token}")

    if artifact is not None:
        rows = json.loads(artifact.read_text())
        require(isinstance(rows, list) and len(rows) == 54, "artifact row count is not 54")
        require([row["scenario_id"] for row in rows] == ids, "artifact ordering drifted")
        for row in rows:
            require(row["accepted_predecessor"] == PREDECESSOR, "row predecessor drifted")
            require(len(row["canonical_row_fingerprint_sha256"]) == 64, "bad row fingerprint")
            require(
                len(row["lifecycle_checkpoint_fingerprint_sha256"]) == 64,
                "bad lifecycle checkpoint fingerprint",
            )
            require(not any(row["closed_surfaces"].values()), "a closed surface opened")
        source_corpus = "\n".join(
            path.read_text(errors="replace")
            for base in (root / "crates", root / "scripts")
            for path in base.rglob("*")
            if path.is_file() and path.suffix in {".rs", ".py", ".sh"}
        )
        witness_rows = [
            row for row in rows if row["evidence_kind"] == "executable_accepted_witness"
        ]
        require(len(witness_rows) == 20, "accepted witness family count drifted")
        for row in witness_rows:
            require(
                row["pre_runtime_fingerprint_sha256"] is None
                and row["post_runtime_fingerprint_sha256"] is None,
                "witness-only row fabricated a runtime fingerprint",
            )
            for witness in row["executable_witnesses"]:
                exists = (root / witness).is_file() if "/" in witness else witness in source_corpus
                require(exists, f"missing executable witness: {witness}")
        lifecycle_rows = [
            row for row in rows if row["evidence_kind"] == "source_produced_lifecycle_artifact"
        ]
        require(len(lifecycle_rows) == 26, "source lifecycle family count drifted")
        require(
            all(row["family"] in {"ACK", "ORDER_POSITION"} for row in lifecycle_rows),
            "unexpected family claimed source lifecycle authority",
        )
        ack = [row for row in rows if row["family"] == "ACK"]
        require(len(ack) == 10, "ACK family count drifted")
        for row in ack:
            require(
                row["evidence_kind"] == "source_produced_lifecycle_artifact",
                "ACK evidence downgraded",
            )
            require(row["pre_runtime_fingerprint_sha256"] is not None, "ACK pre runtime missing")
            require(row["post_runtime_fingerprint_sha256"] is not None, "ACK post runtime missing")
        order_position = [row for row in rows if row["family"] == "ORDER_POSITION"]
        require(len(order_position) == 16, "order/position family count drifted")
        for row in order_position:
            require(
                row["evidence_kind"] == "source_produced_lifecycle_artifact",
                "order/position evidence downgraded",
            )
        require(
            all(row["family"] in {"TIMER", "RESTART"} for row in witness_rows),
            "only frozen timer/restart witnesses may remain witness-only",
        )
        protective = [row for row in rows if row["family"] == "PROTECTIVE"]
        require(len(protective) == 8, "protective family count drifted")
        for row in protective:
            require(row["evidence_kind"] == "source_produced_runtime_artifact", "GPRT evidence downgraded")
            require(row["pre_runtime_fingerprint_sha256"] is not None, "GPRT pre runtime missing")
            require(row["post_runtime_fingerprint_sha256"] is not None, "GPRT post runtime missing")
            require(row["restart_package_fingerprint_sha256"] is not None, "GPRT restart missing")

    print("stage5g-g-matrix-check: PASS")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--artifact", type=Path)
    args = parser.parse_args()
    check(args.root.resolve(), args.artifact.resolve() if args.artifact else None)


if __name__ == "__main__":
    main()
