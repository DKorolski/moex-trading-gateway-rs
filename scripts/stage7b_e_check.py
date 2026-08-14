#!/usr/bin/env python3
"""Stage 7B-e aggregate durability closure checker."""
from __future__ import annotations

import difflib
import hashlib
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BRANCH = "stage7b-production-durability"
ACCEPTED_D_C = "2b6371adb905654e0ddd8b6714159bcef737b577"
ACCEPTED_STAGE7A = "2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64"
NORMATIVE_MATRIX_SHA256 = "d4f5dc4ee8a65ee007a2fe01075927dd6136ec1df8557c8dc37e8105dd0936c9"
ALLOWED_R3_PRODUCTION_PREFIX_SHA256 = {
    "crates/runtime-durable-service/src/lib.rs": "a19cf7393a89a271592f3eceb64c48446310a756f261843da085b2af01a4fff4",
    "crates/strategy-runtime-core/src/stage6_journal_backend.rs": "13ca5b838fbe01b33a86d1cc97f6422edceb9d0cf98df1da2d3f06c7160b611f",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def source_block(source: str, needle: str) -> str:
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


def git_show(ref: str, path: str) -> str:
    return subprocess.check_output(["git", "show", f"{ref}:{path}"], cwd=ROOT, text=True)


def production_prefix(source: str) -> str:
    marker = "#[cfg(test)]\nmod tests"
    require(marker in source, "test-module boundary absent")
    return source.split(marker, 1)[0]


def check_lineage() -> None:
    merge_base = subprocess.check_output(
        ["git", "merge-base", "HEAD", ACCEPTED_D_C], cwd=ROOT, text=True
    ).strip()
    require(merge_base == ACCEPTED_D_C, "candidate is not based on accepted d-c-R2")
    branch = subprocess.check_output(
        ["git", "branch", "--show-current"], cwd=ROOT, text=True
    ).strip()
    require(branch == BRANCH, "Stage 7B-e branch drift")


def check_closed_surface() -> None:
    for path in (
        "crates/runtime-durable-service/src/recovery.rs",
        "crates/runtime-durable-service/src/recovery/redis_settlement.rs",
    ):
        current = (ROOT / path).read_text()
        accepted = git_show(ACCEPTED_D_C, path)
        require(
            production_prefix(current) == production_prefix(accepted),
            f"Stage 7B-e changed production code before test boundary: {path}",
        )
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", ACCEPTED_D_C, "--"], cwd=ROOT, text=True
    ).splitlines()
    allowed_crates = {
        "crates/runtime-durable-service/src/lib.rs",
        "crates/runtime-durable-service/src/recovery.rs",
        "crates/runtime-durable-service/src/recovery/redis_settlement.rs",
        "crates/runtime-durable-service/tests/stage7b_writer_lock_subprocess.rs",
        "crates/runtime-durable-service/tests/stage7b_redis_service_subprocess.rs",
        "crates/strategy-runtime-core/src/stage6_journal_backend.rs",
    }
    crate_delta = {path for path in changed if path.startswith("crates/")}
    require(crate_delta <= allowed_crates, f"aggregate production scope expanded: {sorted(crate_delta - allowed_crates)}")
    require(not any(path.startswith(".github/") for path in changed), "CI governance scope changed")
    require(
        not any("stage8" in path.lower() or "stage-8" in path.lower() for path in changed),
        "Stage 8 changed path introduced before Gate 7→8",
    )

    forbidden_added_tokens = (
        "stage8",
        "stage 8",
        "finam",
        "broker-finam",
        "finam-gateway",
        "reqwest",
        "runtime_live",
        "runtime-live",
        "real_order",
        "broker_network_dispatch",
        ".post(",
        ".delete(",
        "method::post",
        "method::delete",
    )
    for path, expected_sha256 in ALLOWED_R3_PRODUCTION_PREFIX_SHA256.items():
        current_prefix = production_prefix((ROOT / path).read_text())
        actual_sha256 = hashlib.sha256(current_prefix.encode()).hexdigest()
        require(
            actual_sha256 == expected_sha256,
            f"non-whitelisted production-prefix delta in R2 diagnostic seam: {path}",
        )
        accepted_prefix = production_prefix(git_show(ACCEPTED_D_C, path))
        added = "\n".join(
            line[1:]
            for line in difflib.unified_diff(
                accepted_prefix.splitlines(), current_prefix.splitlines(), lineterm=""
            )
            if line.startswith("+") and not line.startswith("+++")
        ).lower()
        for token in forbidden_added_tokens:
            require(token not in added, f"hidden Stage8/live token in production delta: {path}: {token}")


