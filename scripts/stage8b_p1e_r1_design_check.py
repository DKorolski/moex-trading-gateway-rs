#!/usr/bin/env python3
"""Fail-closed checker for the Stage 8B-P1-e R1 corrected design."""

from __future__ import annotations

import csv
import hashlib
import io
import json
import pathlib
import re
import subprocess


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "c2a9e1246dfdd59f3a6297268de907dedcb19903"
R0 = "7a186cd2ac78a57eff1ad8f24aa52b9dab82b68b"
P1D4_REVIEW_SHA256 = "93180e3f633c256ab9bb2cdfa43dd63c3fe7a1b7380eac435feae45cf8969142"
R0_REVIEW_SHA256 = "52a4fb8e4dcaada0ba256c15351895744bb5d483111c1ac95505de119450fbf0"

DESIGN = ROOT / "docs/stage-8/stage8b-p1e-deployable-supervisor-design-r1.md"
R0_DESIGN = ROOT / "docs/stage-8/stage8b-p1e-deployable-supervisor-design.md"
MATRIX = ROOT / "docs/stage-8/stage8b-p1e-deployable-supervisor-r1-acceptance-matrix.csv"
RESTART = ROOT / "docs/stage-8/stage8b-p1e-restart-continuation-matrix-v1.csv"
EVENTS = ROOT / "docs/stage-8/stage8b-p1e-supervisor-event-matrix-v1.csv"
PROFILE = ROOT / "docs/stage-8/stage8b-p1e-runtime-profile-v1.json"
SOURCE_SCHEMA = ROOT / "docs/stage-8/stage8b-p1e-first-boot-source-bundle-schema-v1.json"
SOURCE_PLAN = ROOT / "docs/stage-8/stage8b-p1e-first-boot-source-plan-v1.json"
REDIS = ROOT / "docs/stage-8/stage8b-p1e-redis-deployment-manifest-v1.json"
TELEMETRY = ROOT / "docs/stage-8/stage8b-p1e-telemetry-contract-v1.json"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1e-deployable-supervisor-r1-design-evidence.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"
RECOVERY_SOURCE = ROOT / "crates/runtime-durable-service/src/recovery.rs"

CONTRACT_HASHES = {
    "runtime_profile_v1_sha256": "dd5a211e708db0d40175d19ed1eeb51db26497d344a553b41d7afbfdddde0ef6",
    "first_boot_source_bundle_schema_v1_sha256": "aed9219ad0c7e79bc860e7e24d18229c5680213ebb183f4a5ed6e3f4774e2e21",
    "first_boot_source_plan_v1_sha256": "e6152e46cd5b49414372f51681cd2194faf17cb2862301aaaba7a5d257ff637c",
    "redis_deployment_manifest_v1_sha256": "080050c53485cf08c86d1055f6ce071d84077baa121440e6e9051d3960ee5d82",
    "telemetry_contract_v1_sha256": "d2161a02e982a0e5e95b13596d6632a4368b8d74787c3376d5ffa0d11ef120f5",
    "restart_continuation_matrix_v1_file_sha256": "c64a14ad19f40d4ff5964c359b9131b27dc47917404ac283e4301f36dfddf04a",
    "supervisor_event_matrix_v1_file_sha256": "37c4f423f80ebba281964f571511ff23c74f3a5b2355b4e7afb78d36d880bcae",
}

EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-design-evidence.json",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-design.md",
    "scripts/make_stage8b_p1e_design_handoff.py",
    "scripts/stage8b_p1e_design_check.py",
    "scripts/stage8b_p1e_design_gate.sh",
    "scripts/stage8b_p1e_design_handoff_safety_check.py",
    "scripts/stage8b_p1e_design_negative_harness.py",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-design-r1.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r1-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r1-design-evidence.json",
    "docs/stage-8/stage8b-p1e-runtime-profile-v1.json",
    "docs/stage-8/stage8b-p1e-first-boot-source-bundle-schema-v1.json",
    "docs/stage-8/stage8b-p1e-first-boot-source-plan-v1.json",
    "docs/stage-8/stage8b-p1e-redis-deployment-manifest-v1.json",
    "docs/stage-8/stage8b-p1e-telemetry-contract-v1.json",
    "docs/stage-8/stage8b-p1e-restart-continuation-matrix-v1.csv",
    "docs/stage-8/stage8b-p1e-supervisor-event-matrix-v1.csv",
    "scripts/stage8b_p1e_r1_design_check.py",
    "scripts/stage8b_p1e_r1_design_negative_harness.py",
    "scripts/stage8b_p1e_r1_design_gate.sh",
    "scripts/make_stage8b_p1e_r1_design_handoff.py",
    "scripts/stage8b_p1e_r1_design_handoff_safety_check.py",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def canonical_json_sha256(value: object) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
    return hashlib.sha256(encoded).hexdigest()


