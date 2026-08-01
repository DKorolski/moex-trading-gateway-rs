#!/usr/bin/env python3
"""Negative mutations for the design-only Stage 5G-a entry contract."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import shutil
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage5g_entry_plan_check.py"


@dataclass(frozen=True)
class Case:
    name: str
    mutate: Callable[[Any, Path], None]


def load_checker(index: int) -> Any:
    spec = importlib.util.spec_from_file_location(f"stage5g_entry_case_{index}", CHECKER)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load Stage 5G entry checker")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_json(root: Path, relative: str) -> dict[str, Any]:
    value = json.loads((root / relative).read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise RuntimeError(f"expected JSON object: {relative}")
    return value


def write_json(root: Path, relative: str, value: object) -> None:
    (root / relative).write_text(
        json.dumps(value, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def rebind_inventory(module: Any, root: Path, payload: dict[str, Any]) -> None:
    write_json(root, module.INVENTORY, payload)
    module.EXPECTED_INVENTORY_SHA256 = digest(root / module.INVENTORY)


def mutate_inventory(
    module: Any,
    root: Path,
    callback: Callable[[dict[str, Any]], None],
) -> None:
    payload = read_json(root, module.INVENTORY)
    callback(payload)
    rebind_inventory(module, root, payload)


def rebind_closure(module: Any, root: Path, payload: dict[str, Any]) -> None:
    write_json(root, module.CLOSURE, payload)
    closure_sha = digest(root / module.CLOSURE)
    module.EXPECTED_CLOSURE_SHA256 = closure_sha
    inventory = read_json(root, module.INVENTORY)
    inventory["predecessor"]["stage5f_closure_descriptor_sha256"] = closure_sha
    for authority in inventory["reuse_authorities"]:
        if authority["id"] == "STAGE5F_CLOSURE":
            authority["sha256"] = closure_sha
    rebind_inventory(module, root, inventory)
    first = module.EXPECTED_AUTHORITIES[0]
    module.EXPECTED_AUTHORITIES = [
        (first[0], first[1], closure_sha, first[3], first[4]),
        *module.EXPECTED_AUTHORITIES[1:],
    ]


def closure_status(module: Any, root: Path) -> None:
    payload = read_json(root, module.CLOSURE)
    payload["status"] = "accepted_but_open"
    rebind_closure(module, root, payload)


def closure_source(module: Any, root: Path) -> None:
    payload = read_json(root, module.CLOSURE)
    payload["accepted_source"]["source_ref"] = "0" * 40
    rebind_closure(module, root, payload)


def closure_archive(module: Any, root: Path) -> None:
    payload = read_json(root, module.CLOSURE)
    payload["accepted_source"]["archive_sha256"] = "0" * 64
    rebind_closure(module, root, payload)


def closure_transition(module: Any, root: Path) -> None:
    payload = read_json(root, module.CLOSURE)
    payload["transition"]["stage5g_review_status"] = "live_authorized"
    rebind_closure(module, root, payload)


def inventory_stage(module: Any, root: Path) -> None:
    mutate_inventory(module, root, lambda value: value.__setitem__("stage", "5G-live"))


def inventory_status(module: Any, root: Path) -> None:
    mutate_inventory(module, root, lambda value: value.__setitem__("status", "accepted"))


def schema_bool(module: Any, root: Path) -> None:
    mutate_inventory(module, root, lambda value: value.__setitem__("schema_version", True))


def predecessor_ref(module: Any, root: Path) -> None:
    mutate_inventory(
        module,
        root,
        lambda value: value["predecessor"].__setitem__(
            "accepted_stage5f_source_ref", "1" * 40
        ),
    )


def predecessor_verdict(module: Any, root: Path) -> None:
    mutate_inventory(
        module,
        root,
        lambda value: value["predecessor"].__setitem__("verdict", "PENDING"),
    )


def target_symbol(module: Any, root: Path) -> None:
    mutate_inventory(
        module, root, lambda value: value["target"].__setitem__("symbol", "RTS-9.26")
    )


def target_live(module: Any, root: Path) -> None:
    def mutate(value: dict[str, Any]) -> None:
        value["target"]["trade_mode"] = "live"
        value["target"]["mock_feedback_only"] = False

    mutate_inventory(module, root, mutate)


def main_touched(module: Any, root: Path) -> None:
    mutate_inventory(
        module, root, lambda value: value["governance"].__setitem__("main_untouched", False)
    )


def push_becomes_release(module: Any, root: Path) -> None:
    mutate_inventory(
        module,
        root,
        lambda value: value["governance"].__setitem__(
            "direct_branch_push_is_release_authority", True
        ),
    )


def deployment_authorized(module: Any, root: Path) -> None:
    mutate_inventory(
        module,
        root,
        lambda value: value["governance"].__setitem__("deployment_authorized", True),
    )


def authority_removed(module: Any, root: Path) -> None:
    mutate_inventory(
        module, root, lambda value: value["reuse_authorities"].pop()
    )


def authority_source_drift(module: Any, root: Path) -> None:
    path = root / "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    path.write_text(path.read_text(encoding="utf-8") + "\n// unauthorized drift\n")


def ownership_removed(module: Any, root: Path) -> None:
    mutate_inventory(module, root, lambda value: value["ownership_rules"].pop())


def substage_unblocked(module: Any, root: Path) -> None:
    def mutate(value: dict[str, Any]) -> None:
        value["sub_stages"][1]["status"] = "implementation_active"

    mutate_inventory(module, root, mutate)


def rust_allowed(module: Any, root: Path) -> None:
    def mutate(value: dict[str, Any]) -> None:
        value["sub_stages"][1]["rust_changes_allowed_before_5g_a_acceptance"] = True

    mutate_inventory(module, root, mutate)


def family_removed(module: Any, root: Path) -> None:
    def mutate(value: dict[str, Any]) -> None:
        value["scenario_families"].pop()
        value["scenario_case_count"] = 46

    mutate_inventory(module, root, mutate)


def case_removed(module: Any, root: Path) -> None:
    def mutate(value: dict[str, Any]) -> None:
        value["scenario_families"][0]["case_ids"].pop()
        value["scenario_case_count"] = 53

    mutate_inventory(module, root, mutate)


def case_duplicate(module: Any, root: Path) -> None:
    def mutate(value: dict[str, Any]) -> None:
        value["scenario_families"][1]["case_ids"][0] = value["scenario_families"][0]["case_ids"][0]

    mutate_inventory(module, root, mutate)


def family_owner(module: Any, root: Path) -> None:
    def mutate(value: dict[str, Any]) -> None:
        value["scenario_families"][4]["owner_stage"] = "5H"

    mutate_inventory(module, root, mutate)


def count_bool(module: Any, root: Path) -> None:
    mutate_inventory(module, root, lambda value: value.__setitem__("scenario_case_count", True))


def gate_removed(module: Any, root: Path) -> None:
    mutate_inventory(module, root, lambda value: value["required_entry_gates"].pop())


def review_checkpoint_removed(module: Any, root: Path) -> None:
    mutate_inventory(module, root, lambda value: value["review_checkpoints"].pop(1))


def live_surface_open(module: Any, root: Path) -> None:
    mutate_inventory(
        module,
        root,
        lambda value: value["closed_surfaces"].__setitem__("real_finam_post", True),
    )


def stage6_open(module: Any, root: Path) -> None:
    mutate_inventory(
        module, root, lambda value: value["next_transition"].__setitem__("stage6_open", True)
    )


def duplicate_json_key(module: Any, root: Path) -> None:
    path = root / module.INVENTORY
    text = path.read_text(encoding="utf-8")
    path.write_text(
        text.replace(
            '  "schema_version": 1,',
            '  "schema_version": 1,\n  "schema_version": 1,',
            1,
        ),
        encoding="utf-8",
    )
    module.EXPECTED_INVENTORY_SHA256 = digest(path)


def plan_boundary_drift(module: Any, root: Path) -> None:
    path = root / module.PLAN
    text = path.read_text(encoding="utf-8")
    old = "- real FINAM `POST` or `DELETE`;"
    if text.count(old) != 1:
        raise RuntimeError("plan boundary anchor drift")
    path.write_text(
        text.replace(old, "- real FINAM `POST` and `DELETE` are allowed;"),
        encoding="utf-8",
    )


CASES = [
    Case("closure-status-reopened", closure_status),
    Case("closure-source-rebound", closure_source),
    Case("closure-archive-rebound", closure_archive),
    Case("closure-transition-live-authorized", closure_transition),
    Case("stage-rebound", inventory_stage),
    Case("self-declared-acceptance", inventory_status),
    Case("schema-bool-smuggling", schema_bool),
    Case("predecessor-ref-rebound", predecessor_ref),
    Case("predecessor-verdict-rebound", predecessor_verdict),
    Case("target-symbol-rebound", target_symbol),
    Case("target-live-mode", target_live),
    Case("main-marked-touched", main_touched),
    Case("branch-push-becomes-release-authority", push_becomes_release),
    Case("deployment-authorized", deployment_authorized),
    Case("reuse-authority-removed", authority_removed),
    Case("frozen-stage5c-source-drift", authority_source_drift),
    Case("ownership-rule-removed", ownership_removed),
    Case("implementation-unblocked-before-review", substage_unblocked),
    Case("rust-changes-allowed-at-entry", rust_allowed),
    Case("scenario-family-removed", family_removed),
    Case("scenario-case-removed", case_removed),
    Case("scenario-case-duplicated", case_duplicate),
    Case("protective-family-owner-rebound", family_owner),
    Case("scenario-count-bool-smuggling", count_bool),
    Case("required-entry-gate-removed", gate_removed),
    Case("review-checkpoint-removed", review_checkpoint_removed),
    Case("real-finam-post-opened", live_surface_open),
    Case("stage6-opened", stage6_open),
    Case("duplicate-json-key", duplicate_json_key),
    Case("plan-boundary-drift", plan_boundary_drift),
]


def copy_baseline(destination: Path) -> None:
    shutil.copytree(
        ROOT,
        destination,
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns(
            ".git",
            "target",
            "reports",
            "tmp",
            ".env",
            "*.log",
            "__pycache__",
            "__MACOSX",
        ),
    )


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage5g-entry-negative-") as temp:
        baseline = Path(temp) / "baseline"
        copy_baseline(baseline)
        baseline_checker = load_checker(0)
        baseline_checker.validate(baseline, verify_lineage=False)
        for index, case in enumerate(CASES, start=1):
            case_root = Path(temp) / f"case-{index:02}"
            shutil.copytree(baseline, case_root)
            checker = load_checker(index)
            case.mutate(checker, case_root)
            try:
                checker.validate(case_root, verify_lineage=False)
            except checker.CheckFailure:
                print(f"PASS {case.name}")
                continue
            print(f"FAIL {case.name}: mutation was accepted")
            return 1
    print(f"stage5g-entry-plan-negative-harness: ok cases={len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
