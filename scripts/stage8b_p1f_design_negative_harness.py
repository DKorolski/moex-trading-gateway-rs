#!/usr/bin/env python3
"""Semantic mutations that Stage 8B-P1-f R0 design must reject."""

from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage8b_p1f_design_check.py"


def load(index: int) -> Any:
    spec = importlib.util.spec_from_file_location(f"stage8b_p1f_design_case_{index}", CHECKER)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load P1-f checker")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def edit_json(root: Path, relative: str, mutate: Callable[[dict[str, Any]], None]) -> None:
    path = root / relative
    value = json.loads(path.read_text())
    mutate(value)
    path.write_text(json.dumps(value, indent=2) + "\n")


def replace(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if old not in text:
        raise RuntimeError(f"mutation anchor missing: {old}")
    path.write_text(text.replace(old, new, 1))


CASES: list[tuple[str, Callable[[Path, Any], None]]] = [
    ("duplicate-key", lambda root, module: replace(root, module.INVENTORY, '  "schema_version": 1,', '  "schema_version": 1,\n  "schema_version": 1,')),
    ("predecessor", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["accepted_predecessor"].update(commit="0" * 40))),
    ("target-ip", lambda root, module: edit_json(root, module.TARGET, lambda value: value.update(ipv4="127.0.0.1"))),
    ("host-key", lambda root, module: edit_json(root, module.TARGET, lambda value: value.update(ssh_ed25519_fingerprint="SHA256:forged"))),
    ("db15-not-empty", lambda root, module: edit_json(root, module.TARGET, lambda value: value["redis"].update(p1_database_observed_empty=False))),
    ("db0-write", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["isolation"].update(db0_write_allowed=True))),
    ("non-loopback", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["isolation"].update(non_loopback_redis_allowed=True))),
    ("phase-order", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["phases"].reverse())),
    ("provision-start", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["phases"][2].update(service_start=True))),
    ("phase-escalation", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["operator_authority"].update(automatic_phase_escalation=True))),
    ("unattended", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["operator_authority"].update(unattended_activation=True))),
    ("live-ready", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["session_policy"].update(live_ready_forbidden=False))),
    ("unbounded-synthetic", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["session_policy"].update(synthetic_session_max_minutes=0))),
    ("finam-before-synthetic", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["finam_bars_phase"].update(requires_separate_acceptance_after_synthetic=False))),
    ("finam-post", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["finam_bars_phase"].update(finam_order_http_allowed=True))),
    ("command-consumer", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["finam_bars_phase"].update(command_consumer_allowed=True))),
    ("restart-row-removed", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["restart_scenarios"].pop())),
    ("implementation-open", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["closed_surfaces"].update(p1f_source_implementation_authorized=True))),
    ("runtime-live-open", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["closed_surfaces"].update(runtime_live_authorized=True))),
    ("document-activation", lambda root, module: replace(root, module.DOCUMENT, "opens only P1F-I source", "opens operational activation and P1F-I source")),
    ("status-operation", lambda root, module: replace(root, module.STATUS, "grants no operational authority", "grants operational authority")),
    ("roadmap-escalation", lambda root, module: replace(root, module.ROADMAP, "Each operational transition remains fail closed", "Each operational transition runs automatically")),
    ("matrix-optional", lambda root, module: replace(root, module.MATRIX, ",REQUIRED\nP1F-020", ",OPTIONAL\nP1F-020")),
]


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage8b-p1f-design-") as directory:
        baseline = Path(directory) / "baseline"
        shutil.copytree(
            ROOT,
            baseline,
            ignore=shutil.ignore_patterns(".git", "target", "reports", "tmp", ".env", "*.log", "__pycache__"),
        )
        load(0).validate(baseline, verify_lineage=False)
        for index, (name, mutate) in enumerate(CASES, start=1):
            root = Path(directory) / f"case-{index:02}"
            shutil.copytree(baseline, root)
            module = load(index)
            mutate(root, module)
            try:
                module.validate(root, verify_lineage=False)
            except module.CheckFailure:
                print(f"PASS {name}")
                continue
            print(f"FAIL {name}: mutation accepted")
            return 1
    print(f"stage8b-p1f-design-negative-harness: PASS {len(CASES)}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