def check_single_execution_authority() -> None:
    live = (ROOT / "crates/strategy-runtime-core/src/stage6d_live_core.rs").read_text()
    recovery = (ROOT / "crates/runtime-durable-service/src/recovery.rs").read_text()
    service = (ROOT / "crates/runtime-durable-service/src/recovery/redis_service.rs").read_text()
    manifest = (ROOT / "crates/runtime-durable-service/Cargo.toml").read_text().lower()
    recovered = source_block(live, "pub struct Stage6dDurableRuntimeRecovered")
    require(recovered.count("journal: Stage6OwnedJournalBackend") == 1, "one owned Stage6 journal field required")
    require("Stage6MemoryJournalBackend" not in recovered, "production recovered core owns memory journal")
    owner = source_block(recovery, "pub struct Stage7bRecoveryReadyOwner")
    require(owner.count("recovered: Stage6dDurableRuntimeRecovered") == 1, "recovery owner must own one recovered core")
    require("journal:" not in owner, "recovery owner introduced a second journal")
    consumer = source_block(service, "pub struct Stage7bRedisService<P>")
    require(consumer.count("owner: Stage7bRecoveryReadyOwner") == 1, "consumer must own one recovery authority")
    require("journal" not in consumer.lower(), "Redis consumer introduced journal authority")
    for forbidden in ("rusqlite", "sqlx", "orderpathstore", "broker-finam", "reqwest"):
        require(forbidden not in manifest, f"forbidden execution authority/dependency: {forbidden}")
    for token in (
        "stage7b_e_production_authority_is_file_backed_and_single_owned",
        "assert!(recovered.journal_is_file_backed())",
    ):
        require(token in recovery, f"file-backed aggregate witness absent: {token}")


def check_journal_parent_directory_fsync() -> None:
    journal = (ROOT / "crates/strategy-runtime-core/src/stage6_journal_backend.rs").read_text()
    service_root = (ROOT / "crates/runtime-durable-service/src/lib.rs").read_text()
    direct_create = source_block(journal, "pub fn create_new(path:")
    order = [direct_create.index(token) for token in (
        "file.write_all(&journal_header())?",
        "file.sync_data()?",
        "sync_parent_directory(&path)?",
        "Self::from_validated_file",
    )]
    require(order == sorted(order), "direct journal creation directory durability order drift")
    owned_create = source_block(
        journal, "pub fn create_new_from_owned_file_with_pre_sync_observer<F>("
    )
    owned_order = [
        owned_create.index("file.write_all(&journal_header())?"),
        owned_create.index("observer();"),
        owned_create.index("file.sync_data()?"),
        owned_create.index("Self::from_validated_file"),
    ]
    require(owned_order == sorted(owned_order), "X02 owned-journal observer ordering drift")
    ordinary_owned = source_block(journal, "pub fn create_new_from_owned_file(")
    require(
        "create_new_from_owned_file_with_pre_sync_observer(diagnostic_path, file, || {})"
        in ordinary_owned,
        "ordinary owned-journal path does not bind the X02 seam to a no-op",
    )
    open_journal = source_block(service_root, "fn open_journal<F>(")
    require(
        open_journal.index("create_new_from_owned_file_with_pre_sync_observer")
        < open_journal.index("self.root.root_directory.sync_all()?")
        < open_journal.index("journal\n        }"),
        "owned journal parent-directory barrier drift",
    )
    ordinary_create = source_block(service_root, "pub fn create_new(")
    require(
        "Self::open(root, true, |_| {}, || {})" in ordinary_create,
        "ordinary runtime create path does not bind the X02 seam to a no-op",
    )
    x02_create = source_block(
        service_root, "pub fn create_new_with_pre_journal_sync_observer<F>("
    )
    for forbidden in ("File", "Stage7bWritableDurableAuthority", "Stage6FileJournalBackend"):
        require(
            forbidden not in x02_create.split("where", 1)[0],
            f"X02 observer signature exposes authority: {forbidden}",
        )
    require(
        "Self::open(root, true, |_| {}, observer)" in x02_create,
        "X02 seam no longer uses the ordinary production create path",
    )


def check_seal_parent_directory_fsync() -> None:
    recovery = (ROOT / "crates/runtime-durable-service/src/recovery.rs").read_text()
    commit = source_block(recovery, "fn commit_recovery_seal_with_pre_rename_observer")
    order = [commit.index(token) for token in (
        "temp.write_all(&bytes)",
        "temp.sync_all()",
        "rename_child_at(",
        ".root_directory\n                .sync_all()",
        "read_committed_recovery_seal()",
    )]
    require(order == sorted(order), "seal temp/sync/rename/directory-sync/reread order drift")


