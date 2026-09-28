#!/usr/bin/env python3
"""Collect and normalize the authorized read-only P1F-O0 target observation."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RAW = ROOT / "reports/stage8b/stage8b-p1f-o0-readonly-probe.txt"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1f-o0-target-preflight.json"
PROBE = ROOT / "scripts/stage8b_p1f_o0_readonly_probe.sh"
EMPTY_SHA256 = hashlib.sha256(b"").hexdigest()
P1_PATHS = (
    "/usr/local/libexec/moex/stage8b-p1-paper-supervisor",
    "/usr/local/share/moex/stage8b-p1e/installation-v1.json",
    "/etc/systemd/system/moex-finam-p1-paper.service",
    "/etc/systemd/system/moex-finam-p1-paper-bootstrap.service",
    "/etc/systemd/system/moex-finam-p1-paper-bootstrap-recover@.service",
    "/usr/lib/sysusers.d/moex-finam-p1-paper.conf",
    "/usr/lib/tmpfiles.d/moex-finam-p1-paper.conf",
    "/etc/moex-finam-p1-paper",
    "/etc/moex-finam-p1-paper/bootstrap",
    "/etc/moex-finam-p1-paper/credentials",
    "/etc/moex-finam-p1-paper/supervisor.json",
    "/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json",
    "/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key",
    "/var/lib/moex-finam-p1-paper",
    "/var/lib/moex-finam-p1-paper/state",
    "/var/lib/moex-finam-p1-paper/state/.stage8b-p1-first-boot-quarantine",
    "/var/lib/moex-finam-p1-paper-control",
)
P1_UNITS = (
    ("moex-finam-p1-paper.service", "regular"),
    ("moex-finam-p1-paper-bootstrap.service", "regular"),
    ("moex-finam-p1-paper-bootstrap-recover@.service", "template"),
)

EXPECTED_KEYS = {
    "schema_version", "probe_kind", "observed_at_utc", "target_id", "hostname",
    "ipv4", "ssh_ed25519_fingerprint", "os_id", "os_version_id", "architecture",
    "systemd_major", "cpu_count", "memory_kib", "root_free_kib", "ntp_synchronized",
    "redis_cli", "redis_cli_version", "redis_server_version", "redis_ping", "redis_bind", "redis_protected_mode",
    "redis_databases", "redis_appendonly", "redis_listeners", "redis_listener_sha256", "redis_db0_size",
    "redis_db0_keyspace_sha256", "redis_db15_size", "redis_db15_keyspace_sha256",
    "moex_finam_paper_runtime_load_state", "moex_finam_paper_runtime_active_state",
    "moex_finam_paper_runtime_sub_state", "moex_finam_paper_runtime_unit_file_state",
    "moex_finam_paper_runtime_fragment_sha256", "moex_finam_paper_runtime_execstart_sha256",
    "moex_finam_paper_ws_load_state", "moex_finam_paper_ws_active_state",
    "moex_finam_paper_ws_sub_state", "moex_finam_paper_ws_unit_file_state",
    "moex_finam_paper_ws_fragment_sha256", "moex_finam_paper_ws_execstart_sha256",
    "p1_service_user_present", "p1_service_group_present", "p1_systemd_query_ok",
    "p1_recovery_instances_count", "p1_recovery_instances",
    "p1_recovery_instances_sha256", "remote_mutation_performed",
} | {f"p1_path_{index}" for index in range(len(P1_PATHS))} | {
    f"p1_path_{index}_present" for index in range(len(P1_PATHS))
} | {
    f"p1_unit_{index}_{field}"
    for index in range(len(P1_UNITS))
    for field in ("name", "kind", "load_state", "active_state", "unit_file_state", "fragment_path")
}


def parse(raw: bytes) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in raw.decode("utf-8").splitlines():
        if not line or "=" not in line:
            raise ValueError(f"invalid probe line: {line!r}")
        key, value = line.split("=", 1)
        if not key or key in result:
            raise ValueError(f"duplicate/empty probe key: {key!r}")
        result[key] = value
    if set(result) != EXPECTED_KEYS:
        raise ValueError(f"probe key drift: {sorted(set(result) ^ EXPECTED_KEYS)}")
    return result


def boolean(value: str) -> bool:
    if value not in {"true", "false"}:
        raise ValueError(f"invalid boolean: {value!r}")
    return value == "true"


def build(values: dict[str, str], raw: bytes) -> dict[str, object]:
    p1_paths = [
        {
            "path": values[f"p1_path_{index}"],
            "present": boolean(values[f"p1_path_{index}_present"]),
        }
        for index in range(len(P1_PATHS))
    ]
    services = {}
    for name, prefix in (
        ("moex-finam-paper-runtime.service", "moex_finam_paper_runtime"),
        ("moex-finam-paper-ws.service", "moex_finam_paper_ws"),
    ):
        services[name] = {
            "load_state": values[f"{prefix}_load_state"],
            "active_state": values[f"{prefix}_active_state"],
            "sub_state": values[f"{prefix}_sub_state"],
            "unit_file_state": values[f"{prefix}_unit_file_state"],
            "fragment_sha256": values[f"{prefix}_fragment_sha256"],
            "execstart_sha256": values[f"{prefix}_execstart_sha256"],
        }
    p1_units = [
        {
            field: values[f"p1_unit_{index}_{field}"]
            for field in ("name", "kind", "load_state", "active_state", "unit_file_state", "fragment_path")
        }
        for index in range(len(P1_UNITS))
    ]
    p1_units_absent = all(
        unit["name"] == expected_name
        and unit["kind"] == expected_kind
        and unit["unit_file_state"] == "not-found"
        and unit["fragment_path"] == ""
        and (
            (expected_kind == "regular" and unit["load_state"] == "not-found" and unit["active_state"] == "inactive")
            or (expected_kind == "template" and unit["load_state"] == "not-applicable" and unit["active_state"] == "not-applicable")
        )
        for unit, (expected_name, expected_kind) in zip(p1_units, P1_UNITS)
    )
    checks = {
        "target_identity_exact": values["target_id"] == "stage8b-p1f-isolated-vps-1"
        and values["hostname"] == "nektodk1.ispvds.com"
        and values["ipv4"] == "45.150.11.252"
        and values["ssh_ed25519_fingerprint"]
        == "SHA256:8fOAPkfvZ61LYmldUBs6A5ZC+BFr6O3yCvUrBAWZsAo",
        "platform_prerequisites": values["os_id"] == "ubuntu"
        and values["os_version_id"].startswith("24.04")
        and values["architecture"] == "x86_64"
        and values["ntp_synchronized"] == "yes"
        and int(values["systemd_major"]) >= 255
        and int(values["cpu_count"]) >= 2
        and int(values["memory_kib"]) >= 3_670_016
        and int(values["root_free_kib"]) >= 20 * 1024 * 1024,
        "redis_prerequisites": values["redis_server_version"] == "7.0.15"
        and values["redis_ping"] == "PONG"
        and values["redis_bind"] == "127.0.0.1 -::1"
        and values["redis_protected_mode"] == "yes"
        and values["redis_databases"] == "16"
        and values["redis_appendonly"] == "yes"
        and values["redis_listeners"] == "127.0.0.1:6379,[::1]:6379",
        "db15_empty": int(values["redis_db15_size"]) == 0
        and values["redis_db15_keyspace_sha256"] == EMPTY_SHA256,
        "p0_services_unchanged_and_running": all(
            service["load_state"] == "loaded"
            and service["active_state"] == "active"
            and service["sub_state"] == "running"
            and service["unit_file_state"] == "enabled"
            and service["fragment_sha256"] != "ABSENT"
            for service in services.values()
        ),
        "p1_identity_absent": not boolean(values["p1_service_user_present"])
        and not boolean(values["p1_service_group_present"])
        and [item["path"] for item in p1_paths] == list(P1_PATHS)
        and all(not item["present"] for item in p1_paths)
        and boolean(values["p1_systemd_query_ok"])
        and p1_units_absent
        and int(values["p1_recovery_instances_count"]) == 0
        and values["p1_recovery_instances"] == ""
        and values["p1_recovery_instances_sha256"] == EMPTY_SHA256,
        "probe_declares_no_remote_mutation": not boolean(values["remote_mutation_performed"]),
    }
    return {
        "schema_version": 1,
        "stage": "Stage 8B-P1-f O0 immutable read-only target preflight",
        "status": "REVIEW_CANDIDATE_R2_SYSTEMD_QUERY_FAIL_CLOSED_NO_MUTATION",
        "source_baseline": "609f999ae1184c53de6320125a52b93bfdad9ace",
        "prior_o0_target": "e6b2f2dd2a35145dda4db0b2ae09e0581f56d989",
        "accepted_ie_closure": "3a46a460ea4bd5c85c5befd036510c580941a265",
        "hold_review": {
            "file": "FINAM_P1F_O0_REVIEW_e6b2f2d_2026-09-27.md",
            "sha256": "5b4e2f04c9427b67857ec564878185b08214a4d56aa2db88fe1160eb8480ce6b",
            "findings": ["P1-O001", "P1-O002"],
        },
        "r1_hold_review": {
            "file": "FINAM_P1F_O0_R1_REVIEW_609f999_2026-09-27.md",
            "sha256": "10467157b969c3665ff21cf4c77a4d777c178848f2515f96476eec00d371b519",
            "open_finding": "P1-O002-systemd-query-status",
        },
        "observed_at_utc": values["observed_at_utc"],
        "raw_probe_path": str(RAW.relative_to(ROOT)),
        "raw_probe_sha256": hashlib.sha256(raw).hexdigest(),
        "target": {
            "target_id": values["target_id"],
            "hostname": values["hostname"],
            "ipv4": values["ipv4"],
            "ssh_ed25519_fingerprint": values["ssh_ed25519_fingerprint"],
        },
        "platform": {
            "os_id": values["os_id"],
            "os_version_id": values["os_version_id"],
            "architecture": values["architecture"],
            "systemd_major": int(values["systemd_major"]),
            "cpu_count": int(values["cpu_count"]),
            "memory_kib": int(values["memory_kib"]),
            "root_free_kib": int(values["root_free_kib"]),
            "ntp_synchronized": values["ntp_synchronized"],
        },
        "redis": {
            "server_version": values["redis_server_version"],
            "cli_version": values["redis_cli_version"],
            "ping": values["redis_ping"],
            "bind": values["redis_bind"],
            "protected_mode": values["redis_protected_mode"],
            "databases": int(values["redis_databases"]),
            "appendonly": values["redis_appendonly"],
            "listeners": values["redis_listeners"],
            "listener_sha256": values["redis_listener_sha256"],
            "db0_size": int(values["redis_db0_size"]),
            "db0_keyspace_sha256": values["redis_db0_keyspace_sha256"],
            "db15_size": int(values["redis_db15_size"]),
            "db15_keyspace_sha256": values["redis_db15_keyspace_sha256"],
        },
        "p0_services": services,
        "p1_service_user_present": boolean(values["p1_service_user_present"]),
        "p1_service_group_present": boolean(values["p1_service_group_present"]),
        "p1_paths": p1_paths,
        "p1_units": p1_units,
        "p1_recovery_instances": {
            "count": int(values["p1_recovery_instances_count"]),
            "names": values["p1_recovery_instances"],
            "sha256": values["p1_recovery_instances_sha256"],
        },
        "p1_systemd_query_ok": boolean(values["p1_systemd_query_ok"]),
        "checks": checks,
        "all_required_checks_passed": all(checks.values()),
        "remote_mutation_performed": boolean(values["remote_mutation_performed"]),
        "next_after_acceptance": "P1F-O1 non-activating provisioning; still requires separate immutable authorization",
        "closed_surfaces": {
            "p1f_o1_provisioning": False,
            "installation_or_systemd_activation": False,
            "redis_db15_or_db0_mutation": False,
            "paper_provider_execution": False,
            "finam_post_or_delete": False,
            "broker_dispatch": False,
            "runtime_live": False,
            "real_orders": False,
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ssh-key", required=True)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    process = subprocess.run(
        ["bash", str(PROBE), args.ssh_key],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if process.returncode != 0:
        raise SystemExit(process.stderr.decode(errors="replace"))
    values = parse(process.stdout)
    evidence = build(values, process.stdout)
    if not evidence["all_required_checks_passed"]:
        raise SystemExit("stage8b-p1f-o0-collect: FAIL target preflight")
    if args.write:
        RAW.parent.mkdir(parents=True, exist_ok=True)
        RAW.write_bytes(process.stdout)
        EVIDENCE.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
    print(
        "PASS stage8b-p1f-o0-collect "
        f"target={values['target_id']} db0={values['redis_db0_size']} "
        f"db15={values['redis_db15_size']} mutation=false"
    )


if __name__ == "__main__":
    main()
