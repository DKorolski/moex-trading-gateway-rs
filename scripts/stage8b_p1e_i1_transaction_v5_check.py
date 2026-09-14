#!/usr/bin/env python3
"""Static contract check for the Stage 8B-P1-e I1 transaction V5 slice."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ACCEPTED_PREDECESSOR = "21eaf01916f2da5eaacb191b4d7339a8101070ad"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def content(root: Path, relative: str) -> str:
    return (root / relative).read_text(encoding="utf-8")


def check_content(root: Path) -> None:
    transaction = content(root, "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs")
    source = content(root, "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs")
    recovery = content(root, "crates/runtime-durable-service/src/recovery.rs")
    core = content(root, "crates/strategy-runtime-core/src/stage6d_live_core.rs")
    key = content(root, "crates/strategy-runtime-core/src/stage5g_clean_restart.rs")
    durable_lib = content(root, "crates/runtime-durable-service/src/lib.rs")
    core_lib = content(root, "crates/strategy-runtime-core/src/lib.rs")
    implementation = content(root, "docs/stage-8/stage8b-p1e-i1-transaction-v5-implementation.md")

    for token in (
        "STAGE8B_P1E_TRANSACTION_V5_CONTRACT_VERSION: u16 = 5",
        "STAGE8B_P1E_FIRST_BOOT_RECEIPT_SCHEMA_VERSION: u16 = 2",
        "STAGE8B_P1E_TRANSACTION_MARKER_SCHEMA_VERSION: u16 = 4",
        "pub fn first_boot_stage8b_p1e_transaction_v5",
        "pub fn classify_stage8b_p1e_first_boot_v5",
        "pub fn recover_stage8b_p1e_first_boot_adoption_v5",
        "if matches.len() != 1",
        "rename_noreplace(",
        "sync_directory(&state_parent)?",
        "cross_validate_adopted(",
        "receipt_generation: 1",
        "restart_package_schema_version: adoption.package.schema_version",
        "adoption_predicate_version: STAGE8B_P1E_ADOPTION_PREDICATE_VERSION",
    ):
        require(token in transaction, f"transaction contract token missing: {token}")

    classifications = (
        "NoRoot", "UnpublishedMarkerTemp", "PreparedWithoutRoot", "RootWithoutJournal",
        "JournalWithoutSeal", "CommittedRootReceiptTemp", "CommittedRootResponseLost",
        "QuarantinedIncompleteRoot", "ReceiptCommittedMarkerUpdatePending",
        "AdoptedCommittedRoot", "PreparedToRootPublishedMarkerTempPending",
        "RootPublishedToJournalDurableMarkerTempPending",
        "JournalDurableToSealCommittedMarkerTempPending",
        "SealCommittedToAdoptedMarkerTempPending", "CorruptOrIdentityMismatch",
    )
    for classification in classifications:
        require(classification in transaction, f"classification missing: {classification}")

    for action in (
        "RemoveReceiptTempAndAdopt", "AdoptCommittedRoot",
        "StartSealCommittedToAdopted", "CompleteSealCommittedToAdopted",
    ):
        require(action in transaction, f"post-seal recovery action missing: {action}")

    for hook in (
        "after-prepared-marker-temp-sync-before-rename",
        "after-root-published-marker-temp-sync-before-rename",
        "after-root-parent-fsync-before-journal-create",
        "after-journal-durable-marker-temp-sync-before-rename",
        "after-journal-fsync-before-initial-seal-commit",
        "after-seal-committed-marker-temp-sync-before-rename",
        "after-seal-persist-reread-before-bootstrap-success-report",
        "after-receipt-temp-sync-before-final-rename",
        "after-receipt-rename-parent-fsync-before-adopted-marker-temp-create",
        "after-adopted-marker-temp-sync-before-rename",
    ):
        require(hook in transaction and hook in source, f"crash hook not exercised: {hook}")

    require("first_boot_stage8b_p1e_transaction_v5(" in source, "source does not enter V5 transaction")
    require("every_v5_crash_hook_has_one_exact_fail_closed_classification" in source, "crash matrix missing")
    require(
        "quarantined_incomplete_root_is_reachable_for_root_published_and_journal_durable" in source,
        "quarantine positive filesystem fixtures missing",
    )
    require(
        "quarantine_identity_layout_and_committed_seal_conflicts_fail_closed_without_mutation" in source,
        "quarantine negative filesystem fixtures missing",
    )
    require(
        "assert_eq!(filesystem_snapshot(&parent), before);" in source,
        "positive classifier no-mutation assertion missing",
    )
    require(
        "assert_eq!(filesystem_snapshot(parent), before);" in source,
        "negative classifier no-mutation assertion missing",
    )
    require(
        "let observed_root_identity = root_identity.as_ref().or(quarantine_identity.as_ref());"
        in transaction,
        "classifier does not bind marker identity to active-or-quarantine layout",
    )
    require(
        "if root_exists && quarantine_exists" in transaction,
        "active plus quarantine layout is not rejected",
    )
    require("Stage7bP1eJournalDurableFirstBoot" in recovery, "linear journal-durable seam missing")
    require("begin_stage8b_p1e_first_boot" in recovery, "split first-boot entry missing")
    require("commit_initial_seal" in recovery, "initial seal continuation missing")
    require("replace_stage5g_in_stage6d_restart_package(" in recovery, "replacement seal drops schema/provenance")

    for token in (
        "pub fn seal_stage6d_restart_package_v2",
        "pub fn inspect_stage8b_p1e_authenticated_restart_package_v2",
        "pub fn replace_stage5g_in_stage6d_restart_package_v2",
        "pub fn replace_stage5g_in_stage6d_restart_package(",
        "Some(provenance) => seal_stage6d_restart_package_v2(",
        "stage8b_p1e_v2_advance_preserves_exact_first_boot_provenance",
        "stage8b_p1e_v2_missing_or_changed_provenance_fails_closed",
    ):
        require(token in core, f"V2 provenance contract missing: {token}")

    for token in (
        "stage6d_v2_hmac_sha256", "stage6d_v2_verify_hmac_sha256",
        "stage8b_p1e_framed_hmac_sha256", "stage8b_p1e_verify_framed_hmac_sha256",
    ):
        require(token in key, f"domain-separated HMAC seam missing: {token}")

    require("mod stage8b_p1e_first_boot_transaction;" in durable_lib, "transaction module is not compiled")
    require("first_boot_stage8b_p1e_transaction_v5" in durable_lib, "transaction entry not exported")
    require("replace_stage5g_in_stage6d_restart_package_v2" in core_lib, "strict V2 seam not exported")
    require("Pre-seal administrative actions" in implementation, "deferred admin boundary is undocumented")
    require("Operational Redis DB0/DB15" in implementation, "closed operational surfaces are undocumented")

    for forbidden in ("reqwest::", "finam_gateway", "redis::", "XREADGROUP", "XACK", "POST", "DELETE"):
        require(forbidden not in transaction, f"forbidden operational surface in transaction module: {forbidden}")


def check_repository(root: Path) -> None:
    subprocess.run(
        ["git", "merge-base", "--is-ancestor", ACCEPTED_PREDECESSOR, "HEAD"],
        cwd=root,
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", ACCEPTED_PREDECESSOR, "HEAD", "--"],
        cwd=root,
        text=True,
    ).splitlines()
    forbidden_prefixes = (
        ".github/", "crates/finam-", "config/", "deploy/", "docker-compose",
    )
    require(not any(path.startswith(forbidden_prefixes) for path in changed), "operational/deployment surface changed")


def check(root: Path = ROOT) -> None:
    check_content(root)
    check_repository(root)


def main() -> None:
    try:
        check()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"stage8b-p1e-i1-transaction-v5-check: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1e-i1-transaction-v5-check classifications=15 crash_hooks=10 quarantine_fixtures=5 post_seal_recovery=4 redis=false finam=false live=false")


if __name__ == "__main__":
    main()
