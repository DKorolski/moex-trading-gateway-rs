#!/usr/bin/env python3
"""Stage 5G-f negative mutation harness."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

import stage5g_f_check as checker

ROOT = Path(__file__).resolve().parents[1]
SOURCE = "crates/strategy-runtime-core/src/stage5g_protective_completion.rs"
CONTRACT = "docs/stage-5/stage5g-f-protective-completion-contract.json"
DESIGN = "docs/stage-5/stage5g-f-protective-completion-contract.md"
LIB = "crates/strategy-runtime-core/src/lib.rs"
GATE = "scripts/stage5g_f_gate.sh"
PRESEAL = "scripts/stage5g_f_preseal_check.py"
HANDOFF = "scripts/make_stage5g_f_handoff_archive.py"


def fail(message: str) -> None:
    raise SystemExit(f"stage5g-f-negative: FAIL: {message}")


def mutate(root: Path, relative: str, old: str, new: str, count: int | None = 1) -> None:
    path = root / relative
    text = path.read_text()
    if old not in text:
        fail(f"mutation target missing in {relative}: {old}")
    path.write_text(text.replace(old, new) if count is None else text.replace(old, new, count))


def run_checker(root: Path) -> bool:
    result = subprocess.run(
        ["python3", "scripts/stage5g_f_check.py", "--root", str(root), "--skip-git"],
        cwd=root,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        text=True,
        check=False,
    )
    return result.returncode == 0


def source_marker_cases() -> list[tuple[str, callable]]:
    cases: list[tuple[str, callable]] = []
    for idx, marker in enumerate(checker.REQUIRED_SOURCE_MARKERS, start=1):
        cases.append((
            f"source-guard-marker-{idx:02d}",
            lambda root, marker=marker: mutate(root, SOURCE, marker, marker[::-1], count=None),
        ))
    return cases


def focused_test_cases() -> list[tuple[str, callable]]:
    cases: list[tuple[str, callable]] = []
    for test in checker.REQUIRED_TESTS:
        cases.append((
            f"remove-{test}",
            lambda root, test=test: mutate(root, SOURCE, f"fn {test}(", f"fn removed_{test}("),
        ))
    return cases


def contract_cases() -> list[tuple[str, callable]]:
    cases: list[tuple[str, callable]] = []
    for scenario in checker.EXPECTED_SCENARIOS:
        cases.append((
            f"contract-rename-{scenario.lower()}",
            lambda root, scenario=scenario: mutate(root, CONTRACT, scenario, f"{scenario}_DRIFT"),
        ))
    for surface in [
        "finam_native_stop_endpoint",
        "finam_sltp_bracket_endpoint",
        "http_post_delete",
        "redis_live_consumer",
        "broker_dispatch",
        "second_callback_path",
        "runtime_live",
        "real_orders",
        "stage5g_g",
        "stage5g_h",
        "stage6",
    ]:
        cases.append((
            f"open-{surface}",
            lambda root, surface=surface: mutate(root, CONTRACT, f'"{surface}": false', f'"{surface}": true'),
        ))
    cases.extend([
        ("lower-negative-floor", lambda root: mutate(root, CONTRACT, '"current_stage5g_f_minimum": 80', '"current_stage5g_f_minimum": 1')),
        ("open-bar-ohlc-authority", lambda root: mutate(root, CONTRACT, '"bar_ohlc_completion_authority": false', '"bar_ohlc_completion_authority": true')),
        ("wrong-base-ref", lambda root: mutate(root, CONTRACT, checker.BASE, "0" * 40)),
        ("wrong-entry-function", lambda root: mutate(root, CONTRACT, '"apply_stage5g_protective_completion"', '"apply_stage5g_protective_completion_bypass"')),
        ("wrong-predecessor-verification-mode", lambda root: mutate(root, CONTRACT, '"mode": "bounded_detached_stage5g_edc_r3"', '"mode": "recursive_stage5g_edc_r3_gate"')),
        ("wrong-predecessor-verification-commit", lambda root: mutate(root, CONTRACT, '"commit": "' + checker.BASE + '"', '"commit": "' + ("1" * 40) + '"')),
        ("open-recursive-historical-lineage", lambda root: mutate(root, CONTRACT, '"runs_recursive_historical_lineage": false', '"runs_recursive_historical_lineage": true')),
        ("remove-predecessor-check-command", lambda root: mutate(root, CONTRACT, '"python3 scripts/stage5g_edc_r3_check.py"', '"python3 scripts/stage5g_edc_r3_check_removed.py"')),
    ])
    return cases


def governance_cases() -> list[tuple[str, callable]]:
    return [
        ("remove-module-link", lambda root: mutate(root, LIB, "mod stage5g_protective_completion;", "// removed stage5g_f module")),
        ("remove-public-facade", lambda root: mutate(root, LIB, "pub use stage5g_protective_completion::", "pub use stage5g_protective_completion_removed::")),
        ("design-loses-f12-f15", lambda root: mutate(root, DESIGN, "Stage 5F F12–F15 remain no-bar-exit", "Stage 5F F12-F15 drift")),
        ("design-opens-stage5g-g", lambda root: mutate(root, DESIGN, "Only after independent Stage 5G-f acceptance may Stage 5G-g begin", "Stage 5G-g may begin immediately")),
        ("gate-removes-checker", lambda root: mutate(root, GATE, "python3 scripts/stage5g_f_check.py", "# checker removed")),
        ("gate-removes-negative", lambda root: mutate(root, GATE, "python3 scripts/stage5g_f_negative_harness.py", "# negative removed")),
        ("gate-removes-preseal", lambda root: mutate(root, GATE, "python3 scripts/stage5g_f_preseal_check.py", "# preseal removed")),
        ("gate-removes-debug-tests", lambda root: mutate(root, GATE, "cargo test -p strategy-runtime-core --lib stage5g_f_", "# focused debug removed")),
        ("gate-removes-release-tests", lambda root: mutate(root, GATE, "cargo test --release -p strategy-runtime-core --lib stage5g_f_", "# focused release removed")),
        ("gate-removes-predecessor-checker", lambda root: mutate(root, GATE, "python3 scripts/stage5g_edc_r3_check.py", "# predecessor checker removed")),
        ("gate-removes-predecessor-negative", lambda root: mutate(root, GATE, "python3 scripts/stage5g_edc_r3_negative_harness.py", "# predecessor negative removed")),
        ("gate-removes-predecessor-release-tests", lambda root: mutate(root, GATE, "cargo test --release -p strategy-runtime-core --lib stage5g_edc_r3_", "# predecessor release tests removed")),
        ("preseal-loses-allowlist", lambda root: mutate(root, PRESEAL, "EXPECTED = sorted([", "EXPECTED_DISABLED = sorted([")),
        ("handoff-removes-gate", lambda root: mutate(root, HANDOFF, '["bash", "scripts/stage5g_f_gate.sh"]', '["bash", "scripts/stage5g_f_check.py"]')),
    ]


def forbidden_surface_cases() -> list[tuple[str, callable]]:
    return [
        ("inject-reqwest", lambda root: mutate(root, SOURCE, "use broker_core::{", "use reqwest as forbidden_reqwest;\nuse broker_core::{")),
        ("inject-method-post", lambda root: mutate(root, SOURCE, "use broker_core::{", "use http::Method::POST;\nuse broker_core::{")),
        ("inject-method-delete", lambda root: mutate(root, SOURCE, "use broker_core::{", "use http::Method::DELETE;\nuse broker_core::{")),
        ("inject-redis", lambda root: mutate(root, SOURCE, "use broker_core::{", "use redis::Commands;\nuse broker_core::{")),
        ("inject-runtime-live", lambda root: mutate(root, SOURCE, "//! Stage 5G-f paper/mock", "//! runtime_live Stage 5G-f paper/mock")),
        ("inject-bar-event", lambda root: mutate(root, SOURCE, "//! Stage 5G-f paper/mock", "//! BarEvent Stage 5G-f paper/mock")),
        ("inject-bar-high", lambda root: mutate(root, SOURCE, "//! Stage 5G-f paper/mock", "//! .high Stage 5G-f paper/mock")),
        ("inject-bar-low", lambda root: mutate(root, SOURCE, "//! Stage 5G-f paper/mock", "//! .low Stage 5G-f paper/mock")),
        ("inject-wall-clock", lambda root: mutate(root, SOURCE, "//! Stage 5G-f paper/mock", "//! Utc::now Stage 5G-f paper/mock")),
        ("inject-sleep", lambda root: mutate(root, SOURCE, "//! Stage 5G-f paper/mock", "//! thread::sleep Stage 5G-f paper/mock")),
    ]


def cases() -> list[tuple[str, callable]]:
    all_cases = (
        source_marker_cases()
        + focused_test_cases()
        + contract_cases()
        + governance_cases()
        + forbidden_surface_cases()
    )
    if len(all_cases) < 80:
        fail(f"negative floor not met: {len(all_cases)} < 80")
    names = [name for name, _ in all_cases]
    if len(names) != len(set(names)):
        fail("duplicate mutation names")
    return all_cases


def main() -> None:
    all_cases = cases()
    passed = 0
    for name, action in all_cases:
        with tempfile.TemporaryDirectory(prefix=f"stage5g-f-negative-{name}-") as raw:
            root = Path(raw) / "repo"
            ignore = shutil.ignore_patterns("target", ".git", "reports", "tmp", "__MACOSX", "*.log")
            shutil.copytree(ROOT, root, ignore=ignore)
            action(root)
            if run_checker(root):
                fail(f"mutation survived: {name}")
            print(f"PASS {name}")
            passed += 1
    print(f"stage5g-f-negative: PASS {passed}/{len(all_cases)}")


if __name__ == "__main__":
    main()
