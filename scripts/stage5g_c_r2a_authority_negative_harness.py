#!/usr/bin/env python3
"""Adversarial mutations for the Stage 5G-c R2-a authority gate."""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage5g_c_r2a_authority_check.py"
STAGE5C = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
STAGE5F = "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs"
INVENTORY = "docs/stage-5/stage5g-lifecycle-entry-inventory.json"
DESCRIPTOR = "docs/stage-5/stage5g-c-source-projection-extension.json"


def replace(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if old not in text:
        raise RuntimeError(f"mutation anchor missing: {relative}: {old}")
    path.write_text(text.replace(old, new, 1))


def mutate_json(root: Path, relative: str, update) -> None:
    path = root / relative
    value = json.loads(path.read_text())
    update(value)
    path.write_text(json.dumps(value, indent=2) + "\n")


def region_body(root: Path, name: str) -> str:
    text = (root / STAGE5C).read_text()
    begin = f"// STAGE5G-C-SOURCE-PROJECTION-BEGIN: {name}\n"
    end = f"// STAGE5G-C-SOURCE-PROJECTION-END: {name}\n"
    return text.split(begin, 1)[1].split(end, 1)[0]


def baseline_rewritten(root: Path) -> None:
    mutate_json(root, DESCRIPTOR, lambda value: value["stage5c_extension"].update(
        accepted_baseline_sha256="0" * 64,
        stripped_sha256="0" * 64,
    ))


def inventory_rewritten(root: Path) -> None:
    mutate_json(root, INVENTORY, lambda value: value["reuse_authorities"][6].update(sha256="0" * 64))


def marker_removed(root: Path) -> None:
    replace(root, STAGE5C, "// STAGE5G-C-SOURCE-PROJECTION-BEGIN: source-projection-types", "// removed")


def code_outside_region(root: Path) -> None:
    path = root / STAGE5C
    path.write_text(path.read_text() + "\nfn stage5g_escape() {}\n")


def region_rehashed_with_mutation(root: Path) -> None:
    replace(root, STAGE5C, "pub request_id: StrategyRequestId,", "pub request_id: StrategyRequestId,\n    pub forged: bool,")
    digest = hashlib.sha256(region_body(root, "source-projection-types").encode()).hexdigest()
    mutate_json(root, DESCRIPTOR, lambda value: next(
        row.update(sha256=digest) for row in value["stage5c_extension"]["regions"]
        if row["name"] == "source-projection-types"
    ))


def projection_public(root: Path) -> None:
    replace(root, STAGE5C, "pub(crate) struct Stage5gSourceIntentProjection", "pub struct Stage5gSourceIntentProjection")


def caller_constructible(root: Path) -> None:
    replace(root, STAGE5C, "pub(crate) enum Stage5gSourceBaseAction", "pub enum Stage5gSourceBaseAction")


def callback_inserted(root: Path) -> None:
    replace(root, STAGE5C, "fn stage5g_source_intent_projections(", "fn stage5g_source_intent_projections(\n    /* apply_stage5c callback */")


def finam_inserted(root: Path) -> None:
    replace(root, STAGE5C, "#[derive(Debug, Clone, Copy, PartialEq, Eq)]", "// FINAM reqwest dispatch\n#[derive(Debug, Clone, Copy, PartialEq, Eq)]")


def second_writer(root: Path) -> None:
    replace(root, STAGE5C, ".collect()\n}", ".collect()\n}\nfn stage5g_source_intent_projections() {}")


def stage5f_modified(root: Path) -> None:
    path = root / STAGE5F
    path.write_text(path.read_text() + "\n// forbidden Stage 5F mutation\n")


CASES = [
    ("baseline-hash-rewritten", baseline_rewritten),
    ("stage5g-a-inventory-rewritten", inventory_rewritten),
    ("marker-removed", marker_removed),
    ("code-outside-region", code_outside_region),
    ("region-rehashed-with-mutation", region_rehashed_with_mutation),
    ("projection-public", projection_public),
    ("projection-caller-constructible", caller_constructible),
    ("callback-inserted", callback_inserted),
    ("finam-token-inserted", finam_inserted),
    ("second-projection-writer", second_writer),
    ("stage5f-frozen-file-modified", stage5f_modified),
]


def main() -> int:
    passed = 0
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-c-r2a-negative-") as raw:
            root = Path(raw) / "repo"
            shutil.copytree(
                ROOT,
                root,
                ignore=shutil.ignore_patterns(".git", "target", "reports", "tmp", "*.log"),
            )
            mutation(root)
            result = subprocess.run(
                ["python3", CHECKER, "--root", str(root)],
                cwd=root,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if result.returncode == 0:
                print(f"FAIL mutation accepted: {name}")
                return 1
            print(f"PASS {name}")
            passed += 1
    print(f"stage5g-c-r2a-authority-negative-harness: PASS {passed}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
