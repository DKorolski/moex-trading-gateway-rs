#!/usr/bin/env python3
"""Fail-closed Stage 5E handoff descriptor registry."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path


EXPECTED_REGISTRY = {
    "5E-a-lifecycle-event-time-attachment-plan": {
        "inventory": "docs/stage-5/stage5e-lifecycle-event-time-attachment-inventory.json",
        "plan": "docs/stage-5/5e-a-lifecycle-event-time-attachment-plan.md",
        "checker": "scripts/stage5e_lifecycle_event_time_freeze_check.py",
    },
    "5E-b-no-io-lifecycle-capability": {
        "inventory": "docs/stage-5/stage5e-b-no-io-lifecycle-inventory.json",
        "plan": "docs/stage-5/5e-b-no-io-lifecycle-capability-plan.md",
        "checker": "scripts/stage5e_b_no_io_lifecycle_check.py",
    },
    "5E-b3-schedule-window-evidence": {
        "inventory": "docs/stage-5/stage5e-b3-schedule-window-evidence-inventory.json",
        "plan": "docs/stage-5/5e-b3-schedule-window-evidence-plan.md",
        "checker": "scripts/stage5e_b3_schedule_window_evidence_check.py",
    },
    "5E-b3c-private-eligibility-seam": {
        "inventory": "docs/stage-5/stage5e-b3c-private-eligibility-seam-inventory.json",
        "plan": "docs/stage-5/5e-b3c-private-eligibility-seam-plan.md",
        "checker": "scripts/stage5e_b3c_private_eligibility_seam_check.py",
    },
    "5E-b3c-source-authority-freeze-extension": {
        "inventory": "docs/stage-5/stage5e-b3c-source-authority-freeze-extension-inventory.json",
        "plan": "docs/stage-5/5e-b3c-source-authority-freeze-extension-plan.md",
        "checker": "scripts/stage5e_b3c_source_authority_freeze_extension_check.py",
    },
    "5E-b3d-callback-authority-design": {
        "inventory": "docs/stage-5/stage5e-b3d-callback-authority-design-inventory.json",
        "plan": "docs/stage-5/5e-b3d-callback-authority-design.md",
        "checker": "scripts/stage5e_b3d_callback_authority_design_check.py",
    },
    "5E-b3e-callback-invocation-design": {
        "inventory": "docs/stage-5/stage5e-b3e-callback-invocation-design-inventory.json",
        "plan": "docs/stage-5/5e-b3e-callback-invocation-design.md",
        "checker": "scripts/stage5e_b3e_callback_invocation_design_check.py",
    },
    "5E-b3f-callback-settlement-escrow-design": {
        "inventory": "docs/stage-5/stage5e-b3f-callback-settlement-escrow-design-inventory.json",
        "plan": "docs/stage-5/5e-b3f-callback-settlement-escrow-design.md",
        "checker": "scripts/stage5e_b3f_callback_settlement_escrow_design_check.py",
    },
}
REGISTRY = EXPECTED_REGISTRY
EXPECTED_REGISTRY_SHA256 = "6573ba440917d09e288da8435ffa57393f8dd37bb447ac23588b7201599f483a"


def registry_sha256(registry: object) -> str:
    return hashlib.sha256(
        json.dumps(registry, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def descriptor_for_stage(stage: object) -> dict[str, str]:
    if REGISTRY != EXPECTED_REGISTRY or registry_sha256(REGISTRY) != EXPECTED_REGISTRY_SHA256:
        raise ValueError("Stage 5E descriptor registry drift")
    if stage not in REGISTRY:
        raise ValueError("unknown active Stage 5E descriptor")
    return {"stage": stage, **REGISTRY[stage]}


def select_stage5e_descriptor(root: Path) -> dict[str, str]:
    active = root / "docs/stage-5/stage5e-active-descriptor.json"
    try:
        payload = json.loads(active.read_text())
    except (FileNotFoundError, json.JSONDecodeError) as exc:
        raise ValueError("missing or invalid active Stage 5E descriptor") from exc
    if set(payload) != {"schema_version", "stage"} or payload.get("schema_version") != 1:
        raise ValueError("active Stage 5E descriptor schema drift")
    selected = descriptor_for_stage(payload.get("stage"))
    inventory = root / selected["inventory"]
    if not inventory.is_file() or json.loads(inventory.read_text()).get("stage") != selected["stage"]:
        raise ValueError("active Stage 5E descriptor/inventory mismatch")
    for key in ("plan", "checker"):
        if not (root / selected[key]).is_file():
            raise ValueError(f"missing active Stage 5E {key}")
    return selected


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    try:
        print(json.dumps(select_stage5e_descriptor(args.root), sort_keys=True))
    except ValueError as exc:
        print(f"stage5e-descriptor: FAIL: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
