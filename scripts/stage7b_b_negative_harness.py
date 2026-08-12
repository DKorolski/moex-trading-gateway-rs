#!/usr/bin/env python3
"""Descriptor-pinned Stage 7B-b path/lock negative mutation inventory."""
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


OPEN_OLD = """        let writer_lease = Stage7bKernelWriterLease::acquire(&paths.writer_lock_path())?;
        paths.revalidate(identity)?;
        let journal = if create {
            Stage6FileJournalBackend::create_new(paths.journal_path())?
        } else {
            Stage6FileJournalBackend::open_existing(paths.journal_path())?
        };"""
OPEN_JOURNAL_FIRST = """        paths.revalidate(identity)?;
        let journal = if create {
            Stage6FileJournalBackend::create_new(paths.journal_path())?
        } else {
            Stage6FileJournalBackend::open_existing(paths.journal_path())?
        };
        let writer_lease = Stage7bKernelWriterLease::acquire(&paths.writer_lock_path())?;"""


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
        "name": "journal-path-validation-removed",
        "service": SERVICE.replace("&canonical.join(STAGE7B_JOURNAL_FILE)", "&canonical.join(\"unchecked-journal\")", 1),
    },
    {
        "name": "hard-link-alias-allowed",
        "service": SERVICE.replace(" && metadata.nlink() == 1", "", 1),
    },
    {
        "name": "lock-no-follow-removed",
        "service": SERVICE.replace("libc::O_NOFOLLOW | libc::O_CLOEXEC", "libc::O_CLOEXEC", 1),
    },
    {
        "name": "lock-inode-recheck-removed",
        "service": SERVICE.replace("same_regular_file_identity(path, &file)?", "true", 1),
    },
    {
        "name": "journal-inode-recheck-removed",
        "journal": JOURNAL.replace("validate_open_file_identity(&path, &file)?", "", 1),
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
        "name": "journal-open-before-lock",
        "service": SERVICE.replace(OPEN_OLD, OPEN_JOURNAL_FIRST, 1),
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
        "name": "subprocess-kill-witness-removed",
        "subprocess": SUBPROCESS.replace("child.kill().unwrap()", "drop(child)", 1),
    },
    {
        "name": "single-writer-overclaim-removed",
        "descriptor": changed_descriptor("single_writer_implemented", False),
    },
    {
        "name": "recovery-seal-overclaim",
        "descriptor": changed_descriptor("recovery_seal_implemented", True),
    },
    {
        "name": "negative-count-drift",
        "descriptor": changed_descriptor("negative_case_count", 23),
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
