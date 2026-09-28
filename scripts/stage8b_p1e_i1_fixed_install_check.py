#!/usr/bin/env python3
"""Validate the Stage 8B-P1-e fixed-path, non-activating install package."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEPLOY = Path("deploy/stage8b-p1e")
IDENTITY = Path("docs/stage-8/stage8b-p1e-deployment-identity-v2.json")
INSTALLER = Path("scripts/stage8b_p1e_i1_fixed_install.py")
BEHAVIORAL = Path("scripts/stage8b_p1e_i1_fixed_install_behavioral_harness.py")
RUNNER = Path("scripts/stage8b_p1e_i1_fixed_install_linux_runner.sh")
DESIGN = Path("docs/stage-8/stage8b-p1e-i1-fixed-path-installation.md")
MATRIX = Path("docs/stage-8/stage8b-p1e-i1-fixed-path-installation-acceptance-matrix.csv")
MAIN = "moex-finam-p1-paper.service"
BOOTSTRAP = "moex-finam-p1-paper-bootstrap.service"
RECOVERY = "moex-finam-p1-paper-bootstrap-recover@.service"
UNITS = (MAIN, BOOTSTRAP, RECOVERY)


class CheckError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckError(message)


def parse_unit(path: Path) -> dict[str, list[tuple[str, str]]]:
    sections: dict[str, list[tuple[str, str]]] = {}
    section = ""
    for number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith(("#", ";")):
            continue
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1]
            require(section in {"Unit", "Service"}, f"{path}:{number}: forbidden section")
            sections.setdefault(section, [])
            continue
        require(section != "" and "=" in line, f"{path}:{number}: malformed assignment")
        key, value = line.split("=", 1)
        sections[section].append((key, value))
    require(set(sections) == {"Unit", "Service"}, f"{path}: section set drift")
    return sections


def values(unit: dict[str, list[tuple[str, str]]], section: str, key: str) -> list[str]:
    return [value for candidate, value in unit[section] if candidate == key]


def exact(unit: dict[str, list[tuple[str, str]]], section: str, key: str, value: str) -> None:
    require(values(unit, section, key) == [value], f"{section}.{key} drift")


def check_shared(unit: dict[str, list[tuple[str, str]]]) -> None:
    shared = {
        "User": "moex-p1-paper",
        "Group": "moex-p1-paper",
        "UMask": "0077",
        "LoadCredential": "stage8b-p1-lifecycle.key:/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key",
        "NoNewPrivileges": "yes",
        "PrivateDevices": "yes",
        "PrivateTmp": "yes",
        "ProtectClock": "yes",
        "ProtectControlGroups": "yes",
        "ProtectHome": "yes",
        "ProtectHostname": "yes",
        "ProtectKernelLogs": "yes",
        "ProtectKernelModules": "yes",
        "ProtectKernelTunables": "yes",
        "ProtectSystem": "strict",
        "ProtectProc": "invisible",
        "ProcSubset": "pid",
        "RestrictRealtime": "yes",
        "RestrictSUIDSGID": "yes",
        "LockPersonality": "yes",
        "MemoryDenyWriteExecute": "yes",
        "CapabilityBoundingSet": "",
        "AmbientCapabilities": "",
        "SystemCallArchitectures": "native",
        "LimitCORE": "0",
        "ReadOnlyPaths": "/etc/moex-finam-p1-paper",
        "ReadWritePaths": "/var/lib/moex-finam-p1-paper/state",
    }
    for key, value in shared.items():
        exact(unit, "Service", key, value)


def check(root: Path) -> None:
    identity = json.loads((root / IDENTITY).read_text(encoding="utf-8"))
    require(identity["schema_version"] == 2, "deployment identity version drift")
    paths = identity["paths"]
    require(paths["main_unit"] == MAIN, "main unit identity drift")
    require(paths["bootstrap_unit"] == BOOTSTRAP, "bootstrap unit identity drift")
    require(paths["bootstrap_recovery_template_unit"] == RECOVERY, "recovery unit identity drift")
    require(paths["binary"] == "/usr/local/libexec/moex/stage8b-p1-paper-supervisor", "binary path drift")
    require(paths["config"] == "/etc/moex-finam-p1-paper/supervisor.json", "config path drift")
    require(paths["credential_source"] == "/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key", "credential path drift")

    parsed = {name: parse_unit(root / DEPLOY / name) for name in UNITS}
    for unit in parsed.values():
        check_shared(unit)

    main = parsed[MAIN]
    exact(main, "Unit", "StartLimitIntervalSec", "600")
    exact(main, "Unit", "StartLimitBurst", "5")
    exact(main, "Service", "Type", "simple")
    exact(main, "Service", "ExecStart", identity["units"]["main"]["exec_start"])
    exact(main, "Service", "Restart", "on-failure")
    exact(main, "Service", "RestartSec", "5s")
    exact(main, "Service", "KillSignal", "SIGTERM")
    exact(main, "Service", "KillMode", "control-group")
    exact(main, "Service", "TimeoutStopSec", "100s")
    exact(main, "Service", "FinalKillSignal", "SIGKILL")
    exact(main, "Service", "SendSIGKILL", "yes")
    exact(main, "Service", "RestrictAddressFamilies", "AF_UNIX AF_INET AF_INET6")
    exact(main, "Service", "PrivateNetwork", "no")
    exact(main, "Service", "IPAddressDeny", "any")
    require(values(main, "Service", "IPAddressAllow") == ["127.0.0.1/32", "::1/128"], "main loopback allowlist drift")

    for name, identity_key, mode, confirmation in (
        (BOOTSTRAP, "bootstrap", "bootstrap", "CREATE_NEW_STAGE8B_P1_DURABLE_ROOT"),
        (RECOVERY, "bootstrap_recover", "bootstrap-recover", "RECOVER_EXISTING_STAGE8B_P1_FIRST_BOOT_V4"),
    ):
        unit = parsed[name]
        exact(unit, "Service", "Type", "oneshot")
        exact(unit, "Service", "ExecStart", identity["units"][identity_key]["exec_start"])
        exact(unit, "Service", "RestrictAddressFamilies", "AF_UNIX")
        exact(unit, "Service", "PrivateNetwork", "yes")
        exact(unit, "Service", "RemainAfterExit", "no")
        require(mode in values(unit, "Service", "ExecStart")[0], f"{name}: mode drift")
        require(confirmation in values(unit, "Service", "ExecStart")[0], f"{name}: confirmation drift")
        require(not values(unit, "Service", "Restart"), f"{name}: oneshot restart forbidden")
        require(not values(unit, "Service", "IPAddressAllow"), f"{name}: network allow forbidden")

    sysusers = (root / DEPLOY / "moex-finam-p1-paper.sysusers").read_text(encoding="utf-8")
    require(sysusers == 'u moex-p1-paper - "MOEX FINAM Stage 8B P1 paper supervisor" - /usr/sbin/nologin\n', "sysusers drift")
    tmpfiles = (root / DEPLOY / "moex-finam-p1-paper.tmpfiles").read_text(encoding="utf-8").splitlines()
    required_tmpfiles = {
        "d /etc/moex-finam-p1-paper 0750 root moex-p1-paper -",
        "d /etc/moex-finam-p1-paper/bootstrap 0750 root moex-p1-paper -",
        "d /etc/moex-finam-p1-paper/credentials 0700 root root -",
        "d /var/lib/moex-finam-p1-paper 0750 root moex-p1-paper -",
        "d /var/lib/moex-finam-p1-paper/state 0700 moex-p1-paper moex-p1-paper -",
        "d /var/lib/moex-finam-p1-paper/state/.stage8b-p1-first-boot-quarantine 0700 moex-p1-paper moex-p1-paper -",
    }
    require(set(tmpfiles) == required_tmpfiles and len(tmpfiles) == len(required_tmpfiles), "tmpfiles drift")

    installer = (root / INSTALLER).read_text(encoding="utf-8")
    for forbidden in (
        '"daemon-reload"',
        '["systemctl", "start"',
        '["systemctl", "enable"',
        "import redis",
        "import requests",
        "urllib.request",
    ):
        require(forbidden not in installer, f"installer opens forbidden operation: {forbidden}")
    require('choices=("install", "rollback", "status")' in installer, "installer action set drift")
    require('"activation_performed": False' in installer, "non-activation evidence absent")
    require("operator material exists; rollback refused" in installer, "operator-material rollback guard absent")
    require("durable state exists; rollback refused" in installer, "durable rollback guard absent")
    for fixed_path in (
        '"/usr/local/libexec/moex/stage8b-p1-paper-supervisor"',
        '"/etc/moex-finam-p1-paper/supervisor.json"',
        '"/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json"',
        '"/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key"',
        '"/var/lib/moex-finam-p1-paper/state"',
    ):
        require(fixed_path in installer, f"installer fixed path absent: {fixed_path}")
    require("verify_persistent_directories(root)" in installer, "directory custody verification absent")
    require("metadata.st_nlink != 1" in installer, "managed-file hardlink guard absent")
    for token in (
        "set(expected) != set(MANAGED_FILE_MODES)",
        "set(value) != MANIFEST_KEYS",
        "verify_secure_directory_chain(root, path.parent)",
        "protected directory custody drift",
        "service identity must be non-root with its exact primary group",
        "uid <= 0 or gid <= 0 or primary_gid != gid",
        "durable quarantine history exists; rollback refused",
        "for relative in sorted(MANAGED_FILE_MODES, reverse=True)",
        "args.binary)",
        'if hasattr(os, "O_NOFOLLOW"):',
    ):
        require(token in installer, f"installer hardening invariant absent: {token}")
    behavioral = (root / BEHAVIORAL).read_text(encoding="utf-8")
    for token in (
        "manifest-extra-path",
        "manifest-missing-path",
        "binary-parent-writable",
        "managed-file-owner-drift",
        "managed-ancestor-symlink",
        "source-binary-hardlink",
        "conflicting-root-service-identity",
        "quarantine-nonempty",
        "quarantine-symlink",
        "require_unchanged",
        "def expect_rollback_refusal(label: str) -> None:\n    before = snapshot()",
    ):
        require(token in behavioral, f"behavioral filesystem case absent: {token}")
    require(behavioral.count("require_unchanged(before") == 5, "behavioral no-mutation assertions drift")
    runner = (root / RUNNER).read_text(encoding="utf-8")
    for token in (
        "git archive --format=tar.gz",
        "accepted-binary-build-result.json",
        "accepted-binary-source.txt",
        "source-archive-check.txt",
        "linux-runner.log",
        'stat -c %h "$accepted_binary_path"',
        'sha256sum "$accepted_binary_path"',
        "--evidence-dir \"$evidence_dir\"",
    ):
        require(token in runner, f"self-contained runner invariant absent: {token}")

    design = (root / DESIGN).read_text(encoding="utf-8")
    require("blocked-inventory component tests" in (root / "docs/stage-8/stage8b-p1e-i1-telemetry-composition.md").read_text(encoding="utf-8"), "telemetry evidence wording drift")
    require("never runs\n`daemon-reload`, `enable`, `start`" in design, "non-activation design boundary drift")
    require("does not accept, create or overwrite" in design, "operator-material boundary drift")
    require("Aggregate I1 and operational\nactivation are not authorized" in design, "aggregate closure wording drift")
    matrix = (root / MATRIX).read_text(encoding="utf-8").splitlines()
    require(len(matrix) == 35 and matrix[0] == "id,area,requirement,status", "acceptance matrix shape drift")
    require(all(row.endswith(",REQUIRED") for row in matrix[1:]), "acceptance matrix status drift")


def check_evidence(directory: Path) -> None:
    def load(name: str) -> dict[str, object]:
        return json.loads((directory / name).read_text(encoding="utf-8"))

    evidence = load("target-linux-evidence.json")
    require(evidence["domain"] == "moex.stage8b.p1e.fixed-install.target-linux-evidence.v1", "target evidence domain drift")
    require(evidence["target"] == "ubuntu-24.04", "target OS drift")
    require(evidence["systemd_version"] == 255, "target systemd drift")
    require(evidence["network_mode"] == "none", "target network isolation drift")
    for key in (
        "clean_install",
        "systemd_analyze_verify",
        "idempotent_reinstall",
        "operator_material_rollback_refusal",
        "durable_state_rollback_refusal",
        "nonempty_quarantine_rollback_refusal",
        "empty_quarantine_positive_control",
        "behavioral_filesystem_matrix",
        "clean_public_package_rollback",
    ):
        require(evidence[key] == "PASS", f"target evidence failed: {key}")
    for key in (
        "unknown_key_or_lvalue_warnings",
        "unit_start_attempts",
        "daemon_reload_attempts",
        "redis_contacts",
        "finam_contacts",
    ):
        require(evidence[key] == 0, f"target zero invariant drift: {key}")
    for key in ("runtime_live", "real_orders"):
        require(evidence[key] is False, f"target closed surface drift: {key}")
    require(evidence["persistent_state_directories_retained"] is True, "persistent directory evidence drift")
    require(len(str(evidence["installation_manifest_sha256"])) == 64, "manifest evidence hash drift")
    require(len(str(evidence["installed_binary_sha256"])) == 64, "installed binary evidence hash drift")
    require(evidence["installed_binary_source_ref"] == "b6f6d5b6ea924db8c97512bc2bcecb8a5ed760ac", "installed binary source drift")
    require(evidence["installed_binary_kind"] == "accepted-release", "installed binary kind drift")
    require(evidence["behavioral_filesystem_case_count"] == 14, "behavioral case count drift")
    build = load("accepted-binary-build-result.json")
    require(build["build_exit_code"] == 0 and build["locked"] is True, "accepted binary build result drift")
    require(build["profile"] == "release", "accepted binary build profile drift")
    require(build["source_ref"] == evidence["installed_binary_source_ref"], "build/install source ref mismatch")
    require(build["source_tree"] == "5e29d320d9083a877f43a3148fff86766bd0f99e", "accepted source tree drift")
    require(build["binary_sha256"] == evidence["installed_binary_sha256"], "build/install binary hash mismatch")
    require(build["package"] == "runtime-durable-service", "build package drift")
    require(build["binary"] == "stage8b-p1-paper-supervisor", "build binary drift")
    require(build["rust_image"] == "rust@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922", "build image drift")
    require(len(build["source_archive_sha256"]) == 64, "source archive hash drift")
    require((directory / "accepted-binary.sha256").read_text(encoding="utf-8").split()[0] == build["binary_sha256"], "binary sha256 sidecar drift")
    require((directory / "accepted-binary-source.txt").read_text(encoding="utf-8").splitlines() == [build["source_ref"], build["source_tree"]], "accepted source sidecar drift")
    require("Finished `release` profile [optimized]" in (directory / "accepted-binary-build.log").read_text(encoding="utf-8"), "accepted binary build log incomplete")
    archive_fields = dict(
        line.split("=", 1)
        for line in (directory / "source-archive-check.txt").read_text(encoding="utf-8").splitlines()
    )
    require(archive_fields == {
        "source_ref": build["source_ref"],
        "source_tree": build["source_tree"],
        "archive_sha256": build["source_archive_sha256"],
        "verification": "PASS",
    }, "accepted source archive evidence drift")
    behavioral = load("behavioral-filesystem-matrix.json")
    require(behavioral["domain"] == "moex.stage8b.p1e.fixed-install.behavioral-matrix.v1", "behavioral domain drift")
    require(behavioral["all_passed"] is True and behavioral["case_count"] == 14, "behavioral matrix result drift")
    require(len(behavioral["cases"]) == 14 and all(case["result"] == "PASS" for case in behavioral["cases"]), "behavioral matrix cases drift")
    require("PASS 14/14" in (directory / "behavioral-filesystem-matrix.log").read_text(encoding="utf-8"), "behavioral matrix log drift")
    invocation = (directory / "linux-runner-invocation.txt").read_text(encoding="utf-8")
    require("network_mode=none" in invocation, "runner invocation network drift")
    runner_log = (directory / "linux-runner.log").read_text(encoding="utf-8")
    for token in (
        "Finished `release` profile [optimized]",
        "stage8b-p1e-i1-fixed-install-behavioral-harness: PASS 14/14",
        "stage8b-p1e-i1-fixed-install-linux-rehearsal: PASS",
    ):
        require(token in runner_log, f"runner log incomplete: {token}")

    require(load("install-first.json")["result"] == "INSTALLED_OR_ALREADY_EXACT", "clean install evidence drift")
    require(load("install-idempotent.json")["result"] == "INSTALLED_OR_ALREADY_EXACT", "idempotent install evidence drift")
    require(load("status-installed.json")["result"] == "EXACT_INSTALLED", "installed status evidence drift")
    require(load("rollback.json")["result"] == "ROLLED_BACK_PUBLIC_PACKAGE", "rollback evidence drift")
    require(load("status-rolled-back.json")["result"] == "ABSENT_OR_DRIFTED", "rolled-back status evidence drift")
    require("target_linux=true" in (directory / "static-and-systemd-check.txt").read_text(encoding="utf-8"), "target static gate evidence drift")
    require("operator material exists; rollback refused" in (directory / "rollback-operator-material.stderr").read_text(encoding="utf-8"), "operator refusal evidence drift")
    require("durable state exists; rollback refused" in (directory / "rollback-durable-state.stderr").read_text(encoding="utf-8"), "durable refusal evidence drift")


def target_verify(root: Path) -> None:
    command = [
        "systemd-analyze",
        "verify",
        "--man=no",
        f"--root={root}",
        *UNITS,
    ]
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    require(result.returncode == 0, f"target systemd verify failed:\n{result.stdout}")
    require("Unknown key" not in result.stdout and "Unknown lvalue" not in result.stdout, "target systemd parser warning")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--target-root", type=Path)
    parser.add_argument("--evidence-dir", type=Path)
    args = parser.parse_args()
    check(args.root)
    if args.target_root is not None:
        target_verify(args.target_root)
    if args.evidence_dir is not None:
        check_evidence(args.evidence_dir)
    print(
        "stage8b-p1e-i1-fixed-install-check: PASS "
        f"units={len(UNITS)} fixed_paths=true non_activating=true "
        f"target_linux={str(args.target_root is not None).lower()} "
        f"evidence={str(args.evidence_dir is not None).lower()}"
    )


if __name__ == "__main__":
    try:
        main()
    except (CheckError, KeyError, OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p1e-i1-fixed-install-check: FAIL {error}") from error
