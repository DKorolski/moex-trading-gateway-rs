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
        "    let _ = crate::stage5c_paper_host::stage5d_bootstrap_preserving_loaded_at(loaded, now);\n"
        "}\n",
    )


def mutate_bootstrap_bridge_runtime_compat_alias_call(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_bootstrap_bridge_alias_call(loaded: crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy, now: chrono::DateTime<chrono::Utc>) {\n"
        "    use crate::stage5c_paper_host::stage5d_bootstrap_preserving_loaded_at as bypass_bootstrap;\n"
        "    let _ = bypass_bootstrap(loaded, now);\n"
        "}\n",
    )


def mutate_bootstrap_bridge_runtime_compat_forwarding_wrapper(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_bootstrap_bridge_forwarding_wrapper(loaded: crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy, now: chrono::DateTime<chrono::Utc>) {\n"
        "    let _ = crate::stage5c_paper_host::stage5d_bootstrap_preserving_loaded_at(loaded, now);\n"
        "}\n",
    )


def mutate_bootstrap_bridge_runtime_compat_function_reference(root: Path) -> None:
    append_text(
        root / "crates/strategy-runtime-core/src/runtime_compat.rs",
        "\n#[allow(dead_code)]\nfn stage5d_negative_bootstrap_bridge_function_reference() {\n"
        "    let _bridge = crate::stage5c_paper_host::stage5d_bootstrap_preserving_loaded_at;\n"
        "}\n",
    )


def mutate_bootstrap_bridge_second_stage5d_call(root: Path) -> None:
    rel = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    insert_before(
        root / rel,
        "fn validate_stage5d_broker_truth_bootstrap(",
        "#[allow(dead_code)]\nfn stage5d_negative_second_bootstrap_bridge_call(loaded: crate::stage5c_paper_host::Stage5cRuntimeStateLoadedPaperStrategy, now: DateTime<Utc>) {\n"
        "    let _ = crate::stage5c_paper_host::stage5d_bootstrap_preserving_loaded_at(loaded, now);\n"
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
    ("bootstrap_bridge_second_stage5d_call", mutate_bootstrap_bridge_second_stage5d_call, "Stage 5D bootstrap bridge production call count mismatch"),
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
    ("runtime_restored_lifecycle_notification_guard_removed", mutate_runtime_restored_lifecycle_notification_guard_removed, "Stage 5D runtime-restored lifecycle notification timestamp guard missing"),
    ("runtime_restored_flat_side_exact_guard_removed", mutate_runtime_restored_flat_side_exact_guard_removed, "Stage 5D runtime-restored flat-side exact guard missing"),
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
