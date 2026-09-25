#!/usr/bin/env python3
"""Semantic negative mutations for the Stage 8B-P1-f R1 design."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import shutil
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage8b_p1f_r1_design_check.py"


def load(index: int) -> Any:
    spec = importlib.util.spec_from_file_location(f"stage8b_p1f_r1_case_{index}", CHECKER)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load P1-f R1 checker")
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


def input_digest(root: Path, module: Any) -> str:
    digest = hashlib.sha256()
    for relative in sorted((module.DOCUMENT, module.INVENTORY, module.MATRIX, module.MODELS,
                            module.TARGET, module.STATUS, module.ROADMAP)):
        raw = (root / relative).read_bytes()
        digest.update(relative.encode())
        digest.update(len(raw).to_bytes(8, "big"))
        digest.update(raw)
    return digest.hexdigest()


Case = tuple[str, Callable[[Path, Any], None], str]
CASES: list[Case] = [
    ("config-root-substitution", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["isolation"].update(p1_config_root="/etc/moex-finam-paper")), "isolation.p1_config_root value drift"),
    ("state-root-substitution", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["isolation"].update(p1_state_root="/var/lib/moex-finam-paper")), "isolation.p1_state_root value drift"),
    ("binary-substitution", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["isolation"].update(p1_binary="/usr/local/bin/unreviewed-supervisor")), "isolation.p1_binary value drift"),
    ("evidence-renamed", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["evidence"].update(unrelated_placeholder=v["evidence"].pop("p0_nonprefix_negative_and_legitimate_change_controls"))), "evidence key inventory drift"),
    ("evidence-all-placeholder", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v.update(evidence={f"placeholder_{i}": True for i in range(17)})), "evidence key inventory drift"),
    ("evidence-numeric-bool", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["evidence"].update(target_preflight=1)), "evidence strict boolean drift"),
    ("isolation-extra", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["isolation"].update(alias_path="/tmp")), "isolation key inventory drift"),
    ("isolation-missing", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["isolation"].pop("p1_service_user")), "isolation key inventory drift"),
    ("phase-order", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["phases"].reverse()), "phases[0].id value drift"),
    ("o2-materializer-redis", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["freshness_contract"]["o2_materialization"].update(network_policy="read-only FINAM GET and Redis")), "O2 materializer authority drift"),
    ("o2-bootstrap-network", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["freshness_contract"]["o2_materialization"].update(bootstrap_network_policy="host network")), "O2 bootstrap isolation drift"),
    ("f00-age-301", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["freshness_contract"].update(first_boot_truth_max_age_seconds=301)), "F00 freshness drift"),
    ("schedule-cadence-5001", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["freshness_contract"]["schedule_supply"].update(publication_interval_max_ms=5001)), "schedule cadence drift"),
    ("stale-schedule-xack", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["freshness_contract"]["schedule_supply"].update(missing_or_stale_action="XACK and continue")), "stale schedule action drift"),
    ("v4-fresh-readmission", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["freshness_contract"]["schedule_supply"].update(committed_v4_recovery="perform new fresh schedule admission")), "committed V4 recovery drift"),
    ("artifact-removed", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["artifact_contracts"].pop()), "artifact inventory drift"),
    ("artifact-field-renamed", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["artifact_contracts"][0].update(output=v["artifact_contracts"][0].pop("allowed_outputs"))), "artifact schema drift"),
    ("synthetic-test-constructor", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["synthetic_phase"].update(test_only_constructors_allowed=True)), "synthetic phase.test_only_constructors_allowed value drift"),
    ("o4-observer-merged", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["finam_bars_phase"].update(broker_truth_observer_is_separate_role=False)), "FINAM bars phase.broker_truth_observer_is_separate_role value drift"),
    ("publisher-reset", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["publisher_continuity"].update(o3_to_o4_uses_same_durable_high_water=False)), "publisher continuity.o3_to_o4_uses_same_durable_high_water value drift"),
    ("claim-after-effect", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["operator_authority"].update(durable_claim_before_first_effect=False)), "operator authority.durable_claim_before_first_effect value drift"),
    ("state-expired-removed", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["phase_lifecycle"]["states"].pop()), "phase states length drift"),
    ("second-controller-allowed", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["phase_lifecycle"]["claim"].update(second_controller="allow")), "second controller policy drift"),
    ("deadline-extends", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["phase_lifecycle"]["deadline"].update(restart_extends_deadline=True)), "deadline extension opened"),
    ("ssh-required", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["phase_lifecycle"]["deadline"].update(independent_of_ssh=False)), "local deadline enforcement drift"),
    ("terminal-autorestart", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["phase_lifecycle"]["restart"].update(terminal_state_autorestart=True)), "restart authority.terminal_state_autorestart value drift"),
    ("telemetry-drives-xack", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["phase_lifecycle"]["terminal_action"].update(telemetry_failure="XACK then stop")), "telemetry failure authority drift"),
    ("mutating-db0", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["redis_capabilities"].update(mutating_roles_database=0)), "mutating Redis DB drift"),
    ("wildcard-key-pattern", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["redis_capabilities"].update(mutating_roles_key_pattern="*")), "mutating key pattern drift"),
    ("flushdb-unforbidden", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["redis_capabilities"]["global_forbidden_commands"].remove("FLUSHDB")), "forbidden Redis commands length drift"),
    ("flushdb-granted", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["redis_capabilities"]["roles"][0]["allowed"].append("FLUSHDB")), "Redis roles[0].allowed length drift"),
    ("raw-redis-escape", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["redis_capabilities"].update(enforcement="raw Redis connection allowed")), "Redis enforcement drift"),
    ("whole-db0-equality", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["redis_capabilities"]["p0_protection_evidence"].update(full_db0_byte_equality_required=True)), "P0 protection evidence.full_db0_byte_equality_required value drift"),
    ("p0-negative-removed", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["redis_capabilities"]["p0_protection_evidence"].update(existing_nonprefix_p0_key_mutation_negative_control=False)), "P0 protection evidence.existing_nonprefix_p0_key_mutation_negative_control value drift"),
    ("pel-unbounded", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["redis_capabilities"]["resource_limits"].update(total_pel_fail_stop_threshold=0)), "resource limits.total_pel_fail_stop_threshold value drift"),
    ("resource-flush", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["redis_capabilities"]["resource_limits"].update(limit_action="FLUSHDB")), "resource limits.limit_action value drift"),
    ("restart-field-missing", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["restart_scenarios"][0].pop("checkpoint")), "restart schema drift"),
    ("conflict-xacked", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["restart_scenarios"][4].update(pel_final="source XACKed")), "conflicting duplicate boundary drift"),
    ("model-case-removed", lambda r, m: edit_json(r, m.MODELS, lambda v: v["cases"].pop()), "model fixture bytes drift"),
    ("matrix-optional", lambda r, m: replace(r, m.MATRIX, ",REQUIRED\nP1F-R1-032", ",OPTIONAL\nP1F-R1-032"), "acceptance matrix bytes drift"),
    ("runtime-live-open", lambda r, m: edit_json(r, m.INVENTORY, lambda v: v["closed_surfaces"].update(runtime_live_authorized=True)), "closed surface opened"),
    ("document-activation", lambda r, m: replace(r, m.DOCUMENT, "opens only `P1F-I`", "opens operational activation and `P1F-I`"), "design document fragment missing: opens only `P1F-I`"),
    ("status-operation", lambda r, m: replace(r, m.STATUS, "grants no operational authority", "grants operational authority"), "status opened operation"),
    ("roadmap-auto-escalation", lambda r, m: replace(r, m.ROADMAP, "P1F-I remains closed", "P1F-I automatically opens"), "roadmap hold boundary missing"),
]


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage8b-p1f-r1-design-") as directory:
        baseline = Path(directory) / "baseline"
        shutil.copytree(ROOT, baseline, ignore=shutil.ignore_patterns(
            ".git", "target", "reports", "tmp", ".env", "*.log", "__pycache__"))
        module = load(0)
        module.validate(baseline, verify_lineage=False)
        baseline_digest = input_digest(baseline, module)
        for index, (name, mutate, expected_error) in enumerate(CASES, start=1):
            root = Path(directory) / f"case-{index:02}"
            shutil.copytree(baseline, root)
            module = load(index)
            mutate(root, module)
            if input_digest(root, module) == baseline_digest:
                print(f"FAIL {name}: mutation was a no-op")
                return 1
            try:
                module.validate(root, verify_lineage=False)
            except (module.CheckFailure, module.r0.CheckFailure) as error:
                if expected_error not in str(error):
                    print(f"FAIL {name}: wrong rejection: {error}")
                    return 1
                print(f"PASS {name}")
                continue
            print(f"FAIL {name}: mutation accepted")
            return 1
    print(f"stage8b-p1f-r1-design-negative-harness: PASS {len(CASES)}/{len(CASES)} no_op=0")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
