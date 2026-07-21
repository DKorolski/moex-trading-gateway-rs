#!/usr/bin/env python3
"""Negative harness for Stage 5D additive freeze enforcement."""

from __future__ import annotations

import concurrent.futures
import shutil
import subprocess
import sys
import tempfile
import hashlib
import json
import math
import os
import re
import signal
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

sys.dont_write_bytecode = True

from copy_review_baseline import copy_review_baseline


ROOT = Path(__file__).resolve().parents[1]
CHECKER = Path("scripts/stage5d_additive_freeze_check.py")


def copy_workspace(destination: Path) -> None:
    copy_review_baseline(ROOT, destination)


@dataclass(frozen=True)
class CheckerRun:
    returncode: int
    stdout: str
    stderr: str
    duration_seconds: float
    timed_out: bool = False


@dataclass(frozen=True)
class CaseRun:
    index: int
    name: str
    passed: bool
    diagnostics: str
    duration_seconds: float


def run_checker(root: Path, timeout_seconds: int) -> CheckerRun:
    started = time.monotonic()
    process = subprocess.Popen(
        [sys.executable, str(root / CHECKER), "--root", str(root)],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
    try:
        stdout, stderr = process.communicate(timeout=timeout_seconds)
        return CheckerRun(
            returncode=process.returncode,
            stdout=stdout,
            stderr=stderr,
            duration_seconds=time.monotonic() - started,
        )
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate()
        return CheckerRun(
            returncode=124,
            stdout=stdout,
            stderr=stderr + f"\nchecker timed out after {timeout_seconds}s\n",
            duration_seconds=time.monotonic() - started,
            timed_out=True,
        )


def replace_once(path: Path, old: str, new: str) -> None:
    source = path.read_text()
    if old not in source:
        raise RuntimeError(f"pattern not found in {path}: {old}")
    path.write_text(source.replace(old, new, 1))


def replace_all(path: Path, old: str, new: str) -> None:
    source = path.read_text()
    if old not in source:
        raise RuntimeError(f"pattern not found in {path}: {old}")
    path.write_text(source.replace(old, new))


def append_text(path: Path, text: str) -> None:
    path.write_text(path.read_text() + text)


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def update_manifest_bridge_hash(root: Path, rel_path: str) -> None:
    manifest_path = root / "docs/stage-5/stage-5d-additive-freeze-manifest.json"
    manifest = json.loads(manifest_path.read_text())
    manifest["approved_bridge_files"][rel_path]["current_sha256"] = sha256_file(root / rel_path)
    manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")


def strip_additive_region(source: str, region: str) -> str:
    begin = f"// STAGE5D-ADDITIVE-BRIDGE-BEGIN: {region}"
    end = f"// STAGE5D-ADDITIVE-BRIDGE-END: {region}"
    begin_index = source.index(begin)
    end_index = source.index(end, begin_index) + len(end)
    if end_index < len(source) and source[end_index : end_index + 1] == "\n":
        end_index += 1
    return source[:begin_index] + source[end_index:]


def stripped_bridge_hash(root: Path, rel_path: str) -> str:
    regions = {
        "crates/strategy-runtime-core/src/lib.rs": [
            "lib-stage5d-module",
            "lib-stage5d-exports",
        ],
        "crates/strategy-runtime-core/src/stage5c_paper_host.rs": [
            "type-state-transitions"
        ],
        "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs": [
            "runtime-private-snapshot"
        ],
    }[rel_path]
    source = (root / rel_path).read_text()
    for region in regions:
        source = strip_additive_region(source, region)
    return hashlib.sha256(source.encode()).hexdigest()


def update_manifest_bridge_current_and_stripped_hash(root: Path, rel_path: str) -> None:
    manifest_path = root / "docs/stage-5/stage-5d-additive-freeze-manifest.json"
    manifest = json.loads(manifest_path.read_text())
    manifest["approved_bridge_files"][rel_path]["current_sha256"] = sha256_file(root / rel_path)
    manifest["approved_bridge_files"][rel_path][
        "stripped_without_additive_regions_sha256"
    ] = stripped_bridge_hash(root, rel_path)
    manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")


def mutate_private_layout_extensions(
    root: Path, mutator: Callable[[list[dict]], None]
) -> None:
    manifest_path = root / "docs/stage-5/stage-5d-additive-freeze-manifest.json"
    manifest = json.loads(manifest_path.read_text())
    mutator(manifest["stage5c_private_layout_extensions"])
    manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")


def update_manifest_stage5d_hash(root: Path) -> None:
    manifest_path = root / "docs/stage-5/stage-5d-additive-freeze-manifest.json"
    manifest = json.loads(manifest_path.read_text())
    rel_path = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    stage5d_sha256 = sha256_file(root / rel_path)
    manifest["stage5d_persistence_file"]["current_sha256"] = stage5d_sha256
    for extension in manifest.get("controlled_source_semantic_extensions", []):
        if extension.get("stage5d_consumer_path") == rel_path:
            extension["stage5d_consumer_sha256"] = stage5d_sha256
    manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")


def update_stage5d_checker_expected_consumer_hash(root: Path) -> None:
    checker_path = root / CHECKER
    stage5d_sha256 = sha256_file(root / "crates/strategy-runtime-core/src/stage5d_persistence.rs")
    source = checker_path.read_text()
    source = re.sub(
        r'("stage5d_consumer_sha256": ")[0-9a-f]{64}(")',
        rf"\g<1>{stage5d_sha256}\2",
        source,
    )
    checker_path.write_text(source)


def update_stage5d_semantic_mutation_hashes(root: Path) -> None:
    update_manifest_stage5d_hash(root)
    update_stage5d_checker_expected_consumer_hash(root)


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
        '"runtime_private_mutation": "controlled_validated_stage5d_apply_then_broker_truth_bootstrap_then_riskgate_injection_then_restored_callback_only"',
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


def mutate_private_layout_extension_removed(root: Path) -> None:
    mutate_private_layout_extensions(root, lambda extensions: extensions.clear())


def mutate_private_layout_extension_hash_changed(root: Path) -> None:
    def mutator(extensions: list[dict]) -> None:
        extensions[0]["stripped_without_additive_regions_sha256"] = "0" * 64

    mutate_private_layout_extensions(root, mutator)


def mutate_private_layout_extension_additional_path(root: Path) -> None:
    def mutator(extensions: list[dict]) -> None:
        extra = dict(extensions[0])
        extra["path"] = "crates/strategy-runtime-core/src/runtime_compat.rs"
        extensions.append(extra)

    mutate_private_layout_extensions(root, mutator)


def mutate_private_layout_extension_wrapper_path(root: Path) -> None:
    def mutator(extensions: list[dict]) -> None:
        extensions[0]["path"] = "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs"
        extensions[0][
            "stripped_without_additive_regions_sha256"
        ] = stripped_bridge_hash(root, "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs")

    mutate_private_layout_extensions(root, mutator)


def mutate_private_layout_extension_lib_path(root: Path) -> None:
    def mutator(extensions: list[dict]) -> None:
        extensions[0]["path"] = "crates/strategy-runtime-core/src/lib.rs"
        extensions[0]["stripped_without_additive_regions_sha256"] = stripped_bridge_hash(
            root, "crates/strategy-runtime-core/src/lib.rs"
        )

    mutate_private_layout_extensions(root, mutator)


def mutate_private_layout_self_authorized_semantic_drift(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs"
    replace_once(
        root / rel,
        "const RISK_GATE_MAKER_COST_POINTS: f64 = 0.1;",
        "const RISK_GATE_MAKER_COST_POINTS: f64 = 0.2;",
    )
    update_manifest_bridge_current_and_stripped_hash(root, rel)

    def mutator(extensions: list[dict]) -> None:
        extra = dict(extensions[0])
        extra["path"] = rel
        extra["stripped_without_additive_regions_sha256"] = stripped_bridge_hash(root, rel)
        extensions.append(extra)

    mutate_private_layout_extensions(root, mutator)


def mutate_private_layout_extension_reason_id_changed(root: Path) -> None:
    def mutator(extensions: list[dict]) -> None:
        extensions[0]["reason_id"] = "stage5d-b2b-a-unreviewed-private-layout-v2"

    mutate_private_layout_extensions(root, mutator)


def mutate_bootstrap_bridge_runtime_compat_direct_call(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_bootstrap_bridge_direct_call(loaded: crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy, now: chrono::DateTime<chrono::Utc>) {\n"
        "    let _ = crate::stage5c_paper_host::stage5d_bootstrap_preserving_loaded_with_validated_working_sets_at(loaded, now);\n"
        "}\n",
    )


def mutate_bootstrap_bridge_runtime_compat_alias_call(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_bootstrap_bridge_alias_call(loaded: crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy, now: chrono::DateTime<chrono::Utc>) {\n"
        "    use crate::stage5c_paper_host::stage5d_bootstrap_preserving_loaded_with_validated_working_sets_at as bypass_bootstrap;\n"
        "    let _ = bypass_bootstrap(loaded, now);\n"
        "}\n",
    )


def mutate_bootstrap_bridge_runtime_compat_forwarding_wrapper(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_bootstrap_bridge_forwarding_wrapper(loaded: crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy, now: chrono::DateTime<chrono::Utc>) {\n"
        "    let _ = crate::stage5c_paper_host::stage5d_bootstrap_preserving_loaded_with_validated_working_sets_at(loaded, now);\n"
        "}\n",
    )


def mutate_bootstrap_bridge_runtime_compat_function_reference(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_bootstrap_bridge_function_reference() {\n"
        "    let _bridge = crate::stage5c_paper_host::stage5d_bootstrap_preserving_loaded_with_validated_working_sets_at;\n"
        "}\n",
    )


def mutate_bootstrap_bridge_second_stage5d_call(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    insert_before(
        root / rel,
        "fn validate_stage5d_broker_truth_bootstrap(",
        "#[allow(dead_code)]\nfn stage5d_negative_second_bootstrap_bridge_call(loaded: crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy, now: DateTime<Utc>) {\n"
        "    let _ = crate::stage5c_paper_host::stage5d_bootstrap_preserving_loaded_with_validated_working_sets_at(loaded, now);\n"
        "}\n\n",
    )
    update_manifest_stage5d_hash(root)


def mutate_riskgate_bridge_runtime_compat_direct_call(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_riskgate_bridge_direct_call(bootstrapped: crate::stage5c_paper_host::Stage5cBootstrappedPaperStrategy, riskgate: RiskGateRuntimeState) {\n"
        "    let _ = crate::stage5c_paper_host::stage5d_inject_authoritative_riskgate_state(bootstrapped, riskgate);\n"
        "}\n",
    )


def mutate_riskgate_bridge_runtime_compat_alias_call(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_riskgate_bridge_alias_call(bootstrapped: crate::stage5c_paper_host::Stage5cBootstrappedPaperStrategy, riskgate: RiskGateRuntimeState) {\n"
        "    use crate::stage5c_paper_host::stage5d_inject_authoritative_riskgate_state as bypass_riskgate;\n"
        "    let _ = bypass_riskgate(bootstrapped, riskgate);\n"
        "}\n",
    )


def mutate_riskgate_bridge_runtime_compat_forwarding_wrapper(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_riskgate_bridge_forwarding_wrapper(bootstrapped: crate::stage5c_paper_host::Stage5cBootstrappedPaperStrategy, riskgate: RiskGateRuntimeState) {\n"
        "    let _ = crate::stage5c_paper_host::stage5d_inject_authoritative_riskgate_state(bootstrapped, riskgate);\n"
        "}\n",
    )


def mutate_riskgate_bridge_runtime_compat_function_reference(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_riskgate_bridge_function_reference() {\n"
        "    let _bridge = crate::stage5c_paper_host::stage5d_inject_authoritative_riskgate_state;\n"
        "}\n",
    )


def mutate_riskgate_bridge_second_stage5d_call(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    insert_before(
        root / rel,
        "fn stage5d_authoritative_riskgate_state_from_evidence(",
        "#[allow(dead_code)]\nfn stage5d_negative_second_riskgate_bridge_call(bootstrapped: crate::stage5c_paper_host::Stage5cBootstrappedPaperStrategy, riskgate: RiskGateRuntimeState) {\n"
        "    let _ = crate::stage5c_paper_host::stage5d_inject_authoritative_riskgate_state(bootstrapped, riskgate);\n"
        "}\n\n",
    )
    update_manifest_stage5d_hash(root)


def mutate_runtime_restored_bridge_runtime_compat_direct_call(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_runtime_restored_bridge_direct_call(bootstrapped: crate::stage5c_paper_host::Stage5cBootstrappedPaperStrategy, now: chrono::DateTime<chrono::Utc>) {\n"
        "    let _ = crate::stage5c_paper_host::stage5d_notify_runtime_state_restored_bridge_at(bootstrapped, now);\n"
        "}\n",
    )


def mutate_runtime_restored_bridge_runtime_compat_alias_call(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_runtime_restored_bridge_alias_call(bootstrapped: crate::stage5c_paper_host::Stage5cBootstrappedPaperStrategy, now: chrono::DateTime<chrono::Utc>) {\n"
        "    use crate::stage5c_paper_host::stage5d_notify_runtime_state_restored_bridge_at as bypass_restored;\n"
        "    let _ = bypass_restored(bootstrapped, now);\n"
        "}\n",
    )


def mutate_runtime_restored_bridge_runtime_compat_function_reference(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_runtime_restored_bridge_function_reference() {\n"
        "    let _bridge = crate::stage5c_paper_host::stage5d_notify_runtime_state_restored_bridge_at;\n"
        "}\n",
    )


def mutate_runtime_restored_bridge_second_stage5d_call(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    insert_before(
        root / rel,
        "fn validate_stage5d_runtime_state_restored_preflight(",
        "#[allow(dead_code)]\nfn stage5d_negative_second_runtime_restored_bridge_call(bootstrapped: crate::stage5c_paper_host::Stage5cBootstrappedPaperStrategy, now: DateTime<Utc>) {\n"
        "    let _ = crate::stage5c_paper_host::stage5d_notify_runtime_state_restored_bridge_at(bootstrapped, now);\n"
        "}\n\n",
    )
    update_manifest_stage5d_hash(root)


def mutate_runtime_restored_bridge_made_public(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    replace_once(
        root / rel,
        "pub(crate) fn stage5d_notify_runtime_state_restored_bridge_at(",
        "pub fn stage5d_notify_runtime_state_restored_bridge_at(",
    )
    update_manifest_bridge_hash(root, rel)


def mutate_runtime_restored_intent_runtime_guard_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    replace_once(
        root / rel,
        "    if !intents.is_empty() {\n        return Err(Stage5dRuntimeStateRestoredBridgeError::CallbackEmittedIntent);\n    }\n",
        "",
    )
    update_manifest_bridge_hash(root, rel)


def mutate_runtime_restored_intent_guard_after_debug_assert(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    replace_once(
        root / rel,
        "    if !intents.is_empty() {\n        return Err(Stage5dRuntimeStateRestoredBridgeError::CallbackEmittedIntent);\n    }\n    debug_assert!(intents.is_empty());",
        "    debug_assert!(intents.is_empty());\n    if !intents.is_empty() {\n        return Err(Stage5dRuntimeStateRestoredBridgeError::CallbackEmittedIntent);\n    }",
    )
    update_manifest_bridge_hash(root, rel)


def mutate_runtime_restored_post_callback_exact_guard_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    replace_once(
        root / rel,
        "    stage5d_validate_post_runtime_restored_broker_truth_exact(&strategy, admission)?;\n",
        "",
    )
    update_manifest_bridge_hash(root, rel)


def mutate_runtime_restored_callback_count_hook_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    replace_once(
        root / rel,
        "    #[cfg(test)]\n    STAGE5D_RUNTIME_RESTORED_CALLBACK_COUNT.with(|count| count.set(count.get() + 1));\n",
        "",
    )
    update_manifest_bridge_hash(root, rel)


def mutate_runtime_restored_post_callback_position_guard_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    replace_all(
        root / rel,
        "    if (*last_position_qty - broker_qty).abs() > f64::EPSILON {",
        "    if false && (*last_position_qty - broker_qty).abs() > f64::EPSILON {",
    )
    update_manifest_bridge_hash(root, rel)


def mutate_runtime_restored_post_callback_side_guard_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    replace_once(
        root / rel,
        "    if *current_side != expected_side {",
        "    if false && *current_side != expected_side {",
    )
    update_manifest_bridge_hash(root, rel)


def mutate_runtime_restored_post_callback_protective_guard_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    replace_all(
        root / rel,
        "    if tp_order_id.is_some() || sl_stop_order_id.is_some() || sl_exchange_order_id.is_some() {",
        "    if false && (tp_order_id.is_some() || sl_stop_order_id.is_some() || sl_exchange_order_id.is_some()) {",
    )
    update_manifest_bridge_hash(root, rel)


def mutate_runtime_restored_preflight_invocation_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "    if let Err(reason) = validate_stage5d_runtime_state_restored_preflight(&injected, restored_at) {",
        "    if false {",
    )
    update_manifest_stage5d_hash(root)


def mutate_runtime_restored_recovery_complete_guard_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "    if !injected.recovery_plan.recovery_complete",
        "    if false && !injected.recovery_plan.recovery_complete",
    )
    update_manifest_stage5d_hash(root)


def mutate_runtime_restored_pending_finalization_guard_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        ".runtime_pending_finalizations\n        .is_empty()",
        ".runtime_pending_finalizations\n        .len() == usize::MAX",
    )
    update_manifest_stage5d_hash(root)


def mutate_runtime_restored_recovery_plan_binding_guard_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "expected_plan != injected.recovery_plan.plan_fingerprint_sha256",
        "false && expected_plan != injected.recovery_plan.plan_fingerprint_sha256",
    )
    update_manifest_stage5d_hash(root)


def mutate_runtime_restored_recovery_index_guard_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "if injected.bootstrapped.stage5d_restored().known_order_ids",
        "if false && injected.bootstrapped.stage5d_restored().known_order_ids",
    )
    update_manifest_stage5d_hash(root)


def mutate_runtime_restored_closed_boundary_guard_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "|| admission.runtime_host_attached()",
        "|| false",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_blocked_retained_capability_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "injected: Box<Stage5dRiskGateInjectedPaperStrategy>,",
        "snapshot_id_only: String,",
    )
    update_manifest_stage5d_hash(root)


def mutate_runtime_restored_terminal_retry_enabled(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "    pub fn retry_capability_available(&self) -> bool {\n        false\n    }",
        "    pub fn retry_capability_available(&self) -> bool {\n        true\n    }",
    )
    update_manifest_stage5d_hash(root)


def mutate_runtime_restored_lifecycle_notification_guard_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "&& bootstrap_notified_at <= restored_at",
        "true",
    )
    update_manifest_stage5d_hash(root)


def mutate_runtime_restored_flat_side_exact_guard_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "    if *current_side != expected_side {",
        "    if expected_side.is_some() && *current_side != expected_side {",
    )
    update_manifest_stage5d_hash(root)


def mutate_runtime_restored_r4_source_prebind_proof_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "positive path must use exact source semantic state before Stage 5D binding",
        "r4 mutated source prebind proof removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r4_current_shadow_matrix_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "stage5d_b2bd1r3_source_produced_current_shadow_long_short_and_realized_pnl_restore",
        "stage5d_b2bd1r3_source_produced_current_shadow_matrix_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r4_single_row_restored_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "completed single-row recovery must reach restored transition",
        "r4 mutated single-row restored proof removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r4_multi_row_restored_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "completed multi-row recovery must reach restored transition",
        "r4 mutated multi-row restored proof removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r4_actual_long_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        '(3.0, "long", crate::hybrid_intraday::Side::Long)',
        '(3.0, "long-removed", crate::hybrid_intraday::Side::Long)',
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r4_actual_short_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        '(-3.0, "short", crate::hybrid_intraday::Side::Short)',
        '(-3.0, "short-removed", crate::hybrid_intraday::Side::Short)',
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r4_known_order_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "r4 non-empty known-order index must be preserved",
        "r4 mutated known-order proof removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r4_pending_request_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "r4 non-empty pending-request index must be preserved",
        "r4 mutated pending-request proof removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r4_blocked_fingerprint_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_all(
        root / rel,
        "blocked.stage5d_test_strategy_state_fingerprint()",
        "blocked.stage5d_test_strategy_state_fingerprint_removed()",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r4_compilefail_private_field_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "let _raw_bootstrapped = injected.bootstrapped;",
        "let _raw_bootstrapped_removed = ();",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r4_compilefail_private_bridge_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "use strategy_runtime_core::stage5c_paper_host::stage5d_notify_runtime_state_restored_bridge_at;",
        "use strategy_runtime_core::stage5d_persistence::stage5d_notify_runtime_state_restored;",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r4_compilefail_consumed_input_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "let _second = stage5d_notify_runtime_state_restored(injected);",
        "let _second_removed = ();",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r5_strict_helper_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_all(
        root / rel,
        "riskgate_enabled_strict_bootstrapped_fixture_with_evidence",
        "riskgate_enabled_non_strict_bootstrapped_fixture_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r5_known_order_strict_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "r5 strict JSON round-trip known-order index evidence",
        "r5 mutated known-order strict evidence removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r5_not_paper_only_blocker_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_all(
        root / rel,
        "not_paper_only_boundary",
        "paper_only_blocker_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r5_ownership_table_removed(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/5d-b2b-d1-r5-review-gate-summary.md",
        "Stage 5D-b2b-d1-r5 blocker ownership table",
        "Stage 5D-b2b-d1-r5 ownership removed",
    )


def mutate_runtime_restored_r6_strict_long_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "r6 strict JSON round-trip actual Long broker-position evidence",
        "r6 mutated strict Long broker-position evidence removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r6_strict_short_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "r6 strict JSON round-trip actual Short broker-position evidence",
        "r6 mutated strict Short broker-position evidence removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r6_strict_known_order_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "r6 strict JSON round-trip known-order index evidence",
        "r6 mutated strict known-order evidence removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r6_strict_pending_request_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "r6 strict JSON round-trip pending-request index evidence",
        "r6 mutated strict pending-request evidence removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r6_common_blocked_helper_bypassed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "r6 representable blockers use common callback-zero helper",
        "r6 mutated common blocked helper proof removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_r6_quantity_ownership_removed(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage5d-b2bd1-r6-blocker-ownership.json",
        '"case_id": "broker_quantity_not_representable"',
        '"case_id": "broker_quantity_removed"',
    )


def mutate_runtime_restored_r6_ownership_stage_changed(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage5d-b2bd1-r6-blocker-ownership.json",
        '"owning_stage": "Stage 5D-b2b-a"',
        '"owning_stage": "Stage 5D-b2b-d"',
    )


def mutate_runtime_restored_r6_non_ack_decision_removed(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage5d-b2bd1-r6-blocker-ownership.json",
        '"case_id": "non_acknowledged_recovery_decision"',
        '"case_id": "non_acknowledged_recovery_decision_removed"',
    )


def mutate_runtime_restored_r6_expiry_ownership_removed(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage5d-b2bd1-r6-blocker-ownership.json",
        '"case_id": "admission_expired"',
        '"case_id": "admission_expired_removed"',
    )


def mutate_runtime_restored_r6_timestamp_ownership_removed(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage5d-b2bd1-r6-blocker-ownership.json",
        '"case_id": "lifecycle_timestamp_reversal_before_persisted"',
        '"case_id": "lifecycle_timestamp_reversal_removed"',
    )


def mutate_runtime_restored_r6_identity_generation_ownership_removed(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage5d-b2bd1-r6-blocker-ownership.json",
        '"case_id": "riskgate_generation_mismatch"',
        '"case_id": "riskgate_generation_removed"',
    )


def mutate_runtime_restored_final_canonical_export_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_all(
        root / rel,
        "stage5d_export_canonical_envelope_from_runtime",
        "stage5d_export_canonical_envelope_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_final_restart_matrix_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "stage5d_final_canonical_export_restart_matrix_flat_long_short",
        "stage5d_final_restart_matrix_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_final_post_export_mutation_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "stage5d_final_canonical_export_rejects_post_export_mutation_at_restart_boundary",
        "stage5d_final_post_export_mutation_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_final_recovery_index_binding_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "stage5d_final_canonical_export_binds_recovery_indexes_from_source_state",
        "stage5d_final_recovery_index_binding_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_final_package_export_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_all(
        root / rel,
        "stage5d_export_canonical_restart_package_from_runtime",
        "stage5d_export_canonical_restart_package_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_final_package_decode_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_all(
        root / rel,
        "Stage5dCanonicalRestartPackage::from_json_str_strict",
        "Stage5dCanonicalRestartPackage::from_json_str_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_final_package_corruption_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "stage5d_final_restart_package_rejects_evidence_and_package_corruption",
        "stage5d_final_restart_package_corruption_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_final_clean_process_removed(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        root / rel,
        "stage5d_final_clean_process_restart_does_not_reuse_poisoned_source_runtime",
        "stage5d_final_clean_process_restart_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_final_inventory_missing(root: Path) -> None:
    (root / "docs/stage-5/stage5d-final-restart-r2-scenario-inventory.json").unlink()


def mutate_runtime_restored_final_inventory_duplicate(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage5d-final-restart-r2-scenario-inventory.json",
        '"case_id": "positive_broker_consistent_open_long"',
        '"case_id": "positive_clean_flat"',
    )


def mutate_runtime_restored_final_r2_positive_matrix_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_final_r2_package_positive_full_matrix_and_stage5c_continuation",
        "stage5d_final_r2_positive_matrix_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_final_r2_source_callback_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_final_r2_package_source_callback_current_shadow_matrix",
        "stage5d_final_r2_source_callback_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_final_r2_crash_store_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_final_r2_package_crash_store_replay_matrix",
        "stage5d_final_r2_crash_store_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_final_r2_negative_matrix_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_final_r2_package_negative_matrix_fails_closed",
        "stage5d_final_r2_negative_matrix_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_final_r2_golden_vectors_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_final_r2_package_golden_vectors_are_pinned_and_deterministic",
        "stage5d_final_r2_golden_vectors_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_final_r2_inventory_missing(root: Path) -> None:
    (root / "docs/stage-5/stage5d-final-restart-r2-scenario-inventory.json").unlink()


def mutate_runtime_restored_final_r2_inventory_reduced(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage5d-final-restart-r2-scenario-inventory.json",
        '"case_id": "positive_pending_entry"',
        '"case_id": "positive_pending_entry_removed"',
    )


def mutate_runtime_restored_final_r2_inventory_helper_owner(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage5d-final-restart-r2-scenario-inventory.json",
        '"owning_test": "stage5d_final_r2_package_positive_full_matrix_and_stage5c_continuation"',
        '"owning_test": "stage5d_test_closed_boundary_flags"',
    )


def mutate_runtime_restored_final_r2_stage5c_warmup_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "r2 Stage 5C history warmup continuation must succeed",
        "r2 Stage 5C history warmup continuation removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_runtime_restored_final_r2_package_full_validation_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "validate_full_contract",
        "validate_package_checksum",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3a_reproduction_test_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_final_r3a_source_pending_entry_full_restart_matrix",
        "stage5d_final_r3a_source_pending_entry_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3a_post_apply_private_equality_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "actual private partial-entry timer after private apply must equal source",
        "actual private partial-entry timer equality removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3a_post_apply_semantic_equality_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "actual fresh Strategy::state after private apply must preserve exact semantic pending-entry field",
        "actual fresh Strategy::state semantic equality removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3a_restored_callback_moved_before_private_apply(root: Path) -> None:
    insert_before(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "        let applied = expect_stage5d_ok(\n            stage5d_apply_runtime_private_extension(bound),\n            \"r3a source pending private extension must apply\",",
        "        let restored = stage5d_test_assert_injected_restores_indexes_once(\n",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3a_mr_long_short_mapping_swapped(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "Self::MrLong => Stage5dLifecycleReason::MorningMeanReversionLong",
        "Self::MrLong => Stage5dLifecycleReason::MorningMeanReversionShort",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3a_bo_reason_mapping_changed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "Self::BoLong => Stage5dLifecycleReason::BreakoutLong",
        "Self::BoLong => Stage5dLifecycleReason::BreakoutShort",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3a_mr_stop_take_dropped(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "entry.stop_price.is_some() && entry.take_price.is_some()",
        "entry.stop_price.is_some() || entry.take_price.is_some()",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3a_incomplete_mr_accepted(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "incomplete MR stop/take must fail closed after canonical package decode",
        "incomplete MR stop/take accepted",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3a_owner_side_reason_mismatch_accepted(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "owner/side/reason mismatch must fail closed after canonical package decode",
        "owner/side/reason mismatch accepted",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3a_unauthorized_set_state_source_change(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs",
        "entry_style: EntryStyle::Market,",
        "entry_style: EntryStyle::Bracket,",
    )
    update_manifest_bridge_current_and_stripped_hash(
        root, "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs"
    )


def mutate_final_r3_resumption_inventory_removed(root: Path) -> None:
    (root / "docs/stage-5/stage5d-final-restart-r3-scenario-inventory.json").unlink()


def mutate_final_r3_resumption_r3a_reuse_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "r3a_r1_source_pending_reused",
        "r3a_r1_source_pending_reuse_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_inventory_row(
    root: Path,
    case_id: str,
    *,
    execution_status: str | None = None,
    owning_test: object = "__stage5d_unchanged__",
    remove_row: bool = False,
    **extra_fields: object,
) -> None:
    owning_unchanged = "__stage5d_unchanged__"
    inventory_path = root / "docs/stage-5/stage5d-final-restart-r3-scenario-inventory.json"
    inventory = json.loads(inventory_path.read_text())
    rows = inventory["scenario_rows"]
    for index, row in enumerate(rows):
        if row.get("case_id") != case_id:
            continue
        if remove_row:
            del rows[index]
        else:
            if execution_status is not None:
                row["execution_status"] = execution_status
            if owning_test != owning_unchanged:
                row["owning_test"] = owning_test
            for key, value in extra_fields.items():
                row[key] = value
        inventory_path.write_text(json.dumps(inventory, indent=2, ensure_ascii=False) + "\n")
        return
    raise RuntimeError(f"r3 inventory case not found: {case_id}")


def mutate_final_r3_resumption_clean_flat_prematurely_promoted(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_clean_flat",
        execution_status="accepted_r3a_r1_source_produced",
        owning_test="stage5d_final_r3a_source_pending_entry_full_restart_matrix",
    )


def mutate_final_r3_resumption_current_shadow_prematurely_promoted(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_current_shadow_long",
        execution_status="accepted_r3a_r1_source_produced",
        owning_test="stage5d_final_r3a_source_pending_entry_full_restart_matrix",
    )


def mutate_final_r3_resumption_unapproved_retained_status(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_clean_flat",
        execution_status="retained_from_r2_executable",
    )


def mutate_final_r3_resumption_nonexistent_owning_test(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_clean_flat",
        owning_test="stage5d_final_r3_missing_owner",
    )


def mutate_final_r3_resumption_false_resumption_owner(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_clean_flat",
        owning_test="stage5d_final_r3_resumption_inventory_and_r3a_r1_reuse",
    )


def mutate_final_r3_resumption_todo_set_reduced(root: Path) -> None:
    mutate_final_r3_inventory_row(root, "positive_clean_flat", remove_row=True)


def mutate_final_r3_resumption_accepted_r3a_downgraded(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_mr_long_bracket_pending_entry",
        execution_status="todo_source_produced",
        owning_test=None,
    )


def mutate_final_r3_resumption_stage5e_marker_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5e_closed",
        "stage5e_reopened",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_resumption_todo_non_null_owner(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_single_pending_riskgate_finalization",
        owning_test="stage5d_final_r3_resumption_inventory_and_r3a_r1_reuse",
    )


def mutate_final_r3_resumption_accepted_null_owner(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_mr_short_bracket_pending_entry",
        owning_test=None,
    )


def mutate_final_r3_positive_core_clean_fixture_substituted(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_test_r3_positive_core_source_full_restart(case)",
        "stage5d_test_canonical_package_full_restart_with_stage5c_continuation(\"bad\", |_| {})",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_positive_core_long_direct_mutation_substituted(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "positive_core_broker_open_long_short_actual_source_lifecycle",
        "positive_core_broker_open_long_short_direct_mutation",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_positive_core_short_direct_mutation_substituted(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_broker_consistent_open_short",
        producer_kind="direct_envelope_mutation",
    )


def mutate_final_r3_positive_core_source_callback_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_test_source_broker_open_strategy",
        "stage5d_test_broker_open_fixture_strategy",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_positive_core_source_runtime_not_dropped(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "source_runtime_destroyed_before_restart_boundary",
        "source_runtime_reused_after_restart_boundary",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_positive_core_strict_decode_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "strict_package_decode_used_for_positive_core",
        "strict_package_decode_skipped_for_positive_core",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_positive_core_fresh_runtime_removed(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_broker_consistent_open_long",
        fresh_runtime_used=False,
    )


def mutate_final_r3_positive_core_post_apply_equality_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "actual post-apply/restored state must match strict source envelope",
        "actual post-apply/restored state may drift",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_positive_core_broker_truth_equality_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "actual_post_apply_broker_truth_quantity_side_checked",
        "actual_post_apply_broker_truth_quantity_side_unchecked",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_positive_core_stage5c_warmup_removed(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_clean_flat",
        stage5c_continuation_executed=False,
    )


def mutate_final_r3_positive_core_current_shadow_todo_promoted(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_current_shadow_realized_pnl",
        execution_status="accepted_r3_positive_core_r1b_source_produced",
        owning_test="stage5d_final_r3_positive_core_source_produced_full_restart_matrix",
        producer_kind="runtime_callback",
        producer_entrypoint="stage5d_test_source_current_shadow_strategy",
        canonical_package_path=True,
        source_object_destroyed=True,
        strict_decode_used=True,
        fresh_runtime_used=True,
        stage5c_continuation_executed=True,
    )


def mutate_final_r3_positive_core_current_shadow_discovery_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_final_r3_current_shadow_discovery_localizes_materialized_gap",
        "stage5d_final_r3_current_shadow_discovery_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_positive_core_nonexecuting_owner(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_clean_flat",
        owning_test="stage5d_final_r3_current_shadow_discovery_localizes_materialized_gap",
    )


def mutate_final_r3_positive_core_stage5e_or_surface_opened(root: Path) -> None:
    path = root / "docs/stage-5/stage5d-final-restart-r3-scenario-inventory.json"
    data = json.loads(path.read_text())
    data["closed_surfaces"]["runtime_live"] = True
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")


def mutate_final_r3_current_shadow_long_without_full_path(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_current_shadow_long",
        owning_test="stage5d_final_r3_current_shadow_discovery_localizes_materialized_gap",
    )


def mutate_final_r3_current_shadow_short_without_full_path(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "current_shadow_long_short_realized_pnl_source_callbacks",
        "current_shadow_short_without_source_callback_path",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_realized_without_trade_count(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "expected_trade_count,",
        "0,",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_session_lost(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_current_shadow_long",
        materialized_apply_boundary="stage5d_test_apply_approved_current_shadow_materialized_boundary",
        strict_decode_used=False,
    )


def mutate_final_r3_current_shadow_pnl_bit_drift(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        '"1.199999999999997"',
        '"1.199999999999998"',
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_signed_zero_accepted(root: Path) -> None:
    path = root / "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    replace_once(
        path,
        '"stage5d-final-r3-current-shadow-r1-long",',
        '"stage5d-final-r3-current-shadow-r1-long-signed-zero",',
    )
    replace_once(
        path,
        'Some("long"),\n                0,\n                "0.0",',
        'Some("long"),\n                0,\n                "-0.0",',
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_evidence_envelope_mismatch(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "exact_current_shadow_source_state_before_correction",
        "current_shadow_envelope_evidence_mismatch_accepted",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_materialized_apply_skipped(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "approved_current_shadow_materialized_apply_boundary_before_injection",
        "current_shadow_materialized_apply_skipped",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_materialized_apply_after_injection(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_current_shadow_short",
        materialized_apply_boundary="stage5d_test_apply_after_injection",
    )


def mutate_final_r3_current_shadow_callback_before_apply(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "owning_layer_stage5d_materialized_apply_boundary",
        "callback_before_materialized_apply",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_source_runtime_reused(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_current_shadow_realized_pnl",
        source_object_destroyed=False,
    )


def mutate_final_r3_current_shadow_direct_mutation_substituted(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_current_shadow_long",
        producer_kind="direct_envelope_mutation",
    )


def mutate_final_r3_current_shadow_generation_identity_mismatch(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_current_shadow_short",
        producer_entrypoint="stage5d_test_source_wrong_identity_shadow_strategy",
    )


def mutate_final_r3_current_shadow_stage5e_or_surface_opened(root: Path) -> None:
    path = root / "docs/stage-5/stage5d-final-restart-r3-scenario-inventory.json"
    data = json.loads(path.read_text())
    data["closed_surfaces"]["dispatch"] = True
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")


def mutate_final_r3_current_shadow_r1r1_production_boundary_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_apply_validated_materialized_riskgate_for_restart",
        "stage5d_apply_validated_materialized_riskgate_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_boundary_cfg_test_only(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "pub(crate) fn stage5d_apply_validated_materialized_riskgate_for_restart(",
        "#[cfg(test)]\npub(crate) fn stage5d_apply_validated_materialized_riskgate_for_restart(",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_raw_envelope_authority(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "pub(crate) fn stage5d_apply_validated_materialized_riskgate_for_restart(\n    strategy: &mut crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,\n    validated_envelope: Stage5dValidatedPersistenceEnvelope,",
        "pub(crate) fn stage5d_apply_validated_materialized_riskgate_for_restart(\n    strategy: &mut crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,\n    envelope: &Stage5dPersistenceEnvelope,",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_raw_strategy_extractor(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "\n// stage5d_raw_strategy_extractor\n",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_apply_after_bootstrap(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "approved_current_shadow_materialized_apply_boundary_before_injection",
        "current_shadow_materialized_apply_after_bootstrap",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_apply_after_injection(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "owning_layer_stage5d_materialized_apply_boundary",
        "current_shadow_materialized_apply_after_injection",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_callback_before_apply(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "current_shadow_stage5c_continuation_executed",
        "current_shadow_callback_before_materialized_apply",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_blocked_loses_capability(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "input_capability_preserved",
        "input_capability_lost",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_partial_mutation_on_block(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "strategy.on_risk_gate_state(&apply_state.riskgate_state)",
        "direct_current_shadow_materialized_mutation",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_identity_binding_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "LedgerIdentityMismatch",
        "LedgerIdentityBypassed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_generation_binding_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "LedgerGenerationMismatch",
        "LedgerGenerationBypassed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_ledger_tail_binding_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "LedgerTailMismatch",
        "LedgerTailBypassed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_pnl_binding_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "current_shadow_pnl_points",
        "current_shadow_pnl_points_bypassed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_builder_accepts_stale_source(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_validate_canonical_restart_export_self_consistency(",
        "stage5d_validate_canonical_restart_export_self_consistency_bypassed(",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_unrestorable_committed_package(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "current_shadow_no_committed_strict_package_then_materialized_mismatch",
        "current_shadow_unrestorable_committed_package_allowed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_lifecycle_fields_overwritten(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "\n// source_current_shadow_lifecycle_overwrite\n",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_field_level_proof_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "current_shadow_field_level_mismatch_fields_4",
        "current_shadow_field_level_mismatch_fields_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_current_shadow_r1r1_stage5e_or_surface_opened(root: Path) -> None:
    mutate_final_r3_current_shadow_stage5e_or_surface_opened(root)


def mutate_final_r3_operational_source_callback_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "operational_state_partial_post_restore_no_duplicate_callback",
        "operational_state_partial_entry_callback_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_operational_direct_substitution(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "operational_state_no_direct_strategy_state_mutation_as_producer",
        "direct_operational_state_mutation_substitution",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_operational_source_runtime_reused(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "operational_state_probe_uses_restored_runtime_not_source_runtime",
        "operational_state_source_runtime_reused_after_restart_boundary",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_operational_strict_decode_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "strict_package_decode_used_for_operational_state",
        "strict_package_decode_skipped_for_operational_state",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_operational_private_apply_moved(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "operational_state_private_apply_before_bootstrap",
        "operational_state_private_apply_after_bootstrap",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_operational_lifecycle_equality_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "operational_state_pending_exit_request_id_equality_assertion",
        "operational_state_lifecycle_equality_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_operational_partial_timer_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "operational_state_partial_timeout_residual_quantity_assertion",
        "operational_state_partial_timer_quantity_evidence_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_operational_deferred_entry_stop_take_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "operational_state_deferred_entry_one_time_reissue_assertion",
        "operational_state_deferred_entry_stop_take_reason_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_operational_safe_mode_entry_block_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "operational_state_safe_mode_post_restore_entry_attempt_callback",
        "operational_state_safe_mode_entry_block_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_operational_stage5c_continuation_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_test_assert_restored_operational_behavior(case, restored, &strict_envelope)",
        "operational_state_stage5c_continuation_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_operational_premature_next_group_promotion(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_non_empty_known_order_index",
        execution_status="accepted_r3_operational_state_r1_source_produced",
        owning_test="stage5d_final_r3_operational_state_r1_source_produced_full_restart_matrix",
    )


def mutate_final_r3_operational_stage5e_or_surface_opened(root: Path) -> None:
    mutate_final_r3_current_shadow_stage5e_or_surface_opened(root)


def mutate_final_r3_recovery_index_production_boundary_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "recovery_index_r1r1_production_working_set_bootstrap_used",
        "recovery_index_r1r1_test_only_working_set_bootstrap_substituted",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_recovery_index_stop_truth_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "recovery_index_r1r1_working_stop_truth_source_produced",
        "recovery_index_r1r1_working_stop_truth_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_recovery_index_negative_matrix_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "recovery_index_r1r1_negative_matrix_executed",
        "recovery_index_r1r1_negative_matrix_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_recovery_index_pending_field_proof_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "recovery_index_r1r1_pending_request_field_level_assertions",
        "recovery_index_r1r1_pending_field_proof_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_final_r3_recovery_index_tp_sl_swap_proof_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "recovery_index_r1r1_tp_sl_swap_fails_closed",
        "recovery_index_r1r1_swapped_protective_identity_proof_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_recovery_r1r3_unbroken_path_reconstruction_introduced(root: Path) -> None:
    insert_before(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "        expect_stage5d_bootstrap_ok(\n            stage5d_notify_working_set_broker_truth_bootstrap_at(",
        "        let _r1r3_forbidden_reconstruction = \"stage5d_into_parts(\";\n",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_recovery_r1r3_authoritative_admission_moved_after_private_apply(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "\"broker truth must contain the expected working order before Stage 5C closed-boundary working-order bootstrap\"",
        "\"broker truth check moved after private apply\"",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_recovery_r1r3_production_working_set_call_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "            stage5d_notify_working_set_broker_truth_bootstrap_at(\n                applied,\n                validated_stop_truth,\n                notification_now,\n            ),",
        "            stage5d_notify_working_set_broker_truth_bootstrap_removed(\n                applied,\n                validated_stop_truth,\n                notification_now,\n            ),",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_recovery_r1r3_working_set_coordinator_not_crate_visible(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "pub(crate) fn stage5d_notify_working_set_broker_truth_bootstrap_at(",
        "fn stage5d_notify_working_set_broker_truth_bootstrap_at(",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_recovery_r1r3_validated_stop_truth_roundtrip_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "serde_json::to_string(&stop_truth)",
        "serde_json::to_string_pretty(&stop_truth)",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_recovery_r1r3_raw_stop_truth_consumed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_validate_supplemental_working_stop_truth(&applied.envelope, stop_truth)",
        "stage5d_validate_supplemental_working_stop_truth_raw_bypass(&applied.envelope, stop_truth)",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_recovery_r1r3_normalization_call_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "crate::stage5c_paper_host::stage5d_normalize_broker_owned_ids_for_closed_restore_bridge(",
        "crate::stage5c_paper_host::stage5d_normalize_broker_owned_ids_removed(",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_recovery_r1r3_normalization_block_capability_lost(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
        "pub(crate) bootstrapped: Box<Stage5cBootstrappedPaperStrategy>,",
        "pub(crate) bootstrapped_lost: (),",
    )
    update_manifest_bridge_current_and_stripped_hash(
        root, "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    )


def mutate_recovery_r1r3_normalization_partial_mutation_accepted(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
        "    if !tp_is_frozen || !sl_stop_is_frozen || !sl_exchange_is_frozen {\n",
        "    *tp_order_id = None;\n    if !tp_is_frozen || !sl_stop_is_frozen || !sl_exchange_is_frozen {\n",
    )
    update_manifest_bridge_current_and_stripped_hash(
        root, "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    )


def mutate_recovery_r1r3_duplicate_sl_callback_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "let duplicate_sl = probe.on_stop_order(",
        "let duplicate_sl = duplicate.clone();\n                let _removed_duplicate_sl = probe.on_stop_order_removed(",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_recovery_r1r3_terminal_sl_callback_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "let terminal_sl = probe.on_stop_order(",
        "let terminal_sl = terminal.clone();\n                let _removed_terminal_sl = probe.on_stop_order_removed(",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_recovery_r1r3_exact_sl_set_assertion_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "\"working protective duplicate SL callback must preserve exact expected SL set\"",
        "\"working protective duplicate SL set check removed\"",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_recovery_r1r3_pending_stage_assertion_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "\"restored-before-terminal pending entry\"",
        "\"restored pending entry stage proof removed\"",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_recovery_r1r3_pending_terminal_orphan_accepted(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "\"terminal resolution must not leave orphan pending request in runtime state\"",
        "\"terminal orphan accepted\"",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_recovery_r1r3_stage5c_continuation_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_test_warmup_stage5c_history_at(",
        "stage5d_test_warmup_stage5c_history_removed(",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_recovery_r1r3_final_group_or_stage5e_prematurely_opened(root: Path) -> None:
    mutate_final_r3_inventory_row(
        root,
        "positive_single_pending_riskgate_finalization",
        execution_status="accepted_r3_recovery_index_r1_source_produced",
        owning_test="stage5d_final_r3_recovery_index_r1_source_produced_full_restart_matrix",
    )


def mutate_riskrec_stage5d_token(root: Path, old: str, new: str) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        old,
        new,
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_riskrec_single_source_producer_removed(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_source_finalization_producer_entrypoint",
        "riskrec_source_finalization_producer_removed",
    )


def mutate_riskrec_runtime_pending_direct_inserted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_runtime_pending_created_by_source_lifecycle",
        "riskrec_runtime_pending_inserted_directly",
    )


def mutate_riskrec_durable_outbox_direct_inserted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_durable_outbox_created_by_canonical_export_input",
        "riskrec_durable_outbox_inserted_directly",
    )


def mutate_riskrec_identity_mismatch_accepted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_single_row_equality_runtime_outbox_ledger_plan",
        "riskrec_identity_mismatch_accepted",
    )


def mutate_riskrec_materialized_mismatch_accepted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_strict_decode_fresh_runtime_used",
        "materialized mismatch accepted",
    )


def mutate_riskrec_single_action_omitted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_checkpoint_restart_matrix_executed",
        "riskrec_single_recovery_action_omitted",
    )


def mutate_riskrec_single_action_duplicated(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_idempotent_replay_verified",
        "riskrec_single_recovery_action_duplicated",
    )


def mutate_riskrec_multi_order_reversed(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_multi_row_stable_order_assertions",
        "riskrec_multi_row_order_reversed",
    )


def mutate_riskrec_multi_set_comparison(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_multi_row_stable_order_assertions",
        "ordered multi-row compared as set",
    )


def mutate_riskrec_second_action_before_checkpoint(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_checkpoint_restart_matrix_executed",
        "riskrec_second_action_before_checkpoint_allowed",
    )


def mutate_riskrec_retry_duplicates_ledger_append(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC idempotent_replay=true',
        'STAGE5D_RISKREC idempotent_replay=false',
    )


def mutate_riskrec_retry_duplicates_materialized(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_checkpoint_restart_matrix_executed",
        "retry duplicates materialized update",
    )


def mutate_riskrec_retry_duplicates_runtime_ack(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC callback_exactly_once=true',
        'STAGE5D_RISKREC callback_exactly_once=false',
    )


def mutate_riskrec_final_commit_checkpoint_omitted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_complete_plan_noop_from_final_checkpoint",
        "riskrec_final_commit_checkpoint_omitted",
    )


def mutate_riskrec_complete_plan_produces_action(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_complete_plan_noop_from_final_checkpoint",
        "positive_complete_plan_produces_action",
    )


def mutate_riskrec_complete_plan_changes_state(root: Path) -> None:
    replace_once(
        root / "docs/stage-5/stage5d-final-restart-r3-scenario-inventory.json",
        "\"complete_plan_noop_checked\": true",
        "\"complete_plan_changes_state\": true",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_riskrec_source_runtime_reused(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_source_runtime_destroyed_before_decode",
        "// source runtime reused",
    )


def mutate_riskrec_strict_decode_removed(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_strict_decode_fresh_runtime_used",
        "Stage5dCanonicalRestartPackage::from_json_str_lenient(&package_json)",
    )


def mutate_riskrec_committed_guard_removed(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "\"checkpoint_state\":\"full_written_uncommitted\"",
        "\"checkpoint_state\":\"committed\"",
    )


def mutate_riskrec_partial_write_accepted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "riskrec_store_state_matrix_executed",
        "riskrec_partial_write_accepted",
    )


def mutate_riskrec_full_uncommitted_accepted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "STAGE5D_RISKREC durable_store_matrix=true",
        "durable_full_written_uncommitted_accepted",
    )


def mutate_riskrec_restored_callback_duplicated(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "STAGE5D_RISKREC callback_exactly_once=true",
        "STAGE5D_RISKREC callback_exactly_once=false",
    )


def mutate_riskrec_stage5c_continuation_removed(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "STAGE5D_RISKREC stage5c_continuation=true",
        "STAGE5D_RISKREC stage5c_continuation=false",
    )


def mutate_riskrec_stage5e_opened(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        "STAGE5D_RISKREC stage5e_closed=true",
        "STAGE5D_RISKREC stage5e_closed=false",
    )


def mutate_riskrec_r1r1_transition_removed(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC production_recovery_actions=true',
        'STAGE5D_RISKREC production_recovery_actions=false',
    )


def mutate_riskrec_r1r1_test_row_executor_substituted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC production_recovery_actions=true',
        'STAGE5D_RISKREC test_row_executor_substituted=true',
    )


def mutate_riskrec_r1r1_second_action_selected(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC checkpoint_restart_matrix=true',
        'STAGE5D_RISKREC checkpoint_restart_matrix=false',
    )


def mutate_riskrec_r1r1_ledger_append_omitted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC production_recovery_actions=true',
        'STAGE5D_RISKREC ledger_append_omitted=true',
    )


def mutate_riskrec_r1r1_ledger_append_duplicated(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC idempotent_replay=true',
        'STAGE5D_RISKREC ledger_append_duplicated=true',
    )


def mutate_riskrec_r1r1_materialized_update_omitted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC production_recovery_actions=true',
        'STAGE5D_RISKREC materialized_update_omitted=true',
    )


def mutate_riskrec_r1r1_materialized_update_duplicated(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC idempotent_replay=true',
        'STAGE5D_RISKREC materialized_update_duplicated=true',
    )


def mutate_riskrec_r1r1_runtime_ack_omitted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC callback_exactly_once=true',
        'STAGE5D_RISKREC runtime_ack_omitted=true',
    )


def mutate_riskrec_r1r1_runtime_ack_duplicated(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC callback_exactly_once=true',
        'STAGE5D_RISKREC runtime_ack_duplicated=true',
    )


def mutate_riskrec_r1r1_final_receipt_omitted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC final_checkpoint_committed=true',
        'STAGE5D_RISKREC final_checkpoint_committed=false',
    )


def mutate_riskrec_r1r1_final_receipt_forged(root: Path) -> None:
    replace_once(
        root / 'tests/fixtures/stage5/stage5d_riskrec_single_pending_golden.json',
        '"final_commit_receipt_fingerprint": "65add0cc96619fdcdbe2e876358fbaf550d8a04c66d2945cad3d10ea4d92f3b3"',
        '"final_commit_receipt_fingerprint": "0"',
    )


def mutate_riskrec_r1r1_checkpoint_package_not_persisted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC durable_store_matrix=true',
        'STAGE5D_RISKREC durable_store_matrix=false',
    )


def mutate_riskrec_r1r1_store_handles_reused(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC checkpoint_restart_matrix=true',
        'STAGE5D_RISKREC store_handles_reused=true',
    )


def mutate_riskrec_r1r1_partial_file_accepted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC durable_store_matrix=true',
        'STAGE5D_RISKREC partial_file_accepted=true',
    )


def mutate_riskrec_r1r1_full_uncommitted_accepted(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC durable_store_matrix=true',
        'STAGE5D_RISKREC full_uncommitted_accepted=true',
    )


def mutate_riskrec_r1r1_complete_direct_frontier(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'riskrec_complete_plan_noop_from_final_checkpoint',
        'riskrec_complete_plan_direct_frontier',
    )


def mutate_riskrec_r1r1_complete_plan_action(root: Path) -> None:
    replace_once(
        root / 'tests/fixtures/stage5/stage5d_riskrec_complete_noop_golden.json',
        '"already_acknowledged"',
        '',
    )


def mutate_riskrec_r1r1_stage5c_warmup_removed(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC stage5c_continuation=true',
        'STAGE5D_RISKREC stage5c_continuation=false',
    )


def mutate_riskrec_r1r1_restored_callback_duplicated(root: Path) -> None:
    mutate_riskrec_stage5d_token(
        root,
        'STAGE5D_RISKREC callback_exactly_once=true',
        'STAGE5D_RISKREC callback_exactly_once=false',
    )


def mutate_riskrec_r1r1_golden_hash_changed(root: Path) -> None:
    replace_once(
        root / 'tests/fixtures/stage5/stage5d_riskrec_ordered_multi_row_golden.json',
        '"package_sha256": "e61c1f7ba98b32b06c1727eef24e0bd3e1bae3b5445b5f57dfff84fc6a7352ec"',
        '"package_sha256": "0"',
    )


def mutate_riskrec_r1r1_stage5e_opened(root: Path) -> None:
    replace_once(
        root / "tests/fixtures/stage5/stage5d_riskrec_complete_noop_golden.json",
        "\"stage5e_closed\": true",
        "\"stage5e_closed\": false",
    )


def mutate_riskrec_r1r3_exact_package_fixture_changed(root: Path) -> None:
    append_text(
        root / "tests/fixtures/stage5/stage5d_riskrec_single_pending_package.json",
        "\n",
    )


def mutate_riskrec_r1r3_exact_receipt_fixture_changed(root: Path) -> None:
    append_text(
        root / "tests/fixtures/stage5/stage5d_riskrec_single_pending_final_receipt.json",
        "\n",
    )


def mutate_riskrec_r1r3_summary_wrong_fixture_path(root: Path) -> None:
    replace_once(
        root / "tests/fixtures/stage5/stage5d_riskrec_single_pending_golden.json",
        "tests/fixtures/stage5/stage5d_riskrec_single_pending_package.json",
        "tests/fixtures/stage5/stage5d_riskrec_wrong_package.json",
    )


def mutate_riskrec_r1r3_summary_wrong_fixture_sha(root: Path) -> None:
    replace_once(
        root / "tests/fixtures/stage5/stage5d_riskrec_single_pending_golden.json",
        '"package_fixture_sha256": "304b01de4df838952ff07637346e218379f808cb706b24c0165f33c45556bb6e"',
        '"package_fixture_sha256": "0000000000000000000000000000000000000000000000000000000000000000"',
    )


def mutate_riskrec_r1r3_committed_read_validator_removed(root: Path) -> None:
    replace_all(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_validate_riskgate_recovery_committed_read(",
        "stage5d_validate_riskgate_recovery_committed_read_removed(",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_riskrec_r1r3_forged_matrix_removed(root: Path) -> None:
    replace_once(
        root / "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_final_r3_riskgate_recovery_r1r3_forged_receipts_fail_closed",
        "stage5d_final_r3_riskgate_recovery_r1r3_forged_receipts_removed",
    )
    update_stage5d_semantic_mutation_hashes(root)


def mutate_riskrec_r1r1_source_rollover_removed(root: Path) -> None:
    mutate_riskrec_single_source_producer_removed(root)


def mutate_riskrec_r1r1_runtime_pending_direct(root: Path) -> None:
    mutate_riskrec_runtime_pending_direct_inserted(root)


def mutate_riskrec_r1r1_durable_outbox_direct(root: Path) -> None:
    mutate_riskrec_durable_outbox_direct_inserted(root)


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
    ("private_layout_extension_removed", mutate_private_layout_extension_removed, "Stage 5C private layout extension contract mismatch"),
    ("private_layout_extension_hash_changed", mutate_private_layout_extension_hash_changed, "Stage 5C private layout extension contract mismatch"),
    ("private_layout_extension_additional_path", mutate_private_layout_extension_additional_path, "Stage 5C private layout extension contract mismatch"),
    ("private_layout_extension_wrapper_path", mutate_private_layout_extension_wrapper_path, "Stage 5C private layout extension contract mismatch"),
    ("private_layout_extension_lib_path", mutate_private_layout_extension_lib_path, "Stage 5C private layout extension contract mismatch"),
    ("private_layout_self_authorized_semantic_drift", mutate_private_layout_self_authorized_semantic_drift, "Stage 5C private layout extension contract mismatch"),
    ("private_layout_extension_reason_id_changed", mutate_private_layout_extension_reason_id_changed, "Stage 5C private layout extension contract mismatch"),
    ("bootstrap_bridge_runtime_compat_direct_call", mutate_bootstrap_bridge_runtime_compat_direct_call, "Stage 5D bootstrap bridge reference outside allowlist"),
    ("bootstrap_bridge_runtime_compat_alias_call", mutate_bootstrap_bridge_runtime_compat_alias_call, "Stage 5D bootstrap bridge reference outside allowlist"),
    ("bootstrap_bridge_runtime_compat_forwarding_wrapper", mutate_bootstrap_bridge_runtime_compat_forwarding_wrapper, "Stage 5D bootstrap bridge reference outside allowlist"),
    ("bootstrap_bridge_runtime_compat_function_reference", mutate_bootstrap_bridge_runtime_compat_function_reference, "Stage 5D bootstrap bridge reference outside allowlist"),
    ("bootstrap_bridge_second_stage5d_call", mutate_bootstrap_bridge_second_stage5d_call, "controlled source semantic extension contract mismatch"),
    ("riskgate_bridge_runtime_compat_direct_call", mutate_riskgate_bridge_runtime_compat_direct_call, "Stage 5D riskgate bridge reference outside allowlist"),
    ("riskgate_bridge_runtime_compat_alias_call", mutate_riskgate_bridge_runtime_compat_alias_call, "Stage 5D riskgate bridge reference outside allowlist"),
    ("riskgate_bridge_runtime_compat_forwarding_wrapper", mutate_riskgate_bridge_runtime_compat_forwarding_wrapper, "Stage 5D riskgate bridge reference outside allowlist"),
    ("riskgate_bridge_runtime_compat_function_reference", mutate_riskgate_bridge_runtime_compat_function_reference, "Stage 5D riskgate bridge reference outside allowlist"),
    ("riskgate_bridge_second_stage5d_call", mutate_riskgate_bridge_second_stage5d_call, "Stage 5D riskgate bridge production call count mismatch"),
    ("runtime_restored_bridge_runtime_compat_direct_call", mutate_runtime_restored_bridge_runtime_compat_direct_call, "Stage 5D runtime-restored bridge reference outside allowlist"),
    ("runtime_restored_bridge_runtime_compat_alias_call", mutate_runtime_restored_bridge_runtime_compat_alias_call, "Stage 5D runtime-restored bridge reference outside allowlist"),
    ("runtime_restored_bridge_runtime_compat_function_reference", mutate_runtime_restored_bridge_runtime_compat_function_reference, "Stage 5D runtime-restored bridge reference outside allowlist"),
    ("runtime_restored_bridge_second_stage5d_call", mutate_runtime_restored_bridge_second_stage5d_call, "Stage 5D runtime-restored bridge production call count mismatch"),
    ("runtime_restored_bridge_made_public", mutate_runtime_restored_bridge_made_public, "Stage 5D runtime-restored bridge definition contract mismatch"),
    ("runtime_restored_intent_runtime_guard_removed", mutate_runtime_restored_intent_runtime_guard_removed, "Stage 5D runtime-restored intent runtime guard missing"),
    ("runtime_restored_intent_guard_after_debug_assert", mutate_runtime_restored_intent_guard_after_debug_assert, "Stage 5D runtime-restored intent runtime guard must precede debug_assert"),
    ("runtime_restored_post_callback_exact_guard_removed", mutate_runtime_restored_post_callback_exact_guard_removed, "Stage 5D runtime-restored exact post-callback broker-truth guard missing"),
    ("runtime_restored_callback_count_hook_removed", mutate_runtime_restored_callback_count_hook_removed, "Stage 5D runtime-restored callback-count proof hook missing"),
    ("runtime_restored_post_callback_position_guard_removed", mutate_runtime_restored_post_callback_position_guard_removed, "Stage 5D runtime-restored post-callback position guard missing"),
    ("runtime_restored_post_callback_side_guard_removed", mutate_runtime_restored_post_callback_side_guard_removed, "Stage 5D runtime-restored post-callback side guard missing"),
    ("runtime_restored_post_callback_protective_guard_removed", mutate_runtime_restored_post_callback_protective_guard_removed, "Stage 5D runtime-restored post-callback protective-id guard missing"),
    ("runtime_restored_preflight_invocation_removed", mutate_runtime_restored_preflight_invocation_removed, "Stage 5D runtime-restored preflight invocation missing"),
    ("runtime_restored_recovery_complete_guard_removed", mutate_runtime_restored_recovery_complete_guard_removed, "Stage 5D runtime-restored recovery-complete guard missing"),
    ("runtime_restored_pending_finalization_guard_removed", mutate_runtime_restored_pending_finalization_guard_removed, "Stage 5D runtime-restored pending-finalization guard missing"),
    ("runtime_restored_recovery_plan_binding_guard_removed", mutate_runtime_restored_recovery_plan_binding_guard_removed, "Stage 5D runtime-restored recovery-plan binding guard missing"),
    ("runtime_restored_recovery_index_guard_removed", mutate_runtime_restored_recovery_index_guard_removed, "Stage 5D runtime-restored recovery-index guard missing"),
    ("runtime_restored_closed_boundary_guard_removed", mutate_runtime_restored_closed_boundary_guard_removed, "Stage 5D runtime-restored closed-boundary guard missing"),
    ("runtime_restored_blocked_retained_capability_removed", mutate_runtime_restored_blocked_retained_capability_removed, "Stage 5D runtime-restored blocked retained capability missing"),
    ("runtime_restored_terminal_retry_enabled", mutate_runtime_restored_terminal_retry_enabled, "Stage 5D runtime-restored terminal retry denial missing"),
    ("runtime_restored_lifecycle_notification_guard_removed", mutate_runtime_restored_lifecycle_notification_guard_removed, "Stage 5D runtime-restored lifecycle notification timestamp guard missing"),
    ("runtime_restored_flat_side_exact_guard_removed", mutate_runtime_restored_flat_side_exact_guard_removed, "Stage 5D runtime-restored flat-side exact guard missing"),
    ("runtime_restored_r4_source_prebind_proof_removed", mutate_runtime_restored_r4_source_prebind_proof_removed, "Stage 5D runtime-restored source pre-bind exact-state proof missing"),
    ("runtime_restored_r4_current_shadow_matrix_removed", mutate_runtime_restored_r4_current_shadow_matrix_removed, "Stage 5D runtime-restored source-produced current-shadow proof missing"),
    ("runtime_restored_r4_single_row_restored_removed", mutate_runtime_restored_r4_single_row_restored_removed, "Stage 5D runtime-restored single-row recovery transition proof missing"),
    ("runtime_restored_r4_multi_row_restored_removed", mutate_runtime_restored_r4_multi_row_restored_removed, "Stage 5D runtime-restored multi-row recovery transition proof missing"),
    ("runtime_restored_r4_actual_long_removed", mutate_runtime_restored_r4_actual_long_removed, "Stage 5D runtime-restored actual Long broker-position proof missing"),
    ("runtime_restored_r4_actual_short_removed", mutate_runtime_restored_r4_actual_short_removed, "Stage 5D runtime-restored actual Short broker-position proof missing"),
    ("runtime_restored_r4_known_order_removed", mutate_runtime_restored_r4_known_order_removed, "Stage 5D runtime-restored known-order preservation proof missing"),
    ("runtime_restored_r4_pending_request_removed", mutate_runtime_restored_r4_pending_request_removed, "Stage 5D runtime-restored pending-request preservation proof missing"),
    ("runtime_restored_r4_blocked_fingerprint_removed", mutate_runtime_restored_r4_blocked_fingerprint_removed, "Stage 5D runtime-restored blocked strategy fingerprint proof missing"),
    ("runtime_restored_r4_compilefail_private_field_removed", mutate_runtime_restored_r4_compilefail_private_field_removed, "Stage 5D runtime-restored compile-fail private field proof missing"),
    ("runtime_restored_r4_compilefail_private_bridge_removed", mutate_runtime_restored_r4_compilefail_private_bridge_removed, "Stage 5D runtime-restored compile-fail private bridge proof missing"),
    ("runtime_restored_r4_compilefail_consumed_input_removed", mutate_runtime_restored_r4_compilefail_consumed_input_removed, "Stage 5D runtime-restored compile-fail consumed-input proof missing"),
    ("runtime_restored_r5_strict_helper_removed", mutate_runtime_restored_r5_strict_helper_removed, "Stage 5D runtime-restored strict round-trip helper missing"),
    ("runtime_restored_r5_known_order_strict_removed", mutate_runtime_restored_r5_known_order_strict_removed, "Stage 5D runtime-restored strict known-order proof missing"),
    ("runtime_restored_r5_not_paper_only_blocker_removed", mutate_runtime_restored_r5_not_paper_only_blocker_removed, "Stage 5D runtime-restored paper-only blocker proof missing"),
    ("runtime_restored_r5_ownership_table_removed", mutate_runtime_restored_r5_ownership_table_removed, "Stage 5D runtime-restored blocker ownership table missing"),
    ("runtime_restored_r6_strict_long_removed", mutate_runtime_restored_r6_strict_long_removed, "Stage 5D runtime-restored strict Long proof missing"),
    ("runtime_restored_r6_strict_short_removed", mutate_runtime_restored_r6_strict_short_removed, "Stage 5D runtime-restored strict Short proof missing"),
    ("runtime_restored_r6_strict_known_order_removed", mutate_runtime_restored_r6_strict_known_order_removed, "Stage 5D runtime-restored r6 strict known-order proof missing"),
    ("runtime_restored_r6_strict_pending_request_removed", mutate_runtime_restored_r6_strict_pending_request_removed, "Stage 5D runtime-restored r6 strict pending-request proof missing"),
    ("runtime_restored_r6_common_blocked_helper_bypassed", mutate_runtime_restored_r6_common_blocked_helper_bypassed, "Stage 5D runtime-restored r6 common blocked helper proof missing"),
    ("runtime_restored_r6_quantity_ownership_removed", mutate_runtime_restored_r6_quantity_ownership_removed, "Stage 5D runtime-restored r6 ownership case inventory mismatch"),
    ("runtime_restored_r6_ownership_stage_changed", mutate_runtime_restored_r6_ownership_stage_changed, "Stage 5D runtime-restored r6 ownership row 'strategy_mismatch' must not be owned by b2b-d"),
    ("runtime_restored_r6_non_ack_decision_removed", mutate_runtime_restored_r6_non_ack_decision_removed, "Stage 5D runtime-restored r6 ownership case inventory mismatch"),
    ("runtime_restored_r6_expiry_ownership_removed", mutate_runtime_restored_r6_expiry_ownership_removed, "Stage 5D runtime-restored r6 ownership case inventory mismatch"),
    ("runtime_restored_r6_timestamp_ownership_removed", mutate_runtime_restored_r6_timestamp_ownership_removed, "Stage 5D runtime-restored r6 ownership case inventory mismatch"),
    ("runtime_restored_r6_identity_generation_ownership_removed", mutate_runtime_restored_r6_identity_generation_ownership_removed, "Stage 5D runtime-restored r6 ownership case inventory mismatch"),
    ("runtime_restored_final_canonical_export_removed", mutate_runtime_restored_final_canonical_export_removed, "Stage 5D final canonical export production surface missing"),
    ("runtime_restored_final_restart_matrix_removed", mutate_runtime_restored_final_restart_matrix_removed, "Stage 5D final canonical restart matrix proof missing"),
    ("runtime_restored_final_post_export_mutation_removed", mutate_runtime_restored_final_post_export_mutation_removed, "Stage 5D final post-export mutation rejection proof missing"),
    ("runtime_restored_final_recovery_index_binding_removed", mutate_runtime_restored_final_recovery_index_binding_removed, "Stage 5D final recovery-index binding proof missing"),
    ("runtime_restored_final_package_export_removed", mutate_runtime_restored_final_package_export_removed, "Stage 5D final canonical package production surface missing"),
    ("runtime_restored_final_package_decode_removed", mutate_runtime_restored_final_package_decode_removed, "Stage 5D final package strict decode proof missing"),
    ("runtime_restored_final_package_corruption_removed", mutate_runtime_restored_final_package_corruption_removed, "Stage 5D final package corruption proof missing"),
    ("runtime_restored_final_clean_process_removed", mutate_runtime_restored_final_clean_process_removed, "Stage 5D final clean-process poison proof missing"),
    ("runtime_restored_final_inventory_missing", mutate_runtime_restored_final_inventory_missing, "Stage 5D final restart r1 scenario inventory missing"),
    ("runtime_restored_final_inventory_duplicate", mutate_runtime_restored_final_inventory_duplicate, "Stage 5D final restart r1 scenario inventory mismatch"),
    ("runtime_restored_final_r2_positive_matrix_removed", mutate_runtime_restored_final_r2_positive_matrix_removed, "Stage 5D final r2 positive full-matrix proof missing"),
    ("runtime_restored_final_r2_source_callback_removed", mutate_runtime_restored_final_r2_source_callback_removed, "Stage 5D final r2 source-callback proof missing"),
    ("runtime_restored_final_r2_crash_store_removed", mutate_runtime_restored_final_r2_crash_store_removed, "Stage 5D final r2 crash-store replay proof missing"),
    ("runtime_restored_final_r2_negative_matrix_removed", mutate_runtime_restored_final_r2_negative_matrix_removed, "Stage 5D final r2 package negative proof missing"),
    ("runtime_restored_final_r2_golden_vectors_removed", mutate_runtime_restored_final_r2_golden_vectors_removed, "Stage 5D final r2 golden-vector proof missing"),
    ("runtime_restored_final_r2_inventory_missing", mutate_runtime_restored_final_r2_inventory_missing, "Stage 5D final restart r1 scenario inventory missing"),
    ("runtime_restored_final_r2_inventory_reduced", mutate_runtime_restored_final_r2_inventory_reduced, "Stage 5D final restart r1 scenario inventory mismatch"),
    ("runtime_restored_final_r2_inventory_helper_owner", mutate_runtime_restored_final_r2_inventory_helper_owner, "Stage 5D final restart r1 scenario owning item is not a test"),
    ("runtime_restored_final_r2_stage5c_warmup_removed", mutate_runtime_restored_final_r2_stage5c_warmup_removed, "Stage 5D final r2 Stage 5C warmup continuation proof missing"),
    ("runtime_restored_final_r2_package_full_validation_removed", mutate_runtime_restored_final_r2_package_full_validation_removed, "Stage 5D final r2 full package validation proof missing"),
    ("final_r3a_reproduction_test_removed", mutate_final_r3a_reproduction_test_removed, "Stage 5D final r3a source-pending full restart proof missing"),
    ("final_r3a_post_apply_private_equality_removed", mutate_final_r3a_post_apply_private_equality_removed, "Stage 5D final r3a actual private DTO equality proof missing"),
    ("final_r3a_post_apply_semantic_equality_removed", mutate_final_r3a_post_apply_semantic_equality_removed, "Stage 5D final r3a actual semantic post-apply equality proof missing"),
    ("final_r3a_restored_callback_moved_before_private_apply", mutate_final_r3a_restored_callback_moved_before_private_apply, "Stage 5D final r3a restored callback moved before private apply"),
    ("final_r3a_mr_long_short_mapping_swapped", mutate_final_r3a_mr_long_short_mapping_swapped, "Stage 5D final r3a MR Long reason mapping proof missing"),
    ("final_r3a_bo_reason_mapping_changed", mutate_final_r3a_bo_reason_mapping_changed, "Stage 5D final r3a BO Long reason mapping proof missing"),
    ("final_r3a_mr_stop_take_dropped", mutate_final_r3a_mr_stop_take_dropped, "Stage 5D final r3a MR stop/take shape assertion missing"),
    ("final_r3a_incomplete_mr_accepted", mutate_final_r3a_incomplete_mr_accepted, "Stage 5D final r3a fail-closed MR missing stop/take proof missing"),
    ("final_r3a_owner_side_reason_mismatch_accepted", mutate_final_r3a_owner_side_reason_mismatch_accepted, "Stage 5D final r3a fail-closed owner/side/reason mismatch proof missing"),
    ("final_r3a_unauthorized_set_state_source_change", mutate_final_r3a_unauthorized_set_state_source_change, "frozen region does not match Stage 5C closure source"),
    ("final_r3_resumption_inventory_removed", mutate_final_r3_resumption_inventory_removed, "Stage 5D final r3 resumption inventory proof missing"),
    ("final_r3_resumption_r3a_reuse_removed", mutate_final_r3_resumption_r3a_reuse_removed, "Stage 5D final r3 r3a-r1 reuse proof missing"),
    ("final_r3_resumption_clean_flat_prematurely_promoted", mutate_final_r3_resumption_clean_flat_prematurely_promoted, "Stage 5D final r3 accepted executable set mismatch"),
    ("final_r3_resumption_current_shadow_prematurely_promoted", mutate_final_r3_resumption_current_shadow_prematurely_promoted, "Stage 5D final r3 accepted executable set mismatch"),
    ("final_r3_resumption_unapproved_retained_status", mutate_final_r3_resumption_unapproved_retained_status, "Stage 5D final r3 unapproved execution status"),
    ("final_r3_resumption_nonexistent_owning_test", mutate_final_r3_resumption_nonexistent_owning_test, "Stage 5D final r3 positive-core owner/status proof missing"),
    ("final_r3_resumption_false_resumption_owner", mutate_final_r3_resumption_false_resumption_owner, "Stage 5D final r3 positive-core owner/status proof missing"),
    ("final_r3_resumption_todo_set_reduced", mutate_final_r3_resumption_todo_set_reduced, "Stage 5D final r3 mandatory positive inventory mismatch"),
    ("final_r3_resumption_accepted_r3a_downgraded", mutate_final_r3_resumption_accepted_r3a_downgraded, "Stage 5D final r3 accepted executable set mismatch"),
    ("final_r3_resumption_stage5e_marker_removed", mutate_final_r3_resumption_stage5e_marker_removed, "Stage 5D final r3 Stage 5E closed marker missing"),
    ("final_r3_resumption_todo_non_null_owner", mutate_final_r3_resumption_todo_non_null_owner, "Stage 5D final r3 riskgate-recovery owner/status proof missing"),
    ("final_r3_resumption_accepted_null_owner", mutate_final_r3_resumption_accepted_null_owner, "Stage 5D final r3 r3a-r1 reuse proof missing"),
    ("final_r3_positive_core_clean_fixture_substituted", mutate_final_r3_positive_core_clean_fixture_substituted, "Stage 5D final r3 positive-core fixture substitution guard missing"),
    ("final_r3_positive_core_long_direct_mutation_substituted", mutate_final_r3_positive_core_long_direct_mutation_substituted, "Stage 5D final r3 positive-core open position package proof missing"),
    ("final_r3_positive_core_short_direct_mutation_substituted", mutate_final_r3_positive_core_short_direct_mutation_substituted, "Stage 5D final r3 positive-core producer lineage proof missing"),
    ("final_r3_positive_core_source_callback_removed", mutate_final_r3_positive_core_source_callback_removed, "Stage 5D final r3 positive-core producer lineage proof missing"),
    ("final_r3_positive_core_source_runtime_not_dropped", mutate_final_r3_positive_core_source_runtime_not_dropped, "Stage 5D final r3 positive-core r1b marker proof missing"),
    ("final_r3_positive_core_strict_decode_removed", mutate_final_r3_positive_core_strict_decode_removed, "Stage 5D final r3 positive-core r1b marker proof missing"),
    ("final_r3_positive_core_fresh_runtime_removed", mutate_final_r3_positive_core_fresh_runtime_removed, "Stage 5D final r3 positive-core producer lineage proof missing"),
    ("final_r3_positive_core_post_apply_equality_removed", mutate_final_r3_positive_core_post_apply_equality_removed, "Stage 5D final r3 positive-core actual post-apply equality proof missing"),
    ("final_r3_positive_core_broker_truth_equality_removed", mutate_final_r3_positive_core_broker_truth_equality_removed, "Stage 5D final r3 positive-core r1b marker proof missing"),
    ("final_r3_positive_core_stage5c_warmup_removed", mutate_final_r3_positive_core_stage5c_warmup_removed, "Stage 5D final r3 positive-core producer lineage proof missing"),
    ("final_r3_positive_core_current_shadow_todo_promoted", mutate_final_r3_positive_core_current_shadow_todo_promoted, "Stage 5D final r3 accepted executable set mismatch"),
    ("final_r3_positive_core_current_shadow_discovery_removed", mutate_final_r3_positive_core_current_shadow_discovery_removed, "Stage 5D final r3 current-shadow executable discovery proof missing"),
    ("final_r3_positive_core_nonexecuting_owner", mutate_final_r3_positive_core_nonexecuting_owner, "Stage 5D final r3 positive-core owner/status proof missing"),
    ("final_r3_positive_core_stage5e_or_surface_opened", mutate_final_r3_positive_core_stage5e_or_surface_opened, "Stage 5D final r3 resumption inventory closed-surface mismatch"),
    ("final_r3_current_shadow_long_without_full_path", mutate_final_r3_current_shadow_long_without_full_path, "Stage 5D final r3 current-shadow owner/status proof missing"),
    ("final_r3_current_shadow_short_without_full_path", mutate_final_r3_current_shadow_short_without_full_path, "Stage 5D final r3 current-shadow r1 marker proof missing"),
    ("final_r3_current_shadow_realized_without_trade_count", mutate_final_r3_current_shadow_realized_without_trade_count, "Stage 5D final r3 current-shadow full-path proof missing"),
    ("final_r3_current_shadow_session_lost", mutate_final_r3_current_shadow_session_lost, "Stage 5D final r3 current-shadow producer lineage proof missing"),
    ("final_r3_current_shadow_pnl_bit_drift", mutate_final_r3_current_shadow_pnl_bit_drift, "Stage 5D final r3 current-shadow full-path proof missing"),
    ("final_r3_current_shadow_signed_zero_accepted", mutate_final_r3_current_shadow_signed_zero_accepted, "Stage 5D final r3 current-shadow full-path proof missing"),
    ("final_r3_current_shadow_evidence_envelope_mismatch", mutate_final_r3_current_shadow_evidence_envelope_mismatch, "Stage 5D final r3 current-shadow r1 marker proof missing"),
    ("final_r3_current_shadow_materialized_apply_skipped", mutate_final_r3_current_shadow_materialized_apply_skipped, "Stage 5D final r3 current-shadow r1 marker proof missing"),
    ("final_r3_current_shadow_materialized_apply_after_injection", mutate_final_r3_current_shadow_materialized_apply_after_injection, "Stage 5D final r3 current-shadow materialized apply proof missing"),
    ("final_r3_current_shadow_callback_before_apply", mutate_final_r3_current_shadow_callback_before_apply, "Stage 5D final r3 current-shadow r1 marker proof missing"),
    ("final_r3_current_shadow_source_runtime_reused", mutate_final_r3_current_shadow_source_runtime_reused, "Stage 5D final r3 current-shadow producer lineage proof missing"),
    ("final_r3_current_shadow_direct_mutation_substituted", mutate_final_r3_current_shadow_direct_mutation_substituted, "Stage 5D final r3 current-shadow producer lineage proof missing"),
    ("final_r3_current_shadow_generation_identity_mismatch", mutate_final_r3_current_shadow_generation_identity_mismatch, "Stage 5D final r3 current-shadow producer lineage proof missing"),
    ("final_r3_current_shadow_stage5e_or_surface_opened", mutate_final_r3_current_shadow_stage5e_or_surface_opened, "Stage 5D final r3 resumption inventory closed-surface mismatch"),
    ("final_r3_current_shadow_r1r1_production_boundary_removed", mutate_final_r3_current_shadow_r1r1_production_boundary_removed, "Stage 5D final r3 current-shadow production materialized apply boundary missing"),
    ("final_r3_current_shadow_r1r1_boundary_cfg_test_only", mutate_final_r3_current_shadow_r1r1_boundary_cfg_test_only, "Stage 5D final r3 current-shadow production materialized apply proof missing"),
    ("final_r3_current_shadow_r1r1_raw_envelope_authority", mutate_final_r3_current_shadow_r1r1_raw_envelope_authority, "Stage 5D final r3 current-shadow production materialized apply proof missing"),
    ("final_r3_current_shadow_r1r1_raw_strategy_extractor", mutate_final_r3_current_shadow_r1r1_raw_strategy_extractor, "Stage 5D final r3 current-shadow production materialized apply proof missing"),
    ("final_r3_current_shadow_r1r1_apply_after_bootstrap", mutate_final_r3_current_shadow_r1r1_apply_after_bootstrap, "Stage 5D final r3 current-shadow r1 marker proof missing"),
    ("final_r3_current_shadow_r1r1_apply_after_injection", mutate_final_r3_current_shadow_r1r1_apply_after_injection, "Stage 5D final r3 current-shadow r1 marker proof missing"),
    ("final_r3_current_shadow_r1r1_callback_before_apply", mutate_final_r3_current_shadow_r1r1_callback_before_apply, "Stage 5D final r3 current-shadow r1 marker proof missing"),
    ("final_r3_current_shadow_r1r1_blocked_loses_capability", mutate_final_r3_current_shadow_r1r1_blocked_loses_capability, "Stage 5D final r3 current-shadow production materialized apply proof missing"),
    ("final_r3_current_shadow_r1r1_partial_mutation_on_block", mutate_final_r3_current_shadow_r1r1_partial_mutation_on_block, "Stage 5D final r3 current-shadow production materialized apply proof missing"),
    ("final_r3_current_shadow_r1r1_identity_binding_removed", mutate_final_r3_current_shadow_r1r1_identity_binding_removed, "Stage 5D final r3 current-shadow production materialized apply proof missing"),
    ("final_r3_current_shadow_r1r1_generation_binding_removed", mutate_final_r3_current_shadow_r1r1_generation_binding_removed, "Stage 5D final r3 current-shadow production materialized apply proof missing"),
    ("final_r3_current_shadow_r1r1_ledger_tail_binding_removed", mutate_final_r3_current_shadow_r1r1_ledger_tail_binding_removed, "Stage 5D final r3 current-shadow production materialized apply proof missing"),
    ("final_r3_current_shadow_r1r1_pnl_binding_removed", mutate_final_r3_current_shadow_r1r1_pnl_binding_removed, "Stage 5D final r3 current-shadow production materialized apply proof missing"),
    ("final_r3_current_shadow_r1r1_builder_accepts_stale_source", mutate_final_r3_current_shadow_r1r1_builder_accepts_stale_source, "Stage 5D final r3 current-shadow production materialized apply proof missing"),
    ("final_r3_current_shadow_r1r1_unrestorable_committed_package", mutate_final_r3_current_shadow_r1r1_unrestorable_committed_package, "Stage 5D final r3 current-shadow r1 marker proof missing"),
    ("final_r3_current_shadow_r1r1_lifecycle_fields_overwritten", mutate_final_r3_current_shadow_r1r1_lifecycle_fields_overwritten, "Stage 5D final r3 current-shadow production materialized apply proof missing"),
    ("final_r3_current_shadow_r1r1_field_level_proof_removed", mutate_final_r3_current_shadow_r1r1_field_level_proof_removed, "Stage 5D final r3 current-shadow r1 marker proof missing"),
    ("final_r3_current_shadow_r1r1_stage5e_or_surface_opened", mutate_final_r3_current_shadow_r1r1_stage5e_or_surface_opened, "Stage 5D final r3 resumption inventory closed-surface mismatch"),
    ("final_r3_operational_source_callback_removed", mutate_final_r3_operational_source_callback_removed, "Stage 5D final r3 operational-state r1 marker proof missing"),
    ("final_r3_operational_direct_substitution", mutate_final_r3_operational_direct_substitution, "Stage 5D final r3 operational-state direct mutation guard missing"),
    ("final_r3_operational_source_runtime_reused", mutate_final_r3_operational_source_runtime_reused, "Stage 5D final r3 operational-state r1 marker proof missing"),
    ("final_r3_operational_strict_decode_removed", mutate_final_r3_operational_strict_decode_removed, "Stage 5D final r3 operational-state r1 marker proof missing"),
    ("final_r3_operational_private_apply_moved", mutate_final_r3_operational_private_apply_moved, "Stage 5D final r3 operational-state r1 marker proof missing"),
    ("final_r3_operational_lifecycle_equality_removed", mutate_final_r3_operational_lifecycle_equality_removed, "Stage 5D final r3 operational-state r1 marker proof missing"),
    ("final_r3_operational_partial_timer_removed", mutate_final_r3_operational_partial_timer_removed, "Stage 5D final r3 operational-state r1 marker proof missing"),
    ("final_r3_operational_deferred_entry_stop_take_removed", mutate_final_r3_operational_deferred_entry_stop_take_removed, "Stage 5D final r3 operational-state r1 marker proof missing"),
    ("final_r3_operational_safe_mode_entry_block_removed", mutate_final_r3_operational_safe_mode_entry_block_removed, "Stage 5D final r3 operational-state r1 marker proof missing"),
    ("final_r3_operational_stage5c_continuation_removed", mutate_final_r3_operational_stage5c_continuation_removed, "Stage 5D final r3 operational-state post-restored probe ordering invalid"),
    ("final_r3_operational_premature_next_group_promotion", mutate_final_r3_operational_premature_next_group_promotion, "Stage 5D final r3 accepted executable set mismatch"),
    ("final_r3_operational_stage5e_or_surface_opened", mutate_final_r3_operational_stage5e_or_surface_opened, "Stage 5D final r3 resumption inventory closed-surface mismatch"),
    ("final_r3_recovery_index_production_boundary_removed", mutate_final_r3_recovery_index_production_boundary_removed, "Stage 5D final r3 recovery-index r1 marker/code proof missing"),
    ("final_r3_recovery_index_stop_truth_removed", mutate_final_r3_recovery_index_stop_truth_removed, "Stage 5D final r3 recovery-index r1 marker/code proof missing"),
    ("final_r3_recovery_index_negative_matrix_removed", mutate_final_r3_recovery_index_negative_matrix_removed, "Stage 5D final r3 recovery-index r1 marker/code proof missing"),
    ("final_r3_recovery_index_pending_field_proof_removed", mutate_final_r3_recovery_index_pending_field_proof_removed, "Stage 5D final r3 recovery-index r1 marker/code proof missing"),
    ("final_r3_recovery_index_tp_sl_swap_proof_removed", mutate_final_r3_recovery_index_tp_sl_swap_proof_removed, "Stage 5D final r3 recovery-index r1 marker/code proof missing"),
    ("recovery_r1r3_unbroken_path_reconstruction_introduced", mutate_recovery_r1r3_unbroken_path_reconstruction_introduced, "Stage 5D final r3 recovery-index unbroken type-state path violated"),
    ("recovery_r1r3_authoritative_admission_moved_after_private_apply", mutate_recovery_r1r3_authoritative_admission_moved_after_private_apply, "Stage 5D final r3 recovery-index r1 marker/code proof missing"),
    ("recovery_r1r3_production_working_set_call_removed", mutate_recovery_r1r3_production_working_set_call_removed, "Stage 5D final r3 recovery-index validated working-set call path missing"),
    ("recovery_r1r3_working_set_coordinator_not_crate_visible", mutate_recovery_r1r3_working_set_coordinator_not_crate_visible, "Stage 5D final r3 recovery-index working-set coordinator must remain crate-visible"),
    ("recovery_r1r3_validated_stop_truth_roundtrip_removed", mutate_recovery_r1r3_validated_stop_truth_roundtrip_removed, "Stage 5D final r3 recovery-index validated working-set call path missing"),
    ("recovery_r1r3_raw_stop_truth_consumed", mutate_recovery_r1r3_raw_stop_truth_consumed, "Stage 5D final r3 recovery-index validated working-set call path missing"),
    ("recovery_r1r3_normalization_call_removed", mutate_recovery_r1r3_normalization_call_removed, "Stage 5D final r3 recovery-index production normalization retention missing"),
    ("recovery_r1r3_normalization_block_capability_lost", mutate_recovery_r1r3_normalization_block_capability_lost, "Stage 5D final r3 recovery-index normalization retained capability missing"),
    ("recovery_r1r3_normalization_partial_mutation_accepted", mutate_recovery_r1r3_normalization_partial_mutation_accepted, "Stage 5D final r3 recovery-index normalization partial mutation guard missing"),
    ("recovery_r1r3_duplicate_sl_callback_removed", mutate_recovery_r1r3_duplicate_sl_callback_removed, "Stage 5D final r3 recovery-index actual SL callbacks substituted"),
    ("recovery_r1r3_terminal_sl_callback_removed", mutate_recovery_r1r3_terminal_sl_callback_removed, "Stage 5D final r3 recovery-index actual SL callbacks substituted"),
    ("recovery_r1r3_exact_sl_set_assertion_removed", mutate_recovery_r1r3_exact_sl_set_assertion_removed, "Stage 5D final r3 recovery-index SL restored behavior proof missing"),
    ("recovery_r1r3_pending_stage_assertion_removed", mutate_recovery_r1r3_pending_stage_assertion_removed, "Stage 5D final r3 recovery-index SL restored behavior proof missing"),
    ("recovery_r1r3_pending_terminal_orphan_accepted", mutate_recovery_r1r3_pending_terminal_orphan_accepted, "Stage 5D final r3 recovery-index r1 marker/code proof missing"),
    ("recovery_r1r3_stage5c_continuation_removed", mutate_recovery_r1r3_stage5c_continuation_removed, "Stage 5D final r3 recovery-index r1 marker/code proof missing"),
    ("recovery_r1r3_final_group_or_stage5e_prematurely_opened", mutate_recovery_r1r3_final_group_or_stage5e_prematurely_opened, "Stage 5D final r3 accepted executable set mismatch"),
    ("riskrec_single_source_producer_removed", mutate_riskrec_single_source_producer_removed, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_runtime_pending_direct_inserted", mutate_riskrec_runtime_pending_direct_inserted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_durable_outbox_direct_inserted", mutate_riskrec_durable_outbox_direct_inserted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_identity_mismatch_accepted", mutate_riskrec_identity_mismatch_accepted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_materialized_mismatch_accepted", mutate_riskrec_materialized_mismatch_accepted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_single_action_omitted", mutate_riskrec_single_action_omitted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_single_action_duplicated", mutate_riskrec_single_action_duplicated, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_multi_order_reversed", mutate_riskrec_multi_order_reversed, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_multi_set_comparison", mutate_riskrec_multi_set_comparison, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_second_action_before_checkpoint", mutate_riskrec_second_action_before_checkpoint, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_retry_duplicates_ledger_append", mutate_riskrec_retry_duplicates_ledger_append, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_retry_duplicates_materialized", mutate_riskrec_retry_duplicates_materialized, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_retry_duplicates_runtime_ack", mutate_riskrec_retry_duplicates_runtime_ack, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_final_commit_checkpoint_omitted", mutate_riskrec_final_commit_checkpoint_omitted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_complete_plan_produces_action", mutate_riskrec_complete_plan_produces_action, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_complete_plan_changes_state", mutate_riskrec_complete_plan_changes_state, "Stage 5D final r3 riskgate-recovery case-specific proof missing"),
    ("riskrec_source_runtime_reused", mutate_riskrec_source_runtime_reused, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_strict_decode_removed", mutate_riskrec_strict_decode_removed, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_committed_guard_removed", mutate_riskrec_committed_guard_removed, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_partial_write_accepted", mutate_riskrec_partial_write_accepted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_full_uncommitted_accepted", mutate_riskrec_full_uncommitted_accepted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_restored_callback_duplicated", mutate_riskrec_restored_callback_duplicated, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_stage5c_continuation_removed", mutate_riskrec_stage5c_continuation_removed, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_stage5e_opened", mutate_riskrec_stage5e_opened, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_source_rollover_removed", mutate_riskrec_r1r1_source_rollover_removed, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_runtime_pending_direct", mutate_riskrec_r1r1_runtime_pending_direct, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_durable_outbox_direct", mutate_riskrec_r1r1_durable_outbox_direct, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_transition_removed", mutate_riskrec_r1r1_transition_removed, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_test_row_executor_substituted", mutate_riskrec_r1r1_test_row_executor_substituted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_second_action_selected", mutate_riskrec_r1r1_second_action_selected, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_ledger_append_omitted", mutate_riskrec_r1r1_ledger_append_omitted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_ledger_append_duplicated", mutate_riskrec_r1r1_ledger_append_duplicated, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_materialized_update_omitted", mutate_riskrec_r1r1_materialized_update_omitted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_materialized_update_duplicated", mutate_riskrec_r1r1_materialized_update_duplicated, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_runtime_ack_omitted", mutate_riskrec_r1r1_runtime_ack_omitted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_runtime_ack_duplicated", mutate_riskrec_r1r1_runtime_ack_duplicated, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_final_receipt_omitted", mutate_riskrec_r1r1_final_receipt_omitted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_final_receipt_forged", mutate_riskrec_r1r1_final_receipt_forged, "Stage 5D final r3 riskgate-recovery golden fingerprint missing"),
    ("riskrec_r1r1_checkpoint_package_not_persisted", mutate_riskrec_r1r1_checkpoint_package_not_persisted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_store_handles_reused", mutate_riskrec_r1r1_store_handles_reused, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_partial_file_accepted", mutate_riskrec_r1r1_partial_file_accepted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_full_uncommitted_accepted", mutate_riskrec_r1r1_full_uncommitted_accepted, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_complete_direct_frontier", mutate_riskrec_r1r1_complete_direct_frontier, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_complete_plan_action", mutate_riskrec_r1r1_complete_plan_action, "Stage 5D final r3 riskgate-recovery checkpoint golden missing"),
    ("riskrec_r1r1_stage5c_warmup_removed", mutate_riskrec_r1r1_stage5c_warmup_removed, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_restored_callback_duplicated", mutate_riskrec_r1r1_restored_callback_duplicated, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r1_golden_hash_changed", mutate_riskrec_r1r1_golden_hash_changed, "Stage 5D final r3 riskgate-recovery golden fingerprint missing"),
    ("riskrec_r1r1_stage5e_opened", mutate_riskrec_r1r1_stage5e_opened, "Stage 5D final r3 riskgate-recovery golden Stage 5E closure missing"),
    ("riskrec_r1r3_exact_package_fixture_changed", mutate_riskrec_r1r3_exact_package_fixture_changed, "Stage 5D final r3 riskgate-recovery exact fixture drift"),
    ("riskrec_r1r3_exact_receipt_fixture_changed", mutate_riskrec_r1r3_exact_receipt_fixture_changed, "Stage 5D final r3 riskgate-recovery exact fixture drift"),
    ("riskrec_r1r3_summary_wrong_fixture_path", mutate_riskrec_r1r3_summary_wrong_fixture_path, "Stage 5D final r3 riskgate-recovery exact fixture path mismatch"),
    ("riskrec_r1r3_summary_wrong_fixture_sha", mutate_riskrec_r1r3_summary_wrong_fixture_sha, "Stage 5D final r3 riskgate-recovery exact fixture sha mismatch"),
    ("riskrec_r1r3_committed_read_validator_removed", mutate_riskrec_r1r3_committed_read_validator_removed, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
    ("riskrec_r1r3_forged_matrix_removed", mutate_riskrec_r1r3_forged_matrix_removed, "Stage 5D final r3 riskgate-recovery r1 marker/code proof missing"),
]


def run_case(
    base: Path,
    clean: Path,
    index: int,
    case: tuple[str, Callable[[Path], None], str],
    timeout_seconds: int,
) -> CaseRun:
    name, mutator, expected = case
    case_root = base / "cases" / f"{index:02d}-{name}"
    started = time.monotonic()
    try:
        shutil.copytree(clean, case_root)
        mutator(case_root)
        result = run_checker(case_root, timeout_seconds)
        combined = result.stdout + result.stderr
        if result.returncode == 0:
            return CaseRun(
                index,
                name,
                False,
                "mutation unexpectedly passed the checker",
                time.monotonic() - started,
            )
        if result.timed_out:
            return CaseRun(
                index,
                name,
                False,
                combined.strip(),
                time.monotonic() - started,
            )
        if "Traceback (most recent call last)" in combined:
            return CaseRun(
                index,
                name,
                False,
                f"infrastructure traceback is not a semantic PASS\n{combined}".strip(),
                time.monotonic() - started,
            )
        if expected not in combined:
            return CaseRun(
                index,
                name,
                False,
                f"expected marker {expected!r} missing\n{combined}".strip(),
                time.monotonic() - started,
            )
        return CaseRun(index, name, True, "", time.monotonic() - started)
    except Exception as error:  # noqa: BLE001 - diagnostics must cross worker boundary
        return CaseRun(index, name, False, repr(error), time.monotonic() - started)
    finally:
        shutil.rmtree(case_root, ignore_errors=True)


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage5d-negative-") as tmp:
        base = Path(tmp)
        clean = base / "clean"
        copy_workspace(clean)
        manifest = json.loads(
            (clean / "docs/stage-5/stage-5d-additive-freeze-manifest.json").read_text()
        )
        declared_names = manifest.get("negative_cases", [])
        implemented_names = [name for name, _mutator, _expected in CASES]
        missing = sorted(set(declared_names) - set(implemented_names))
        extra = sorted(set(implemented_names) - set(declared_names))
        if (
            declared_names != implemented_names
            or len(set(declared_names)) != len(declared_names)
            or missing
            or extra
        ):
            print(
                "stage5d-negative-harness: manifest/case inventory mismatch "
                f"missing={missing} extra={extra}",
                file=sys.stderr,
            )
            return 1

        clean_result = run_checker(clean, timeout_seconds=120)
        if clean_result.returncode != 0:
            print(clean_result.stdout)
            print(clean_result.stderr, file=sys.stderr)
            print("stage5d-negative-harness: clean checker run failed", file=sys.stderr)
            return 1

        measured_timeout = max(10, min(120, math.ceil(clean_result.duration_seconds * 8)))
        configured_workers = int(os.environ.get("STAGE5D_NEGATIVE_WORKERS", "4"))
        worker_count = max(1, min(configured_workers, 4, len(CASES)))
        (base / "cases").mkdir()
        results: list[CaseRun] = []
        with concurrent.futures.ThreadPoolExecutor(max_workers=worker_count) as executor:
            futures = [
                executor.submit(run_case, base, clean, index, case, measured_timeout)
                for index, case in enumerate(CASES)
            ]
            for future in concurrent.futures.as_completed(futures):
                results.append(future.result())

        results.sort(key=lambda result: result.index)
        failures = [result for result in results if not result.passed]
        print("Stage 5D negative harness isolated bounded-parallel verification")
        print(f"cases_declared={len(declared_names)}")
        print(f"workers={worker_count}")
        print(f"case_timeout_seconds={measured_timeout}")
        print(f"passed={len(results) - len(failures)}")
        print(f"missing={missing}")
        print(f"extra={extra}")
        print(
            "worst_case_seconds="
            f"{max((result.duration_seconds for result in results), default=0.0):.3f}"
        )
        for result in results:
            print(f"{'PASS' if result.passed else 'FAIL'} {result.name}")
            if result.diagnostics:
                print(result.diagnostics, file=sys.stderr)
        if failures:
            return 1
    print("stage5d-negative-harness: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