def file_sha256(value: str) -> str:
    return hashlib.sha256(value.encode()).hexdigest()


def csv_rows(value: str) -> list[dict[str, str]]:
    return list(csv.DictReader(io.StringIO(value)))


def changed_files() -> set[str]:
    tracked = subprocess.run(
        ["git", "diff", "--name-only", BASE], cwd=ROOT, check=True,
        text=True, capture_output=True,
    ).stdout.splitlines()
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT,
        check=True, text=True, capture_output=True,
    ).stdout.splitlines()
    return set(tracked) | set(untracked)


def restart_variants(source: str) -> list[str]:
    body = source.split("pub enum Stage7bRestartOutcome {", 1)[1].split("\n}", 1)[0]
    return re.findall(r"^    ([A-Za-z][A-Za-z0-9]+)(?:\(|,)", body, flags=re.MULTILINE)


def validate(
    design: str,
    r0_design: str,
    matrix_text: str,
    restart_text: str,
    event_text: str,
    profile: dict[str, object],
    source_schema: dict[str, object],
    source_plan: dict[str, object],
    redis: dict[str, object],
    telemetry: dict[str, object],
    evidence: dict[str, object],
    status: str,
    roadmap: str,
    recovery_source: str,
) -> None:
    required_design = (
        "Status: R1 design-only review candidate",
        BASE,
        R0,
        P1D4_REVIEW_SHA256,
        R0_REVIEW_SHA256,
        "HOLD / SUPERSEDED BY R1",
        "build_stage8b_p1_first_boot_source_v1",
        "Stage8bP1RiskGateHistoryOracleV1::rebuild",
        "explicitly not a\nriskgate-ledger source",
        "stage5d_validate_riskgate_ledger_evidence",
        "stage8b_p1_test_first_boot_material",
        "create the identity-derived durable root only inside first_boot_stage8b_p1",
        "moex-finam-p1-paper-bootstrap.service",
        "RestrictAddressFamilies=AF_UNIX",
        "S06R execute the matrix continuation",
        "Pre-evidence/dispatch-only phases may reacquire only the exact deterministic",
        "Every post-evidence phase forbids provider, schedule or callback authority",
        "redis://127.0.0.1:6379/15",
        "redis://[::1]:6379/15",
        "NOMKSTREAM MAXLEN = 4096",
        "RestartSec=5s",
        "StartLimitIntervalSec=300",
        "StartLimitBurst=5",
        "12 attempts over at most 60 seconds",
        "At most 16 stale consumers",
        "idle time at least 24 hours",
        "owner panic or return without owner destroys linear drain authority",
        "check shutdown latch again before parse/callback/provider/schedule",
        "observation_ts_utc\nboot_id\nconsumer_name",
        "0 < child_pid <= u32::MAX",
        "No P1-e result authorizes installation, startup or DB0/VPS use",
    )
    for token in required_design:
        require(token in design, f"missing R1 design invariant: {token}")
    require("R0 design candidate is HOLD and superseded" in r0_design, "R0 status not superseded")

    matrix = csv_rows(matrix_text)
    require(len(matrix) == 88, f"acceptance rows drifted: {len(matrix)}")
    require(list(matrix[0]) == ["id", "area", "requirement", "status"], "acceptance fields drifted")
    require([row["id"] for row in matrix] == [f"P1ER1-{i:03d}" for i in range(1, 89)], "acceptance IDs drifted")
    require(all(row["status"] == "REQUIRED" for row in matrix), "acceptance weakened")
    by_id = {row["id"]: row["requirement"] for row in matrix}
    for row_id, token in {
        "P1ER1-017": "recomputed and matched before root or Redis effect",
        "P1ER1-026": "Production riskgate history oracle",
        "P1ER1-027": "derived row fields are forbidden",
        "P1ER1-034": "stage8b_p1_test_first_boot_material",
        "P1ER1-041": "S06R",
        "P1ER1-042": "every current Stage7bRestartOutcome variant",
        "P1ER1-059": "NOMKSTREAM MAXLEN equals 4096",
        "P1ER1-065": "Lost owner forbids drain",
        "P1ER1-071": "before and after XREADGROUP delivery",
        "P1ER1-077": "forbids fresh XREADGROUP greater-than",
        "P1ER1-086": "Only observation timestamp boot ID and consumer name",
        "P1ER1-088": "remain closed",
    }.items():
        require(token in by_id[row_id], f"acceptance semantic drift: {row_id}")

    restarts = csv_rows(restart_text)
    expected_restart_fields = [
        "variant", "redis_attachment", "source_or_pel_precondition",
        "equivalent_authority_reissue", "exact_continuation", "target_boundary",
        "xack_rule", "readiness_after_s06r", "shutdown_rule",
    ]
    require(len(restarts) == 22, f"restart rows drifted: {len(restarts)}")
    require(list(restarts[0]) == expected_restart_fields, "restart fields drifted")
    variants = restart_variants(recovery_source)
    require(len(variants) == 22 and len(set(variants)) == 22, "source restart enum inventory drifted")
    require([row["variant"] for row in restarts] == variants, "restart matrix not exhaustive or ordered")
    require(all(row["exact_continuation"] != "" for row in restarts), "restart continuation empty")
    restart_by_variant = {row["variant"]: row for row in restarts}
    require(restart_by_variant["Blocked"]["redis_attachment"] == "forbidden", "Blocked attached Redis")
    require(restart_by_variant["Stage8a4I3Pending"]["readiness_after_s06r"] == "never", "legacy pending ready")
    require("required_exact_schedule_step" in restart_by_variant["P1d3DispatchPending"]["equivalent_authority_reissue"], "P1-d3 reissue missing")
    require(restart_by_variant["P1d3AckCommitted"]["equivalent_authority_reissue"] == "forbidden", "post-evidence reissue opened")

    events = csv_rows(event_text)
    expected_event_fields = [
        "id", "event", "owner_availability", "owner_phase", "allowed_next_effect",
        "exit_code", "readiness_phase", "source_pel_disposition", "restart_authority",
    ]
    require(len(events) == 24, f"event rows drifted: {len(events)}")
    require(list(events[0]) == expected_event_fields, "event fields drifted")
    require([row["id"] for row in events] == [f"E{i:02d}" for i in range(1, 25)], "event IDs drifted")
    require(len({tuple(row.values()) for row in events}) == 24, "duplicate event rows")
    event_by_id = {row["id"]: row for row in events}
    require(event_by_id["E08"]["allowed_next_effect"] == "no_drain_no_telemetry_claimed_from_owner", "panic drain opened")
    require(event_by_id["E11"]["owner_availability"] == "lost", "ownerless return classification drifted")
    require("no_parse_callback_provider_or_schedule" in event_by_id["E18"]["allowed_next_effect"], "post-read latch check missing")
    require(event_by_id["E21"]["exit_code"] == "72", "grace exit drifted")

    contract_values = {
        "runtime_profile_v1_sha256": canonical_json_sha256(profile),
        "first_boot_source_bundle_schema_v1_sha256": canonical_json_sha256(source_schema),
        "first_boot_source_plan_v1_sha256": canonical_json_sha256(source_plan),
        "redis_deployment_manifest_v1_sha256": canonical_json_sha256(redis),
        "telemetry_contract_v1_sha256": canonical_json_sha256(telemetry),
        "restart_continuation_matrix_v1_file_sha256": file_sha256(restart_text),
        "supervisor_event_matrix_v1_file_sha256": file_sha256(event_text),
    }
    require(contract_values == CONTRACT_HASHES, f"canonical contract hash drift: {contract_values}")

    require(profile.get("schema_version") == 1 and profile.get("profile_id") == "imoexf-hybrid-high180-paper-v1", "profile identity drifted")
    semantic = profile.get("semantic_config")
    require(isinstance(semantic, dict), "semantic config missing")
    require(len(semantic) == 29, f"runtime field inventory drifted: {len(semantic)}")
    require(semantic.get("timezone_offset_hours") == 3, "timezone drifted")
    require(semantic.get("pending_timeout_sec") == 60, "pending timeout drifted")
    require(semantic.get("qty") == "1.0", "paper quantity drifted")
    require(semantic.get("risk_gate_seed_file") is None and semantic.get("risk_gate_ledger_key") is None, "external riskgate identity opened")
    require(profile.get("paper_safety", {}).get("allow_live_orders") is False, "live order profile opened")

    require(source_schema.get("$id") == "moex.stage8b.p1e.first-boot-source-bundle.v1", "source schema ID drifted")
    require(source_schema.get("additionalProperties") is False, "source unknown fields opened")
    source_properties = source_schema.get("properties", {})
    source_required = source_schema.get("required", [])
    require("riskgate_history" in source_required, "riskgate source observations are optional")
    require(source_properties.get("runtime_profile_sha256", {}).get("const") == CONTRACT_HASHES["runtime_profile_v1_sha256"], "source/profile binding drifted")
    require(source_properties.get("broker_truth", {}).get("additionalProperties") is False, "broker truth unknown fields opened")
    require(source_properties.get("broker_truth", {}).get("properties", {}).get("target_position_qty", {}).get("const") == "0", "first boot nonflat opened")
    riskgate_history = source_properties.get("riskgate_history", {})
    require(riskgate_history.get("additionalProperties") is False, "riskgate source unknown fields opened")
    require(riskgate_history.get("properties", {}).get("source_mode", {}).get("const") == "source-compatible-high180-shadow-history-v1", "riskgate source mode drifted")
    require(riskgate_history.get("properties", {}).get("state_generation", {}).get("const") == "runtime-ledger-v1", "riskgate generation drifted")
    require(riskgate_history.get("properties", {}).get("session_observations", {}).get("minItems") == 120, "riskgate observation bound drifted")
    riskgate_observation = source_schema.get("$defs", {}).get("riskgate_session_observation", {})
    require(riskgate_observation.get("additionalProperties") is False, "riskgate derived observation fields opened")
    require(riskgate_observation.get("required") == ["session_date", "shadow_pnl_points", "shadow_trade_count"], "riskgate observation fields drifted")

    require(source_plan.get("production_facade") == "build_stage8b_p1_first_boot_source_v1", "production source facade drifted")
    source_bundle = source_plan.get("source_bundle", {})
    require(source_bundle.get("path_override_allowed") is False and source_bundle.get("test_fixture_feature_allowed") is False, "source override opened")
    history = source_plan.get("history", {})
    require(history.get("minimum_complete_moscow_sessions") == 121 and history.get("history_callbacks_may_publish_commands") is False, "history contract drifted")
    riskgate = source_plan.get("riskgate", {})
    require(riskgate.get("minimum_finalized_sessions") == 120 and riskgate.get("external_ledger_allowed") is False, "riskgate source drifted")
    require(riskgate.get("history_oracle") == "Stage8bP1RiskGateHistoryOracleV1::rebuild", "riskgate oracle drifted")
    require(riskgate.get("warmup_from_history_is_ledger_source") is False, "warmup falsely opened as ledger source")
    require(riskgate.get("cross_validation") == "oracle-output-exactly-equals-bundle-observations", "riskgate cross-validation drifted")
    require(riskgate.get("derived_fields_accepted_from_bundle") is False, "riskgate derived fields opened")
    require(riskgate.get("first_boot_startup_mode") == "bootstrap_from_seed" and riskgate.get("steady_state_startup_mode") == "normal_append", "riskgate startup modes drifted")
    require(riskgate.get("startup_planner") == "plan_risk_gate_startup", "riskgate startup planner drifted")
    require(riskgate.get("stage5d_validator") == "stage5d_validate_riskgate_ledger_evidence", "Stage5D riskgate validation drifted")
    require(source_plan.get("effect_order", [])[-1] == "create-durable-root-last", "root creation order drifted")

    require(redis.get("redis_url_allowlist") == ["redis://127.0.0.1:6379/15", "redis://[::1]:6379/15"], "Redis URL allowlist drifted")
    require(redis.get("redis_db_index") == 15, "Redis DB drifted")
    require(redis.get("namespace_digest_sha256") == "18efd270fb03fa68f92b8288968f6cf16e8f529330c4335cb86cdaaba0b826ed", "namespace digest drifted")
    require(len(redis.get("keys", [])) == 10, "Redis inventory drifted")
    require(redis.get("run_may_create_or_repair") is False and redis.get("manifest_write_allowed_in_p1e") is False, "Redis repair opened")
    require("NOMKSTREAM MAXLEN = 4096" in redis.get("telemetry_write_command", ""), "telemetry create protection drifted")

    require(telemetry.get("readiness_phase_enum") == ["starting", "paper_ready", "degraded", "draining", "stopped"], "readiness enum drifted")
    require(len(telemetry.get("failure_class_enum", [])) == 17, "failure enum drifted")
    require(telemetry.get("two_run_volatile_allowlist") == ["observation_ts_utc", "boot_id", "consumer_name"], "volatile allowlist drifted")
    require(telemetry.get("redis_write", {}).get("implicit_stream_creation") is False, "telemetry implicit create opened")
    require(telemetry.get("redaction", {}).get("raw_error_text") == "never-serialized-map-to-closed-enum", "raw errors opened")

    require(evidence.get("stage") == "Stage 8B-P1-e R1 deployable paper supervisor design correction", "evidence stage drifted")
    require(evidence.get("status") == "R1_DESIGN_REVIEW_CANDIDATE", "evidence status drifted")
    require(evidence.get("accepted_p1d4_closure_ref") == BASE, "evidence predecessor drifted")
    require(evidence.get("r0_ref") == R0 and evidence.get("r0_verdict") == "HOLD_SUPERSEDED_BY_R1", "evidence R0 lineage drifted")
    require(evidence.get("acceptance_rows") == 88 and evidence.get("negative_cases") == 64, "evidence gate counts drifted")
    require(evidence.get("restart_matrix_rows") == 22 and evidence.get("supervisor_event_matrix_rows") == 24, "evidence matrix counts drifted")
    require(evidence.get("canonical_contracts") == CONTRACT_HASHES, "evidence contract hashes drifted")
    require(evidence.get("design_only") is True and evidence.get("implementation_authorized") is False, "implementation opened")
    require(evidence.get("operational_activation_authorized") is False, "activation opened")
    require(all(value is False for value in evidence.get("closed_surfaces", {}).values()), "closed surface opened")
    require(evidence.get("startup_phases") == ["S00", "S01", "S02", "S03", "S04", "S05", "S06", "S06R", "S07", "S08", "S09"], "startup phases drifted")
    require(evidence.get("pel_contract", {}).get("fresh_read_while_unclaimable_pending") is False, "fresh read opened")
    first_boot = evidence.get("first_boot_contract", {})
    require(first_boot.get("riskgate_history_oracle") == "Stage8bP1RiskGateHistoryOracleV1::rebuild", "evidence riskgate oracle drifted")
    require(first_boot.get("warmup_from_history_is_ledger_source") is False, "evidence warmup ledger claim drifted")
    require(first_boot.get("external_riskgate_derived_fields_allowed") is False and first_boot.get("riskgate_stage5d_validation_required") is True, "evidence riskgate authority drifted")
    require(evidence.get("deferred_p2", {}).get("required_bound") == "0 < child_pid <= u32::MAX", "deferred P2 drifted")

    for token in (
        "P1-e R0 design at `7a186cd2ac78a57eff1ad8f24aa52b9dab82b68b` is HOLD",
        "active candidate is P1-e R1 design correction",
        "P1-e source implementation remains unauthorized",
    ):
        require(token in status, f"status drifted: {token}")
    for token in (
        "P1-e R0 design is HOLD",
        "P1-e R1 design correction is the active candidate",
        "Only independent R1 design acceptance may open P1-e source implementation",
    ):
        require(token in roadmap, f"roadmap drifted: {token}")


