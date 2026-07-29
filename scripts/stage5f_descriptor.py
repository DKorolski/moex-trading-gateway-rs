#!/usr/bin/env python3
"""Fail-closed Stage 5F handoff descriptor registry."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path


EXPECTED_REGISTRY = {
    "5F-a-atomic-hybrid-semantics-entry": {
        "inventory": "docs/stage-5/stage5f-a-atomic-hybrid-semantics-entry-inventory.json",
        "plan": "docs/stage-5/5f-a-atomic-hybrid-semantics-entry.md",
        "checker": "scripts/stage5f_atomic_hybrid_semantics_entry_check.py",
    },
}
REGISTRY = EXPECTED_REGISTRY
EXPECTED_REGISTRY_SHA256 = "d6446a15b27b9d3ad8978ce6df00808be8375cd8075dec42ececa7a20dc44821"


def registry_sha256(registry: object) -> str:
    return hashlib.sha256(
        json.dumps(registry, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def descriptor_for_stage(stage: object) -> dict[str, str]:
    if (
        REGISTRY != EXPECTED_REGISTRY
        or registry_sha256(REGISTRY) != EXPECTED_REGISTRY_SHA256
    ):
        raise ValueError("Stage 5F descriptor registry drift")
    if stage not in REGISTRY:
        raise ValueError("unknown active Stage 5F descriptor")
    return {"stage": stage, **REGISTRY[stage]}


def select_stage5f_descriptor(root: Path) -> dict[str, str]:
    active = root / "docs/stage-5/stage5f-active-descriptor.json"
    try:
        payload = json.loads(active.read_text())
    except (FileNotFoundError, json.JSONDecodeError) as exc:
        raise ValueError("missing or invalid active Stage 5F descriptor") from exc
    if set(payload) != {"schema_version", "stage"} or payload.get("schema_version") != 1:
        raise ValueError("active Stage 5F descriptor schema drift")
    selected = descriptor_for_stage(payload.get("stage"))
    inventory = root / selected["inventory"]
    if not inventory.is_file() or json.loads(inventory.read_text()).get("stage") != selected["stage"]:
        raise ValueError("active Stage 5F descriptor/inventory mismatch")
    for key in ("plan", "checker"):
        if not (root / selected[key]).is_file():
            raise ValueError(f"missing active Stage 5F {key}")
    return selected


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    try:
        print(json.dumps(select_stage5f_descriptor(args.root), sort_keys=True))
    except ValueError as exc:
        print(f"stage5f-descriptor: FAIL: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
