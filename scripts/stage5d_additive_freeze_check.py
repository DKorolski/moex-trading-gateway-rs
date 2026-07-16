#!/usr/bin/env python3
"""Validate the Stage 5D additive freeze baseline.

The checker is intentionally local and conservative. Stage 5D has a dual
baseline:

* Stage 5C closure baseline: immutable historical public API/source evidence;
* Stage 5D additive baseline: reviewed bridge regions and Stage5d* API.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import re
import sys
from pathlib import Path
from typing import Any

sys.dont_write_bytecode = True


DEFAULT_ROOT = Path(__file__).resolve().parents[1]
MANIFEST_REL = Path("docs/stage-5/stage-5d-additive-freeze-manifest.json")
STAGE5C_MANIFEST_REL = Path("docs/stage-5/stage-5c-api-freeze-manifest.json")
STAGE5C_CHECKER_REL = Path("scripts/stage5c_api_freeze_check.py")
STAGE5C_CLOSURE_CHECKER_REL = Path("tests/fixtures/stage5/stage5c_api_freeze_check.closure.py")
LIB_REL = Path("crates/strategy-runtime-core/src/lib.rs")
STAGE5C_HOST_REL = Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs")
WRAPPER_REL = Path("crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs")
STAGE5D_REL = Path("crates/strategy-runtime-core/src/stage5d_persistence.rs")
STAGE5D_BOOTSTRAP_BRIDGE_IDENTIFIER = "stage5d_bootstrap_preserving_loaded_at"
STAGE5D_BOOTSTRAP_BRIDGE_ALLOWED_CALL_FUNCTION = "stage5d_notify_broker_truth_bootstrap_at"
STAGE5D_RISKGATE_BRIDGE_IDENTIFIER = "stage5d_inject_authoritative_riskgate_state"
STAGE5D_RISKGATE_BRIDGE_ALLOWED_CALL_FUNCTION = "stage5d_inject_authoritative_riskgate_with_evidence"
FORBIDDEN_SCANNER_REL = Path("scripts/forbidden_surface_scan.sh")
CI_REL = Path(".github/workflows/ci.yml")

EXPECTED_MANIFEST_CHECKER = "scripts/stage5d_additive_freeze_check.py"
EXPECTED_NEGATIVE_HARNESS = "scripts/stage5d_additive_freeze_negative_harness.py"
EXPECTED_FORBIDDEN_NEGATIVE_HARNESS_CONTRACT = {
    "launcher_path": "scripts/forbidden_surface_negative_harness.sh",
    "launcher_sha256": "1b4e6b494a7831640201924783d1f1bf7ea3deba0fd9051102b24ae7908dfc36",
    "coordinator_path": "scripts/forbidden_surface_negative_harness.py",
    "coordinator_sha256": "1c2fd307aa7752ca2e8325d36c2cf26e54cbeee9aa801b22030d80568cfb3e93",
    "worker_path": "scripts/forbidden_surface_negative_case_worker.sh",
    "worker_sha256": "6f81c025526a1bf4ddd2e1206c745048aa17189b9b723a4294bd300dabcfa7d0",
    "scanner_contract": "stage5d-b2bc1-r2-v1",
    "declared_cases": 81,
    "negative_cases": 80,
    "positive_controls": 1,
    "default_workers": 4,
    "max_workers": 8,
    "minimum_case_timeout_seconds": 20,
    "ci_timeout_minutes": 8,
}
EXPECTED_STAGE5C_COMPATIBILITY_CHECKER = {
    "path": "scripts/stage5c_api_freeze_check.py",
    "sha256": "6aa1a30f87d296df09bf18c84b29b944ad7af8aaa59961c54bcbe6ff0a601537",
}
EXPECTED_HISTORICAL_STAGE5C_CHECKER = {
    "path": "tests/fixtures/stage5/stage5c_api_freeze_check.closure.py",
    "sha256": "e494e92ffb5f8d90b6a581c7b99e4e80f1906aeedfa1e7446d428eb31c757209",
}

EXPECTED_STAGE5C_CLOSURE = {
    "short_commit": "69cc73b",
    "full_commit": "69cc73b7f33d8cb418c784ac993856d8a487693d",
    "handoff_archive": "moex-trading-project-69cc73b.zip",
    "handoff_sha256": "0b614ebe83b0a8af85cde0ca7a1ae481457813edad72626cd4bb5972c9c83f91",
    "manifest_sha256": "f8c555d11de1271f5041b4d3abf880ac7a406d6fb23f5e4d38ca25468a974323",
    "report_sha256": "1d15c992ce1658fea6d7ec8a25094b094400ba00b764ac23d32c525207d19b48",
    "original_checker_sha256": "e494e92ffb5f8d90b6a581c7b99e4e80f1906aeedfa1e7446d428eb31c757209",
}

APPROVED_BRIDGE_FILES = {
    str(LIB_REL): ["lib-stage5d-module", "lib-stage5d-exports"],
    str(STAGE5C_HOST_REL): ["type-state-transitions"],
    str(WRAPPER_REL): ["runtime-private-snapshot"],
}

EXPECTED_CLOSED_SURFACES = {
    "redis": False,
    "finam": False,
    "transport": False,
    "dispatch": False,
    "runtime_live": False,
    "broker_execution": False,
    "runtime_private_mutation": "controlled_validated_stage5d_apply_then_broker_truth_bootstrap_then_riskgate_injection_only",
}

EXPECTED_STAGE5C_PRIVATE_LAYOUT_EXTENSIONS = [
    {
        "path": "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
        "reason_id": "stage5d-b2b-a-persisted-load-provenance-v1",
        "public_api_unchanged": True,
        "stripped_without_additive_regions_sha256": (
            "0ea1a8982965eb34cc113d41c821b22a3ab2c2e2f5a80112ce2561b48fc4ed3c"
        ),
    }
]

EXPECTED_NEGATIVE_CASES = [
    "stage5c_api_drift",
    "trading_region_drift",
    "additive_region_escape",
    "public_namespace_leakage",
    "raw_strategy_extractor",
    "missing_historical_baseline",
    "closed_surface_downgrade",
    "negative_cases_removed",
    "manifest_checker_changed",
    "negative_harness_changed",
    "stage5d_symbol_removed",
    "stage5d_symbol_added",
    "current_compat_checker_drift",
    "historical_checker_missing",
    "historical_checker_content_drift",
    "historical_current_checker_substitution",
    "legacy_restore_direct_call",
    "legacy_restore_alias_call",
    "legacy_restore_multiline_call",
    "legacy_restore_function_reference",
    "legacy_restore_qualified_whitespace",
    "legacy_alias_reexport_in_lib_additive_region",
    "legacy_wrapper_in_stage5c_additive_region",
    "legacy_alias_in_stage5d_persistence",
    "unexpected_legacy_reference_in_allowed_file",
    "legacy_reference_moved_to_wrong_region",
    "stage5d_api_surface_drift",
    "private_layout_extension_removed",
    "private_layout_extension_hash_changed",
    "private_layout_extension_additional_path",
    "private_layout_extension_wrapper_path",
    "private_layout_extension_lib_path",
    "private_layout_self_authorized_semantic_drift",
    "private_layout_extension_reason_id_changed",
    "bootstrap_bridge_runtime_compat_direct_call",
    "bootstrap_bridge_runtime_compat_alias_call",
    "bootstrap_bridge_runtime_compat_forwarding_wrapper",
    "bootstrap_bridge_runtime_compat_function_reference",
    "bootstrap_bridge_second_stage5d_call",
    "riskgate_bridge_runtime_compat_direct_call",
    "riskgate_bridge_runtime_compat_alias_call",
    "riskgate_bridge_runtime_compat_forwarding_wrapper",
    "riskgate_bridge_runtime_compat_function_reference",
    "riskgate_bridge_second_stage5d_call",
]

EXPECTED_STAGE5D_PUBLIC_SYMBOLS = [
    "STAGE5D_ADDITIVE_FREEZE_SCHEMA_VERSION",
    "STAGE5D_PERSISTENCE_ENVELOPE_SCHEMA_VERSION",
    "STAGE5D_RISKGATE_SCHEMA_VERSION",
    "STAGE5D_RUNTIME_PRIVATE_EXTENSION_SCHEMA_VERSION",
    "STAGE5D_STRATEGY_STATE_PAYLOAD_SCHEMA_VERSION",
    "Stage5dAdditiveFreezeEvidence",
    "Stage5dBootstrapBlockReason",
    "Stage5dBootstrapBlocked",
    "Stage5dBootstrappedPaperStrategy",
    "Stage5dBracketReconciliationTimer",
    "Stage5dCleanupRetryState",
    "Stage5dEntryStyle",
    "Stage5dEnvelopeBoundRuntimeStateLoaded",
    "Stage5dEnvelopeValidationError",
    "Stage5dExpectedWorkingSets",
    "Stage5dHybridIntradayStrategyStateV1",
    "Stage5dInstrumentBinding",
    "Stage5dLifecycleReason",
    "Stage5dLifecycleWatermarks",
    "Stage5dOwner",
    "Stage5dPartialEntryTimer",
    "Stage5dPendingEntryExtension",
    "Stage5dPendingExitExtension",
    "Stage5dPersistenceEnvelope",
    "Stage5dPersistenceStage",
    "Stage5dPrivateStateAppliedPaperStrategy",
    "Stage5dRecoveryIndexes",
    "Stage5dRestoreBlockReason",
    "Stage5dRestoreBlocked",
    "Stage5dRiskGateFinalizationOutboxRecord",
    "Stage5dRiskGateFinalizationState",
    "Stage5dRiskGateIdentity",
    "Stage5dRiskGateInjectedPaperStrategy",
    "Stage5dRiskGateInjectionBlockReason",
    "Stage5dRiskGateInjectionBlocked",
    "Stage5dRiskGateLedgerEvidence",
    "Stage5dRiskGateLedgerRecord",
    "Stage5dRiskGateMaterializedState",
    "Stage5dRiskGatePersistence",
    "Stage5dRiskGateRowSource",
    "Stage5dRiskGateRowStatus",
    "Stage5dRuntimePendingRiskGateFinalization",
    "Stage5dRuntimePrivateApplyBlocked",
    "Stage5dRuntimePrivateExtension",
    "Stage5dSemanticStrategyStateV1",
    "Stage5dSide",
    "Stage5dSnapshotBinding",
    "Stage5dStrategyKind",
    "Stage5dStrategyStatePayload",
    "Stage5dStructuredTimestampFormat",
    "Stage5dTimestampPolicy",
    "Stage5dTimestampUnits",
    "Stage5dValidatedPersistenceEnvelope",
    "Stage5dValidatedRiskGateLedgerEvidence",
    "Stage5dValidatedRuntimePrivateExtension",
    "stage5d_apply_runtime_private_extension",
    "stage5d_bind_runtime_state_loaded",
    "stage5d_inject_authoritative_riskgate",
    "stage5d_notify_broker_truth_bootstrap",
    "stage5d_retry_authoritative_riskgate_injection",
    "stage5d_retry_bind_runtime_state_loaded",
    "stage5d_retry_broker_truth_bootstrap",
    "stage5d_validate_riskgate_ledger_evidence",
]

FORBIDDEN_STAGE5D_PUBLIC_PATTERNS = [
    re.compile(r"pub\s+fn\s+.*(?:raw|inner|extract|into_parts|strategy)", re.I),
    re.compile(r"pub\s+struct\s+(?!Stage5d)[A-Za-z0-9_]+"),
    re.compile(r"pub\s+enum\s+(?!Stage5d)[A-Za-z0-9_]+"),
    re.compile(r"pub\s+const\s+(?!STAGE5D)[A-Za-z0-9_]+"),
]

LEGACY_RESTORE_IDENTIFIERS = [
    "restore_stage5c_runtime_state",
    "notify_stage5c_bootstrap",
    "notify_stage5c_runtime_state_restored",
]

ALLOWED_LEGACY_RESTORE_CALL_PATHS = {
    str(LIB_REL),
    str(STAGE5C_HOST_REL),
    str(STAGE5D_REL),
}

EXPECTED_LEGACY_REFERENCE_COUNTS = {
    str(LIB_REL): {
        "restore_stage5c_runtime_state": 1,
        "notify_stage5c_bootstrap": 1,
        "notify_stage5c_runtime_state_restored": 1,
    },
    str(STAGE5C_HOST_REL): {
        "restore_stage5c_runtime_state": 2,
        "notify_stage5c_bootstrap": 4,
        "notify_stage5c_runtime_state_restored": 1,
    },
    str(STAGE5D_REL): {
        "restore_stage5c_runtime_state": 0,
        "notify_stage5c_bootstrap": 0,
        "notify_stage5c_runtime_state_restored": 0,
    },
}


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def load_stage5c_checker(root: Path):
    checker_path = root / STAGE5C_CHECKER_REL
    spec = importlib.util.spec_from_file_location("stage5c_api_freeze_check_for_stage5d", checker_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {checker_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def additive_markers(region: str) -> tuple[bytes, bytes]:
    return (
        f"// STAGE5D-ADDITIVE-BRIDGE-BEGIN: {region}".encode(),
        f"// STAGE5D-ADDITIVE-BRIDGE-END: {region}".encode(),
    )


def strip_additive_regions(path: Path, regions: list[str]) -> tuple[bytes, list[str]]:
    payload = path.read_bytes()
    failures: list[str] = []
    stripped = payload
    previous_start = -1
    for region in regions:
        begin, end = additive_markers(region)
        begin_count = stripped.count(begin)
        end_count = stripped.count(end)
        if begin_count != 1 or end_count != 1:
            failures.append(
                f"{path}: additive region {region} markers must appear exactly once "
                f"(begin={begin_count}, end={end_count})"
            )
            continue
        begin_index = stripped.find(begin)
        end_index = stripped.find(end)
        if begin_index <= previous_start:
            failures.append(f"{path}: additive region {region} marker order drifted")
        if end_index <= begin_index:
            failures.append(f"{path}: additive region {region} closing marker precedes opening marker")
            continue
        line_end = stripped.find(b"\n", end_index)
        if line_end == -1:
            line_end = len(stripped)
        else:
            line_end += 1
        stripped = stripped[:begin_index] + stripped[line_end:]
        previous_start = begin_index
    return stripped, failures


def collect_additive_regions(path: Path, regions: list[str]) -> tuple[dict[str, str], list[str]]:
    payload = path.read_text()
    failures: list[str] = []
    collected: dict[str, str] = {}
    previous_start = -1
    for region in regions:
        begin = f"// STAGE5D-ADDITIVE-BRIDGE-BEGIN: {region}"
        end = f"// STAGE5D-ADDITIVE-BRIDGE-END: {region}"
        begin_count = payload.count(begin)
        end_count = payload.count(end)
        if begin_count != 1 or end_count != 1:
            failures.append(
                f"{path}: additive region {region} markers must appear exactly once "
                f"(begin={begin_count}, end={end_count})"
            )
            continue
        begin_index = payload.find(begin)
        end_index = payload.find(end)
        if begin_index <= previous_start:
            failures.append(f"{path}: additive region {region} marker order drifted")
        if end_index <= begin_index:
            failures.append(f"{path}: additive region {region} closing marker precedes opening marker")
            continue
        collected[region] = payload[begin_index:end_index + len(end)]
        previous_start = begin_index
    return collected, failures


def legacy_identifier_hits(source: str) -> list[str]:
    return [
        identifier
        for identifier in LEGACY_RESTORE_IDENTIFIERS
        if re.search(rf"\b{re.escape(identifier)}\b", source)
    ]


def legacy_identifier_counts(source: str) -> dict[str, int]:
    return {
        identifier: len(re.findall(rf"\b{re.escape(identifier)}\b", source))
        for identifier in LEGACY_RESTORE_IDENTIFIERS
    }


def parse_stage5d_public_symbols(source: str) -> list[str]:
    symbols: set[str] = set()
    for pattern in [
        r"^pub\s+struct\s+(Stage5d[A-Za-z0-9_]+)",
        r"^pub\s+enum\s+(Stage5d[A-Za-z0-9_]+)",
        r"^pub\s+const\s+(STAGE5D[A-Za-z0-9_]+)",
        r"^pub\s+fn\s+(stage5d_[A-Za-z0-9_]+)",
    ]:
        for match in re.finditer(pattern, source, re.M):
            symbols.add(match.group(1))
    return sorted(symbols)


def normalize_signature(text: str) -> str:
    text = re.sub(r"//.*", "", text)
    text = re.sub(r"\s+", " ", text).strip()
    return text.removesuffix("{").removesuffix(";").strip()


def collect_signature(lines: list[str], start_index: int) -> tuple[str, int]:
    parts = []
    index = start_index
    while index < len(lines):
        line = lines[index].strip()
        parts.append(line)
        if "{" in line or line.endswith(";"):
            break
        index += 1
    signature = " ".join(parts)
    if "{" in signature:
        signature = signature.split("{", 1)[0]
    return normalize_signature(signature), index


def top_level_brace_delta(line: str) -> int:
    stripped = line.strip()
    if stripped.startswith("//"):
        return 0
    return stripped.count("{") - stripped.count("}")


def collect_block(lines: list[str], start_index: int) -> tuple[list[str], int]:
    block = [lines[start_index]]
    depth = top_level_brace_delta(lines[start_index])
    index = start_index + 1
    while index < len(lines) and depth > 0:
        block.append(lines[index])
        depth += top_level_brace_delta(lines[index])
        index += 1
    return block, index - 1


def parse_struct_fields(block: list[str]) -> list[dict[str, str]]:
    fields = []
    for line in block[1:-1]:
        stripped = line.strip().rstrip(",")
        if not stripped.startswith("pub "):
            continue
        declaration = stripped.removeprefix("pub ").strip()
        if ":" not in declaration:
            continue
        name, type_name = declaration.split(":", 1)
        fields.append({"name": name.strip(), "type": normalize_signature(type_name)})
    return fields


def parse_enum_variants(block: list[str]) -> list[str]:
    variants = []
    depth = 0
    for raw in block[1:-1]:
        stripped = raw.strip()
        if not stripped or stripped.startswith("#") or stripped.startswith("//"):
            continue
        if depth == 0:
            match = re.match(r"([A-Z][A-Za-z0-9_]*)", stripped)
            if match:
                variants.append(match.group(1))
        depth += stripped.count("{") + stripped.count("(") - stripped.count("}") - stripped.count(")")
        if depth < 0:
            depth = 0
    return variants


def public_api_hash(surface: dict[str, Any]) -> str:
    payload = json.dumps(surface, sort_keys=True, separators=(",", ":")).encode()
    return sha256_bytes(payload)


def parse_stage5d_surface(stage5d_source: str, lib_source: str) -> dict[str, Any]:
    lines = stage5d_source.splitlines()
    constants = []
    free_functions = []
    types: dict[str, dict[str, Any]] = {}
    methods = []

    index = 0
    while index < len(lines):
        stripped = lines[index].strip()

        const_match = re.match(r"^pub const (STAGE5D[A-Za-z0-9_]+)\s*:\s*([^=]+)=", stripped)
        if const_match:
            constants.append(
                {
                    "name": const_match.group(1),
                    "type": normalize_signature(const_match.group(2)),
                    "signature": normalize_signature(stripped),
                }
            )

        struct_match = re.match(r"^pub struct (Stage5d[A-Za-z0-9_]+)", stripped)
        if struct_match:
            name = struct_match.group(1)
            block, end_index = collect_block(lines, index) if "{" in stripped else ([lines[index]], index)
            fields = parse_struct_fields(block) if "{" in stripped else []
            types[name] = {
                "name": name,
                "kind": "struct",
                "opaque": len(fields) == 0,
                "public_fields": fields,
                "public_variants": [],
            }
            index = end_index

        enum_match = re.match(r"^pub enum (Stage5d[A-Za-z0-9_]+)", stripped)
        if enum_match:
            name = enum_match.group(1)
            block, end_index = collect_block(lines, index)
            types[name] = {
                "name": name,
                "kind": "enum",
                "opaque": False,
                "public_fields": [],
                "public_variants": parse_enum_variants(block),
            }
            index = end_index

        fn_match = re.match(r"^pub fn (stage5d_[A-Za-z0-9_]+)", stripped)
        if fn_match:
            signature, end_index = collect_signature(lines, index)
            free_functions.append({"name": fn_match.group(1), "signature": signature})
            index = end_index

        impl_match = re.match(r"^impl (Stage5d[A-Za-z0-9_]+)", stripped)
        if impl_match and "{" in stripped:
            type_name = impl_match.group(1)
            block, end_index = collect_block(lines, index)
            for offset, block_line in enumerate(block):
                method_stripped = block_line.strip()
                method_match = re.match(r"^pub fn ([A-Za-z0-9_]+)", method_stripped)
                if not method_match:
                    continue
                signature, _ = collect_signature(block, offset)
                methods.append(
                    {
                        "type": type_name,
                        "name": method_match.group(1),
                        "signature": signature,
                    }
                )
            index = end_index

        index += 1

    surface = {
        "public_reexports": parse_stage5d_reexports(lib_source),
        "public_constants": sorted(constants, key=lambda item: item["name"]),
        "public_free_functions": sorted(free_functions, key=lambda item: item["name"]),
        "public_types": sorted(types.values(), key=lambda item: item["name"]),
        "public_methods": sorted(methods, key=lambda item: (item["type"], item["name"], item["signature"])),
    }
    surface["opaque_capabilities"] = sorted(
        item["name"] for item in surface["public_types"] if item["kind"] == "struct" and item["opaque"]
    )
    surface["externally_constructible_enums"] = sorted(
        item["name"] for item in surface["public_types"] if item["kind"] == "enum"
    )
    surface["normalized_signature_hash"] = public_api_hash(surface)
    surface["public_surface_counts"] = {
        "public_reexports": len(surface["public_reexports"]),
        "public_constants": len(surface["public_constants"]),
        "public_free_functions": len(surface["public_free_functions"]),
        "public_types": len(surface["public_types"]),
        "public_methods": len(surface["public_methods"]),
        "opaque_capabilities": len(surface["opaque_capabilities"]),
        "externally_constructible_enums": len(surface["externally_constructible_enums"]),
    }
    return surface


def parse_stage5d_reexports(lib_source: str) -> list[str]:
    match = re.search(r"pub use stage5d_persistence::\{(?P<body>.*?)\};", lib_source, re.S)
    if not match:
        return []
    body = match.group("body")
    return sorted(token.strip() for token in body.replace("\n", " ").split(",") if token.strip())


def validate_stage5c_public_shape(root: Path, manifest: dict[str, Any], failures: list[str]) -> None:
    stage5c_checker = load_stage5c_checker(root)
    stage5c_manifest = json.loads((root / STAGE5C_MANIFEST_REL).read_text())
    surface = stage5c_checker.derive_manifest_surface()
    for key in [
        "public_reexports",
        "public_constants",
        "public_free_functions",
        "public_types",
        "public_methods",
        "opaque_capabilities",
        "externally_constructible_enums",
        "normalized_signature_hash",
    ]:
        if surface.get(key) != stage5c_manifest.get(key):
            failures.append(f"Stage 5C public API shape drifted for {key}")
    declared_count = (
        len(surface["public_constants"])
        + len(surface["public_free_functions"])
        + len(surface["public_types"])
    )
    expected_count = manifest.get("stage5c_public_api", {}).get("public_symbol_count")
    if declared_count != expected_count:
        failures.append(
            f"Stage 5C public symbol count mismatch: actual={declared_count} expected={expected_count}"
        )
    expected_hash = manifest.get("stage5c_public_api", {}).get("normalized_signature_hash")
    if surface.get("normalized_signature_hash") != expected_hash:
        failures.append("Stage 5C normalized signature hash mismatch")


def validate_legacy_restore_call_sites(root: Path, failures: list[str]) -> None:
    for path in sorted((root / "crates").glob("**/*.rs")):
        rel = str(path.relative_to(root))
        if rel in ALLOWED_LEGACY_RESTORE_CALL_PATHS:
            continue
        source = path.read_text(errors="replace")
        for identifier in legacy_identifier_hits(source):
            failures.append(f"legacy Stage 5C restore bypass symbol forbidden: {rel}: {identifier}")

    for rel, expected_counts in EXPECTED_LEGACY_REFERENCE_COUNTS.items():
        path = root / rel
        if not path.is_file():
            failures.append(f"legacy Stage 5C restore allowlisted file missing: {rel}")
            continue
        actual_counts = legacy_identifier_counts(path.read_text(errors="replace"))
        if actual_counts != expected_counts:
            failures.append(
                f"legacy Stage 5C restore reference count mismatch for {rel}: "
                f"actual={actual_counts} expected={expected_counts}"
            )


def validate_no_legacy_identifiers_in_additive_regions(
    root: Path,
    approved_bridge_regions: dict[str, list[str]],
    failures: list[str],
) -> None:
    for rel, regions in approved_bridge_regions.items():
        path = root / rel
        if not path.is_file():
            continue
        collected, marker_failures = collect_additive_regions(path, regions)
        failures.extend(marker_failures)
        for region, source in collected.items():
            for identifier in legacy_identifier_hits(source):
                if (
                    rel == str(STAGE5C_HOST_REL)
                    and region == "type-state-transitions"
                    and identifier == "restore_stage5c_runtime_state"
                    and "mod stage5d_pair_binding_restore_tests" in source
                    and len(re.findall(r"\brestore_stage5c_runtime_state\s*\(", source)) == 1
                ):
                    continue
                failures.append(
                    "legacy Stage 5C restore symbol forbidden in additive region: "
                    f"{rel}:{region}:{identifier}"
                )

    stage5d_path = root / STAGE5D_REL
    if stage5d_path.is_file():
        for identifier in legacy_identifier_hits(stage5d_path.read_text(errors="replace")):
            failures.append(
                f"legacy Stage 5C restore symbol forbidden in Stage 5D persistence surface: {identifier}"
            )


def source_function_slice(source: str, function_name: str) -> str:
    match = re.search(rf"\n(?:pub\s+)?fn\s+{re.escape(function_name)}\b", source)
    if not match:
        return ""
    start = match.start()
    next_match = re.search(r"\n(?:pub\s+)?fn\s+[A-Za-z0-9_]+\b", source[start + 1 :])
    if next_match:
        return source[start : start + 1 + next_match.start()]
    return source[start:]


def validate_stage5d_single_bridge_call_sites(
    root: Path,
    failures: list[str],
    *,
    label: str,
    identifier: str,
    allowed_call_function: str,
) -> None:
    pattern = re.compile(rf"\b{re.escape(identifier)}\b")
    source_root = root / "crates/strategy-runtime-core/src"
    refs: dict[str, int] = {}
    for path in sorted(source_root.rglob("*.rs")):
        rel = str(path.relative_to(root))
        refs[rel] = len(pattern.findall(path.read_text(errors="replace")))

    stage5c_rel = str(STAGE5C_HOST_REL)
    stage5d_rel = str(STAGE5D_REL)
    stage5c_source = (root / STAGE5C_HOST_REL).read_text(errors="replace")
    stage5d_source = (root / STAGE5D_REL).read_text(errors="replace")
    expected_definition = rf"pub\(crate\)\s+fn\s+{re.escape(identifier)}\s*\("
    if refs.get(stage5c_rel) != 1 or not re.search(expected_definition, stage5c_source):
        failures.append(
            f"Stage 5D {label} bridge definition contract mismatch: {stage5c_rel}"
        )
    if refs.get(stage5d_rel) != 1:
        failures.append(
            f"Stage 5D {label} bridge production call count mismatch: {stage5d_rel} "
            f"actual={refs.get(stage5d_rel)} expected=1"
        )
    allowed_function = source_function_slice(stage5d_source, allowed_call_function)
    if len(pattern.findall(allowed_function)) != 1:
        failures.append(
            f"Stage 5D {label} bridge call must remain inside "
            f"{allowed_call_function}"
        )
    for rel, count in refs.items():
        if rel not in {stage5c_rel, stage5d_rel} and count:
            failures.append(
                f"Stage 5D {label} bridge reference outside allowlist: {rel} count={count}"
            )


def validate_stage5d_bridge_call_sites(root: Path, failures: list[str]) -> None:
    validate_stage5d_single_bridge_call_sites(
        root,
        failures,
        label="bootstrap",
        identifier=STAGE5D_BOOTSTRAP_BRIDGE_IDENTIFIER,
        allowed_call_function=STAGE5D_BOOTSTRAP_BRIDGE_ALLOWED_CALL_FUNCTION,
    )
    validate_stage5d_single_bridge_call_sites(
        root,
        failures,
        label="riskgate",
        identifier=STAGE5D_RISKGATE_BRIDGE_IDENTIFIER,
        allowed_call_function=STAGE5D_RISKGATE_BRIDGE_ALLOWED_CALL_FUNCTION,
    )


def validate(root: Path, manifest_path: Path) -> list[str]:
    failures: list[str] = []
    manifest = json.loads(manifest_path.read_text())

    if manifest.get("schema_version") != 1:
        failures.append("schema_version must be 1")
    if manifest.get("stage") != "5D-b2b-c1-r2":
        failures.append("stage must be 5D-b2b-c1-r2")
    if manifest.get("status") != "additive_freeze_candidate":
        failures.append("status must be additive_freeze_candidate")
    if manifest.get("stage5c_closure_baseline") != EXPECTED_STAGE5C_CLOSURE:
        failures.append("Stage 5C closure baseline reference mismatch")
    if manifest.get("manifest_checker") != EXPECTED_MANIFEST_CHECKER:
        failures.append("manifest_checker mismatch")
    if manifest.get("negative_harness") != EXPECTED_NEGATIVE_HARNESS:
        failures.append("negative_harness mismatch")
    forbidden_contract = manifest.get("forbidden_negative_harness_contract")
    if forbidden_contract != EXPECTED_FORBIDDEN_NEGATIVE_HARNESS_CONTRACT:
        failures.append("forbidden negative harness contract mismatch")
    else:
        for path_key, hash_key in (
            ("launcher_path", "launcher_sha256"),
            ("coordinator_path", "coordinator_sha256"),
            ("worker_path", "worker_sha256"),
        ):
            artifact = root / forbidden_contract[path_key]
            if not artifact.is_file() or sha256_file(artifact) != forbidden_contract[hash_key]:
                failures.append(
                    "forbidden negative harness artifact hash mismatch: "
                    f"{forbidden_contract[path_key]}"
                )
        coordinator_source = (root / forbidden_contract["coordinator_path"]).read_text(
            errors="replace"
        )
        worker_source = (root / forbidden_contract["worker_path"]).read_text(errors="replace")
        required_coordinator_tokens = (
            "if clean_result.returncode != 0:",
            "case.expected_marker",
            "declared != implemented",
            "ThreadPoolExecutor",
            "os.killpg",
        )
        required_worker_tokens = (
            'grep -F -- "$expected_marker"',
            "infrastructure failure is not valid case evidence",
            "selected case did not execute exactly once",
        )
        if any(token not in coordinator_source for token in required_coordinator_tokens):
            failures.append("forbidden negative harness coordinator contract incomplete")
        if any(token not in worker_source for token in required_worker_tokens):
            failures.append("forbidden negative harness worker contract incomplete")
        if coordinator_source.count("Case(") != forbidden_contract["declared_cases"]:
            failures.append("forbidden negative harness declared case inventory mismatch")
        if worker_source.count("|failure'") != forbidden_contract["negative_cases"]:
            failures.append("forbidden negative harness worker negative inventory mismatch")
        if worker_source.count("|success'") != forbidden_contract["positive_controls"]:
            failures.append("forbidden negative harness worker positive inventory mismatch")
        scanner_source = (root / FORBIDDEN_SCANNER_REL).read_text(errors="replace")
        scanner_marker = (
            'FORBIDDEN_SURFACE_SCANNER_CONTRACT="'
            f'{forbidden_contract["scanner_contract"]}"'
        )
        if scanner_source.count(scanner_marker) != 1:
            failures.append("forbidden scanner contract marker mismatch")
        ci_source = (root / CI_REL).read_text(errors="replace")
        ci_timeout_match = re.search(
            r"- name: Forbidden surface negative harness\s+"
            r"run: bash scripts/forbidden_surface_negative_harness\.sh\s+"
            r"timeout-minutes: (?P<minutes>\d+)",
            ci_source,
        )
        if (
            ci_timeout_match is None
            or int(ci_timeout_match.group("minutes")) < forbidden_contract["ci_timeout_minutes"]
        ):
            failures.append(
                "forbidden negative harness CI timeout is below "
                f"{forbidden_contract['ci_timeout_minutes']} minutes"
            )
    if manifest.get("closed_surfaces") != EXPECTED_CLOSED_SURFACES:
        failures.append("closed_surfaces mismatch")
    if manifest.get("negative_cases") != EXPECTED_NEGATIVE_CASES:
        failures.append("negative_cases mismatch")
    if manifest.get("stage5d_public_symbols") != EXPECTED_STAGE5D_PUBLIC_SYMBOLS:
        failures.append("Stage5d public symbol contract mismatch")
    if (
        manifest.get("stage5c_private_layout_extensions")
        != EXPECTED_STAGE5C_PRIVATE_LAYOUT_EXTENSIONS
    ):
        failures.append("Stage 5C private layout extension contract mismatch")
    approved_private_layout_extensions = {
        extension.get("path"): extension
        for extension in manifest.get("stage5c_private_layout_extensions", [])
        if isinstance(extension, dict)
    }
    if manifest.get("stage5c_compatibility_checker") != EXPECTED_STAGE5C_COMPATIBILITY_CHECKER:
        failures.append("Stage 5C compatibility checker manifest entry mismatch")
    if manifest.get("historical_stage5c_checker") != EXPECTED_HISTORICAL_STAGE5C_CHECKER:
        failures.append("historical Stage 5C checker manifest entry mismatch")

    stage5c_manifest_hash = sha256_file(root / STAGE5C_MANIFEST_REL)
    if stage5c_manifest_hash != EXPECTED_STAGE5C_CLOSURE["manifest_sha256"]:
        failures.append(
            f"Stage 5C closure manifest hash mismatch: actual={stage5c_manifest_hash}"
        )
    report_hash = sha256_file(root / "docs/stage-5/stage-5c-acceptance-api-freeze-report.md")
    if report_hash != EXPECTED_STAGE5C_CLOSURE["report_sha256"]:
        failures.append(f"Stage 5C closure report hash mismatch: actual={report_hash}")
    compatibility_checker_hash = sha256_file(root / STAGE5C_CHECKER_REL)
    if compatibility_checker_hash != EXPECTED_STAGE5C_COMPATIBILITY_CHECKER["sha256"]:
        failures.append(
            f"Stage 5C compatibility checker hash mismatch: actual={compatibility_checker_hash}"
        )
    historical_checker_path = root / STAGE5C_CLOSURE_CHECKER_REL
    if not historical_checker_path.is_file():
        failures.append("historical Stage 5C closure checker artifact missing")
    else:
        historical_checker_hash = sha256_file(historical_checker_path)
        if historical_checker_hash != EXPECTED_HISTORICAL_STAGE5C_CHECKER["sha256"]:
            failures.append(
                f"historical Stage 5C closure checker hash mismatch: actual={historical_checker_hash}"
            )

    validate_stage5c_public_shape(root, manifest, failures)

    approved = manifest.get("approved_bridge_files", {})
    if set(approved) != set(APPROVED_BRIDGE_FILES):
        failures.append(
            f"approved bridge file set mismatch: actual={sorted(approved)} "
            f"expected={sorted(APPROVED_BRIDGE_FILES)}"
        )
    stage5c_manifest = json.loads((root / STAGE5C_MANIFEST_REL).read_text())
    closure_hashes = stage5c_manifest.get("source_hashes", {})
    for rel, regions in APPROVED_BRIDGE_FILES.items():
        path = root / rel
        record = approved.get(rel, {})
        if not path.is_file():
            failures.append(f"approved bridge file missing: {rel}")
            continue
        current_hash = sha256_file(path)
        if record.get("current_sha256") != current_hash:
            failures.append(f"{rel}: current hash mismatch actual={current_hash}")
        if record.get("closure_sha256") != closure_hashes.get(rel):
            failures.append(f"{rel}: closure hash reference mismatch")
        stripped, marker_failures = strip_additive_regions(path, regions)
        failures.extend(marker_failures)
        stripped_hash = sha256_bytes(stripped)
        if record.get("stripped_without_additive_regions_sha256") != stripped_hash:
            failures.append(f"{rel}: stripped hash mismatch actual={stripped_hash}")
        if stripped_hash != closure_hashes.get(rel):
            extension = approved_private_layout_extensions.get(rel)
            if (
                extension is None
                or extension.get("stripped_without_additive_regions_sha256") != stripped_hash
                or extension.get("public_api_unchanged") is not True
                or extension.get("reason_id")
                != "stage5d-b2b-a-persisted-load-provenance-v1"
            ):
                failures.append(f"{rel}: frozen region does not match Stage 5C closure source")

    validate_no_legacy_identifiers_in_additive_regions(root, APPROVED_BRIDGE_FILES, failures)
    validate_stage5d_bridge_call_sites(root, failures)

    stage5d_record = manifest.get("stage5d_persistence_file", {})
    stage5d_path = root / STAGE5D_REL
    if not stage5d_path.is_file():
        failures.append("stage5d_persistence.rs missing")
    else:
        stage5d_hash = sha256_file(stage5d_path)
        if stage5d_record.get("path") != str(STAGE5D_REL):
            failures.append("Stage 5D persistence file path mismatch")
        if stage5d_record.get("current_sha256") != stage5d_hash:
            failures.append(f"stage5d_persistence.rs hash mismatch actual={stage5d_hash}")
        stage5d_source = stage5d_path.read_text()
        for pattern in FORBIDDEN_STAGE5D_PUBLIC_PATTERNS:
            for match in pattern.finditer(stage5d_source):
                failures.append(f"forbidden Stage 5D public surface: {match.group(0)}")
        public_symbols = parse_stage5d_public_symbols(stage5d_source)
        if public_symbols != EXPECTED_STAGE5D_PUBLIC_SYMBOLS:
            failures.append(
                f"Stage5d public symbol mismatch actual={public_symbols} "
                f"expected={EXPECTED_STAGE5D_PUBLIC_SYMBOLS}"
            )
        reexports = parse_stage5d_reexports((root / LIB_REL).read_text())
        if reexports != public_symbols:
            failures.append(f"Stage5d re-export mismatch actual={reexports} expected={public_symbols}")
        surface = parse_stage5d_surface(stage5d_source, (root / LIB_REL).read_text())
        if surface != manifest.get("stage5d_public_api"):
            failures.append("Stage5d public API surface mismatch")

    validate_legacy_restore_call_sites(root, failures)
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=str(DEFAULT_ROOT), help="workspace root")
    parser.add_argument("--manifest", default=None, help="manifest path")
    args = parser.parse_args()

    root = Path(args.root).resolve()
    manifest_path = Path(args.manifest).resolve() if args.manifest else root / MANIFEST_REL
    failures = validate(root, manifest_path)
    if failures:
        for failure in failures:
            print(f"stage5d-additive-freeze-check: {failure}", file=sys.stderr)
        return 1
    print("stage5d-additive-freeze-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
