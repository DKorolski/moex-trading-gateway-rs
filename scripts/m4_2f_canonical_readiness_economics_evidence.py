#!/usr/bin/env python3
"""Generate M4-2f canonical readiness / economics closure evidence.

No broker calls are performed. The script validates the canonical FINAM
readiness package boundary, instrument-derived margin policy, missing-margin
blocker, stop-order blocking policy, and canonical-only preflight direction.
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

DOC = Path("docs/m4-2f-canonical-readiness-economics-closure.md")
BROKER_CORE_SNAPSHOT = Path("crates/broker-core/src/operational_snapshot.rs")
BROKER_CORE_CONFIG = Path("crates/broker-core/src/operational_config.rs")
BROKER_CORE_LIB = Path("crates/broker-core/src/lib.rs")
BROKER_FINAM_MAPPER = Path("crates/broker-finam/src/mapper.rs")
BROKER_CLI = Path("crates/broker-cli/src/main.rs")

DOC_MARKERS = [
    "FinamCanonicalReadinessPackage",
    "BrokerTruthSnapshot",
    "BrokerReadinessSnapshot",
    "BrokerOrderMarginSufficiency",
    "MissingInitialMargin",
    "BrokerStopOrderReadiness::UnsupportedBlocked",
    "Live expansion remains blocked after M4-2f",
    "no broker/live calls are performed",
    "future preflight source is documented as canonical package only",
]

CORE_SNAPSHOT_MARKERS = [
    "long_initial_margin",
    "short_initial_margin",
    "BrokerRequiredMargin",
    "BrokerRequiredMarginFailure",
    "BrokerOrderMarginSufficiency",
    "required_margin_for_order",
    "margin_sufficiency_for_instrument_order",
    "MissingInitialMargin",
    "instrument_derived_required_margin_uses_side_qty_and_reference_price_guardrails",
    "missing_instrument_margin_is_explicit_not_silent_sufficient",
]

CORE_LIB_MARKERS = [
    "BrokerOrderMarginSufficiency",
    "BrokerRequiredMargin",
    "BrokerRequiredMarginFailure",
]

CORE_CONFIG_MARKERS = [
    "BrokerStopOrderReadiness",
    "UnsupportedBlocked",
    "StopOrderUnsupportedBlocked",
]

FINAM_MAPPER_MARKERS = [
    "FinamCanonicalReadinessPackage",
    "FinamCanonicalReadinessPackageInput",
    "build_finam_canonical_readiness_package",
    "map_finam_broker_truth_snapshot_with_readonly_artifacts",
    "map_finam_broker_readiness_snapshot",
    "margin_sufficiency_for_instrument_order",
    "no_live_authorization: true",
    "long_initial_margin = params",
    "short_initial_margin = params",
    "m4_2f_canonical_readiness_package_derives_margin_but_keeps_live_blocked",
    "m4_2f_canonical_readiness_package_blocks_missing_initial_margin",
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
        default=Path("reports/m4/m4-2f-canonical-readiness-economics-evidence.json"),
    )
    args = parser.parse_args()

    artifacts = [
        artifact(DOC),
        artifact(BROKER_CORE_SNAPSHOT),
        artifact(BROKER_CORE_CONFIG),
        artifact(BROKER_CORE_LIB),
        artifact(BROKER_FINAM_MAPPER),
        artifact(BROKER_CLI),
    ]
    doc_check = marker_check(DOC, DOC_MARKERS)
    core_snapshot_check = marker_check(BROKER_CORE_SNAPSHOT, CORE_SNAPSHOT_MARKERS)
    core_lib_check = marker_check(BROKER_CORE_LIB, CORE_LIB_MARKERS)
    core_config_check = marker_check(BROKER_CORE_CONFIG, CORE_CONFIG_MARKERS)
    finam_mapper_check = marker_check(BROKER_FINAM_MAPPER, FINAM_MAPPER_MARKERS)
    cli_check = marker_check(BROKER_CLI, CLI_MARKERS)

    broker_core_operational = run(["cargo", "test", "-p", "broker-core", "operational"])
    broker_finam_m4_2f = run(["cargo", "test", "-p", "broker-finam", "m4_2f"])
    broker_finam_m4_2d = run(["cargo", "test", "-p", "broker-finam", "m4_2d"])
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
        "canonical_readiness_package_builder_ok": finam_mapper_check["ok"]
        and broker_finam_m4_2f["exit_code"] == 0,
        "broker_truth_snapshot_output_ok": finam_mapper_check["ok"]
        and broker_finam_m4_2f["exit_code"] == 0,
        "broker_readiness_snapshot_output_ok": finam_mapper_check["ok"]
        and broker_finam_m4_2f["exit_code"] == 0,
        "instrument_derived_margin_ok": core_snapshot_check["ok"]
        and core_lib_check["ok"]
        and broker_core_operational["exit_code"] == 0
        and broker_finam_m4_2f["exit_code"] == 0,
        "missing_margin_is_blocker_ok": core_snapshot_check["ok"]
        and broker_core_operational["exit_code"] == 0
        and broker_finam_m4_2f["exit_code"] == 0,
        "stop_order_policy_blocking_ok": core_config_check["ok"]
        and broker_finam_m4_2f["exit_code"] == 0,
        "m4_preflight_canonical_only_ok": doc_check["ok"]
        and cli_check["ok"]
        and broker_cli_m4_1c["exit_code"] == 0,
        "m4_2d_regression_ok": broker_finam_m4_2d["exit_code"] == 0,
        "forbidden_surface_scan_ok": forbidden_scan["exit_code"] == 0,
        "forbidden_surface_negative_harness_ok": forbidden_negative["exit_code"] == 0,
        "order_endpoint_transition_scan_ok": order_transition_scan["exit_code"] == 0,
        "doc_ok": doc_check["ok"],
        "live_expansion_blocked": doc_check["ok"],
    }
    evidence_ready = all(checks.values())
    head = git_head()
    payload: dict[str, Any] = {
        "evidence_kind": "m4-2f-canonical-readiness-economics-closure-v1",
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
        "economics_policy": {
            "required_margin_source": "BrokerInstrumentSpec.long_initial_margin/short_initial_margin",
            "margin_inputs": ["instrument", "side", "quantity", "reference_price"],
            "missing_initial_margin_policy": "blocking",
            "full_fee_variation_margin_parity_closed": False,
        },
        "readiness_policy": {
            "future_preflight_source": "FinamCanonicalReadinessPackage",
            "truth_source": "BrokerTruthSnapshot",
            "readiness_source": "BrokerReadinessSnapshot",
            "local_counters_as_truth_allowed": False,
            "stop_order_waiver_accepted": False,
            "stop_order_policy": "BrokerStopOrderReadiness::UnsupportedBlocked",
        },
        "artifacts": artifacts,
        "marker_checks": {
            "doc": doc_check,
            "broker_core_snapshot": core_snapshot_check,
            "broker_core_lib": core_lib_check,
            "broker_core_config": core_config_check,
            "broker_finam_mapper": finam_mapper_check,
            "broker_cli": cli_check,
        },
        "test_commands": {
            "broker_core_operational": broker_core_operational,
            "broker_finam_m4_2f": broker_finam_m4_2f,
            "broker_finam_m4_2d": broker_finam_m4_2d,
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
