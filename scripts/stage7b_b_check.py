#!/usr/bin/env python3
"""Stage 7B-b path and kernel single-writer acceptance checker."""
from __future__ import annotations

import csv
import hashlib
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "a947c24bb413a91c5eb0ad97f4ac0b402bfd0641"
STAGE7A_BASE = "2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64"
BRANCH = "stage7b-production-durability"
TZ_SHA256 = "200e42acef2bb30cf24e3d2a5bc38df99ed853d70d6310653f315e76d1f4c1e0"
MATRIX_SHA256 = "083cc6e1e0925f11efa4bc093fd7c2d3d4cbeb05fd275f68ed71be3bdac1931d"


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def block(source: str, needle: str) -> str:
    start = source.index(needle)
    opening = source.index("{", start)
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start : index + 1]
    raise CheckFailure(f"unterminated source block: {needle}")


def check_lineage(root: Path) -> None:
    merge_base = subprocess.check_output(
        ["git", "merge-base", "HEAD", BASE], cwd=root, text=True
    ).strip()
    require(merge_base == BASE, "candidate is not based on accepted Stage 7B-a-R1")
    branch = subprocess.check_output(
        ["git", "branch", "--show-current"], cwd=root, text=True
    ).strip()
    require(branch == BRANCH, "Stage 7B-b branch drift")


def check_governance(root: Path) -> None:
    status = " ".join((root / "docs/current-status.md").read_text().split())
    roadmap = " ".join((root / "docs/roadmap.md").read_text().split())
    for token in (
        "Stage 7B-a-R1 is independently accepted and CLOSED",
        BASE,
        "Stage 7B-b is the only active implementation candidate",
    ):
        require(token in status, f"current status token absent: {token}")
    require("Stage 7B-b — durable path validation" in roadmap, "roadmap active slice drift")
    require("Recovery-seal and Redis-settlement work remain closed" in roadmap, "follow-on boundary absent")


def check_dependencies(workspace: str, service_manifest: str) -> None:
    require('"crates/runtime-durable-service"' in workspace, "durable service absent from workspace")
    for forbidden in ("redis", "broker-finam", "finam-gateway", "reqwest", "rusqlite"):
        require(forbidden not in service_manifest, f"forbidden durable-service dependency: {forbidden}")
    for required in (
        'strategy-runtime-core = { path = "../strategy-runtime-core" }',
        "libc.workspace = true",
    ):
        require(required in service_manifest, f"required durable-service dependency absent: {required}")


def validate_source(service: str, journal: str, subprocess_test: str) -> None:
    required = (
        "stage6d_operational_identity_sha256(identity)",
        "if !root.is_absolute()",
        "fs::symlink_metadata(root)",
        "metadata.file_type().is_symlink()",
        "let canonical = fs::canonicalize(root)?",
        "if canonical != root",
        "if canonical.file_name().and_then(|value| value.to_str()) != Some(expected.as_str())",
        "IdentityDirectoryMismatch",
        "validate_optional_regular(\n            &canonical.join(STAGE7B_JOURNAL_FILE)",
        "validate_optional_regular(\n            &canonical.join(STAGE7B_WRITER_LOCK_FILE)",
        "validate_optional_regular(\n            &canonical.join(STAGE7B_RECOVERY_SEAL_FILE)",
        "validate_optional_directory(\n            &canonical.join(STAGE7B_TMP_DIRECTORY)",
        "libc::O_NOFOLLOW | libc::O_CLOEXEC",
        "libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB)",
        "Stage7bDurableStorageError::WriterAlreadyHeld",
        "STAGE7B_STORAGE_OPEN_ORDER",
        "authorization.authorizes_deployment(&identity.deployment_id)",
        "FirstBootAuthorizationMismatch",
        "metadata.file_type().is_file() && metadata.nlink() == 1",
        "opened.nlink() == 1",
        "named.nlink() == 1",
    )
    for token in required:
        require(token in service, f"durable service invariant absent: {token}")
    require(
        service.count("same_regular_file_identity(path, &file)?") == 2,
        "writer-lock pathname/inode identity must be checked before and after flock",
    )
    require("custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)" in journal, "journal no-follow open absent")
    require(
        journal.count("validate_open_file_identity(&path, &file)?") == 2,
        "journal inode/link identity must be checked on create and reopen",
    )

    opening = block(service, "fn open(\n")
    order = (
        "Stage7bKernelWriterLease::acquire",
        "paths.revalidate(identity)?",
        "Stage6FileJournalBackend::create_new",
    )
    positions = [opening.index(token) for token in order]
    require(positions == sorted(positions), "writer lock/path validation must precede journal open")
    creation = block(service, "pub fn create_new(\n")
    require(
        creation.index("authorization.authorizes_deployment")
        < creation.index("Self::open(paths, identity, true)"),
        "first-boot authorization must precede lock/journal creation",
    )

    authority = block(service, "pub struct Stage7bWritableDurableAuthority")
    require("_writer_lease: Stage7bKernelWriterLease" in authority, "authority does not own kernel lease")
    require("journal: Stage6FileJournalBackend" in authority, "authority does not own journal")
    require(
        authority.index("journal: Stage6FileJournalBackend")
        < authority.index("_writer_lease: Stage7bKernelWriterLease"),
        "journal must drop before kernel writer lease",
    )
    require("#[derive" not in service[max(0, service.index("pub struct Stage7bWritableDurableAuthority") - 80) : service.index("pub struct Stage7bWritableDurableAuthority")], "writable authority became derivable")
    require("pub fn into_parts" not in service, "raw lock/journal extraction became public")
    require("pub fn writer" not in service and "pub fn journal" not in service, "raw authority handle escaped")
    require(
        "Serialize for Stage7bWritableDurableAuthority" not in service,
        "writable authority became serializable",
    )

    for token in (
        "stage7b_b_second_process_is_rejected_and_sigkill_releases_kernel_lock",
        "Err(Stage7bDurableStorageError::WriterAlreadyHeld)",
        "child.kill().unwrap()",
        "child.wait().unwrap()",
        "Stage7bWritableDurableAuthority::open_existing(recovered_paths",
    ):
        require(token in subprocess_test, f"subprocess lock witness absent: {token}")


