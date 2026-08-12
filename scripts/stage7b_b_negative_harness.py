#!/usr/bin/env python3
"""Descriptor-pinned Stage 7B-b-R1 anchored namespace mutation inventory."""
from __future__ import annotations

import json
from pathlib import Path

import stage7b_b_check as checker

ROOT = Path(__file__).resolve().parents[1]
WORKSPACE = (ROOT / "Cargo.toml").read_text()
MANIFEST = (ROOT / "crates/runtime-durable-service/Cargo.toml").read_text()
SERVICE = (ROOT / "crates/runtime-durable-service/src/lib.rs").read_text()
JOURNAL = (ROOT / "crates/strategy-runtime-core/src/stage6_journal_backend.rs").read_text()
SUBPROCESS = (
    ROOT / "crates/runtime-durable-service/tests/stage7b_writer_lock_subprocess.rs"
).read_text()
DESCRIPTOR = json.loads(
    (ROOT / "docs/stage-7/stage7b-b-entry-descriptor.json").read_text()
)


def changed_descriptor(key: str, value: object) -> dict:
    changed = dict(DESCRIPTOR)
    changed[key] = value
    return changed


CASES = [
    {
        "name": "workspace-service-removed",
        "workspace": WORKSPACE.replace('    "crates/runtime-durable-service",\n', "", 1),
    },
    {"name": "redis-dependency-added", "manifest": MANIFEST + "\nredis.workspace = true\n"},
    {
        "name": "core-dependency-removed",
        "manifest": MANIFEST.replace(
            'strategy-runtime-core = { path = "../strategy-runtime-core" }',
            'removed-core = { path = "../strategy-runtime-core" }',
            1,
        ),
    },
    {
        "name": "relative-path-allowed",
        "service": SERVICE.replace("if !root.is_absolute()", "if false", 1),
    },
    {
        "name": "root-symlink-check-removed",
        "service": SERVICE.replace("metadata.file_type().is_symlink()", "false", 1),
    },
    {
        "name": "canonical-alias-check-removed",
        "service": SERVICE.replace("if canonical != root", "if false", 1),
    },
    {
        "name": "identity-directory-binding-removed",
        "service": SERVICE.replace(
            "if canonical.file_name().and_then(|value| value.to_str()) != Some(expected.as_str())",
            "if false",
            1,
        ),
    },
    {
        "name": "root-directory-fd-removed",
        "service": SERVICE.replace("    root_directory: File,\n", "", 1),
    },
    {
        "name": "trusted-parent-fd-removed",
        "service": SERVICE.replace("    parent_directory: File,\n", "", 1),
    },
    {
        "name": "trusted-parent-dev-binding-removed",
        "service": SERVICE.replace("    parent_dev: u64,\n", "", 1),
    },
    {
        "name": "root-dev-binding-removed",
        "service": SERVICE.replace("    root_dev: u64,\n", "", 1),
    },
    {
        "name": "root-ino-binding-removed",
        "service": SERVICE.replace("    root_ino: u64,\n", "", 1),
    },
    {
        "name": "root-directory-open-guard-removed",
        "service": SERVICE.replace(
            "libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC",
            "libc::O_RDONLY",
        ),
    },
    {
        "name": "child-openat-reverted",
        "service": SERVICE.replace("libc::openat(", "libc::open(", 1),
    },
    {
        "name": "journal-relative-validation-removed",
        "service": SERVICE.replace("            STAGE7B_JOURNAL_FILE,", "            \"unchecked-journal\",", 1),
    },
    {
        "name": "hard-link-alias-allowed",
        "service": SERVICE.replace("metadata.file_type().is_file() && metadata.nlink() == 1", "metadata.file_type().is_file()"),
    },
    {
        "name": "lock-no-follow-removed",
        "service": SERVICE.replace(
            "libc::O_RDWR | libc::O_CREAT | libc::O_NOFOLLOW | libc::O_CLOEXEC",
            "libc::O_RDWR | libc::O_CREAT | libc::O_CLOEXEC",
        ),
    },
    {
        "name": "root-directory-lock-removed",
        "service": SERVICE.replace("        acquire_nonblocking_exclusive_lock(&root.root_directory)?;\n", "", 1),
    },
    {
        "name": "identity-scoped-parent-namespace-lock-removed",
        "service": SERVICE.replace("        acquire_nonblocking_exclusive_lock(&namespace_lock_file)?;\n", "", 1),
    },
    {
        "name": "sidecar-lock-removed",
        "service": SERVICE.replace("        acquire_nonblocking_exclusive_lock(&lock_file)?;\n", "", 1),
    },
    {
        "name": "shared-writer-lock",
        "service": SERVICE.replace("libc::LOCK_EX | libc::LOCK_NB", "libc::LOCK_SH | libc::LOCK_NB", 1),
    },
    {
        "name": "blocking-writer-lock",
        "service": SERVICE.replace("libc::LOCK_EX | libc::LOCK_NB", "libc::LOCK_EX", 1),
    },
    {
        "name": "post-lock-root-validation-removed",
        "service": SERVICE.replace("        root.validate_external_root_identity()?;\n", "", 1),
    },
    {
        "name": "lock-lifetime-validation-removed",
        "service": SERVICE.replace("        lease.validate_namespace()?;\n", "", 1),
    },
    {
        "name": "journal-open-reverts-path-resolution",
        "service": SERVICE.replace("        let journal = writer_lease.open_journal(create)?;", "        let journal = Stage6FileJournalBackend::open_existing(\"stage6.journal\")?;", 1),
    },
    {
        "name": "owned-create-constructor-removed",
        "service": SERVICE.replace("Stage6FileJournalBackend::create_new_from_owned_file", "Stage6FileJournalBackend::create_new", 1),
    },
    {
        "name": "owned-open-constructor-removed",
        "service": SERVICE.replace("Stage6FileJournalBackend::open_existing_from_owned_file", "Stage6FileJournalBackend::open_existing", 1),
    },
    {
        "name": "framed-read-re-resolves-path",
        "journal": JOURNAL.replace("let mut file = self.file.try_clone()?", "let mut file = File::open(&self._diagnostic_path)?", 1),
    },
    {
        "name": "first-boot-authorization-bypassed",
        "service": SERVICE.replace(
            "if !authorization.authorizes_deployment(&identity.deployment_id)",
            "if false",
            1,
        ),
    },
    {
        "name": "writable-authority-clone",
        "service": SERVICE.replace(
            "pub struct Stage7bWritableDurableAuthority",
            "#[derive(Clone)]\npub struct Stage7bWritableDurableAuthority",
            1,
        ),
    },
    {
        "name": "root-authority-clone",
        "service": SERVICE + "\nimpl Clone for Stage7bDurableRootAuthority { fn clone(&self) -> Self { unreachable!() } }\n",
    },
    {
        "name": "writer-lease-drops-before-journal",
        "service": SERVICE.replace(
            "    journal: Stage6FileJournalBackend,\n    _writer_lease: Stage7bKernelWriterLease,",
            "    _writer_lease: Stage7bKernelWriterLease,\n    journal: Stage6FileJournalBackend,",
            1,
        ),
    },
    {
        "name": "raw-authority-extractor",
        "service": SERVICE + "\nimpl Stage7bWritableDurableAuthority { pub fn into_parts(self) {} }\n",
    },
    {
        "name": "writable-authority-serialize-impl",
        "service": SERVICE
        + "\nimpl Serialize for Stage7bWritableDurableAuthority {}\n",
    },
    {
        "name": "root-authority-serialize-impl",
        "service": SERVICE + "\nimpl Serialize for Stage7bDurableRootAuthority {}\n",
    },
    {
        "name": "subprocess-kill-witness-removed",
        "subprocess": SUBPROCESS.replace("stage7b_b_second_process_is_rejected_and_sigkill_releases_kernel_lock", "removed_sigkill_witness", 1),
    },
    {
        "name": "root-race-witness-removed",
        "subprocess": SUBPROCESS.replace("stage7b_b_root_replacement_between_lock_and_journal_fails_closed", "removed_root_race_witness", 1),
    },
    {
        "name": "lock-replacement-witness-removed",
        "subprocess": SUBPROCESS.replace("stage7b_b_replaced_lock_path_cannot_admit_second_writer", "removed_lock_replacement_witness", 1),
    },
    {
        "name": "ready-root-replacement-witness-removed",
        "subprocess": SUBPROCESS.replace("stage7b_b_replaced_root_after_ready_cannot_admit_second_writer", "removed_ready_root_replacement_witness", 1),
    },
    {
        "name": "identity-scoped-namespace-witness-removed",
        "service": SERVICE.replace("stage7b_b_parent_namespace_lock_is_identity_scoped", "removed_identity_scope_witness", 1),
    },
    {
        "name": "live-root-drift-witness-removed",
        "service": SERVICE.replace("stage7b_b_live_authority_rejects_root_drift_before_journal_access", "removed_live_root_drift_witness", 1),
    },
    {
        "name": "live-lock-drift-witness-removed",
        "service": SERVICE.replace("stage7b_b_live_authority_rejects_lock_drift_before_journal_access", "removed_live_lock_drift_witness", 1),
    },
    {
        "name": "recovery-seal-overclaim",
        "descriptor": changed_descriptor("recovery_seal_implemented", True),
    },
    {
        "name": "negative-count-drift",
        "descriptor": changed_descriptor("negative_case_count", 43),
    },
]


def must_fail(case: dict) -> None:
    try:
        if "descriptor" in case:
            checker.validate_descriptor(case["descriptor"])
        elif "workspace" in case or "manifest" in case:
            checker.check_dependencies(
                case.get("workspace", WORKSPACE), case.get("manifest", MANIFEST)
            )
        else:
            checker.validate_source(
                case.get("service", SERVICE),
                case.get("journal", JOURNAL),
                case.get("subprocess", SUBPROCESS),
            )
    except (checker.CheckFailure, ValueError):
        print(f"PASS {case['name']}")
        return
    raise SystemExit(f"stage7b-b-negative: mutation survived: {case['name']}")


def main() -> None:
    expected = DESCRIPTOR["negative_case_count"]
    actual = len(CASES)
    if actual != expected:
        raise SystemExit(
            f"stage7b-b-negative: inventory mismatch expected={expected} actual={actual}"
        )
    for case in CASES:
        must_fail(case)
    print(f"stage7b-b-negative: PASS cases={actual}")


if __name__ == "__main__":
    main()
