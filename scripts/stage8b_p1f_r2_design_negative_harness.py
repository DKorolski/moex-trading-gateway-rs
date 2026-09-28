#!/usr/bin/env python3
"""Semantic negative mutations for the Stage 8B-P1-f R2 design."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import shutil
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage8b_p1f_r2_design_check.py"


def load(index: int) -> Any:
    spec = importlib.util.spec_from_file_location(f"stage8b_p1f_r2_case_{index}", CHECKER)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load P1-f R2 checker")
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
    relatives = (
        module.DOCUMENT, module.INVENTORY, module.MATRIX, module.MODELS,
        module.TARGET, module.STATUS, module.ROADMAP,
        "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
        "docs/stage-8/stage8b-p1e-atomic-stale-consumer-delete-v1.lua",
    )
    for relative in sorted(relatives):
        raw = (root / relative).read_bytes()
        digest.update(relative.encode())
        digest.update(len(raw).to_bytes(8, "big"))
        digest.update(raw)
    return digest.hexdigest()


def inv(path: Path, module: Any, mutate: Callable[[dict[str, Any]], None]) -> None:
    edit_json(path, module.INVENTORY, mutate)


Case = tuple[str, Callable[[Path, Any], None], str]
CASES: list[Case] = [
    ("config-root-substitution", lambda r, m: inv(r, m, lambda v: v["isolation"].update(p1_config_root="/tmp/p1")), "isolation.p1_config_root value drift"),
    ("mutating-db0", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"].update(mutating_roles_database=0)), "Redis DB/key boundary drift"),
    ("wildcard-key-pattern", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"].update(mutating_roles_key_pattern="*")), "Redis DB/key boundary drift"),
    ("o2-redis-network", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"].update(network_policy="FINAM GET and Redis")), "O2 materialization contract digest drift"),
    ("o2-state-reordered", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"]["materialization_states"].reverse()), "O2 materialization states[0] value drift"),
    ("o2-policy-finalizes-any-field", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"]["signed_policy"].update(only_finalizable_config_field="*")), "O2 signed policy.only_finalizable_config_field value drift"),
    ("o2-policy-binding-removed", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"]["signed_policy"]["binds"].pop()), "O2 signed policy.binds length drift"),
    ("o2-source-path", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"]["identity_chain"][1].update(path="/tmp/source.json")), "O2 source path drift"),
    ("o2-config-path", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"]["identity_chain"][2].update(path="/tmp/config.json")), "O2 config path drift"),
    ("o2-identity-state-missing", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"]["identity_chain"].pop()), "O2 identity chain drift"),
    ("o2-root-0400", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"].update(source_custody="root:root 0400")), "O2 nonroot custody drift"),
    ("o2-bootstrap-without-ready", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"].update(bootstrap_admission="bootstrap immediately")), "O2 admission drift"),
    ("o2-partial-bootstrap", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"].update(new_admission_recovery="bootstrap after source commit")), "O2 partial recovery drift"),
    ("v5-replaced-by-v4", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"].update(historical_v5_recovery="schedule V4 recovery")), "V5 historical recovery drift"),
    ("v5-admin-reads-f00", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["o2_materialization"].update(administrative_v5_recovery="read F00")), "V5 administrative recovery drift"),
    ("f00-age-301", lambda r, m: inv(r, m, lambda v: v["freshness_contract"].update(first_boot_truth_max_age_seconds=301)), "F00 freshness drift"),
    ("schedule-global-precallback", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["schedule_supply"].update(missing_or_stale_action="reject before callback")), "schedule failure disposition drift"),
    ("v4-new-read", lambda r, m: inv(r, m, lambda v: v["freshness_contract"]["schedule_supply"].update(committed_v4_recovery="perform new read")), "V4/V5 recovery conflated"),
    ("artifact-receipt-removed", lambda r, m: inv(r, m, lambda v: v["artifact_contracts"].pop(2)), "artifact inventory drift"),
    ("artifact-source-unreadable", lambda r, m: inv(r, m, lambda v: v["artifact_contracts"][1].update(custody="root 0400")), "first-boot source custody drift"),
    ("artifact-receipt-no-ready", lambda r, m: inv(r, m, lambda v: v["artifact_contracts"][2].update(allowed_outputs="bootstrap token")), "materialized receipt output drift"),
    ("operator-dynamic-config", lambda r, m: inv(r, m, lambda v: v["operator_authority"].update(o2_manifest_binds_policy_template_not_dynamic_source_or_final_config=False)), "operator authority.o2_manifest_binds_policy_template_not_dynamic_source_or_final_config value drift"),
    ("terminal-no-force-kill", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["terminal_action"].update(force_kill_after_grace=False)), "phase lifecycle.terminal_action.force_kill_after_grace value drift"),
    ("clock-rollback-continue", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["deadline"].update(clock_rollback_or_untrusted_time="continue")), "phase lifecycle.deadline.clock_rollback_or_untrusted_time value drift"),
    ("reboot-autostart", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["deadline"].update(reboot_policy="autostart all children")), "phase lifecycle.deadline.reboot_policy value drift"),
    ("deadline-after-effect", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["deadline"].update(starts_at="after first effect")), "phase lifecycle.deadline.starts_at value drift"),
    ("deadline-field-missing", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["deadline"].pop("independent_of_ssh")), "phase lifecycle.deadline key inventory drift"),
    ("deadline-field-extra", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["deadline"].update(extension_seconds=1)), "phase lifecycle.deadline key inventory drift"),
    ("deadline-type-smuggling", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["deadline"].update(shutdown_grace_seconds=True)), "phase lifecycle.deadline.shutdown_grace_seconds type drift"),
    ("terminal-order", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["terminal_action"]["process_order"].reverse()), "phase lifecycle.terminal_action.process_order[0] value drift"),
    ("terminal-xack", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["terminal_action"].update(telemetry_failure="XACK then stop")), "phase lifecycle.terminal_action.telemetry_failure value drift"),
    ("terminal-autorestart", lambda r, m: inv(r, m, lambda v: v["phase_lifecycle"]["restart"].update(terminal_state_autorestart=True)), "phase lifecycle.restart.terminal_state_autorestart value drift"),
    ("script-hash", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["scripts"][1].update(sha256="00" * 32)), "Redis scripts contract digest drift"),
    ("script-key", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["scripts"][2].update(keys="any key")), "Redis scripts contract digest drift"),
    ("script-argv", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["scripts"][2].update(argv="any argv")), "Redis scripts contract digest drift"),
    ("script-nested-command", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["scripts"][2]["nested_commands"].append("FLUSHDB")), "Redis scripts contract digest drift"),
    ("script-effect", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["scripts"][2].update(effect="arbitrary write")), "Redis scripts contract digest drift"),
    ("script-removed", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["scripts"].pop()), "Redis scripts contract digest drift"),
    ("operation-count-one", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["source_operations"][4].update(command="XREVRANGE COUNT 1")), "Redis source_operations contract digest drift"),
    ("operation-wrong-key", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["source_operations"][2].update(command="EVAL on any key")), "Redis source_operations contract digest drift"),
    ("operation-response-loss", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["source_operations"][2].update(response_loss="republish")), "Redis source_operations contract digest drift"),
    ("operation-removed", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["source_operations"].pop()), "Redis source_operations contract digest drift"),
    ("route-start-frontier", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["schedule_route_frontiers"][3].update(start_frontier="before callback")), "Redis schedule_route_frontiers contract digest drift"),
    ("route-xack", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["schedule_route_frontiers"][3].update(failure_disposition="XACK and continue")), "Redis schedule_route_frontiers contract digest drift"),
    ("route-predecessor-effects", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["schedule_route_frontiers"][3].update(allowed_predecessor_effects="none")), "Redis schedule_route_frontiers contract digest drift"),
    ("route-removed", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["schedule_route_frontiers"].pop()), "Redis schedule_route_frontiers contract digest drift"),
    ("feeder-direct-xadd", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["roles"][1]["allowed"].append("XADD any")), "Redis roles contract digest drift"),
    ("supervisor-count-one", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["roles"][4]["allowed"].__setitem__(8, "XREVRANGE market-schedule COUNT 1")), "Redis roles contract digest drift"),
    ("supervisor-arbitrary-eval", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["roles"][4]["allowed"].append("EVAL")), "Redis roles contract digest drift"),
    ("supervisor-no-xlen", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["roles"][4]["allowed"].remove("XLEN canonical-m10")), "Redis roles contract digest drift"),
    ("flushdb-unforbidden", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["global_forbidden_commands"].remove("FLUSHDB")), "forbidden Redis commands length drift"),
    ("whole-db0-equality", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["p0_protection_evidence"].update(full_db0_byte_equality_required=True)), "P0 protection evidence.full_db0_byte_equality_required value drift"),
    ("resource-flush", lambda r, m: inv(r, m, lambda v: v["redis_capabilities"]["resource_limits"].update(limit_action="FLUSHDB")), "resource limits.limit_action value drift"),
    ("model-case-removed", lambda r, m: edit_json(r, m.MODELS, lambda v: v["cases"].pop()), "model fixture bytes drift"),
    ("matrix-optional", lambda r, m: replace(r, m.MATRIX, ",REQUIRED\nP1F-R2-032", ",OPTIONAL\nP1F-R2-032"), "acceptance matrix bytes drift"),
    ("runtime-live-open", lambda r, m: inv(r, m, lambda v: v["closed_surfaces"].update(runtime_live_authorized=True)), "closed surface opened"),
    ("document-activation", lambda r, m: replace(r, m.DOCUMENT, "opens only `P1F-I`", "opens activation and `P1F-I`"), "design document fragment missing: opens only `P1F-I`"),
    ("status-operation", lambda r, m: replace(r, m.STATUS, "grants no operational authority", "grants operational authority"), "status opened operation"),
    ("roadmap-auto-open", lambda r, m: replace(r, m.ROADMAP, "P1F-I remains closed", "P1F-I automatically opens"), "roadmap R2 hold boundary missing"),
    ("production-script-drift", lambda r, m: replace(r, "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs", "STAGE8B_P1_M10_GROUP_MISSING", "STAGE8B_P1_M10_GROUP_DRIFT"), "production Redis script hashes.m10-publication-v1 value drift"),
]


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage8b-p1f-r2-design-") as directory:
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
            except (module.CheckFailure, module.r1.CheckFailure,
                    module.r1.r0.CheckFailure) as error:
                if expected_error not in str(error):
                    print(f"FAIL {name}: wrong rejection: {error}")
                    return 1
                print(f"PASS {name}")
                continue
            print(f"FAIL {name}: mutation accepted")
            return 1
    print(f"stage8b-p1f-r2-design-negative-harness: PASS {len(CASES)}/{len(CASES)} no_op=0")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