def validate_descriptor(descriptor: dict) -> None:
    expected = {
        "stage": "7B",
        "slice": "7B-b",
        "accepted_stage7a_predecessor": STAGE7A_BASE,
        "accepted_slice_predecessor": BASE,
        "branch": BRANCH,
        "blocking_acceptance_rows": 80,
        "semantic_proof_map_count": 80,
        "implemented_count": 26,
        "pending_count": 54,
        "negative_case_count": 24,
        "kernel_writer_lock": True,
        "single_writer_implemented": True,
        "durable_path_validation": True,
        "recovery_seal_implemented": False,
        "redis_consumer_attached": False,
        "cross_process_fault_matrix_implemented": False,
        "cross_process_exactly_once_claimed": False,
        "finam_post_delete": False,
        "broker_network_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
        "normative_tz_sha256": TZ_SHA256,
        "normative_matrix_sha256": MATRIX_SHA256,
    }
    for key, value in expected.items():
        require(descriptor.get(key) == value, f"descriptor drift: {key}")


def check(root: Path) -> None:
    check_lineage(root)
    check_governance(root)
    tz = root / "docs/stage-7/TZ_STAGE7B_PRODUCTION_DURABILITY_COMPOSITION_2026-08-12.md"
    matrix = root / "docs/stage-7/STAGE7B_ACCEPTANCE_MATRIX_2026-08-12.csv"
    require(sha256(tz) == TZ_SHA256, "normative Stage 7B TZ drift")
    require(sha256(matrix) == MATRIX_SHA256, "normative Stage 7B matrix drift")
    with matrix.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 80, "Stage 7B matrix must contain exactly 80 rows")
    require([row["ID"] for row in rows] == [f"B-{i:03d}" for i in range(1, 81)], "matrix IDs drift")

    workspace = (root / "Cargo.toml").read_text()
    service_manifest = (root / "crates/runtime-durable-service/Cargo.toml").read_text()
    check_dependencies(workspace, service_manifest)
    validate_source(
        (root / "crates/runtime-durable-service/src/lib.rs").read_text(),
        (root / "crates/strategy-runtime-core/src/stage6_journal_backend.rs").read_text(),
        (root / "crates/runtime-durable-service/tests/stage7b_writer_lock_subprocess.rs").read_text(),
    )
    descriptor = json.loads((root / "docs/stage-7/stage7b-b-entry-descriptor.json").read_text())
    validate_descriptor(descriptor)
    subprocess.run(["python3", "scripts/stage7b_proof_map.py"], cwd=root, check=True)
    print("stage7b-b-check: PASS rows=80 implemented=26 pending=54 accepted=false")


if __name__ == "__main__":
    check(ROOT)
