#!/usr/bin/env python3
"""Mutation matrix for the Stage 5F-c characterization boundary."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import shutil
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER_PATH = ROOT / "scripts/stage5f_controlled_characterization_check.py"


def load_checker(case_index: int):
    spec = importlib.util.spec_from_file_location(
        f"stage5f_controlled_characterization_case_{case_index}", CHECKER_PATH
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load Stage 5F-c checker")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def copy_contract(module: Any, root: Path) -> None:
    files = [
        module.SCENARIOS,
        module.STATES,
        module.RISKGATE,
        module.CORRECTIONS,
        module.CANDIDATE,
        module.HARNESS,
        module.LIB,
        module.CALLBACK,
        module.STAGE5C,
        module.STAGE5E,
        module.INHERITED_B1_GATE,
        module.FUNCTIONAL_GATE,
        module.REPORT,
        module.INVENTORY,
    ]
    for relative in files:
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)


def replace(root: Path, relative: str, old: str, new: str, count: int = 1) -> None:
    path = root / relative
    text = path.read_text()
    if old not in text:
        raise RuntimeError(f"mutation anchor missing in {relative}: {old}")
    path.write_text(text.replace(old, new, count))


def read_json(root: Path, relative: str) -> dict[str, Any]:
    value = json.loads((root / relative).read_text())
    if not isinstance(value, dict):
        raise RuntimeError(f"expected object: {relative}")
    return value


def write_json(root: Path, relative: str, value: object) -> None:
    (root / relative).write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def rebind_results(module: Any, root: Path, payload: dict[str, Any]) -> None:
    encoded = json.dumps(payload["results"], indent=2, ensure_ascii=False).encode() + b"\n"
    digest = hashlib.sha256(encoded).hexdigest()
    payload["generation"]["results_array_sha256"] = digest
    module.RESULTS_ARRAY_SHA256 = digest
    write_json(root, module.CANDIDATE, payload)


def rebind_correction(module: Any, root: Path, payload: dict[str, Any]) -> None:
    write_json(root, module.CORRECTIONS, payload)
    digest = hashlib.sha256((root / module.CORRECTIONS).read_bytes()).hexdigest()
    module.INPUT_HASHES[module.CORRECTIONS] = digest
    candidate = read_json(root, module.CANDIDATE)
    candidate["inputs"]["source_validity_corrections_sha256"] = digest
    write_json(root, module.CANDIDATE, candidate)


Mutation = Callable[[Any, Path], None]


CASES: list[tuple[str, Mutation]] = [
    (
        "observer-module-not-cfg-test",
        lambda m, r: replace(r, m.LIB, "#[cfg(test)]\nmod stage5f_atomic_hybrid_semantics;", "mod stage5f_atomic_hybrid_semantics;"),
    ),
    (
        "observer-module-public",
        lambda m, r: replace(r, m.LIB, "mod stage5f_atomic_hybrid_semantics;", "pub mod stage5f_atomic_hybrid_semantics;"),
    ),
    (
        "observer-call-not-cfg-test",
        lambda m, r: replace(r, m.CALLBACK, "        #[cfg(test)]\n        crate::stage5f_atomic_hybrid_semantics::observe_exact_on_bar_result", "        crate::stage5f_atomic_hybrid_semantics::observe_exact_on_bar_result"),
    ),
    (
        "observer-before-source-expression",
        lambda m, r: replace(
            r,
            m.CALLBACK,
            "        let intents = Strategy::on_bar(self, &context, &bar);\n        #[cfg(test)]\n        crate::stage5f_atomic_hybrid_semantics::observe_exact_on_bar_result(&intents);",
            "        #[cfg(test)]\n        crate::stage5f_atomic_hybrid_semantics::observe_exact_on_bar_result(&[]);\n        let intents = Strategy::on_bar(self, &context, &bar);",
        ),
    ),
    (
        "observer-controls-flow",
        lambda m, r: replace(r, m.CALLBACK, "        #[cfg(test)]\n        crate::stage5f_atomic_hybrid_semantics::observe_exact_on_bar_result(&intents);", "        #[cfg(test)]\n        if intents.is_empty() { crate::stage5f_atomic_hybrid_semantics::observe_exact_on_bar_result(&intents); }"),
    ),
    (
        "callback-return-vector-replaced",
        lambda m, r: replace(r, m.CALLBACK, "        Ok(intents)", "        Ok(Vec::new())"),
    ),
    (
        "stage5c-factory-not-cfg-test",
        lambda m, r: replace(r, m.STAGE5C, "#[cfg(test)]\n#[allow(clippy::too_many_arguments)]\npub(crate) fn stage5f_test_sequence", "#[allow(clippy::too_many_arguments)]\npub(crate) fn stage5f_test_sequence"),
    ),
    (
        "b3c-factory-not-cfg-test",
        lambda m, r: replace(r, m.STAGE5E, "    #[cfg(test)]\n    pub(crate) fn stage5f_test_b3c", "    pub(crate) fn stage5f_test_b3c"),
    ),
    (
        "callback-validation-seam-not-cfg-test",
        lambda m, r: replace(r, m.STAGE5E, "        #[cfg(test)]\n        pub(crate) fn stage5f_test_force_callback", "        pub(crate) fn stage5f_test_force_callback"),
    ),
    (
        "observer-scope-clone",
        lambda m, r: replace(r, m.HARNESS, "pub(crate) struct Stage5fObservationScope", "#[derive(Clone)]\npub(crate) struct Stage5fObservationScope"),
    ),
    (
        "observer-result-debug",
        lambda m, r: replace(r, m.HARNESS, "pub(crate) struct Stage5fObservedIntentVector", "#[derive(Debug)]\npub(crate) struct Stage5fObservedIntentVector"),
    ),
    (
        "observer-result-public",
        lambda m, r: replace(r, m.HARNESS, "pub(crate) struct Stage5fObservedIntentVector", "pub struct Stage5fObservedIntentVector"),
    ),
    (
        "observer-retains-raw-intents",
        lambda m, r: replace(r, m.HARNESS, "projections: Vec<Stage5fIntentProjection>,", "projections: Vec<Stage5fIntentProjection>,\n    raw_intents: Vec<Intent>,"),
    ),
    (
        "observer-second-callback-not-terminal",
        lambda m, r: replace(r, m.HARNESS, 'panic!("Stage 5F observer saw a second callback before consume")', "return"),
    ),
    (
        "observer-consume-borrows-self",
        lambda m, r: replace(r, m.HARNESS, "pub(crate) fn consume_once(mut self)", "pub(crate) fn consume_once(&mut self)"),
    ),
    (
        "second-callback-site",
        lambda m, r: replace(r, m.HARNESS, "let callback_count =", "let _forged = invoke_stage5e_authorized_paper_callback_at(authority, input.callback_at);\n    let callback_count ="),
    ),
    (
        "second-settlement-site",
        lambda m, r: replace(r, m.HARNESS, "let settlement =", "let _forged = validate_and_settle_stage5e_paper_callback_escrow(escrow);\n    let settlement ="),
    ),
    (
        "direct-source-callback-bypass",
        lambda m, r: replace(r, m.HARNESS, "fn characterize", "// Strategy::on_bar(&mut strategy, &ctx, &bar);\nfn characterize"),
    ),
    (
        "alternate-orchestrator",
        lambda m, r: replace(r, m.HARNESS, "fn characterize", "// HybridOrchestrator::new(config);\nfn characterize"),
    ),
    (
        "callback-counter-reset-removed",
        lambda m, r: replace(r, m.HARNESS, "crate::stage5c_paper_host::stage5e_test_reset_b3e_callback_count();", "// removed"),
    ),
    (
        "second-observer-negative-test-removed",
        lambda m, r: replace(r, m.HARNESS, "fn stage5f_observer_rejects_second_callback_before_consume()", "fn forged_removed_second_callback_test()"),
    ),
    (
        "candidate-row-removed",
        lambda m, r: (lambda p: (p["results"].pop(), rebind_results(m, r, p)))(read_json(r, m.CANDIDATE)),
    ),
    (
        "candidate-callback-count-two",
        lambda m, r: (lambda p: (p["results"][0].__setitem__("callback_count", 2), rebind_results(m, r, p)))(read_json(r, m.CANDIDATE)),
    ),
    (
        "terminal-mapped-accepted",
        lambda m, r: (lambda p: (p["results"][4].__setitem__("disposition", "accepted"), rebind_results(m, r, p)))(read_json(r, m.CANDIDATE)),
    ),
    (
        "raw-comment-exported",
        lambda m, r: (lambda p: (p["results"][1]["ordered_intent_vector"][0].__setitem__("comment", "raw"), rebind_results(m, r, p)))(read_json(r, m.CANDIDATE)),
    ),
    (
        "vector-hash-forged",
        lambda m, r: (lambda p: (p["results"][1].__setitem__("ordered_intent_vector_sha256", "0" * 64), rebind_results(m, r, p)))(read_json(r, m.CANDIDATE)),
    ),
    (
        "redis-surface-opened",
        lambda m, r: (lambda p: (p["closed_surfaces"].__setitem__("redis", True), write_json(r, m.CANDIDATE, p)))(read_json(r, m.CANDIDATE)),
    ),
    (
        "correction-extra-row",
        lambda m, r: (lambda p: (p["corrections"].append(dict(p["corrections"][0], row_id="F03")), rebind_correction(m, r, p)))(read_json(r, m.CORRECTIONS)),
    ),
    (
        "f02-correction-reverted",
        lambda m, r: (lambda p: (p["corrections"][0]["overrides"]["bar"].__setitem__("close_time_utc", "2026-01-06T07:10:00Z"), rebind_correction(m, r, p)))(read_json(r, m.CORRECTIONS)),
    ),
    (
        "f04-cycle-correction-reverted",
        lambda m, r: (lambda p: (p["corrections"][1]["overrides"]["state_seed"].__setitem__("active_cycle_id", "bo00000001"), rebind_correction(m, r, p)))(read_json(r, m.CORRECTIONS)),
    ),
    (
        "inherited-scanner-anchor-rebound-to-head",
        lambda m, r: replace(
            r,
            m.INHERITED_B1_GATE,
            'readonly accepted_b1_ref="86b43c448fb65a3c54b6118d04d3f40e08e74ad7"',
            'readonly accepted_b1_ref="HEAD"',
        ),
    ),
    (
        "inherited-scanner-made-nonblocking",
        lambda m, r: replace(
            r,
            m.INHERITED_B1_GATE,
            "  bash scripts/forbidden_surface_scan.sh",
            "  bash scripts/forbidden_surface_scan.sh || true",
        ),
    ),
    (
        "functional-gate-scans-current-tree",
        lambda m, r: replace(
            r,
            m.FUNCTIONAL_GATE,
            "bash scripts/stage5f_inherited_b1_snapshot_gate.sh",
            "bash scripts/forbidden_surface_scan.sh",
        ),
    ),
    (
        "functional-gate-inherited-check-nonblocking",
        lambda m, r: replace(
            r,
            m.FUNCTIONAL_GATE,
            "bash scripts/stage5f_inherited_b1_snapshot_gate.sh",
            "bash scripts/stage5f_inherited_b1_snapshot_gate.sh || true",
        ),
    ),
    (
        "candidate-promoted-to-golden-before-review",
        lambda m, r: (lambda p: (p["evidence"].__setitem__("candidate_is_golden", True), write_json(r, m.INVENTORY, p)))(read_json(r, m.INVENTORY)),
    ),
    (
        "stage5f-d-opened-before-review",
        lambda m, r: (lambda p: (p["next_stage"].__setitem__("allowed_before_independent_review", True), write_json(r, m.INVENTORY, p)))(read_json(r, m.INVENTORY)),
    ),
    (
        "compositional-proof-caveat-removed",
        lambda m, r: replace(
            r,
            m.REPORT,
            "It is a compositional proof, not a claim that",
            "It is asserted without qualification that",
        ),
    ),
]


def main() -> int:
    for index, (name, mutation) in enumerate(CASES, start=1):
        module = load_checker(index)
        with tempfile.TemporaryDirectory(prefix="stage5f-c-negative-") as directory:
            root = Path(directory)
            copy_contract(module, root)
            mutation(module, root)
            try:
                module.run(root, check_lineage=False)
            except module.CheckFailure:
                print(f"PASS {name}")
                continue
            raise RuntimeError(f"negative mutation escaped checker: {name}")
    print(f"stage5f-controlled-characterization-negative-harness: ok cases={len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
