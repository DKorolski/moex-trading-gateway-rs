#!/usr/bin/env python3
"""Mutation harness for the Stage 8B-P1-e installation material gate."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage8b_p1e_i1_fixed_install_check.py"
FILES = (
    "deploy/stage8b-p1e/moex-finam-p1-paper.service",
    "deploy/stage8b-p1e/moex-finam-p1-paper-bootstrap.service",
    "deploy/stage8b-p1e/moex-finam-p1-paper-bootstrap-recover@.service",
    "deploy/stage8b-p1e/moex-finam-p1-paper.sysusers",
    "deploy/stage8b-p1e/moex-finam-p1-paper.tmpfiles",
    "docs/stage-8/stage8b-p1e-deployment-identity-v2.json",
    "scripts/stage8b_p1e_i1_fixed_install.py",
    "docs/stage-8/stage8b-p1e-i1-fixed-path-installation.md",
    "docs/stage-8/stage8b-p1e-i1-fixed-path-installation-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1e-i1-telemetry-composition.md",
)
MUTATIONS = (
    ("main-mode", FILES[0], " run /etc/moex", " bootstrap /etc/moex"),
    ("main-private-network", FILES[0], "PrivateNetwork=no", "PrivateNetwork=yes"),
    ("main-address-family", FILES[0], "AF_UNIX AF_INET AF_INET6", "AF_UNIX AF_INET"),
    ("main-loopback", FILES[0], "IPAddressAllow=::1/128", "IPAddressAllow=10.0.0.0/8"),
    ("main-start-limit", FILES[0], "StartLimitIntervalSec=600", "StartLimitIntervalSec=300"),
    ("main-restart", FILES[0], "Restart=on-failure", "Restart=always"),
    ("main-stop-timeout", FILES[0], "TimeoutStopSec=100s", "TimeoutStopSec=30s"),
    ("shared-user", FILES[0], "User=moex-p1-paper", "User=root"),
    ("shared-credential", FILES[1], "credentials/stage8b-p1-lifecycle.key", "credentials/alternate.key"),
    ("shared-write-path", FILES[2], "ReadWritePaths=/var/lib/moex-finam-p1-paper/state", "ReadWritePaths=/var/lib/moex-finam-p1-paper"),
    ("main-no-new-privileges", FILES[0], "NoNewPrivileges=yes\n", ""),
    ("bootstrap-network", FILES[1], "RestrictAddressFamilies=AF_UNIX", "RestrictAddressFamilies=AF_UNIX AF_INET"),
    ("bootstrap-private-network", FILES[1], "PrivateNetwork=yes", "PrivateNetwork=no"),
    ("bootstrap-confirmation", FILES[1], "CREATE_NEW_STAGE8B_P1_DURABLE_ROOT", "CREATE_STAGE8B_ROOT"),
    ("bootstrap-restart", FILES[1], "RemainAfterExit=no", "RemainAfterExit=no\nRestart=always"),
    ("recovery-instance", FILES[2], " %i RECOVER_EXISTING", " %I RECOVER_EXISTING"),
    ("recovery-confirmation", FILES[2], "FIRST_BOOT_V4", "FIRST_BOOT_V3"),
    ("install-section", FILES[0], "\n[Service]\n", "\n[Install]\nWantedBy=multi-user.target\n\n[Service]\n"),
    ("sysusers-login", FILES[3], "/usr/sbin/nologin", "/bin/bash"),
    ("tmpfiles-state-mode", FILES[4], "state 0700", "state 0750"),
    ("tmpfiles-credential-owner", FILES[4], "credentials 0700 root root", "credentials 0750 root moex-p1-paper"),
    ("installer-start", FILES[6], 'choices=("install", "rollback", "status")', '["systemctl", "start"]\n# choices=("install", "rollback", "status")'),
    ("installer-redis", FILES[6], "import shutil", "import shutil\nimport redis"),
    ("installer-operator-guard", FILES[6], "operator material exists; rollback refused", "operator material ignored"),
    ("installer-durable-guard", FILES[6], "durable state exists; rollback refused", "durable state ignored"),
)
EVIDENCE_MUTATIONS = (
    ("evidence-network", "target-linux-evidence.json", '"network_mode":"none"', '"network_mode":"host"'),
    ("evidence-unit-start", "target-linux-evidence.json", '"unit_start_attempts":0', '"unit_start_attempts":1'),
    ("evidence-redis-contact", "target-linux-evidence.json", '"redis_contacts":0', '"redis_contacts":1'),
    ("evidence-source-ref", "target-linux-evidence.json", "b6f6d5b6ea924db8c97512bc2bcecb8a5ed760ac", "06f6d5b6ea924db8c97512bc2bcecb8a5ed760ac"),
    ("evidence-binary-kind", "target-linux-evidence.json", '"installed_binary_kind":"accepted-release"', '"installed_binary_kind":"fixture"'),
    ("evidence-systemd", "target-linux-evidence.json", '"systemd_version":255', '"systemd_version":254'),
    ("evidence-result", "target-linux-evidence.json", '"clean_install":"PASS"', '"clean_install":"FAIL"'),
    ("evidence-target-gate", "static-and-systemd-check.txt", "target_linux=true", "target_linux=false"),
    ("evidence-rollback", "rollback-durable-state.stderr", "durable state exists; rollback refused", "durable state ignored"),
    ("evidence-build-hash", "accepted-binary-build-result.json", '"binary_sha256":"' + "2" * 64 + '"', '"binary_sha256":"' + "3" * 64 + '"'),
)


def run_checker(root: Path, evidence: Path | None = None) -> subprocess.CompletedProcess[str]:
    command = [sys.executable, str(CHECKER), "--root", str(root)]
    if evidence is not None:
        command.extend(("--evidence-dir", str(evidence)))
    return subprocess.run(
        command,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )


def copy_baseline(destination: Path) -> None:
    for relative in FILES:
        source = ROOT / relative
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def write_evidence_fixture(directory: Path) -> None:
    directory.mkdir(parents=True)
    target = {
        "schema_version": 1,
        "domain": "moex.stage8b.p1e.fixed-install.target-linux-evidence.v1",
        "target": "ubuntu-24.04",
        "systemd_version": 255,
        "network_mode": "none",
        "clean_install": "PASS",
        "systemd_analyze_verify": "PASS",
        "unknown_key_or_lvalue_warnings": 0,
        "idempotent_reinstall": "PASS",
        "installation_manifest_sha256": "1" * 64,
        "installed_binary_sha256": "2" * 64,
        "installed_binary_source_ref": "b6f6d5b6ea924db8c97512bc2bcecb8a5ed760ac",
        "installed_binary_kind": "accepted-release",
        "operator_material_rollback_refusal": "PASS",
        "durable_state_rollback_refusal": "PASS",
        "clean_public_package_rollback": "PASS",
        "unit_start_attempts": 0,
        "daemon_reload_attempts": 0,
        "redis_contacts": 0,
        "finam_contacts": 0,
        "runtime_live": False,
        "real_orders": False,
        "persistent_state_directories_retained": True,
    }
    (directory / "target-linux-evidence.json").write_text(
        json.dumps(target, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    fixtures = {
        "install-first.json": {"result": "INSTALLED_OR_ALREADY_EXACT"},
        "install-idempotent.json": {"result": "INSTALLED_OR_ALREADY_EXACT"},
        "status-installed.json": {"result": "EXACT_INSTALLED"},
        "rollback.json": {"result": "ROLLED_BACK_PUBLIC_PACKAGE"},
        "status-rolled-back.json": {"result": "ABSENT_OR_DRIFTED"},
    }
    for name, value in fixtures.items():
        (directory / name).write_text(json.dumps(value) + "\n", encoding="utf-8")
    (directory / "static-and-systemd-check.txt").write_text("PASS target_linux=true\n", encoding="utf-8")
    (directory / "rollback-operator-material.stderr").write_text("operator material exists; rollback refused\n", encoding="utf-8")
    (directory / "rollback-durable-state.stderr").write_text("durable state exists; rollback refused\n", encoding="utf-8")
    build = {
        "binary_sha256": "2" * 64,
        "build_exit_code": 0,
        "locked": True,
        "profile": "release",
        "source_ref": "b6f6d5b6ea924db8c97512bc2bcecb8a5ed760ac",
        "source_tree": "5e29d320d9083a877f43a3148fff86766bd0f99e",
    }
    (directory / "accepted-binary-build-result.json").write_text(
        json.dumps(build, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    (directory / "accepted-binary.sha256").write_text("2" * 64 + "  stage8b-p1-paper-supervisor\n", encoding="utf-8")
    (directory / "accepted-binary-source.txt").write_text(
        "b6f6d5b6ea924db8c97512bc2bcecb8a5ed760ac\n"
        "5e29d320d9083a877f43a3148fff86766bd0f99e\n",
        encoding="utf-8",
    )
    (directory / "accepted-binary-build.log").write_text(
        "Finished `release` profile [optimized] target(s) in 1s\n",
        encoding="utf-8",
    )
    (directory / "source-archive-check.txt").write_text(
        "/tmp/moex-trading-project-b6f6d5b.tar.gz: OK\n",
        encoding="utf-8",
    )


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="stage8b-p1e-install-negative-") as raw:
        base = Path(raw)
        baseline = base / "baseline"
        copy_baseline(baseline)
        result = run_checker(baseline)
        if result.returncode != 0:
            raise SystemExit(f"baseline rejected:\n{result.stdout}")
        passed = 0
        for index, (name, relative, old, new) in enumerate(MUTATIONS):
            candidate = base / f"case-{index:02d}"
            shutil.copytree(baseline, candidate)
            path = candidate / relative
            text = path.read_text(encoding="utf-8")
            if text.count(old) != 1:
                raise SystemExit(f"mutation source is not unique: {name}")
            path.write_text(text.replace(old, new, 1), encoding="utf-8")
            result = run_checker(candidate)
            if result.returncode == 0:
                raise SystemExit(f"mutation escaped checker: {name}\n{result.stdout}")
            passed += 1
            print(f"PASS {name}")
        evidence_baseline = base / "evidence-baseline"
        write_evidence_fixture(evidence_baseline)
        result = run_checker(baseline, evidence_baseline)
        if result.returncode != 0:
            raise SystemExit(f"evidence baseline rejected:\n{result.stdout}")
        for index, (name, relative, old, new) in enumerate(EVIDENCE_MUTATIONS):
            candidate = base / f"evidence-{index:02d}"
            shutil.copytree(evidence_baseline, candidate)
            path = candidate / relative
            text = path.read_text(encoding="utf-8")
            if text.count(old) != 1:
                raise SystemExit(f"evidence mutation source is not unique: {name}")
            path.write_text(text.replace(old, new, 1), encoding="utf-8")
            result = run_checker(baseline, candidate)
            if result.returncode == 0:
                raise SystemExit(f"evidence mutation escaped checker: {name}\n{result.stdout}")
            passed += 1
            print(f"PASS {name}")
    total = len(MUTATIONS) + len(EVIDENCE_MUTATIONS)
    print(f"stage8b-p1e-i1-fixed-install-negative-harness: PASS {passed}/{total}")


if __name__ == "__main__":
    main()
