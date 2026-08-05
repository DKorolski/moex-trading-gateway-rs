#!/usr/bin/env python3
"""Whole-source/gate/provenance mutation matrix for Stage 5G-e-d-a R5."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

import stage5g_eda_r3_negative_harness as r3
import stage5g_eda_r4_negative_harness as r4


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage5g_eda_r5_check.py"
SOURCE = r3.SOURCE
LIB = r3.LIB
EXISTING_SIBLING = "crates/strategy-runtime-core/src/stage5g_order_position.rs"
NEW_SIBLING = "crates/strategy-runtime-core/src/stage5g_r5_mutant_reducer.rs"
GATE = "scripts/stage5g_eda_r5_gate.sh"
BUILDER = "scripts/make_stage5g_ed_handoff_archive.py"
R5_FILES = [
    "docs/stage-5/stage5g-e-d-a-r5-runtime-core-source-freeze.json",
    "scripts/stage5g_eda_r5_check.py",
    "scripts/stage5g_eda_r5_gate.sh",
    "scripts/stage5g_eda_r5_negative_harness.py",
    "scripts/stage5g_eda_r5_preseal_check.py",
]
RUST_FILES = [
    path.relative_to(ROOT).as_posix()
    for path in sorted((ROOT / "crates/strategy-runtime-core/src").rglob("*.rs"))
]
FILES = sorted(set(r4.FILES + R5_FILES + RUST_FILES))


def replace_once(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError(
            f"mutation anchor must occur once in {relative}: {old!r}; got {text.count(old)}"
        )
    path.write_text(text.replace(old, new, 1))


def append_text(root: Path, relative: str, value: str) -> None:
    path = root / relative
    path.write_text(path.read_text() + value)


def add_sibling_reducer(root: Path) -> None:
    sibling = root / NEW_SIBLING
    sibling.write_text(
        "use crate::stage5g_fresh_broker_truth::{\n"
        "    Stage5gRestartReconciliationDisposition,\n"
        "    Stage5gValidatedFreshBrokerTruthPackage,\n"
        "};\n\n"
        "#[allow(dead_code)]\n"
        "pub(crate) fn reduce_fresh_truth(\n"
        "    _package: Stage5gValidatedFreshBrokerTruthPackage,\n"
        ") -> Stage5gRestartReconciliationDisposition {\n"
        "    Stage5gRestartReconciliationDisposition::ExactReplay\n"
        "}\n"
    )
    replace_once(
        root,
        LIB,
        "#[allow(dead_code)] // Stage 5G-e-d-a contract is consumed by the reviewed e-d-b reducer.\n"
        "mod stage5g_fresh_broker_truth;",
        "#[allow(dead_code)] // Stage 5G-e-d-a contract is consumed by the reviewed e-d-b reducer.\n"
        "mod stage5g_fresh_broker_truth;\n"
        "#[allow(dead_code)]\nmod stage5g_r5_mutant_reducer;",
    )


def run_case(name: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix=f"stage5g-eda-r5-{name}-") as directory:
        root = Path(directory)
        for relative in FILES:
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        mutation(root)
        result = subprocess.run(
            ["python3", str(CHECKER), "--root", str(root)],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
        if result.returncode == 0:
            raise SystemExit(f"stage5g-eda-r5-negative: FAIL: mutation survived: {name}")
        print(f"PASS {name}")


def r5_mutation_cases() -> list[tuple[str, object]]:
    return [
        ("append-direct-after-test-reducer", lambda root: append_text(
            root, SOURCE,
            "\npub(crate) fn classify_truth_after_tests(\n"
            "    _package: Stage5gValidatedFreshBrokerTruthPackage,\n"
            ") -> Stage5gRestartReconciliationDisposition {\n"
            "    Stage5gRestartReconciliationDisposition::ExactReplay\n"
            "}\n")),
        ("append-alias-after-test-reducer", lambda root: append_text(
            root, SOURCE,
            "\ntype FreshTruthAfterTests = Stage5gValidatedFreshBrokerTruthPackage;\n"
            "type DecisionAfterTests = Stage5gRestartReconciliationDisposition;\n\n"
            "pub(crate) fn decide_after_tests(\n"
            "    _package: FreshTruthAfterTests,\n"
            ") -> DecisionAfterTests {\n"
            "    Stage5gRestartReconciliationDisposition::ExactReplay\n"
            "}\n")),
        ("append-production-item-after-test", lambda root: append_text(
            root, SOURCE, "\npub(crate) fn production_after_tests_marker() -> bool { true }\n")),
        ("add-sibling-reducer-module", lambda root: add_sibling_reducer(root)),
        ("add-reducer-to-existing-sibling", lambda root: append_text(
            root, EXISTING_SIBLING,
            "\n#[allow(dead_code)]\n"
            "pub(crate) fn reduce_fresh_truth_in_sibling(\n"
            "    _package: crate::stage5g_fresh_broker_truth::Stage5gValidatedFreshBrokerTruthPackage,\n"
            ") -> crate::stage5g_fresh_broker_truth::Stage5gRestartReconciliationDisposition {\n"
            "    crate::stage5g_fresh_broker_truth::Stage5gRestartReconciliationDisposition::ExactReplay\n"
            "}\n")),
        ("gate-drop-current-checker", lambda root: replace_once(
            root, GATE, "python3 scripts/stage5g_eda_r5_check.py\n", "")),
        ("gate-drop-negative-harness", lambda root: replace_once(
            root, GATE, "python3 scripts/stage5g_eda_r5_negative_harness.py\n", "")),
        ("gate-drop-preseal", lambda root: replace_once(
            root, GATE, "python3 scripts/stage5g_eda_r5_preseal_check.py\n", "")),
        ("gate-drop-fmt", lambda root: replace_once(
            root, GATE, "cargo fmt --all -- --check\n", "")),
        ("gate-drop-focused-debug", lambda root: replace_once(
            root, GATE,
            "cargo test -p strategy-runtime-core --lib stage5g_fresh_broker_truth\n", "")),
        ("gate-drop-focused-release", lambda root: replace_once(
            root, GATE,
            "cargo test --release -p strategy-runtime-core --lib stage5g_fresh_broker_truth\n", "")),
        ("gate-drop-full-core", lambda root: replace_once(
            root, GATE, "cargo test -p strategy-runtime-core --lib\n", "")),
        ("gate-drop-clippy", lambda root: replace_once(
            root, GATE,
            "cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings\n", "")),
        ("gate-drop-detached-r4", lambda root: replace_once(
            root, GATE, "  bash scripts/stage5g_eda_r4_gate.sh\n", "")),
        ("builder-drop-clean-tree-check", lambda root: replace_once(
            root, BUILDER, "    if status:\n", "    if False and status:\n")),
        ("builder-drop-branch-check", lambda root: replace_once(
            root, BUILDER, "    if branch != BRANCH:\n", "    if False and branch != BRANCH:\n")),
        ("builder-drop-parent-check", lambda root: replace_once(
            root, BUILDER, "    if parent_ref != REQUIRED_PARENT:\n",
            "    if False and parent_ref != REQUIRED_PARENT:\n")),
        ("builder-drop-origin-check", lambda root: replace_once(
            root, BUILDER, "    if origin_ref != source_ref:\n",
            "    if False and origin_ref != source_ref:\n")),
        ("builder-ignore-gate-failure", lambda root: replace_once(
            root, BUILDER, "    if gate.returncode != 0:\n",
            "    if False and gate.returncode != 0:\n")),
    ]


def main() -> None:
    inherited_r3 = r3.mutation_cases()
    inherited_r4 = r4.r4_mutation_cases()
    current = r5_mutation_cases()
    cases = inherited_r3 + inherited_r4 + current
    if len(inherited_r3) != 56:
        raise SystemExit(f"stage5g-eda-r5-negative: FAIL: inherited R3 {len(inherited_r3)} != 56")
    if len(inherited_r4) != 23:
        raise SystemExit(f"stage5g-eda-r5-negative: FAIL: inherited R4 {len(inherited_r4)} != 23")
    if len(current) != 19 or len(cases) != 98:
        raise SystemExit(
            f"stage5g-eda-r5-negative: FAIL: matrix counts "
            f"R3={len(inherited_r3)} R4={len(inherited_r4)} R5={len(current)} total={len(cases)}"
        )
    names = [name for name, _ in cases]
    if len(names) != len(set(names)):
        raise SystemExit("stage5g-eda-r5-negative: FAIL: duplicate mutation names")
    for name, mutation in cases:
        run_case(name, mutation)
    print("stage5g-eda-r5-negative: PASS (98/98)")


if __name__ == "__main__":
    main()
