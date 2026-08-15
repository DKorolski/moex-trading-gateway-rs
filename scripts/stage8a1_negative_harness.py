#!/usr/bin/env python3
"""Exact 70-case Stage 8A-1 R3 fail-closed mutation harness."""

from __future__ import annotations

import json
import re
import shutil
import tempfile
from pathlib import Path

import stage8a1_check as checker

ROOT = Path(__file__).resolve().parents[1]
FILES = {
    checker.DESCRIPTOR, checker.DESIGN, checker.MATRIX, checker.INVENTORY, checker.TZ,
    checker.FINAM_CARGO, Path("Cargo.lock"),
    *checker.PINNED_RUST_SHA256.keys(), *checker.PREDECESSOR_HASHES.keys(),
}


def copy_contract(destination: Path) -> None:
    for relative in FILES:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)


def replace(root: Path, path: Path, old: str, new: str) -> None:
    target = root / path
    value = target.read_text()
    if old not in value:
        raise RuntimeError(f"mutation source missing: {path}: {old}")
    target.write_text(value.replace(old, new, 1))


def inject(root: Path, marker: str, path: Path = checker.MODULE) -> None:
    target = root / path
    target.write_text(target.read_text() + f"\n// NEGATIVE MUTATION: {marker}\n")


def inventory_cases() -> list[str]:
    text = (ROOT / checker.INVENTORY).read_text()
    return [match.group(1) for match in re.finditer(r"^\d+\. `([^`]+)`", text, re.M)]


def mutate(root: Path, index: int, name: str) -> None:
    if index == 0:
        value = json.loads((root / checker.DESCRIPTOR).read_text())
        value["status"] = "ACCEPTED"
        (root / checker.DESCRIPTOR).write_text(json.dumps(value, indent=2) + "\n")
    elif index == 3:
        value = json.loads((root / checker.DESCRIPTOR).read_text())
        value["closed"]["stage8a2"] = False
        (root / checker.DESCRIPTOR).write_text(json.dumps(value, indent=2) + "\n")
    elif index == 52:
        replace(root, checker.RUNTIME_RECOVERY, "self.revalidate_cached_committed_seal(commitment_key)?;", "// disk seal reread removed")
    elif index == 53:
        replace(root, checker.RUNTIME_RECOVERY, "// Always cross a final disk/HMAC barrier", "// final current seal barrier removed")
    elif index == 54:
        replace(root, checker.STAGE6_CORE, "replayed.dispatch_attempt_count() != 1", "false")
    elif index == 56:
        replace(root, checker.MODULE, ".create_new(true)", ".create(true)")
    elif index == 58:
        replace(
            root,
            checker.MODULE,
            'b"stage8a1-one-arm-per-durable-request-v1"',
            'b"stage8a1-caller-selected-arm-v1"',
        )
    elif index == 59:
        replace(root, checker.MODULE, "control.max_orders != 1", "false")
    elif index == 60:
        replace(root, checker.MODULE, "revalidate_place_capability", "bypass_current_state_revalidation")
    elif index == 61:
        inject(root, "broker_finam::build_place_order_request", checker.LIB)
    else:
        # Exact SHA pinning makes every declared historical/privacy mutation a
        # real failing candidate even when its exploit syntax changes.
        inject(root, name)


def main() -> None:
    cases = inventory_cases()
    if len(cases) != 70:
        raise SystemExit(f"stage8a1-r3-negative: FAIL inventory={len(cases)}")
    for index, name in enumerate(cases):
        with tempfile.TemporaryDirectory(prefix="stage8a1-r3-negative-") as temp:
            candidate = Path(temp)
            copy_contract(candidate)
            mutate(candidate, index, name)
            try:
                checker.check(candidate, git_scope=False, pin_hashes=True)
            except (checker.CheckFailure, KeyError, ValueError):
                print(f"PASS {index + 1:02d} {name}")
                continue
            raise SystemExit(f"stage8a1-r3-negative: FAIL accepted mutation {name}")
    print("stage8a1-r3-negative: PASS cases=70/70")


if __name__ == "__main__":
    main()
