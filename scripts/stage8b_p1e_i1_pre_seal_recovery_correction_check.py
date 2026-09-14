#!/usr/bin/env python3
"""Static gate for the Stage 8B-P1-e P1-PSR01 freshness correction."""

from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
HELD_PREDECESSOR = "be6707391dc53327fa3a29a40836d24f71eca850"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def content(root: Path, relative: str) -> str:
    return (root / relative).read_text(encoding="utf-8")


def bounded(body: str, start: str, end: str) -> str:
    require(start in body, f"section start missing: {start}")
    tail = body.split(start, 1)[1]
    require(end in tail, f"section end missing: {end}")
    return tail.split(end, 1)[0]


def check_content(root: Path) -> None:
    transaction = content(
        root,
        "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs",
    )
    source = content(
        root, "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs"
    )
    durable_lib = content(root, "crates/runtime-durable-service/src/lib.rs")
    correction = content(
        root, "docs/stage-8/stage8b-p1e-i1-pre-seal-recovery-correction.md"
    )

    for token in (
        "pub fn recover_stage8b_p1e_first_boot_pre_seal_administrative_v5",
        "pub fn recover_stage8b_p1e_first_boot_pre_seal_from_supervisor_v5",
        "fn recover_pre_seal_administrative_v5_with_observer",
        "fn marker_matches_durable_recovery",
        "fn historical_source_binding_v5",
        "read_expected_durable_recovery_marker(",
        "transaction_id_sha256(\n            &marker.operational_identity_sha256",
        "marker.runtime_profile_sha256 == STAGE8B_P1E_RUNTIME_PROFILE_SHA256",
        "marker.source_plan_sha256 == STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V2_SHA256",
        "inspection.required_action != selector.action.as_str()",
    ):
        require(token in transaction, f"durable recovery guard missing: {token}")
    require(
        "false && inspection.required_action != selector.action.as_str()" not in transaction,
        "fresh action classification was made unreachable",
    )

    admin_entry = bounded(
        transaction,
        "pub fn recover_stage8b_p1e_first_boot_pre_seal_administrative_v5",
        "/// Production recovery composition",
    )
    for forbidden in (
        "Stage8bP1ePreparedFirstBootV1",
        "trusted_now",
        "source_bundle",
        "build_stage8b_p1_first_boot_source_v1",
    ):
        require(forbidden not in admin_entry, f"administrative entry requires {forbidden}")

    admin_body = bounded(
        transaction,
        "fn recover_pre_seal_administrative_v5_with_observer",
        "/// Executes exactly one authenticated pre-seal administrative selector",
    )
    for token in (
        "!selector.action.is_administrative()",
        "fresh_runtime.stage5c_config_fingerprint()",
        "classify_stage8b_p1e_first_boot_v5(",
        "marker_matches_durable_recovery(",
        "quarantine_incomplete_root(",
        "finalize_quarantine(",
    ):
        require(token in admin_body, f"administrative recovery check missing: {token}")
    for forbidden in ("trusted_now", "source_bundle", "export_stage5g_clean_restart"):
        require(forbidden not in admin_body, f"administrative body reaches {forbidden}")

    for token in (
        "enum FirstBootTruthPolicy",
        "FreshAdmission",
        "HistoricalRecovery",
        "truth_policy == FirstBootTruthPolicy::FreshAdmission",
        "> Duration::seconds(STAGE8B_P1E_FIRST_BOOT_TRUTH_MAX_AGE_SECONDS)",
        "build_stage8b_p1_historical_recovery_source_v1",
        "prepare_stage8b_p1_historical_recovery_source_from_bytes_v1",
        "historical_binding_matches_source(binding, &source)",
        "source.source_bundle_sha256 == binding.source_bundle_sha256",
        "source.history_bars_sha256 == binding.history_bars_sha256",
        "source.riskgate_session_observations_sha256",
        "source.candidate_semantic_id_sha256 == binding.candidate_semantic_id_sha256",
    ):
        require(token in source, f"historical source guard missing: {token}")

    for test in (
        "pre_seal_administrative_recovery_remains_available_after_long_downtime",
        "administrative_recovery_rejects_wrong_selector_and_marker_auth_without_mutation",
        "historical_continuation_is_marker_bound_at_300_301_and_long_downtime",
        "historical_continuation_rejects_changed_bundle_without_mutation",
        "Duration::seconds(298)",
        "Duration::seconds(299)",
        "Duration::days(30)",
        "filesystem_snapshot(&parent), before",
    ):
        require(test in source, f"time-advance evidence missing: {test}")

    for export in (
        "recover_stage8b_p1e_first_boot_pre_seal_administrative_v5",
        "recover_stage8b_p1e_first_boot_pre_seal_from_supervisor_v5",
    ):
        require(export in durable_lib, f"production recovery export missing: {export}")

    for token in (
        "P1-PSR01",
        "accepted at age\n300 seconds and rejected at age 301 seconds",
        "no `Stage8bP1ePreparedFirstBootV1`, source bundle or",
        "If the original\nbundle is missing, unreadable or byte-different, continuation fails",
        "Operational\nRedis DB0/DB15",
    ):
        require(token in correction, f"correction contract missing: {token}")

    for forbidden in ("reqwest::", "finam_gateway", "redis::", "XREADGROUP", "XACK"):
        require(
            forbidden not in transaction,
            f"forbidden operational surface in recovery module: {forbidden}",
        )


def check_repository(root: Path) -> None:
    subprocess.run(
        ["git", "merge-base", "--is-ancestor", HELD_PREDECESSOR, "HEAD"],
        cwd=root,
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", HELD_PREDECESSOR, "--"],
        cwd=root,
        text=True,
    ).splitlines()
    forbidden_prefixes = (
        ".github/",
        "crates/finam-",
        "config/",
        "deploy/",
        "docker-compose",
    )
    require(
        not any(path.startswith(forbidden_prefixes) for path in changed),
        "operational/deployment surface changed",
    )


def check(root: Path = ROOT) -> None:
    check_content(root)
    check_repository(root)


def main() -> None:
    try:
        check()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"stage8b-p1e-i1-pre-seal-recovery-correction-check: FAIL {error}")
        raise SystemExit(1)
    print(
        "PASS stage8b-p1e-i1-pre-seal-recovery-correction-check "
        "fresh_admission_max_age=300 historical_exact=true admin_without_f00=true "
        "redis=false finam=false dispatch=false live=false"
    )


if __name__ == "__main__":
    main()
