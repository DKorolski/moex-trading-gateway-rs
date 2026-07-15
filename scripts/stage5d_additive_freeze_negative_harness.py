#!/usr/bin/env python3
"""Negative harness for Stage 5D additive freeze enforcement."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECKER = Path("scripts/stage5d_additive_freeze_check.py")


def copy_workspace(destination: Path) -> None:
    def ignore(directory: str, names: list[str]) -> set[str]:
        ignored = {".git", "target", "tmp", "reports", "__pycache__"}
        return {name for name in names if name in ignored}

    shutil.copytree(ROOT, destination, ignore=ignore)


def run_checker(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(root / CHECKER), "--root", str(root)],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def replace_once(path: Path, old: str, new: str) -> None:
    source = path.read_text()
    if old not in source:
        raise RuntimeError(f"pattern not found in {path}: {old}")
    path.write_text(source.replace(old, new, 1))


def append_text(path: Path, text: str) -> None:
    path.write_text(path.read_text() + text)


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def update_manifest_bridge_hash(root: Path, rel_path: str) -> None:
    manifest_path = root / "docs/stage-5/stage-5d-additive-freeze-manifest.json"
    manifest = json.loads(manifest_path.read_text())
    manifest["approved_bridge_files"][rel_path]["current_sha256"] = sha256_file(root / rel_path)
    manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")


def update_manifest_stage5d_hash(root: Path) -> None:
    manifest_path = root / "docs/stage-5/stage-5d-additive-freeze-manifest.json"
    manifest = json.loads(manifest_path.read_text())
    rel_path = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    manifest["stage5d_persistence_file"]["current_sha256"] = sha256_file(root / rel_path)
    manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")


def insert_before(path: Path, marker: str, text: str) -> None:
    source = path.read_text()
    if marker not in source:
        raise RuntimeError(f"marker not found in {path}: {marker}")
    path.write_text(source.replace(marker, text + marker, 1))


def mutate_stage5c_api_drift(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
        "pub fn notify_stage5c_bootstrap(",
        "pub fn notify_stage5c_bootstrap_drift(",
    )


def mutate_trading_region_drift(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs",
        "const RISK_GATE_MAKER_COST_POINTS: f64 = 0.1;",
        "// forbidden trading-region drift\nconst RISK_GATE_MAKER_COST_POINTS: f64 = 0.1;",
    )


def mutate_additive_region_escape(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
        "pub const STAGE5C_PAPER_HOST_ADMISSION_SCHEMA_VERSION: u16 = 1;",
        "pub const STAGE5C_PAPER_HOST_ADMISSION_SCHEMA_VERSION: u16 = 1;\n// forbidden additive escape",
    )


def mutate_public_namespace_leakage(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "\npub struct PersistenceLeak;\n",
    )


def mutate_raw_strategy_extractor(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "\npub fn stage5d_raw_strategy_extractor() {}\n",
    )


def mutate_missing_historical_baseline(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage-5d-additive-freeze-manifest.json",
        '"original_checker_sha256": "e494e92ffb5f8d90b6a581c7b99e4e80f1906aeedfa1e7446d428eb31c757209"',
        '"original_checker_sha256": "0000000000000000000000000000000000000000000000000000000000000000"',
    )


def mutate_closed_surface_downgrade(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage-5d-additive-freeze-manifest.json",
        '"redis": false',
        '"redis": true',
    )
    replace_once(
        root / "docs/stage-5/stage-5d-additive-freeze-manifest.json",
        '"runtime_private_mutation": "controlled_validated_stage5d_apply_only"',
        '"runtime_private_mutation": "raw_mutation_allowed"',
    )


def mutate_negative_cases_removed(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage-5d-additive-freeze-manifest.json",
        '"negative_cases": [',
        '"negative_cases": [],\n  "negative_cases_removed_original": [',
    )


def mutate_manifest_checker_changed(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage-5d-additive-freeze-manifest.json",
        '"manifest_checker": "scripts/stage5d_additive_freeze_check.py"',
        '"manifest_checker": "scripts/other.py"',
    )


def mutate_negative_harness_changed(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage-5d-additive-freeze-manifest.json",
        '"negative_harness": "scripts/stage5d_additive_freeze_negative_harness.py"',
        '"negative_harness": "scripts/other.py"',
    )


def mutate_stage5d_symbol_removed(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage-5d-additive-freeze-manifest.json",
        '    "Stage5dValidatedRuntimePrivateExtension"',
        '    "Stage5dSymbolRemovedForNegativeTest"',
    )


def mutate_stage5d_symbol_added(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage-5d-additive-freeze-manifest.json",
        '    "Stage5dValidatedRuntimePrivateExtension"',
        '    "Stage5dValidatedRuntimePrivateExtension",\n    "Stage5dUnexpected"',
    )


def mutate_current_compat_checker_drift(root: Path) -> None:
    append_text(root / "scripts/stage5c_api_freeze_check.py", "\n# forbidden compat drift\n")


def mutate_historical_checker_missing(root: Path) -> None:
    (root / "tests/fixtures/stage5/stage5c_api_freeze_check.closure.py").unlink()


def mutate_historical_checker_content_drift(root: Path) -> None:
    append_text(
        root / "tests/fixtures/stage5/stage5c_api_freeze_check.closure.py",
        "\n# forbidden historical drift\n",
    )


def mutate_historical_current_checker_substitution(root: Path) -> None:
    current = root / "scripts/stage5c_api_freeze_check.py"
    historical = root / "tests/fixtures/stage5/stage5c_api_freeze_check.closure.py"
    historical.write_bytes(current.read_bytes())


def append_forbidden_restore_reference(root: Path, body: str) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_legacy_restore_reference() {\n"
        + body
        + "\n}\n",
    )


def mutate_legacy_restore_direct_call(root: Path) -> None:
    append_forbidden_restore_reference(root, "    restore_stage5c_runtime_state();")


def mutate_legacy_restore_alias_call(root: Path) -> None:
    append_forbidden_restore_reference(
        root,
        "    use crate::restore_stage5c_runtime_state as legacy_restore;\n    legacy_restore();",
    )


def mutate_legacy_restore_multiline_call(root: Path) -> None:
    append_forbidden_restore_reference(root, "    restore_stage5c_runtime_state\n        ();")


def mutate_legacy_restore_function_reference(root: Path) -> None:
    append_forbidden_restore_reference(root, "    let _legacy = crate::restore_stage5c_runtime_state;")


def mutate_legacy_restore_qualified_whitespace(root: Path) -> None:
    append_forbidden_restore_reference(root, "    crate :: notify_stage5c_runtime_state_restored();")


def mutate_legacy_alias_reexport_in_lib_additive_region(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/lib.rs"
    insert_before(
        root / rel,
        "// STAGE5D-ADDITIVE-BRIDGE-END: lib-stage5d-exports",
        "pub use stage5c_paper_host::restore_stage5c_runtime_state as stage5d_legacy_restore_alias;\n",
    )
    update_manifest_bridge_hash(root, rel)
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_transitive_alias() {\n"
        "    let _ = crate::stage5d_legacy_restore_alias;\n}\n",
    )


def mutate_legacy_wrapper_in_stage5c_additive_region(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    insert_before(
        root / rel,
        "// STAGE5D-ADDITIVE-BRIDGE-END: type-state-transitions",
        "pub(crate) fn stage5d_legacy_restore_wrapper_for_negative_test() {\n"
        "    let _ = restore_stage5c_runtime_state;\n"
        "}\n",
    )
    update_manifest_bridge_hash(root, rel)
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_wrapper_alias() {\n"
        "    let _ = crate::stage5c_paper_host::stage5d_legacy_restore_wrapper_for_negative_test;\n}\n",
    )


def mutate_legacy_alias_in_stage5d_persistence(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "\npub(crate) use crate::restore_stage5c_runtime_state as stage5d_private_legacy_alias;\n",
    )
    update_manifest_stage5d_hash(root)


def mutate_unexpected_legacy_reference_in_allowed_file(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    insert_before(
        root / rel,
        "// STAGE5D-ADDITIVE-BRIDGE-END: type-state-transitions",
        "const STAGE5D_NEGATIVE_LEGACY_REF: &str = \"notify_stage5c_bootstrap\";\n",
    )
    update_manifest_bridge_hash(root, rel)


def mutate_legacy_reference_moved_to_wrong_region(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/lib.rs"
    insert_before(
        root / rel,
        "// STAGE5D-ADDITIVE-BRIDGE-END: lib-stage5d-module",
        "use crate::stage5c_paper_host::notify_stage5c_runtime_state_restored as _stage5d_wrong_region;\n",
    )
    update_manifest_bridge_hash(root, rel)


def mutate_stage5d_api_surface_drift(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "pub pending_requests: Vec<StrategyRequestId>,",
        "pub pending_requests: Vec<StrategyRequestId>,\n    pub negative_api_surface_drift: String,",
    )
    update_manifest_stage5d_hash(root)


def mutate_legacy_restore_bypass(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "\nfn allowed_bridge_reference() {\n    let _ = \"restore_stage5c_runtime_state(\";\n}\n",
    )
    append_text(
        root / "crates/strategy-runtime-core/src/hybrid_intraday/mod.rs",
        "\nfn forbidden_production_bypass_marker() {\n    let _ = \"restore_stage5c_runtime_state(\";\n}\n",
    )


CASES = [
    ("stage5c_api_drift", mutate_stage5c_api_drift, "Stage 5C public API shape drifted"),
    ("trading_region_drift", mutate_trading_region_drift, "frozen region does not match"),
    ("additive_region_escape", mutate_additive_region_escape, "frozen region does not match"),
    ("public_namespace_leakage", mutate_public_namespace_leakage, "forbidden Stage 5D public surface"),
    ("raw_strategy_extractor", mutate_raw_strategy_extractor, "forbidden Stage 5D public surface"),
    ("missing_historical_baseline", mutate_missing_historical_baseline, "closure baseline reference mismatch"),
    ("closed_surface_downgrade", mutate_closed_surface_downgrade, "closed_surfaces mismatch"),
    ("negative_cases_removed", mutate_negative_cases_removed, "negative_cases mismatch"),
    ("manifest_checker_changed", mutate_manifest_checker_changed, "manifest_checker mismatch"),
    ("negative_harness_changed", mutate_negative_harness_changed, "negative_harness mismatch"),
    ("stage5d_symbol_removed", mutate_stage5d_symbol_removed, "Stage5d public symbol contract mismatch"),
    ("stage5d_symbol_added", mutate_stage5d_symbol_added, "Stage5d public symbol contract mismatch"),
    ("current_compat_checker_drift", mutate_current_compat_checker_drift, "compatibility checker hash mismatch"),
    ("historical_checker_missing", mutate_historical_checker_missing, "historical Stage 5C closure checker artifact missing"),
    ("historical_checker_content_drift", mutate_historical_checker_content_drift, "historical Stage 5C closure checker hash mismatch"),
    ("historical_current_checker_substitution", mutate_historical_current_checker_substitution, "historical Stage 5C closure checker hash mismatch"),
    ("legacy_restore_direct_call", mutate_legacy_restore_direct_call, "legacy Stage 5C restore bypass symbol forbidden"),
    ("legacy_restore_alias_call", mutate_legacy_restore_alias_call, "legacy Stage 5C restore bypass symbol forbidden"),
    ("legacy_restore_multiline_call", mutate_legacy_restore_multiline_call, "legacy Stage 5C restore bypass symbol forbidden"),
    ("legacy_restore_function_reference", mutate_legacy_restore_function_reference, "legacy Stage 5C restore bypass symbol forbidden"),
    ("legacy_restore_qualified_whitespace", mutate_legacy_restore_qualified_whitespace, "legacy Stage 5C restore bypass symbol forbidden"),
    ("legacy_alias_reexport_in_lib_additive_region", mutate_legacy_alias_reexport_in_lib_additive_region, "forbidden in additive region"),
    ("legacy_wrapper_in_stage5c_additive_region", mutate_legacy_wrapper_in_stage5c_additive_region, "reference count mismatch"),
    ("legacy_alias_in_stage5d_persistence", mutate_legacy_alias_in_stage5d_persistence, "forbidden in Stage 5D persistence surface"),
    ("unexpected_legacy_reference_in_allowed_file", mutate_unexpected_legacy_reference_in_allowed_file, "reference count mismatch"),
    ("legacy_reference_moved_to_wrong_region", mutate_legacy_reference_moved_to_wrong_region, "forbidden in additive region"),
    ("stage5d_api_surface_drift", mutate_stage5d_api_surface_drift, "Stage5d public API surface mismatch"),
]


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage5d-negative-") as tmp:
        base = Path(tmp)
        clean = base / "clean"
        copy_workspace(clean)
        clean_result = run_checker(clean)
        if clean_result.returncode != 0:
            print(clean_result.stdout)
            print(clean_result.stderr, file=sys.stderr)
            print("stage5d-negative-harness: clean checker run failed", file=sys.stderr)
            return 1

        case_root = base / "case"
        for name, mutator, expected in CASES:
            if case_root.exists():
                shutil.rmtree(case_root)
            shutil.copytree(clean, case_root)
            mutator(case_root)
            result = run_checker(case_root)
            combined = result.stdout + result.stderr
            if result.returncode == 0:
                print(f"stage5d-negative-harness: {name} unexpectedly passed", file=sys.stderr)
                return 1
            if expected not in combined:
                print(combined, file=sys.stderr)
                print(
                    f"stage5d-negative-harness: {name} failed without expected marker {expected!r}",
                    file=sys.stderr,
                )
                return 1
            shutil.rmtree(case_root)
    print("stage5d-negative-harness: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
