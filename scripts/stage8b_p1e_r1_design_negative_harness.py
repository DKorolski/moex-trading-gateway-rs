#!/usr/bin/env python3
"""Targeted 64-case mutation harness for the P1-e R1 design."""

from __future__ import annotations

import copy
import csv
import io

import stage8b_p1e_r1_design_check as checker


def replace_once(value: str, old: str, new: str) -> str:
    if old not in value:
        raise RuntimeError(f"mutation anchor missing: {old!r}")
    return value.replace(old, new, 1)


def csv_text(rows: list[dict[str, str]], fields: list[str]) -> str:
    output = io.StringIO()
    writer = csv.DictWriter(output, fieldnames=fields, lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    return output.getvalue()


design = checker.DESIGN.read_text(encoding="utf-8")
r0_design = checker.R0_DESIGN.read_text(encoding="utf-8")
matrix = checker.MATRIX.read_text(encoding="utf-8")
restart = checker.RESTART.read_text(encoding="utf-8")
events = checker.EVENTS.read_text(encoding="utf-8")
profile = checker.load_json(checker.PROFILE)
source_schema = checker.load_json(checker.SOURCE_SCHEMA)
source_plan = checker.load_json(checker.SOURCE_PLAN)
redis = checker.load_json(checker.REDIS)
telemetry = checker.load_json(checker.TELEMETRY)
evidence = checker.load_json(checker.EVIDENCE)
status = checker.STATUS.read_text(encoding="utf-8")
roadmap = checker.ROADMAP.read_text(encoding="utf-8")
recovery_source = checker.RECOVERY_SOURCE.read_text(encoding="utf-8")

Artifact = tuple[
    str, str, str, str, str, str, dict[str, object], dict[str, object],
    dict[str, object], dict[str, object], dict[str, object], dict[str, object],
    str, str, str,
]


def base(name: str) -> list[object]:
    return [
        name, design, r0_design, matrix, restart, events, copy.deepcopy(profile),
        copy.deepcopy(source_schema), copy.deepcopy(source_plan), copy.deepcopy(redis),
        copy.deepcopy(telemetry), copy.deepcopy(evidence), status, roadmap, recovery_source,
    ]


cases: list[Artifact] = []

for name, old, new in [
    ("authorize-source-before-review", "Status: R1 design-only review candidate", "Status: source implementation authorized"),
    ("erase-r0-hold", "HOLD / SUPERSEDED BY R1", "ACCEPTED"),
    ("remove-production-source-facade", "build_stage8b_p1_first_boot_source_v1", "build_fixture_source"),
    ("allow-test-helper", "stage8b_p1_test_first_boot_material", "production_first_boot_material"),
    ("create-root-early", "create the identity-derived durable root only inside first_boot_stage8b_p1", "create root before validation"),
    ("remove-bootstrap-unit", "moex-finam-p1-paper-bootstrap.service", "manual-bootstrap.sh"),
    ("bootstrap-network", "RestrictAddressFamilies=AF_UNIX", "RestrictAddressFamilies=AF_INET"),
    ("remove-recovery-drain", "S06R execute the matrix continuation", "S06R skipped"),
    ("forbid-required-reissue", "Pre-evidence/dispatch-only phases may reacquire only the exact deterministic", "Pre-evidence phases cannot reacquire the required deterministic"),
    ("allow-post-evidence-reissue", "Every post-evidence phase forbids provider, schedule or callback authority", "Post-evidence phases may reacquire provider authority"),
    ("telemetry-mkstream", "NOMKSTREAM MAXLEN = 4096", "MAXLEN ~ 4096"),
    ("restart-loop-fast", "RestartSec=5s", "RestartSec=0"),
    ("unbounded-claim", "12 attempts over at most 60 seconds", "retry forever"),
    ("panic-drain", "owner panic or return without owner destroys linear drain authority", "coordinator fabricates owner after panic"),
    ("omit-postread-check", "check shutdown latch again before parse/callback/provider/schedule", "process response without second latch check"),
]:
    item = base(name)
    item[1] = replace_once(design, old, new)
    cases.append(tuple(item))  # type: ignore[arg-type]

matrix_rows = checker.csv_rows(matrix)
matrix_fields = list(matrix_rows[0])
for name, mutate in [
    ("acceptance-delete", lambda rows: rows.pop()),
    ("acceptance-id", lambda rows: rows[0].update(id="P1ER1-999")),
    ("acceptance-optional", lambda rows: rows[87].update(status="OPTIONAL")),
    ("acceptance-runtime-effect", lambda rows: rows[16].update(requirement="fingerprint checked after root")),
    ("acceptance-fixture", lambda rows: rows[33].update(requirement="fixture helper allowed")),
    ("acceptance-no-s06r", lambda rows: rows[40].update(requirement="fresh read immediately after claim")),
    ("acceptance-mkstream", lambda rows: rows[58].update(requirement="telemetry may create stream")),
    ("acceptance-normalize-all", lambda rows: rows[85].update(requirement="all telemetry may be normalized")),
]:
    rows = copy.deepcopy(matrix_rows)
    mutate(rows)
    item = base(name)
    item[3] = csv_text(rows, matrix_fields)
    cases.append(tuple(item))  # type: ignore[arg-type]

restart_rows = checker.csv_rows(restart)
restart_fields = list(restart_rows[0])
for name, mutate in [
    ("restart-delete", lambda rows: rows.pop()),
    ("restart-reorder", lambda rows: rows.__setitem__(slice(0, 2), list(reversed(rows[0:2])))),
    ("restart-variant", lambda rows: rows[0].update(variant="Unknown")),
    ("restart-p1d3-reissue-forbidden", lambda rows: rows[15].update(equivalent_authority_reissue="forbidden")),
    ("restart-blocked-redis", lambda rows: rows[21].update(redis_attachment="required_verify_only")),
    ("restart-post-evidence-reissue", lambda rows: rows[17].update(equivalent_authority_reissue="schedule_allowed")),
]:
    rows = copy.deepcopy(restart_rows)
    mutate(rows)
    item = base(name)
    item[4] = csv_text(rows, restart_fields)
    cases.append(tuple(item))  # type: ignore[arg-type]

event_rows = checker.csv_rows(events)
event_fields = list(event_rows[0])
for name, mutate in [
    ("events-delete", lambda rows: rows.pop()),
    ("events-id", lambda rows: rows[0].update(id="E99")),
    ("events-panic-drain", lambda rows: rows[7].update(allowed_next_effect="drain_reconstructed_owner")),
    ("events-ownerless-available", lambda rows: rows[10].update(owner_availability="available")),
    ("events-post-latch-callback", lambda rows: rows[17].update(allowed_next_effect="process_callback")),
    ("events-grace-zero", lambda rows: rows[20].update(exit_code="0")),
]:
    rows = copy.deepcopy(event_rows)
    mutate(rows)
    item = base(name)
    item[5] = csv_text(rows, event_fields)
    cases.append(tuple(item))  # type: ignore[arg-type]

for name, path, value in [
    ("profile-timezone", ("semantic_config", "timezone_offset_hours"), -3),
    ("profile-pending-timeout", ("semantic_config", "pending_timeout_sec"), 30),
    ("profile-quantity", ("semantic_config", "qty"), "3.0"),
    ("profile-riskgate-file", ("semantic_config", "risk_gate_seed_file"), "/tmp/seed"),
    ("profile-live", ("paper_safety", "allow_live_orders"), True),
]:
    item = base(name)
    target = item[6]
    assert isinstance(target, dict)
    cursor = target
    for part in path[:-1]:
        cursor = cursor[part]  # type: ignore[assignment]
    cursor[path[-1]] = value
    cases.append(tuple(item))  # type: ignore[arg-type]

item = base("schema-riskgate-optional")
target = item[7]
assert isinstance(target, dict)
target["required"].remove("riskgate_history")  # type: ignore[union-attr]
cases.append(tuple(item))  # type: ignore[arg-type]

item = base("schema-riskgate-derived-fields")
target = item[7]
assert isinstance(target, dict)
target["$defs"]["riskgate_session_observation"]["additionalProperties"] = True  # type: ignore[index]
cases.append(tuple(item))  # type: ignore[arg-type]

item = base("plan-warmup-as-ledger-source")
target = item[8]
assert isinstance(target, dict)
target["riskgate"]["warmup_from_history_is_ledger_source"] = True  # type: ignore[index]
cases.append(tuple(item))  # type: ignore[arg-type]

item = base("plan-trust-riskgate-observations")
target = item[8]
assert isinstance(target, dict)
target["riskgate"]["cross_validation"] = "trust-bundle-without-oracle"  # type: ignore[index]
cases.append(tuple(item))  # type: ignore[arg-type]

for name, path, value in [
    ("schema-unknown-fields", ("additionalProperties",), True),
    ("schema-profile-binding", ("properties", "runtime_profile_sha256", "const"), "0" * 64),
    ("schema-nonflat", ("properties", "broker_truth", "properties", "target_position_qty", "const"), "1"),
]:
    item = base(name)
    target = item[7]
    assert isinstance(target, dict)
    cursor = target
    for part in path[:-1]:
        cursor = cursor[part]  # type: ignore[assignment]
    cursor[path[-1]] = value
    cases.append(tuple(item))  # type: ignore[arg-type]

for name, path, value in [
    ("plan-fixture", ("source_bundle", "test_fixture_feature_allowed"), True),
    ("plan-short-history", ("history", "minimum_complete_moscow_sessions"), 2),
    ("plan-external-ledger", ("riskgate", "external_ledger_allowed"), True),
    ("plan-root-first", ("effect_order",), ["create-durable-root-last", "validate"]),
]:
    item = base(name)
    target = item[8]
    assert isinstance(target, dict)
    cursor = target
    for part in path[:-1]:
        cursor = cursor[part]  # type: ignore[assignment]
    cursor[path[-1]] = value
    cases.append(tuple(item))  # type: ignore[arg-type]

for name, path, value in [
    ("redis-db0", ("redis_db_index",), 0),
    ("redis-alias", ("redis_url_allowlist",), ["redis://localhost:6379/15"]),
    ("redis-namespace", ("namespace_digest_sha256",), "0" * 64),
    ("redis-repair", ("run_may_create_or_repair",), True),
    ("redis-mkstream", ("telemetry_write_command",), "XADD key * payload value"),
]:
    item = base(name)
    target = item[9]
    assert isinstance(target, dict)
    cursor = target
    for part in path[:-1]:
        cursor = cursor[part]  # type: ignore[assignment]
    cursor[path[-1]] = value
    cases.append(tuple(item))  # type: ignore[arg-type]

for name, path, value in [
    ("telemetry-live-ready", ("readiness_phase_enum",), ["live_ready"]),
    ("telemetry-open-failure", ("failure_class_enum",), ["raw_error"]),
    ("telemetry-normalize-durable", ("two_run_volatile_allowlist",), ["operational_identity_sha256"]),
    ("telemetry-implicit-create", ("redis_write", "implicit_stream_creation"), True),
]:
    item = base(name)
    target = item[10]
    assert isinstance(target, dict)
    cursor = target
    for part in path[:-1]:
        cursor = cursor[part]  # type: ignore[assignment]
    cursor[path[-1]] = value
    cases.append(tuple(item))  # type: ignore[arg-type]

for name, path, value in [
    ("evidence-accepted", ("status",), "ACCEPTED"),
    ("evidence-open-source", ("implementation_authorized",), True),
    ("evidence-open-vps", ("closed_surfaces", "vps_activation"), True),
    ("evidence-count", ("negative_cases",), 59),
]:
    item = base(name)
    target = item[11]
    assert isinstance(target, dict)
    cursor = target
    for part in path[:-1]:
        cursor = cursor[part]  # type: ignore[assignment]
    cursor[path[-1]] = value
    cases.append(tuple(item))  # type: ignore[arg-type]

if len(cases) != 64:
    raise SystemExit(f"mutation inventory drifted: {len(cases)}")

escaped: list[str] = []
for case in cases:
    name, *args = case
    try:
        checker.validate(*args)
    except checker.CheckFailure:
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")

if escaped:
    raise SystemExit("mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1e-r1-design-negative-harness 64/64")