def check_descriptors() -> None:
    descriptor = json.loads((ROOT / "docs/stage-7/stage7b-entry-descriptor.json").read_text())
    expected = {
        "slice": "7B-e",
        "status": "aggregate_acceptance_candidate",
        "candidate_revision": "r3",
        "accepted_predecessor": ACCEPTED_STAGE7A,
        "accepted_stage7b_d_c_ref": ACCEPTED_D_C,
        "implemented_count": 80,
        "pending_count": 0,
        "cross_process_fault_count": 20,
        "negative_case_count": 58,
        "e_negative_case_count": 18,
        "single_writer_required": True,
        "recovery_seal_required": True,
        "inherited_stage7a_gate_required": True,
        "normative_matrix_sha256": NORMATIVE_MATRIX_SHA256,
        "stage7b_d_c_open": False,
        "stage7b_d_c_acceptance_pending": False,
        "stage7b_e_open": True,
        "stage7b_e_acceptance_pending": True,
        "stage7b_accepted": False,
        "finam_post_delete": False,
        "broker_network_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
        "cross_process_exactly_once_claimed": False,
    }
    for key, value in expected.items():
        require(descriptor.get(key) == value, f"aggregate descriptor drift: {key}")


def check_proof_map() -> None:
    subprocess.run(["python3", "scripts/stage7b_proof_map.py"], cwd=ROOT, check=True)
    proof = json.loads((ROOT / "docs/stage-7/stage7b-acceptance-proof-map.json").read_text())
    require(proof.get("slice") == "7B-e", "proof-map slice drift")
    require(proof.get("implemented_count") == 80, "proof-map must reach 80 implemented")
    require(proof.get("pending_count") == 0, "proof-map retains pending rows")
    require(proof.get("stage7b_accepted") is False, "candidate self-accepted Stage 7B")
    for row in proof["proofs"]:
        require(row["status"] == "implemented", f"pending proof row: {row['row_id']}")
        require(not row["exact_witness"].startswith("pending"), f"missing witness: {row['row_id']}")
    by_id = {row["row_id"]: row for row in proof["proofs"]}
    for row_id, tokens in {
        "B-073": ("normative", "X01-X20", "X02", "X12", "X19"),
        "B-075": ("inherited-stage7a-gate.txt", "workspace", "clippy", "fmt"),
        "B-076": ("cases=18", "inherited=40", "aggregate=58"),
        "B-077": ("B-073", "B-075", "B-076", "B-079", "accepted=false"),
        "B-079": ("check_closed_surface", "accepted d-c-R2", "exact production-prefix"),
    }.items():
        witness = by_id[row_id]["exact_witness"]
        for token in tokens:
            require(token in witness, f"{row_id} aggregate proof token absent: {token}")


def check_docs() -> None:
    docs = "\n".join(
        (ROOT / path).read_text()
        for path in (
            "docs/stage-7/stage7b-e-aggregate-closure.md",
            "docs/stage-7/stage7b-d-c-r2-review-closure.md",
            "docs/current-status.md",
            "docs/roadmap.md",
        )
    )
    for token in (
        ACCEPTED_D_C,
        "X01-X20",
        "80/80",
        "R2",
        "inherited Stage 7A",
        "independent acceptance pending",
        "FINAM POST/DELETE",
        "runtime-live",
        "real orders",
        "Gate 7→8",
    ):
        require(token in docs, f"aggregate documentation invariant absent: {token}")


def check_inherited_gate_contract() -> None:
    gate = (ROOT / "scripts/stage7b_e_gate.sh").read_text()
    for token in (
        ACCEPTED_STAGE7A,
        "scripts/stage7a_gate.sh",
        "inherited-stage7a-gate.txt",
        "inherited-stage7a-artifacts",
    ):
        require(token in gate, f"mandatory inherited Stage 7A gate token absent: {token}")


def check_preseal_contract() -> None:
    handoff = (ROOT / "scripts/make_stage7b_e_handoff_archive.py").read_text()
    for token in (
        'run(["git", "status", "--porcelain"])',
        'subprocess.check_output(["git", "archive", "--format=tar", "HEAD"]',
        '"source-tree-manifest.json"',
        '"handoff-commit.txt"',
        '"source_manifest_sha256"',
        'target.with_suffix(".zip.sha256")',
        '"stage7b_accepted": False',
        '"inherited_stage7a_gate_required": True',
        '"candidate_revision": "r3"',
        'STAGE7B_REQUIRE_ORIGIN',
    ):
        require(token in handoff, f"preseal contract absent: {token}")


def main() -> None:
    check_lineage()
    check_closed_surface()
    check_single_execution_authority()
    check_journal_parent_directory_fsync()
    check_seal_parent_directory_fsync()
    check_descriptors()
    check_proof_map()
    subprocess.run(["python3", "scripts/stage7b_fault_matrix_check.py"], cwd=ROOT, check=True)
    check_docs()
    check_inherited_gate_contract()
    check_preseal_contract()
    print("stage7b-e-check: PASS rows=80/80 faults=20/20 accepted=false")


if __name__ == "__main__":
    try:
        main()
    except (CheckFailure, ValueError, KeyError) as error:
        raise SystemExit(f"stage7b-e-check: FAIL: {error}") from error
