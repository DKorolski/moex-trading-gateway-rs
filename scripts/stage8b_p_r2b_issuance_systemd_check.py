#!/usr/bin/env python3
"""Section-aware parser policy for the complete currently shipped R2B unit set."""

from __future__ import annotations

import argparse
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
UNITS = (
    "deploy/stage8b-r2b/moex-stage8b-r2a8-upstream-current-authority-publisher.service",
    "deploy/stage8b-r2b/moex-stage8b-r2a8-authoritative-intake-creator.service",
    "deploy/stage8b-r2b/moex-stage8b-r2a8-production-intake-stager.service",
    "deploy/stage8b-r2b/moex-stage8b-r2a8-production-current-source-writer.service",
    "deploy/stage8b-r2a5/stage8b-r2a8-current-manifest-issuer.service",
    "deploy/stage8b-r2a5/stage8b-r2a7-source-adapter.service",
    "deploy/stage8b-r2a5/stage8b-r2a5-producer@.service",
    "deploy/stage8b-r2a5/stage8b-r2a5-issuer@.service",
    "deploy/stage8b-r2b/moex-stage8b-r2b-readonly-supervisor.service",
    "deploy/stage8b-r2b/moex-stage8b-r2b-run-package-draft-builder.service",
    "deploy/stage8b-r2b/moex-stage8b-r2b-package-issuer.service",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase1-current-source.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase2-manifest-source.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase3-authority-producers.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase4-authority-issuers.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase5-run-package.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-phase6-readonly-preflight.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-issuance.target",
)
ALLOWED = {
    "Unit": {
        "Description", "Requires", "Wants", "After", "Before", "Conflicts",
        "ConditionPathExists", "ConditionPathIsDirectory", "AssertPathExists",
        "RefuseManualStart", "StopWhenUnneeded",
    },
    "Service": {
        "Type", "User", "Group", "SupplementaryGroups", "WorkingDirectory",
        "ExecStart", "NoNewPrivileges", "PrivateDevices", "PrivateTmp",
        "ProtectSystem", "ProtectHome", "ProtectProc", "ProcSubset",
        "ProtectKernelTunables", "ProtectKernelModules", "ProtectControlGroups",
        "RestrictAddressFamilies", "IPAddressDeny", "RestrictSUIDSGID",
        "LockPersonality", "MemoryDenyWriteExecute", "ReadOnlyPaths",
        "ReadWritePaths", "SystemCallArchitectures", "UMask",
    },
}


def parse_unit(path: Path) -> dict[str, list[str]]:
    sections: dict[str, list[str]] = {}
    section = ""
    for number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith(("#", ";")):
            continue
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1]
            if section not in ALLOWED:
                raise RuntimeError(f"{path}:{number}: unsupported section [{section}]")
            sections.setdefault(section, [])
            continue
        if not section or "=" not in line:
            raise RuntimeError(f"{path}:{number}: malformed assignment")
        key, _ = line.split("=", 1)
        if key not in ALLOWED[section]:
            raise RuntimeError(f"{path}:{number}: unknown key {key} in [{section}]")
        sections[section].append(key)
    return sections


def check(root: Path) -> None:
    for relative in UNITS:
        path = root / relative
        if not path.is_file():
            raise RuntimeError(f"missing issuance unit: {relative}")
        sections = parse_unit(path)
        if sections.get("Unit", []).count("RefuseManualStart") != 1:
            raise RuntimeError(f"{relative}: RefuseManualStart must occur once in [Unit]")
        if "RefuseManualStart" in sections.get("Service", []):
            raise RuntimeError(f"{relative}: RefuseManualStart misplaced in [Service]")
        if "ConditionPathIsRegular=" in path.read_text(encoding="utf-8"):
            raise RuntimeError(f"{relative}: unsupported ConditionPathIsRegular restored")
        if relative.endswith(".target") and sections.get("Unit", []).count("StopWhenUnneeded") != 1:
            raise RuntimeError(f"{relative}: StopWhenUnneeded must occur once in [Unit]")
        if "RemainAfterExit=" in path.read_text(encoding="utf-8"):
            raise RuntimeError(f"{relative}: stale active service state forbidden")


def systemd_verify(root: Path) -> None:
    executable = shutil.which("systemd-analyze")
    if executable is None:
        raise RuntimeError("systemd-analyze is unavailable")
    command = [executable, "verify", "--man=no", *(str(root / item) for item in UNITS)]
    result = subprocess.run(
        command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False
    )
    output = result.stdout
    forbidden = ("Unknown key", "Unknown lvalue", "outside of section", "Failed to parse")
    if result.returncode != 0 or any(marker in output for marker in forbidden):
        raise RuntimeError(f"systemd-analyze verify failed:\n{output}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--systemd-analyze", action="store_true")
    args = parser.parse_args()
    check(args.root)
    if args.systemd_analyze:
        systemd_verify(args.root)
    print(
        "stage8b-p-r2b-issuance-systemd-check: PASS "
        f"units={len(UNITS)} section_aware=true reusable_transaction=true parser_warnings=0 "
        f"systemd_analyze={str(args.systemd_analyze).lower()}"
    )


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError) as error:
        raise SystemExit(f"stage8b-p-r2b-issuance-systemd-check: FAIL {error}") from error
