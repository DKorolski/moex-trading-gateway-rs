#!/usr/bin/env python3
"""Fail-closed source/scope checker for Stage 8B-P1-e I1A."""

from __future__ import annotations

import csv
import json
import pathlib
import subprocess


ROOT = pathlib.Path(__file__).resolve().parents[1]
ACCEPTED_DESIGN = "aa24e840ed8b7d18c80be6f1fdd8f50facf5b6d4"
EXPECTED_CHANGED = {
    "crates/finam-gateway/src/lib.rs",
    "crates/finam-gateway/src/stage8b_p1e_schedule_publisher.rs",
    "crates/runtime-durable-service/src/lib.rs",
    "crates/runtime-durable-service/src/recovery.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "crates/runtime-durable-service/src/stage8b_p1_supervisor.rs",
    "crates/runtime-durable-service/src/stage8b_p1e_schedule_source.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
    "crates/strategy-runtime-core/src/stage6_reconciliation_v2.rs",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    "crates/strategy-runtime-core/src/stage8b_p1d1_paper_provider.rs",
    "crates/strategy-runtime-core/src/stage8b_p1d3_working_limit.rs",
    "docs/stage-8/stage8b-p1e-i1a-schedule-source-implementation.md",
    "docs/stage-8/stage8b-p1e-i1a-schedule-source-implementation-evidence.json",
    "scripts/make_stage8b_p1e_i1a_source_handoff.py",
    "scripts/stage8b_p1e_i1a_source_check.py",
    "scripts/stage8b_p1e_i1a_source_gate.sh",
    "scripts/stage8b_p1e_i1a_source_handoff_safety_check.py",
    "scripts/stage8b_p1e_i1a_source_negative_harness.py",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def load_content(root: pathlib.Path = ROOT) -> dict[str, str]:
    paths = {
        "core": "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
        "journal": "crates/strategy-runtime-core/src/stage6_reconciliation_v2.rs",
        "live": "crates/strategy-runtime-core/src/stage6d_live_core.rs",
        "service": "crates/runtime-durable-service/src/recovery.rs",
        "reader": "crates/runtime-durable-service/src/stage8b_p1e_schedule_source.rs",
        "redis": "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
        "supervisor": "crates/runtime-durable-service/src/stage8b_p1_supervisor.rs",
        "publisher": "crates/finam-gateway/src/stage8b_p1e_schedule_publisher.rs",
        "core_lib": "crates/strategy-runtime-core/src/lib.rs",
        "service_lib": "crates/runtime-durable-service/src/lib.rs",
        "finam_lib": "crates/finam-gateway/src/lib.rs",
        "document": "docs/stage-8/stage8b-p1e-i1a-schedule-source-implementation.md",
        "evidence": "docs/stage-8/stage8b-p1e-i1a-schedule-source-implementation-evidence.json",
        "matrix": "docs/stage-8/stage8b-p1e-i1a-schedule-source-acceptance-matrix-v1.csv",
        "r2_matrix": "docs/stage-8/stage8b-p1e-i1a-r2-acceptance-matrix-v1.csv",
    }
    return {name: (root / path).read_text(encoding="utf-8") for name, path in paths.items()}


def section(text: str, start: str, end: str) -> str:
    begin = text.find(start)
    require(begin >= 0, f"section start missing: {start}")
    finish = text.find(end, begin + len(start))
    require(finish >= 0, f"section end missing: {end}")
    return text[begin:finish]


def rust_impl_section(text: str, start: str, implementation: str) -> str:
    begin = text.find(start)
    require(begin >= 0, f"Rust section start missing: {start}")
    impl_start = text.find(implementation, begin)
    require(impl_start >= 0, f"Rust impl missing: {implementation}")
    brace = text.find("{", impl_start)
    require(brace >= 0, f"Rust impl brace missing: {implementation}")
    depth = 0
    for index in range(brace, len(text)):
        if text[index] == "{":
            depth += 1
        elif text[index] == "}":
            depth -= 1
            if depth == 0:
                return text[begin : index + 1]
    raise CheckFailure(f"unterminated Rust impl: {implementation}")


def validate_content(content: dict[str, str]) -> None:
    core = content["core"]
    journal = content["journal"]
    live = content["live"]
    service = content["service"]
    reader = content["reader"]
    redis_source = content["redis"]
    supervisor = content["supervisor"]
    publisher = content["publisher"]
    document = content["document"]

    for token in (
        '"finam_imoexf_paper:{finam-imoexf-p1}:market-schedule"',
        'SEMANTIC_HASH_DOMAIN: &[u8] = b"moex.stage8b.p1e.schedule-semantic-identity.sha256.v1"',
        "pub struct Stage8bP1eScheduleSemanticIdentityV1",
        "pub struct Stage8bP1eScheduleEnvelopeV3",
        "#[serde(deny_unknown_fields)]\n    pub struct Stage8bP1eScheduleInstrumentV1",
        "pub fn stage8b_p1e_schedule_semantic_sha256(",
        "pub fn stage8b_p1e_schedule_unsigned_signature_sha256(",
        "pub fn verify_stage8b_p1e_schedule_envelope_v3(",
        "envelope.semantic_identity != expected_identity",
        "fn verify_signature(",
        "Stage8bP1eScheduleProgressionV1::Idempotent",
        "Stage8bP1eScheduleProgressionV1::MonotonicSnapshot",
        "heartbeat_and_open_to_closed_follow_one_semantic_revision_domain",
        "closed_boundary_proof_is_exactly_the_last_tradable_m10_grid_boundary",
        "authentication_freshness_identity_and_stage4_negative_matrix_fails_closed",
        "signed_trading_day_transition_is_monotonic_and_payload_projection_is_exact",
        "accepted_r2_semantic_fixture_hashes_match_the_production_hasher",
        "signed_v4_recovery_reconstructs_only_the_exact_historical_binding",
    ):
        require(token in core, f"core source invariant missing: {token}")
    require(
        core.count("stage8b_p1e_canonical_json(&envelope)? != exact_envelope_bytes") == 2,
        "strict canonical envelope checks must cover fresh verify and retained observation",
    )

    for token in (
        "pub const STAGE6_DURABLE_RECORD_SCHEMA_VERSION_V4: u16 = 4",
        "pub struct Stage6JournalRecordV4",
        'record_kind: "schedule_evidence_bound".to_string()',
        "V4(Stage6JournalRecordV4)",
        "4 => Stage6JournalRecordV4::decode_canonical(bytes).map(Self::V4)",
        "v1_golden_bytes_and_record_identity_remain_unchanged",
        "canonical_golden_matrix_is_stable",
        "v4_schedule_binding_roundtrips_and_fails_closed_on_wire_drift",
        "market_v4_is_the_exact_request_predecessor_of_dispatch",
    ):
        require(token in journal, f"V4/compatibility invariant missing: {token}")

    for token in (
        "pub fn classify_stage8b_p1e_schedule_journal_ahead_candidate(",
        "Stage6Stage8bP1eScheduleBindingPending",
        ".append_versioned(&Stage6JournalRecordVersioned::V4(record.clone()))",
        "self.refresh_after_append()?",
        "p1e_v4_journal_ahead_classifier_accepts_only_one_exact_successor",
    ):
        require(token in live, f"journal-ahead invariant missing: {token}")

    binding_commit = section(
        service,
        "    pub(crate) fn commit_stage8b_p1e_schedule_binding(",
        "    /// Delegates command admission",
    )
    for token in (
        "pub struct Stage8bP1eScheduleBindingCommittedOwner",
        "commit_stage8b_p1e_schedule_binding",
        "The existing seal writer performs write/fsync/rename",
        "Stage7bRestartOutcome::P1eScheduleBindingCommitted",
    ):
        require(token in service, f"durable composition invariant missing: {token}")
    require(
        "self.advance_recovery_seal(commitment_key)?;\n        if self.committed_seal.seal_generation() != pending.expected_covering_seal_generation()"
        in binding_commit,
        "V4 binding does not complete and cross-check one covering seal",
    )

    read_transport = section(
        reader,
        "pub struct Stage8bP1eRedisScheduleReader",
        "fn verify_newest_reply(",
    )
    require(
        "pub enum Stage8bP1eScheduleLatchCheckpointV1 {\n    BeforeScheduleRead,\n    AfterScheduleReadBeforeBinding,\n    AfterBinding,\n    BeforeEffect,\n}"
        in reader,
        "latch C-F checkpoint inventory drifted",
    )
    for token in (
        'redis::cmd("XREVRANGE")',
        ".arg(STAGE8B_P1E_SCHEDULE_STREAM)",
        '.arg("+")',
        '.arg("-")',
        '.arg("COUNT")',
        ".arg(64)",
        "BeforeScheduleRead",
    ):
        require(token in read_transport, f"read-only transport invariant missing: {token}")
    for forbidden in ("XREADGROUP", "XGROUP", "XACK", "XAUTOCLAIM", "XDEL", "FLUSHDB"):
        require(
            f'redis::cmd("{forbidden}")' not in reader,
            f"schedule reader opened forbidden Redis command: {forbidden}",
        )

    for token in (
        "AfterScheduleReadBeforeBinding",
        "AfterBinding",
        "BeforeEffect",
        "commit_stage8b_p1e_market_schedule",
        "commit_stage8b_p1e_working_limit_schedule",
        "commit_stage8b_p1e_cancel_schedule",
        "commit_stage8b_p1e_day_expiry_schedule",
        "latch_c_stops_before_schedule_transport_and_carries_no_authority",
        "latch_d_returns_the_exact_owner_without_a_binding_seal",
        "latches_e_and_f_retain_the_exact_committed_binding_without_a_second_seal",
        "clear_latches_issue_one_route_bound_authority_after_one_binding_seal",
        "v4_binding_uses_existing_append_cover_reread_chain",
        "v4_journal_ahead_restart_commits_exactly_one_covering_seal",
    ):
        require(token in reader, f"binding/latch invariant missing: {token}")

    for token in (
        "Stage8bP1eReadonlyScheduleAdapterInputV1",
        "pub fn adapt_stage8b_p1e_readonly_schedule(",
        "raw_response_sha256: sha256_hex(&input.exact_finam_response_bytes)",
        "report_sha256: sha256_hex(&stage4_report_bytes)",
        "registry_identity_sha256: input.registry_identity_sha256",
        "inclusive_session_endpoints_reject_adjacency_and_overlap",
        "pub enum Stage8bP1eSchedulePublisherPhaseV1 {\n    Prepared,\n    Published,\n}",
        "file.sync_all()?",
        "File::open(parent)?.sync_all()?",
        'redis::cmd("XADD")',
        '.arg("NOMKSTREAM")',
        '.arg("MAXLEN")',
        '.arg("=")',
        "prepared_state_survives_response_loss_and_replays_exact_signed_bytes",
        "publisher_open_to_closed_changes_revision_with_unchanged_sessions",
    ):
        require(token in publisher, f"publisher invariant missing: {token}")
    require(
        publisher.count('timezone: "Europe/Moscow".to_string(),') == 2,
        "publisher and fixture no longer share the exact Moscow timezone",
    )
    for forbidden in ('redis::cmd("POST")', 'redis::cmd("DELETE")', "place_order", "cancel_order"):
        require(forbidden not in publisher, f"publisher opened forbidden execution token: {forbidden}")

    require(
        supervisor.count("    P1eScheduleBindingCommitted,\n    Blocked,") == 1
        and "        Self::P1eScheduleBindingCommitted,\n        Self::Blocked," in supervisor
        and "Stage7bRestartOutcome::P1eScheduleBindingCommitted(_)" in supervisor,
        "V4 restart kind inventory/classification drifted",
    )
    require(
        'Stage7bRestartOutcome::P1eScheduleBindingCommitted(_) => "P1eScheduleBindingCommitted"'
        in redis_source,
        "I0 restart match is not exhaustive",
    )

    for token in (
        "Stage8bP1eScheduleEnvelopeV3",
        "Stage6JournalRecordV4",
        "Stage8bP1eRedisScheduleReader",
        "Stage8bP1eSchedulePublisherStateV1",
    ):
        require(
            token in content["core_lib"] + content["service_lib"] + content["finam_lib"],
            f"public facade export missing: {token}",
        )

    rows = list(csv.DictReader(content["matrix"].splitlines()))
    r2_rows = list(csv.DictReader(content["r2_matrix"].splitlines()))
    require(len(rows) == 81, "I1A source acceptance inventory must remain 81 rows")
    require(len(r2_rows) == 8, "I1A R2 overlay must remain 8 rows")
    require(len({row["id"] for row in rows}) == 81, "I1A source acceptance ids must be unique")
    require(all(row["status"] == "REQUIRED" for row in rows + r2_rows), "acceptance row opened or removed")

    for token in (
        "81-row source acceptance inventory",
        "V1/V2/V3 wire and replay compatibility",
        "XREVRANGE + - COUNT 64",
        "XADD NOMKSTREAM MAXLEN = 4096",
        "105 x 2",
        "Redis DB15/DB0 activation",
        "FINAM POST/DELETE",
    ):
        require(token in document, f"implementation document invariant missing: {token}")

    evidence = json.loads(content["evidence"])
    require(evidence["stage"] == "Stage 8B-P1-e I1A source implementation", "evidence stage drift")
    require(evidence["status"] == "SOURCE_REVIEW_CANDIDATE", "evidence status drift")
    require(evidence["accepted_design_ref"] == ACCEPTED_DESIGN, "evidence design ref drift")
    require(evidence["acceptance_rows"] == 81 and evidence["r2_overlay_rows"] == 8, "evidence inventory drift")
    require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")
    require(evidence["next_stage_authorized"] is False, "next source/deployment stage opened early")


def validate_repository() -> None:
    require(git("rev-parse", "HEAD") != ACCEPTED_DESIGN, "source implementation must be a new commit")
    require(
        not git("status", "--porcelain", "--untracked-files=all"),
        "source acceptance requires a clean immutable worktree",
    )
    changed = set(filter(None, git("diff", "--name-only", ACCEPTED_DESIGN, "HEAD", "--").splitlines()))
    require(changed == EXPECTED_CHANGED, f"source scope drift: {sorted(changed ^ EXPECTED_CHANGED)}")
    require(not any(path.startswith(".github/workflows/") for path in changed), "workflow change is closed")
    require(not any(path.startswith(("config/", "configs/", "deploy/", "deployment/")) for path in changed), "operational config/deployment change is closed")
    require(not any(path == "Cargo.lock" or path.endswith("/Cargo.toml") for path in changed), "dependency change is closed")

    frozen = [
        path
        for path in git("ls-tree", "-r", "--name-only", ACCEPTED_DESIGN, "--", "docs/stage-8", "scripts").splitlines()
        if "stage8b-p1e-i1a" in path or "stage8b_p1e_i1a" in path
    ]
    require(bool(frozen), "accepted I1A artifact inventory is empty")
    for path in frozen:
        require((ROOT / path).is_file(), f"accepted design artifact removed: {path}")
        require(
            (ROOT / path).read_bytes()
            == subprocess.check_output(["git", "show", f"{ACCEPTED_DESIGN}:{path}"], cwd=ROOT),
            f"accepted design artifact changed: {path}",
        )

    current = (ROOT / "crates/strategy-runtime-core/src/stage6_reconciliation_v2.rs").read_text()
    baseline = subprocess.check_output(
        ["git", "show", f"{ACCEPTED_DESIGN}:crates/strategy-runtime-core/src/stage6_reconciliation_v2.rs"],
        cwd=ROOT,
        text=True,
    )
    v2_start = "#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]\n#[serde(rename_all = \"snake_case\")]\npub enum Stage6ReconciliationEndpointKindV2"
    v3_start = "#[derive(Debug, Clone, PartialEq, Serialize)]\npub struct Stage6JournalRecordV3"
    require(section(current, v2_start, v3_start) == section(baseline, v2_start, v3_start), "V2 source contract changed")
    require(
        rust_impl_section(current, v3_start, "impl Stage6JournalRecordV3 {")
        == rust_impl_section(baseline, v3_start, "impl Stage6JournalRecordV3 {"),
        "V3 source contract changed",
    )
    require(
        (ROOT / "crates/strategy-runtime-core/src/stage6_durable_identity.rs").read_bytes()
        == subprocess.check_output(
            ["git", "show", f"{ACCEPTED_DESIGN}:crates/strategy-runtime-core/src/stage6_durable_identity.rs"],
            cwd=ROOT,
        ),
        "V1 source contract changed",
    )


def main() -> None:
    try:
        validate_content(load_content())
        validate_repository()
    except (CheckFailure, OSError, KeyError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"stage8b-p1e-i1a-source-check: FAIL {error}")
        raise SystemExit(1)
    print(
        "PASS stage8b-p1e-i1a-source-check "
        "acceptance=81 r2_overlay=8 v1_v2_v3=unchanged v4=schedule_evidence_bound "
        "reader=newest-only publisher=signed-durable latches=A-F db0=false db15=false finam_write=false live=false"
    )


if __name__ == "__main__":
    main()
