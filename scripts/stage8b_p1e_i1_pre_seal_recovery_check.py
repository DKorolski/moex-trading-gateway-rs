#!/usr/bin/env python3
"""Static gate for Stage 8B-P1-e I1 pre-seal administrative recovery."""

from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ACCEPTED_PREDECESSOR = "5e2e157e032406fdbb9047c33c641f5973514504"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def content(root: Path, relative: str) -> str:
    return (root / relative).read_text(encoding="utf-8")


def check_content(root: Path) -> None:
    transaction = content(
        root,
        "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs",
    )
    source = content(
        root, "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs"
    )
    recovery = content(root, "crates/runtime-durable-service/src/recovery.rs")
    bootstrap = content(root, "crates/runtime-durable-service/src/stage8b_p1_bootstrap.rs")
    durable_lib = content(root, "crates/runtime-durable-service/src/lib.rs")
    implementation = content(
        root, "docs/stage-8/stage8b-p1e-i1-pre-seal-recovery-implementation.md"
    )

    for token in (
        "pub enum Stage8bP1ePreSealRecoveryActionV5",
        "pub struct Stage8bP1ePreSealRecoverySelectorV5",
        "pub enum Stage8bP1ePreSealRecoveryOutcomeV5",
        "pub fn authorize_stage8b_p1e_pre_seal_recovery_v5",
        "pub fn recover_stage8b_p1e_first_boot_pre_seal_v5",
        "RECOVER_EXISTING_STAGE8B_P1_FIRST_BOOT_V4",
        "inspection.required_action != selector.action.as_str()",
        "expected_transaction_id != selector.transaction_id_sha256",
        "complete_marker_temp_transition(",
        "quarantine_incomplete_root(",
        "finalize_quarantine(",
        "require_generation_after_quarantine_history(",
        "rename_noreplace_between(",
        "libc::RENAME_NOREPLACE",
        "sync_directory(&quarantine_parent)?",
        "sync_directory(&quarantine_root)?",
    ):
        require(token in transaction, f"pre-seal transaction token missing: {token}")

    require(
        "false && inspection.required_action != selector.action.as_str()" not in transaction,
        "action selector check was made unreachable",
    )
    require(
        "false && expected_transaction_id != selector.transaction_id_sha256" not in transaction,
        "transaction selector check was made unreachable",
    )
    for token, minimum in (
        ("complete_marker_temp_transition(", 3),
        ("quarantine_incomplete_root(", 1),
        ("finalize_quarantine(", 1),
        ("require_generation_after_quarantine_history(", 2),
        ("sync_directory(&quarantine_root)?", 2),
        ("libc::RENAME_NOREPLACE", 2),
    ):
        require(
            transaction.count(token) >= minimum,
            f"pre-seal operation coverage reduced: {token}",
        )

    actions = (
        "remove-marker-temp",
        "resume-prepared",
        "quarantine-root",
        "finalize-quarantine",
        "complete-prepared-to-root-published",
        "complete-root-published-to-journal-durable",
        "complete-journal-durable-to-seal-committed",
    )
    for action in actions:
        require(action in transaction, f"pre-seal action missing: {action}")
        require(action in implementation, f"documented pre-seal action missing: {action}")
    require(
        'Self::ResumePrepared => "resume-prepared"' in transaction,
        "typed resume selector drifted",
    )

    for token in (
        "resume_stage8b_p1e_journal_durable_first_boot",
        "Stage7bWritableDurableAuthority::open_existing(root, &identity)?",
        "first_boot_stage6d_paper_from_validated_stage5g_seed_with_owned_journal(",
    ):
        require(token in recovery, f"existing-journal linear seam missing: {token}")
    require(
        "Stage7bWritableDurableAuthority::create_new(root, &identity" not in recovery.split(
            "resume_stage8b_p1e_journal_durable_first_boot", 1
        )[1].split("pub fn first_boot", 1)[0],
        "existing-journal recovery can create a second journal",
    )

    for test in (
        "pre_seal_recovery_requires_exact_selector_and_completes_all_continuable_frontiers",
        "pre_seal_remove_and_quarantine_actions_are_durable_and_generation_guarded",
        "pre_seal_response_loss_reclassifies_without_repeating_completed_effects",
    ):
        require(test in source, f"pre-seal behavioral test missing: {test}")
    for hook in (
        "after-remove-marker-temp-before-parent-fsync",
        "after-pre-seal-marker-temp-rename-before-parent-fsync",
        "after-quarantine-root-rename-before-parent-fsync",
        "after-finalize-quarantine-marker-rename-before-parent-fsync",
    ):
        require(hook in transaction and hook in source, f"recovery crash hook missing: {hook}")

    require(
        "duplicate_for_internal_classification" in bootstrap,
        "opaque config internal classification seam missing",
    )
    require(
        "Stage8bP1ePreSealRecoverySelectorV5" in durable_lib,
        "opaque selector is not exported",
    )
    require("It never\ndeletes the root" in implementation, "no-delete boundary undocumented")
    require("Operational Redis DB0/DB15" in implementation, "closed surfaces undocumented")

    for forbidden in (
        "reqwest::",
        "finam_gateway",
        "redis::",
        "XREADGROUP",
        "XACK",
    ):
        require(
            forbidden not in transaction,
            f"forbidden operational surface in recovery module: {forbidden}",
        )


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
        print(f"stage8b-p1e-i1-pre-seal-recovery-check: FAIL {error}")
        raise SystemExit(1)
    print(
        "PASS stage8b-p1e-i1-pre-seal-recovery-check "
        "actions=7 continuable=4 quarantine_frontiers=2 response_loss_hooks=4 "
        "redis=false finam=false dispatch=false live=false"
    )


if __name__ == "__main__":
    main()
