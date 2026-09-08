#!/usr/bin/env python3
"""Targeted mutation harness for the Stage 8B-P1-e R0 design contract."""

from __future__ import annotations

import csv
import io
import json

import stage8b_p1e_design_check as checker


def replace_once(value: str, old: str, new: str) -> str:
    if old not in value:
        raise RuntimeError(f"mutation anchor missing: {old!r}")
    return value.replace(old, new, 1)


design = checker.DESIGN.read_text(encoding="utf-8")
matrix = checker.MATRIX.read_text(encoding="utf-8")
evidence = json.loads(checker.EVIDENCE.read_text(encoding="utf-8"))
status = checker.STATUS.read_text(encoding="utf-8")
roadmap = checker.ROADMAP.read_text(encoding="utf-8")


def mutate_evidence(path: tuple[str, ...], value: object) -> dict[str, object]:
    result = json.loads(json.dumps(evidence))
    cursor: dict[str, object] = result
    for name in path[:-1]:
        cursor = cursor[name]  # type: ignore[assignment]
    cursor[path[-1]] = value
    return result


Case = tuple[str, str, str, dict[str, object], str, str]
cases: list[Case] = []

for name, old, new in [
    ("open-source-before-review", "Status: R0 design-only review candidate", "Status: implementation authorized"),
    ("move-activation-to-p1e", "P1-f owns isolated operational acceptance", "P1-e owns operational acceptance"),
    ("broker-cli-coupling", "not a `broker-cli` subcommand", "is a `broker-cli` subcommand"),
    ("remove-validate-mode", "`validate-config CONFIG`", "`check CONFIG`"),
    ("remove-bootstrap-confirmation", "CREATE_NEW_STAGE8B_P1_DURABLE_ROOT", "CREATE"),
    ("allow-first-boot-run", "`run CONFIG` is restart-only", "`run CONFIG` may bootstrap"),
    ("allow-argv-secret", "No mode accepts secret bytes or a caller-selected credential filename/path", "Modes may accept a commitment key path"),
    ("mutate-p0", "remain byte-for-byte", "may be upgraded in place"),
    ("allow-db0", "loopback Redis URL with an explicit nonzero database", "Redis URL including DB0"),
    ("credential-filename", "LoadCredential=stage8b-p1-lifecycle.key", "LoadCredential=secret.key"),
    ("redis-before-recovery", "No Redis connection, stream creation, group creation, XADD, XACK or consumer", "Redis connection and XADD are allowed"),
    ("omit-last-startup-phase", "S09 publish PaperReady and enter the single-owner loop", "S09 omitted"),
    ("clone-owner", "One task owns the mutable P1 composition", "Tasks share the mutable P1 composition"),
    ("internal-restart", "Internal task restart is forbidden", "Internal tasks restart independently"),
    ("unbounded-telemetry", "exact `MAXLEN = 4096`", "unbounded retention"),
    ("allow-live-ready", "must never serialize `LiveReady`", "may serialize `LiveReady`"),
    ("ignore-second-signal", "A second signal does not bypass the\ndurable protocol", "A second signal exits immediately"),
    ("restart-always", "Restart=on-failure", "Restart=always"),
    ("activate-vps", "does not authorize installing or starting the resulting service on a VPS", "authorizes VPS start"),
]:
    cases.append((name, replace_once(design, old, new), matrix, evidence, status, roadmap))

rows = list(csv.DictReader(matrix.splitlines()))


def matrix_with(changed_rows: list[dict[str, str]]) -> str:
    output = io.StringIO()
    writer = csv.DictWriter(output, fieldnames=["id", "area", "requirement", "status"], lineterminator="\n")
    writer.writeheader()
    writer.writerows(changed_rows)
    return output.getvalue()


cases.append(("matrix-delete", design, matrix_with(rows[:-1]), evidence, status, roadmap))
cases.append(("matrix-duplicate", design, matrix_with(rows + [rows[-1]]), evidence, status, roadmap))
changed = json.loads(json.dumps(rows)); changed[0]["id"] = "P1E-999"
cases.append(("matrix-id", design, matrix_with(changed), evidence, status, roadmap))
changed = json.loads(json.dumps(rows)); changed[29]["requirement"] = "LiveReady is allowed"
cases.append(("matrix-live-ready", design, matrix_with(changed), evidence, status, roadmap))
changed = json.loads(json.dumps(rows)); changed[47]["status"] = "OPTIONAL"
cases.append(("matrix-optional", design, matrix_with(changed), evidence, status, roadmap))

for name, path, value in [
    ("evidence-status", ("status",), "ACCEPTED"),
    ("evidence-predecessor", ("accepted_p1d4_closure_ref",), "0" * 40),
    ("evidence-review", ("accepted_p1d4_review_sha256",), "0" * 64),
    ("evidence-count", ("acceptance_rows",), 47),
    ("evidence-open-source", ("implementation_authorized",), True),
    ("evidence-open-activation", ("operational_activation_authorized",), True),
    ("evidence-broker-cli", ("binary_contract", "broker_cli_subcommand"), True),
    ("evidence-credential-size", ("credential_contract", "expected_bytes"), 64),
    ("evidence-live-phase", ("readiness_phases",), ["LiveReady"]),
    ("evidence-open-d b0".replace(" ", ""), ("closed_surfaces", "operational_redis_db0"), True),
]:
    cases.append((name, design, matrix, mutate_evidence(path, value), status, roadmap))

cases.append(("status-drift", design, matrix, evidence, replace_once(status, "active next candidate is the\ndesign-only P1-e", "active P1-e implementation"), roadmap))
cases.append(("roadmap-drift", design, matrix, evidence, status, replace_once(roadmap, "Acceptance may open only P1-e source implementation", "Acceptance opens P1-f")))

if len(cases) != 36:
    raise SystemExit(f"mutation inventory drifted: {len(cases)}")

escaped: list[str] = []
for name, changed_design, changed_matrix, changed_evidence, changed_status, changed_roadmap in cases:
    try:
        checker.validate(changed_design, changed_matrix, changed_evidence, changed_status, changed_roadmap)
    except checker.CheckFailure:
        print(f"PASS {name}")
    else:
        escaped.append(name)
        print(f"FAIL {name}")

if escaped:
    raise SystemExit("mutations escaped: " + ", ".join(escaped))
print("PASS stage8b-p1e-design-negative-harness 36/36")
