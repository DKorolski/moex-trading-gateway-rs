#!/usr/bin/env python3
"""Collect and normalize Stage 8B-P1-f O1 operational evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROBE = ROOT / "scripts/stage8b_p1f_o1_readonly_probe.sh"
WORK = ROOT / "reports/stage8b-p1f-o1-operational"
PRE_RAW_INPUT = WORK / "pre-o0-readonly-probe.txt"
PRE_EVIDENCE_INPUT = WORK / "pre-o0-evidence.json"
PRE_RAW = ROOT / "reports/stage8b/stage8b-p1f-o1-pre-o0-readonly-probe.txt"
POST_RAW = ROOT / "reports/stage8b/stage8b-p1f-o1-post-install-readonly-probe.txt"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1f-o1-operational-evidence.json"
EMPTY_SHA256 = hashlib.sha256(b"").hexdigest()
ACCEPTANCE_REVIEW = "FINAM_P1F_O1_PACKAGE_ACCEPT_8864a2b_2026-09-27.md"
ACCEPTANCE_REVIEW_SHA256 = "b3faa0eaceca62b2c7991791cdb348e0530359466b7aca29e9da4dafd8ae16e8"
PACKAGE_REF = "8864a2bbba64ef930073fae4e71dfcde82ceba58"
OUTER_SHA256 = "d8f9695bdb7a29b220dfe1396e31856fa7e71fb8a932dea126cd13e79c78e985"
BUNDLE_SHA256 = "f90fea1357a0f959d119027ef07becd4e1175995223037c83ed9e70c93db73c1"
BINARY_SHA256 = "cee324a4e4f251227a25d4a7b23dda332a94522b45407671982f4fc896614406"

MANAGED = (
    ("/etc/systemd/system/moex-finam-p1-paper-bootstrap-recover@.service", "022e71072b86487f34b63015cf70bb4a1d0fb1e31006dd783f2fa385668e2987", 0, 0, "644"),
    ("/etc/systemd/system/moex-finam-p1-paper-bootstrap.service", "f5f6db6e08d45f39fa3976701f78526a55e1485f3398f4140d13eaba1b2bc62b", 0, 0, "644"),
    ("/etc/systemd/system/moex-finam-p1-paper.service", "7e260e3d46f94dbea93d48aa39d1c16bf8fd6f14aa76cc9be2b197e7ab4f536f", 0, 0, "644"),
    ("/usr/lib/sysusers.d/moex-finam-p1-paper.conf", "efaa190119271332ed3aedc14b5fd45cdfc216c529e8eff8a7f0a956e6ee9d90", 0, 0, "644"),
    ("/usr/lib/tmpfiles.d/moex-finam-p1-paper.conf", "aa187d73dab4b526cf3bb100c43a6a971de6d1cafb8ecb7c326064623dd1f519", 0, 0, "644"),
    ("/usr/local/libexec/moex/stage8b-p1-paper-supervisor", BINARY_SHA256, 0, 0, "755"),
)
DIRECTORY_PATHS = (
    "/etc/moex-finam-p1-paper",
    "/etc/moex-finam-p1-paper/bootstrap",
    "/etc/moex-finam-p1-paper/credentials",
    "/var/lib/moex-finam-p1-paper",
    "/var/lib/moex-finam-p1-paper/state",
    "/var/lib/moex-finam-p1-paper/state/.stage8b-p1-first-boot-quarantine",
)
OPERATOR_PATHS = (
    "/etc/moex-finam-p1-paper/supervisor.json",
    "/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json",
    "/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key",
)
P1_UNITS = (
    "moex-finam-p1-paper.service",
    "moex-finam-p1-paper-bootstrap.service",
    "moex-finam-p1-paper-bootstrap-recover@.service",
)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def parse(raw: bytes) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in raw.decode("utf-8").splitlines():
        if not line or "=" not in line:
            raise ValueError(f"invalid probe line: {line!r}")
        key, value = line.split("=", 1)
        if not key or key in result:
            raise ValueError(f"duplicate/empty probe key: {key!r}")
        result[key] = value
    return result


def load_json(path: Path) -> dict[str, object]:
    return json.loads(path.read_text(), object_pairs_hook=reject_duplicates)


def reject_duplicates(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def build(values: dict[str, str], raw: bytes, pre: dict[str, object], pre_raw: bytes) -> dict[str, object]:
    status = json.loads(values["status_json"], object_pairs_hook=reject_duplicates)
    installation_manifest = json.loads(
        values["installation_manifest_json"], object_pairs_hook=reject_duplicates
    )
    managed = []
    for index, (path, expected_hash, expected_uid, expected_gid, expected_mode) in enumerate(MANAGED):
        managed.append({
            "path": values[f"managed_{index}_path"],
            "sha256": values[f"managed_{index}_sha256"],
            "uid": int(values[f"managed_{index}_uid"]),
            "gid": int(values[f"managed_{index}_gid"]),
            "mode": values[f"managed_{index}_mode"],
            "nlink": int(values[f"managed_{index}_nlink"]),
            "type": values[f"managed_{index}_type"],
            "expected": {
                "path": path,
                "sha256": expected_hash,
                "uid": expected_uid,
                "gid": expected_gid,
                "mode": expected_mode,
            },
        })
    manifest_item = {
        "path": values["managed_6_path"],
        "sha256": values["managed_6_sha256"],
        "uid": int(values["managed_6_uid"]),
        "gid": int(values["managed_6_gid"]),
        "mode": values["managed_6_mode"],
        "nlink": int(values["managed_6_nlink"]),
        "type": values["managed_6_type"],
    }
    directories = [
        {
            "path": values[f"directory_{index}_path"],
            "uid": int(values[f"directory_{index}_uid"]),
            "gid": int(values[f"directory_{index}_gid"]),
            "mode": values[f"directory_{index}_mode"],
            "type": values[f"directory_{index}_type"],
        }
        for index in range(len(DIRECTORY_PATHS))
    ]
    operator_files = [
        {
            "path": values[f"operator_{index}_path"],
            "present": values[f"operator_{index}_present"] == "true",
        }
        for index in range(len(OPERATOR_PATHS))
    ]
    p1_units = [
        {
            "name": values[f"p1_unit_{index}_name"],
            "active": values[f"p1_unit_{index}_active"],
            "enabled": values[f"p1_unit_{index}_enabled"],
        }
        for index in range(len(P1_UNITS))
    ]
    post_p0 = {}
    for name, prefix in (
        ("moex-finam-paper-runtime.service", "moex_finam_paper_runtime"),
        ("moex-finam-paper-ws.service", "moex_finam_paper_ws"),
    ):
        post_p0[name] = {
            "load_state": values[f"{prefix}_load_state"],
            "active_state": values[f"{prefix}_active_state"],
            "sub_state": values[f"{prefix}_sub_state"],
            "unit_file_state": values[f"{prefix}_unit_file_state"],
            "fragment_sha256": values[f"{prefix}_fragment_sha256"],
            "execstart_sha256": values[f"{prefix}_execstart_sha256"],
        }
    uid = int(values["service_uid"])
    gid = int(values["service_gid"])
    expected_directories = [
        (DIRECTORY_PATHS[0], 0, gid, "750"),
        (DIRECTORY_PATHS[1], 0, gid, "750"),
        (DIRECTORY_PATHS[2], 0, 0, "700"),
        (DIRECTORY_PATHS[3], 0, gid, "750"),
        (DIRECTORY_PATHS[4], uid, gid, "700"),
        (DIRECTORY_PATHS[5], uid, gid, "700"),
    ]
    checks = {
        "target_identity_exact": values["target_id"] == "stage8b-p1f-isolated-vps-1"
        and values["hostname"] == "nektodk1.ispvds.com"
        and values["ipv4"] == "45.150.11.252"
        and values["ssh_ed25519_fingerprint"] == "SHA256:8fOAPkfvZ61LYmldUBs6A5ZC+BFr6O3yCvUrBAWZsAo",
        "fresh_o0_complete": pre.get("all_required_checks_passed") is True
        and pre.get("remote_mutation_performed") is False
        and pre.get("raw_probe_sha256") == sha256(pre_raw),
        "staging_exact_and_root_only": values["staging_directory"] == "/root/stage8b-p1f-o1-8864a2b"
        and values["staging_directory_uid"] == "0"
        and values["staging_directory_gid"] == "0"
        and values["staging_directory_mode"] == "700"
        and values["staging_bundle_sha256"] == BUNDLE_SHA256,
        "status_exact_installed": status == {
            "activation_performed": False,
            "result": "EXACT_INSTALLED",
            "root": "/",
            "units": list(P1_UNITS),
        },
        "managed_payloads_exact": all(
            item["path"] == item["expected"]["path"]
            and item["sha256"] == item["expected"]["sha256"]
            and item["uid"] == item["expected"]["uid"]
            and item["gid"] == item["expected"]["gid"]
            and item["mode"] == item["expected"]["mode"]
            and item["nlink"] == 1
            and item["type"] == "regular file"
            for item in managed
        ),
        "installation_manifest_custody": manifest_item["path"] == "/usr/local/share/moex/stage8b-p1e/installation-v1.json"
        and manifest_item["sha256"] == values["installation_manifest_sha256"]
        and manifest_item["uid"] == 0
        and manifest_item["gid"] == 0
        and manifest_item["mode"] == "644"
        and manifest_item["nlink"] == 1
        and manifest_item["type"] == "regular file",
        "installation_manifest_false_boundaries": installation_manifest.get("domain")
        == "moex.stage8b.p1e.fixed-install.v1"
        and installation_manifest.get("binary_sha256") == BINARY_SHA256
        and installation_manifest.get("managed_payload_sha256")
        == {path: digest for path, digest, _uid, _gid, _mode in MANAGED}
        and installation_manifest.get("persistent_directories") == list(DIRECTORY_PATHS)
        and installation_manifest.get("operator_files_required_before_activation")
        == list(OPERATOR_PATHS)
        and all(
            installation_manifest.get(key) is False
            for key in (
                "activation_performed",
                "daemon_reload_performed",
                "redis_contact_performed",
                "finam_contact_performed",
                "operator_config_installed",
                "first_boot_source_installed",
                "lifecycle_credential_installed",
            )
        ),
        "service_identity_exact": values["service_user"] == "moex-p1-paper"
        and values["service_group"] == "moex-p1-paper"
        and uid > 0
        and gid > 0
        and int(values["service_primary_gid"]) == gid
        and values["service_shell"] == "/usr/sbin/nologin"
        and values["service_group_members"] == "",
        "persistent_directory_custody": all(
            item["path"] == path
            and item["uid"] == expected_uid
            and item["gid"] == expected_gid
            and item["mode"] == mode
            and item["type"] == "directory"
            for item, (path, expected_uid, expected_gid, mode) in zip(directories, expected_directories)
        ),
        "operator_material_absent": [item["path"] for item in operator_files] == list(OPERATOR_PATHS)
        and all(not item["present"] for item in operator_files),
        "durable_state_uninitialized": int(values["state_extra_entry_count"]) == 0
        and int(values["quarantine_entry_count"]) == 0,
        "p1_not_activated": int(values["p1_process_count"]) == 0
        and [item["name"] for item in p1_units] == list(P1_UNITS)
        and all(item["active"] != "active" and item["enabled"] != "enabled" for item in p1_units)
        and values["p1_recovery_instances"] == ""
        and values["p1_recovery_instances_sha256"] == EMPTY_SHA256,
        "p0_identity_and_configuration_unchanged": post_p0 == pre["p0_services"]
        and all(item["active_state"] == "active" and item["sub_state"] == "running" for item in post_p0.values()),
        "db15_remains_empty": int(values["redis_db15_size"]) == 0
        and values["redis_db15_keyspace_sha256"] == EMPTY_SHA256,
        "probe_declares_no_mutation": values["remote_probe_mutation_performed"] == "false",
    }
    return {
        "schema_version": 1,
        "stage": "Stage 8B-P1-f O1 non-activating provisioning operational evidence",
        "status": "O1_OPERATIONAL_EVIDENCE_REVIEW_CANDIDATE",
        "accepted_package": {
            "source_ref": PACKAGE_REF,
            "outer_sha256": OUTER_SHA256,
            "nested_bundle_sha256": BUNDLE_SHA256,
            "binary_sha256": BINARY_SHA256,
            "acceptance_review": ACCEPTANCE_REVIEW,
            "acceptance_review_sha256": ACCEPTANCE_REVIEW_SHA256,
        },
        "installation_execution": {
            "performed": True,
            "remote_mutation_performed": True,
            "installer_exit_code": 0,
            "installer_result": "INSTALLED_OR_ALREADY_EXACT",
            "command": "python3 $bundle_dir/scripts/stage8b_p1e_i1_fixed_install.py install --root / --binary $bundle_dir/payload/stage8b-p1-paper-supervisor",
            "post_installer_wrapper_exit_code": 1,
            "post_installer_wrapper_failure": "operator assertion expected an incorrect result label after the installer had exited zero; no second install was run",
            "verification": "independent status reread returned EXACT_INSTALLED and every installed byte/custody boundary was reread",
        },
        "pre_o0": {
            "observed_at_utc": pre["observed_at_utc"],
            "raw_probe_path": str(PRE_RAW.relative_to(ROOT)),
            "raw_probe_sha256": sha256(pre_raw),
            "db0_size": pre["redis"]["db0_size"],
            "db0_keyspace_sha256": pre["redis"]["db0_keyspace_sha256"],
            "db15_size": pre["redis"]["db15_size"],
            "db15_keyspace_sha256": pre["redis"]["db15_keyspace_sha256"],
            "all_required_checks_passed": pre["all_required_checks_passed"],
        },
        "post_install": {
            "observed_at_utc": values["observed_at_utc"],
            "raw_probe_path": str(POST_RAW.relative_to(ROOT)),
            "raw_probe_sha256": sha256(raw),
            "status": status,
            "managed_payloads": managed,
            "installation_manifest": manifest_item,
            "installation_manifest_content": installation_manifest,
            "service_identity": {
                "user": values["service_user"],
                "group": values["service_group"],
                "uid": uid,
                "gid": gid,
                "primary_gid": int(values["service_primary_gid"]),
                "home": values["service_home"],
                "shell": values["service_shell"],
                "group_members": values["service_group_members"],
            },
            "persistent_directories": directories,
            "operator_files": operator_files,
            "state_extra_entry_count": int(values["state_extra_entry_count"]),
            "quarantine_entry_count": int(values["quarantine_entry_count"]),
            "p1_process_count": int(values["p1_process_count"]),
            "p1_units": p1_units,
            "p1_recovery_instances": values["p1_recovery_instances"],
            "p0_services": post_p0,
            "redis": {
                "db0_size": int(values["redis_db0_size"]),
                "db0_keyspace_sha256": values["redis_db0_keyspace_sha256"],
                "db15_size": int(values["redis_db15_size"]),
                "db15_keyspace_sha256": values["redis_db15_keyspace_sha256"],
            },
        },
        "db0_policy": "P0 is active; digest equality is not required. O1 installer manifest declares redis_contact_performed=false.",
        "checks": checks,
        "all_required_checks_passed": all(checks.values()),
        "closed_surfaces": {
            "daemon_reload": False,
            "service_enable_or_start": False,
            "o2_bootstrap": False,
            "redis_mutation_by_o1": False,
            "paper_provider_execution": False,
            "finam_post_or_delete": False,
            "broker_dispatch": False,
            "runtime_live": False,
            "real_orders": False,
        },
        "next_after_acceptance": "separate P1F-O2 fresh materialization and network-isolated one-shot bootstrap package",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ssh-key", required=True)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    pre_raw = PRE_RAW_INPUT.read_bytes()
    pre = load_json(PRE_EVIDENCE_INPUT)
    process = subprocess.run(
        ["bash", str(PROBE), args.ssh_key],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if process.returncode != 0:
        raise SystemExit(process.stderr.decode(errors="replace"))
    evidence = build(parse(process.stdout), process.stdout, pre, pre_raw)
    if not evidence["all_required_checks_passed"]:
        failed = [name for name, passed in evidence["checks"].items() if not passed]
        raise SystemExit(f"stage8b-p1f-o1-collect: FAIL {failed}")
    if args.write:
        PRE_RAW.parent.mkdir(parents=True, exist_ok=True)
        PRE_RAW.write_bytes(pre_raw)
        POST_RAW.write_bytes(process.stdout)
        EVIDENCE.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
    print(
        "PASS stage8b-p1f-o1-collect "
        f"target=stage8b-p1f-isolated-vps-1 status={evidence['post_install']['status']['result']} "
        f"db15={evidence['post_install']['redis']['db15_size']} activation=false"
    )


if __name__ == "__main__":
    main()
