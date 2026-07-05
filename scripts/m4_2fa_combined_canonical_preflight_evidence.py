#!/usr/bin/env python3
"""Generate M4-2f-a combined canonical preflight decision evidence.

No broker calls are performed. The script validates that package-level
preflight uses a single combined decision over readiness, margin sufficiency,
and canonical truth/order safety.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]

DOC = Path("docs/m4-2f-a-combined-canonical-preflight-decision.md")
M4_2F_DOC = Path("docs/m4-2f-canonical-readiness-economics-closure.md")
BROKER_CORE_CONFIG = Path("crates/broker-core/src/operational_config.rs")
BROKER_CORE_LIB = Path("crates/broker-core/src/lib.rs")
BROKER_FINAM_MAPPER = Path("crates/broker-finam/src/mapper.rs")
BROKER_CLI = Path("crates/broker-cli/src/main.rs")

DOC_MARKERS = [
    "BrokerCanonicalPreflightDecision",
    "FinamCanonicalReadinessPackage",
    "canonical_preflight_decision",
    "final_preflight_allowed",
    "MarginInsufficient",
    "MissingInitialMargin",
    "TargetPositionNotFlat",
    "AccountActiveOrdersPresent",
    "reference_price is a sanity guardrail input",
    "Live expansion remains blocked after M4-2f-a",
]

M4_2F_DOC_MARKERS = [
    "reference price sanity guardrail",
    "required_margin = broker_provided_initial_margin_per_contract * qty",
]

CORE_CONFIG_MARKERS = [
    "BrokerCanonicalPreflightBlock",
    "BrokerCanonicalPreflightDecision",
    "from_readiness_margin_and_truth",
    "margin_sufficiency_block",
    "Readiness(BrokerLiveEntryBlock)",
    "MarginInsufficient",
    "MissingInitialMargin",
    "TargetPositionNotFlat",
    "AccountActiveOrdersPresent",
    "combined_canonical_preflight_allows_only_when_readiness_margin_and_truth_are_clean",
    "combined_canonical_preflight_blocks_all_margin_failures_even_when_readiness_is_clean",
    "combined_canonical_preflight_blocks_target_and_account_order_safety_gaps",
]

CORE_LIB_MARKERS = [
    "BrokerCanonicalPreflightBlock",
    "BrokerCanonicalPreflightDecision",
]

FINAM_MAPPER_MARKERS = [
    "BrokerCanonicalPreflightDecision",
    "canonical_preflight_decision",
    "from_readiness_margin_and_truth",
    "summarize_for_instrument",
    "m4_2f_canonical_readiness_package_derives_margin_but_keeps_live_blocked",
    "m4_2f_canonical_readiness_package_blocks_missing_initial_margin",
    "m4_2fa_canonical_preflight_decision_blocks_insufficient_margin",
]

CLI_MARKERS = [
    "m4_1c_canonical_report_golden_requires_broker_truth_snapshot_source",
    '"truth_source": "BrokerTruthSnapshot"',
    '"final_truth_source": "BrokerTruthSnapshot"',
]


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def artifact(path: Path) -> dict[str, Any]:
    full_path = ROOT / path
    result: dict[str, Any] = {"path": str(path), "exists": full_path.exists()}
    if full_path.exists():
        data = full_path.read_bytes()
        result.update({"sha256": sha256_bytes(data), "bytes": len(data)})
    return result


def run(cmd: list[str]) -> dict[str, Any]:
    completed = subprocess.run(
        cmd,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    return {
        "cmd": cmd,
        "exit_code": completed.returncode,
        "stdout_sha256": sha256_bytes(completed.stdout.encode()),
        "stderr_sha256": sha256_bytes(completed.stderr.encode()),
        "stdout_tail": completed.stdout[-4000:],
        "stderr_tail": completed.stderr[-4000:],
    }


def git_head() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


def marker_check(path: Path, markers: list[str]) -> dict[str, Any]:
    full_path = ROOT / path
    result: dict[str, Any] = {"path": str(path), "exists": full_path.exists()}
    if not full_path.exists():
        result.update({"ok": False, "missing": markers, "checked": markers})
        return result
    text = full_path.read_text()
    missing = [marker for marker in markers if marker not in text]
    result.update({"ok": not missing, "missing": missing, "checked": markers})
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-2fa-combined-canonical-preflight-evidence.json"),
    )
    args = parser.parse_args()

    artifacts = [
        artifact(DOC),
        artifact(M4_2F_DOC),
        artifact(BROKER_CORE_CONFIG),
        artifact(BROKER_CORE_LIB),
        artifact(BROKER_FINAM_MAPPER),
        artifact(BROKER_CLI),
    ]
    doc_check = marker_check(DOC, DOC_MARKERS)
    m4_2f_doc_check = marker_check(M4_2F_DOC, M4_2F_DOC_MARKERS)
    core_config_check = marker_check(BROKER_CORE_CONFIG, CORE_CONFIG_MARKERS)
    core_lib_check = marker_check(BROKER_CORE_LIB, CORE_LIB_MARKERS)
    finam_mapper_check = marker_check(BROKER_FINAM_MAPPER, FINAM_MAPPER_MARKERS)
    cli_check = marker_check(BROKER_CLI, CLI_MARKERS)

    broker_core_combined = run(["cargo", "test", "-p", "broker-core", "combined_canonical_preflight"])
    broker_core_operational = run(["cargo", "test", "-p", "broker-core", "operational"])
    broker_finam_m4_2f = run(["cargo", "test", "-p", "broker-finam", "m4_2f"])
    broker_finam_m4_2fa = run(["cargo", "test", "-p", "broker-finam", "m4_2fa"])
    broker_cli_m4_1c = run(
        ["cargo", "test", "-p", "broker-cli", "m4_1c", "--no-default-features"]
    )
    forbidden_scan = run(["bash", "scripts/forbidden_surface_scan.sh"])
    forbidden_negative = run(["bash", "scripts/forbidden_surface_negative_harness.sh"])
    order_transition_scan = run(["bash", "scripts/order_endpoint_scanner_transition_spec.sh"])

    checks = {
        "artifacts_present": all(item["exists"] for item in artifacts),
        "no_broker_calls_performed": True,
        "no_live_calls_performed": True,
        "combined_preflight_type_exported_ok": core_config_check["ok"] and core_lib_check["ok"],
        "readiness_blocks_preserved_ok": broker_core_combined["exit_code"] == 0
        and core_config_check["ok"],
        "margin_failures_block_combined_decision_ok": broker_core_combined["exit_code"] == 0
        and broker_finam_m4_2fa["exit_code"] == 0,
        "missing_initial_margin_blocks_combined_decision_ok": broker_finam_m4_2f["exit_code"] == 0,
        "insufficient_margin_blocks_combined_decision_ok": broker_finam_m4_2fa["exit_code"] == 0,
        "target_account_truth_safety_blocks_ok": broker_core_combined["exit_code"] == 0,
        "finam_package_carries_combined_decision_ok": finam_mapper_check["ok"]
        and broker_finam_m4_2f["exit_code"] == 0,
        "m4_1c_canonical_report_regression_ok": cli_check["ok"]
        and broker_cli_m4_1c["exit_code"] == 0,
        "broker_core_operational_regression_ok": broker_core_operational["exit_code"] == 0,
        "reference_price_guardrail_doc_ok": m4_2f_doc_check["ok"] and doc_check["ok"],
        "forbidden_surface_scan_ok": forbidden_scan["exit_code"] == 0,
        "forbidden_surface_negative_harness_ok": forbidden_negative["exit_code"] == 0,
        "order_endpoint_transition_scan_ok": order_transition_scan["exit_code"] == 0,
        "doc_ok": doc_check["ok"],
        "live_expansion_blocked": doc_check["ok"],
    }
    evidence_ready = all(checks.values())
    head = git_head()
    payload: dict[str, Any] = {
        "evidence_kind": "m4-2f-a-combined-canonical-preflight-decision-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "trading_boundary": {
            "broker_api_calls_performed": False,
            "live_calls_performed": False,
            "live_position_tests_allowed": False,
            "runtime_live_attachment_allowed": False,
            "command_consumer_to_real_finam_allowed": False,
            "stop_sltp_bracket_replace_multi_leg_allowed": False,
        },
        "combined_decision_policy": {
            "package_level_decision": "FinamCanonicalReadinessPackage.canonical_preflight_decision",
            "final_allowed_source": "BrokerCanonicalPreflightDecision.allowed",
            "readiness_only_decision_as_final_allowed": False,
            "margin_sufficiency_required": "Sufficient",
            "target_flat_required": True,
            "account_active_unknown_orphan_orders_block": True,
            "stop_order_unsupported_blocks_via_readiness": True,
        },
        "reference_price_policy": {
            "used_as_formula_multiplier": False,
            "used_as_positive_sanity_guardrail": True,
            "required_margin_formula": "initial_margin_per_contract * qty",
        },
        "artifacts": artifacts,
        "marker_checks": {
            "doc": doc_check,
            "m4_2f_doc": m4_2f_doc_check,
            "broker_core_config": core_config_check,
            "broker_core_lib": core_lib_check,
            "broker_finam_mapper": finam_mapper_check,
            "broker_cli": cli_check,
        },
        "test_commands": {
            "broker_core_combined_canonical_preflight": broker_core_combined,
            "broker_core_operational": broker_core_operational,
            "broker_finam_m4_2f": broker_finam_m4_2f,
            "broker_finam_m4_2fa": broker_finam_m4_2fa,
            "broker_cli_m4_1c_no_default_features": broker_cli_m4_1c,
            "forbidden_surface_scan": forbidden_scan,
            "forbidden_surface_negative_harness": forbidden_negative,
            "order_endpoint_scanner_transition_spec": order_transition_scan,
        },
        "checks": checks,
        "evidence_ready_for_review": evidence_ready,
    }

    output = ROOT / args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    print(json.dumps(checks | {"evidence_ready_for_review": evidence_ready}, indent=2))
    return 0 if evidence_ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
