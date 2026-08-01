#!/usr/bin/env python3
"""Negative mutation matrix for the Stage 5F-d atomic Hybrid contract."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import shutil
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER_PATH = ROOT / "scripts/stage5f_d_atomic_matrix_check.py"


def load_checker(case_index: int) -> Any:
    spec = importlib.util.spec_from_file_location(
        f"stage5f_d_atomic_matrix_check_case_{case_index}", CHECKER_PATH
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load Stage 5F-d checker")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


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


def digest(root: Path, relative: str) -> str:
    return hashlib.sha256((root / relative).read_bytes()).hexdigest()


def result(payload: dict[str, Any], row_id: str) -> dict[str, Any]:
    return next(row for row in payload["results"] if row["row_id"] == row_id)


def scenario(payload: dict[str, Any], row_id: str) -> dict[str, Any]:
    return next(row for row in payload["records"] if row["row_id"] == row_id)


def inventory_row(payload: dict[str, Any], row_id: str) -> dict[str, Any]:
    return next(row for row in payload["rows"] if row["row_id"] == row_id)


def replace_text(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise RuntimeError(f"mutation anchor cardinality drift: {relative}: {old!r}")
    path.write_text(text.replace(old, new), encoding="utf-8")


def copy_contract(module: Any, destination: Path) -> None:
    files = set(module.EXPECTED_HASHES)
    files.update(
        {
            module.INVENTORY,
            module.R3_CANDIDATE,
            module.R3_SNAPSHOT_GATE,
            module.FUNCTIONAL_GATE,
            module.NEGATIVE_HARNESS,
            "Cargo.lock",
        }
    )
    for relative in files:
        source = ROOT / relative
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def rebind_golden(module: Any, root: Path, *, results_changed: bool = False) -> None:
    golden = read_json(root, module.GOLDEN)
    if results_changed:
        encoded = (json.dumps(golden["results"], indent=2, ensure_ascii=False) + "\n").encode()
        result_hash = hashlib.sha256(encoded).hexdigest()
        golden["generation"]["results_array_sha256"] = result_hash
        module.RESULTS_ARRAY_SHA256 = result_hash
    write_json(root, module.GOLDEN, golden)
    golden_hash = digest(root, module.GOLDEN)
    module.EXPECTED_HASHES[module.GOLDEN] = golden_hash
    inventory = read_json(root, module.INVENTORY)
    inventory["authorities"]["golden_results"]["sha256"] = golden_hash
    write_json(root, module.INVENTORY, inventory)


def rebind_scenario(module: Any, root: Path) -> None:
    scenario_hash = digest(root, module.SCENARIOS)
    module.EXPECTED_HASHES[module.SCENARIOS] = scenario_hash
    golden = read_json(root, module.GOLDEN)
    golden["inputs"]["scenario_catalog_sha256"] = scenario_hash
    write_json(root, module.GOLDEN, golden)
    inventory = read_json(root, module.INVENTORY)
    inventory["authorities"]["scenario_catalog"]["sha256"] = scenario_hash
    write_json(root, module.INVENTORY, inventory)
    rebind_golden(module, root)


def rebind_source(module: Any, root: Path, relative: str) -> None:
    source_hash = digest(root, relative)
    module.EXPECTED_HASHES[relative] = source_hash
    inventory = read_json(root, module.INVENTORY)
    inventory["source_bindings"][relative] = source_hash
    write_json(root, module.INVENTORY, inventory)


def sync_inventory_result(module: Any, root: Path, row_id: str) -> None:
    golden = read_json(root, module.GOLDEN)
    row = result(golden, row_id)
    inventory = read_json(root, module.INVENTORY)
    owned = inventory_row(inventory, row_id)
    for key in (
        "disposition",
        "callback_count",
        "observer_count",
        "settlement_attempt_count",
        "pre_state_fingerprint",
        "accepted_post_state_fingerprint",
        "ordered_intent_vector_sha256",
        "b3f_outcome",
        "settlement_identity_sha256",
    ):
        owned[key] = row[key]
    owned["intent_count"] = len(row["ordered_intent_vector"])
    write_json(root, module.INVENTORY, inventory)


def mutate_scenario(
    module: Any, root: Path, row_id: str, callback: Callable[[dict[str, Any]], None], *, rebind: bool = False
) -> None:
    payload = read_json(root, module.SCENARIOS)
    callback(scenario(payload, row_id))
    write_json(root, module.SCENARIOS, payload)
    if rebind:
        rebind_scenario(module, root)


def mutate_golden(
    module: Any,
    root: Path,
    row_id: str,
    callback: Callable[[dict[str, Any]], None],
    *,
    full_rebind: bool = False,
) -> None:
    payload = read_json(root, module.GOLDEN)
    callback(result(payload, row_id))
    write_json(root, module.GOLDEN, payload)
    if full_rebind:
        sync_inventory_result(module, root, row_id)
        rebind_golden(module, root, results_changed=True)


def duplicate_json_key(module: Any, root: Path) -> None:
    path = root / module.GOLDEN
    text = path.read_text(encoding="utf-8")
    path.write_text(text.replace('  "schema_version": 2,', '  "schema_version": 2,\n  "schema_version": 2,', 1))
    module.EXPECTED_HASHES[module.GOLDEN] = digest(root, module.GOLDEN)


def missing_result(module: Any, root: Path) -> None:
    payload = read_json(root, module.GOLDEN)
    payload["results"].pop()
    write_json(root, module.GOLDEN, payload)


def extra_result(module: Any, root: Path) -> None:
    payload = read_json(root, module.GOLDEN)
    row = dict(payload["results"][-1])
    row["row_id"] = "F35"
    payload["results"].append(row)
    write_json(root, module.GOLDEN, payload)


def duplicate_result(module: Any, root: Path) -> None:
    payload = read_json(root, module.GOLDEN)
    payload["results"][1] = dict(payload["results"][0])
    write_json(root, module.GOLDEN, payload)


def terminal_reclassified(module: Any, root: Path) -> None:
    def mutate(row: dict[str, Any]) -> None:
        row.update(
            {
                "disposition": "accepted",
                "observer_count": 1,
                "accepted_post_state_fingerprint": "a" * 64,
                "ordered_intent_vector_sha256": hashlib.sha256(
                    b"moex.stage5f.ordered-intent-vector.v1\0[]"
                ).hexdigest(),
                "b3f_outcome": "settled",
                "settlement_identity_sha256": "b" * 64,
            }
        )

    mutate_golden(module, root, "F31", mutate, full_rebind=True)
    golden = read_json(root, module.GOLDEN)
    golden["matrix"]["dispositions"] = {
        "accepted": 27,
        "structural_invariant": 1,
        "blocked_before_callback": 3,
        "terminal_after_callback": 3,
    }
    write_json(root, module.GOLDEN, golden)
    inventory = read_json(root, module.INVENTORY)
    inventory["summary"].update({"accepted": 27, "terminal_after_callback": 3})
    write_json(root, module.INVENTORY, inventory)
    rebind_golden(module, root, results_changed=True)


def normal_append_enforced(module: Any, root: Path) -> None:
    payload = read_json(root, module.RISKGATE)
    normal = next(seed for seed in payload["seeds"] if seed["seed_id"] == "valid_normal_append")
    normal["enforced_for_entry"] = True
    write_json(root, module.RISKGATE, payload)
    module.EXPECTED_HASHES[module.RISKGATE] = digest(root, module.RISKGATE)


def state_fixture_tamper(module: Any, root: Path) -> None:
    payload = read_json(root, module.STATES)
    payload["seeds"][0]["position_qty"] = "1.0"
    write_json(root, module.STATES, payload)


Mutation = Callable[[Any, Path], None]


CASES: list[tuple[str, Mutation]] = [
    ("wrong-instrument-context-rebound", lambda m, r: mutate_scenario(m, r, "F01", lambda x: x["target"]["instrument"].__setitem__("symbol", "RI"), rebind=True)),
    ("wrong-account-context-rebound", lambda m, r: mutate_scenario(m, r, "F01", lambda x: x["target"].__setitem__("account_id", "ACC_OTHER"), rebind=True)),
    ("non-final-bar-rebound", lambda m, r: mutate_scenario(m, r, "F01", lambda x: x["bar"].__setitem__("is_final", False), rebind=True)),
    ("non-m10-bar-rebound", lambda m, r: mutate_scenario(m, r, "F01", lambda x: x["bar"].__setitem__("timeframe_sec", 60), rebind=True)),
    ("non-live-origin-rebound", lambda m, r: mutate_scenario(m, r, "F01", lambda x: x["bar"].__setitem__("origin", "Historical"), rebind=True)),
    ("duplicate-golden-json-key", duplicate_json_key),
    ("unknown-disposition", lambda m, r: mutate_golden(m, r, "F01", lambda x: x.__setitem__("disposition", "unknown"))),
    ("missing-result-row", missing_result),
    ("extra-result-row", extra_result),
    ("duplicate-result-row", duplicate_result),
    ("missing-scenario-row", lambda m, r: (lambda p: (p["records"].pop(), write_json(r, m.SCENARIOS, p)))(read_json(r, m.SCENARIOS))),
    ("duplicate-group-rebound", lambda m, r: mutate_scenario(m, r, "F02", lambda x: x.__setitem__("group_id", "G01_NO_SIGNAL"), rebind=True)),
    ("state-fixture-tamper", state_fixture_tamper),
    ("pre-fingerprint-tamper", lambda m, r: mutate_golden(m, r, "F02", lambda x: x.__setitem__("pre_state_fingerprint", "a" * 64), full_rebind=True)),
    ("post-fingerprint-tamper", lambda m, r: mutate_golden(m, r, "F08", lambda x: x.__setitem__("accepted_post_state_fingerprint", "b" * 64))),
    ("intent-side-tamper-rebound", lambda m, r: mutate_golden(m, r, "F02", lambda x: x["ordered_intent_vector"][0].__setitem__("side", "sell"), full_rebind=True)),
    ("intent-owner-tamper-rebound", lambda m, r: mutate_golden(m, r, "F08", lambda x: x["ordered_intent_vector"][0].__setitem__("owner", "BO"), full_rebind=True)),
    ("request-id-invalid-rebound", lambda m, r: mutate_golden(m, r, "F08", lambda x: x["ordered_intent_vector"][0].__setitem__("settled_strategy_request_id", "not-a-uuid"), full_rebind=True)),
    ("intent-count-partial-acceptance", lambda m, r: mutate_golden(m, r, "F20", lambda x: x.__setitem__("ordered_intent_vector", []), full_rebind=True)),
    ("blocked-row-callback-count", lambda m, r: mutate_golden(m, r, "F24", lambda x: x.__setitem__("callback_count", 1), full_rebind=True)),
    ("structural-row-fabricated-callback", lambda m, r: mutate_golden(m, r, "F16", lambda x: x.update({"disposition": "accepted", "callback_count": 1, "observer_count": 1, "settlement_attempt_count": 1}))),
    ("terminal-misclassified-accepted", terminal_reclassified),
    ("normal-append-misclassified-enforced", normal_append_enforced),
    ("closed-redis-surface-opened", lambda m, r: (lambda p: (p["closed_surfaces"].__setitem__("redis", True), write_json(r, m.GOLDEN, p)))(read_json(r, m.GOLDEN))),
    ("stage5g-lifecycle-opened", lambda m, r: (lambda p: (p["closed_surfaces"].__setitem__("stage5g", True), write_json(r, m.GOLDEN, p)))(read_json(r, m.GOLDEN))),
    ("second-callback-site-rebound", lambda m, r: (replace_text(r, m.HARNESS, "let callback_count = crate::stage5c_paper_host::stage5e_test_b3e_callback_count();", "let _second = invoke_stage5e_authorized_paper_callback_at(authority, callback_at);\n    let callback_count = crate::stage5c_paper_host::stage5e_test_b3e_callback_count();"), rebind_source(m, r, m.HARNESS))),
    ("second-settlement-site-rebound", lambda m, r: (replace_text(r, m.HARNESS, "let settlement = match validate_and_settle_stage5e_paper_callback_escrow(escrow)", "let _second = validate_and_settle_stage5e_paper_callback_escrow(escrow);\n    let settlement = match validate_and_settle_stage5e_paper_callback_escrow(escrow)"), rebind_source(m, r, m.HARNESS))),
    ("observer-production-visibility", lambda m, r: replace_text(r, m.HARNESS, "pub(crate) struct Stage5fObservationScope", "pub struct Stage5fObservationScope")),
    ("observer-mutates-return-vector", lambda m, r: replace_text(r, m.HARNESS, "observe_exact_on_bar_result(&intents);", "intents.clear();\n    observe_exact_on_bar_result(&intents);")),
    ("observer-before-source-expression", lambda m, r: replace_text(r, m.HARNESS, "    let scope = arm_observer_once();\n    crate::stage5c_paper_host::stage5e_test_reset_b3e_callback_count();", "    observe_exact_on_bar_result(&[]);\n    let scope = arm_observer_once();\n    crate::stage5c_paper_host::stage5e_test_reset_b3e_callback_count();")),
    ("alternate-orchestrator-added", lambda m, r: replace_text(r, m.HARNESS, "fn stage5f_d_results()", "fn alternate_stage5f_orchestrator() {}\n\nfn stage5f_d_results()")),
    ("direct-stage5c-route-added", lambda m, r: replace_text(r, m.HARNESS, "fn stage5f_d_results()", "fn direct_stage5c_route() { let _ = stage5c_paper_host::run(); }\n\nfn stage5f_d_results()")),
    ("f19-paired-proof-removed-rebound", lambda m, r: (replace_text(r, m.HARNESS, "assert!(owner.ordered_intent_vector.is_empty());", "assert_eq!(owner.ordered_intent_vector.len(), 1);"), rebind_source(m, r, m.HARNESS))),
    ("f26-runtime-consumed-proof-removed-rebound", lambda m, r: (replace_text(r, m.HARNESS, "F26 callback must retain the broker-truth working order", "F26 working proof removed"), rebind_source(m, r, m.HARNESS))),
    ("f34-pending-seam-escaped", lambda m, r: replace_text(r, m.STAGE5E, "pub(crate) fn test_clear_public_pending_entry_request", "pub fn test_clear_public_pending_entry_request")),
    ("owning-test-missing-rebound", lambda m, r: mutate_scenario(m, r, "F34", lambda x: x.__setitem__("owning_test", "missing_stage5f_f34_test"), rebind=True)),
    ("r3-snapshot-authority-rebound", lambda m, r: replace_text(r, m.R3_SNAPSHOT_GATE, m.ACCEPTED_R3, "0" * 40)),
    ("r3-snapshot-gate-nonblocking", lambda m, r: replace_text(r, m.FUNCTIONAL_GATE, "bash scripts/stage5f_r3_snapshot_gate.sh", "bash scripts/stage5f_r3_snapshot_gate.sh || true")),
    ("stage5fd-checker-removed-from-gate", lambda m, r: replace_text(r, m.FUNCTIONAL_GATE, '  "$python_bin" scripts/stage5f_d_atomic_matrix_check.py\n', "")),
    ("stage5fd-negative-removed-from-gate", lambda m, r: replace_text(r, m.FUNCTIONAL_GATE, '  "$python_bin" scripts/stage5f_d_atomic_matrix_negative_harness.py\n', "")),
]


def main() -> int:
    for index, (name, mutation) in enumerate(CASES, start=1):
        module = load_checker(index)
        with tempfile.TemporaryDirectory(prefix=f"stage5f-d-negative-{index:02}-") as raw:
            root = Path(raw)
            copy_contract(module, root)
            try:
                mutation(module, root)
                module.check(root, verify_lineage=False)
            except module.CheckFailure:
                print(f"PASS {name}")
                continue
            except Exception as exc:
                print(f"FAIL {name}: mutation error: {exc}", file=sys.stderr)
                return 1
            print(f"FAIL {name}: checker accepted mutation", file=sys.stderr)
            return 1
    print(f"stage5f-d-atomic-matrix-negative-harness: ok cases={len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