def load_json(path: pathlib.Path) -> dict[str, object]:
    def reject_duplicates(pairs: list[tuple[str, object]]) -> dict[str, object]:
        value: dict[str, object] = {}
        for key, item in pairs:
            if key in value:
                raise CheckFailure(f"duplicate JSON key in {path}: {key}")
            value[key] = item
        return value

    decoded = json.loads(
        path.read_text(encoding="utf-8"), object_pairs_hook=reject_duplicates
    )
    require(isinstance(decoded, dict), f"JSON contract is not an object: {path}")
    return decoded


def main() -> None:
    try:
        require(changed_files() == EXPECTED_CHANGED, f"changed path drift: {sorted(changed_files())}")
        validate(
            DESIGN.read_text(encoding="utf-8"),
            R0_DESIGN.read_text(encoding="utf-8"),
            MATRIX.read_text(encoding="utf-8"),
            RESTART.read_text(encoding="utf-8"),
            EVENTS.read_text(encoding="utf-8"),
            load_json(PROFILE), load_json(SOURCE_SCHEMA), load_json(SOURCE_PLAN),
            load_json(REDIS), load_json(TELEMETRY), load_json(EVIDENCE),
            STATUS.read_text(encoding="utf-8"), ROADMAP.read_text(encoding="utf-8"),
            RECOVERY_SOURCE.read_text(encoding="utf-8"),
        )
    except (OSError, KeyError, IndexError, ValueError, json.JSONDecodeError, subprocess.CalledProcessError, CheckFailure) as error:
        print(f"stage8b-p1e-r1-design-check: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1e-r1-design-scope files=25 acceptance=88 restart=22 events=24 riskgate_source=history_oracle design_only=true")


if __name__ == "__main__":
    main()
