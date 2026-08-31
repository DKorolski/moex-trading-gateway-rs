#!/usr/bin/env python3
"""Section-aware syntax policy for the exact Stage 8B-P R2B chain units."""

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
)
ALLOWED = {
    "Unit": {
        "Description", "Requires", "Wants", "After", "Before", "Conflicts",
        "ConditionPathExists", "AssertPathExists", "RefuseManualStart",
    },
    "Service": {
        "Type", "User", "Group", "SupplementaryGroups", "WorkingDirectory",
        "ExecStart", "NoNewPrivileges", "PrivateDevices", "PrivateTmp",
        "PrivateNetwork", "LoadCredential", "BindReadOnlyPaths", "InaccessiblePaths",
        "CapabilityBoundingSet", "AmbientCapabilities",
        "ProtectSystem", "ProtectHome", "ProtectProc", "ProcSubset",
        "RestrictAddressFamilies", "IPAddressDeny", "RestrictSUIDSGID",
        "LockPersonality", "MemoryDenyWriteExecute", "ReadOnlyPaths",
        "ReadWritePaths",
    },
}


def parse_unit(path: Path) -> dict[str, list[tuple[str, str]]]:
    sections: dict[str, list[tuple[str, str]]] = {}
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
        key, value = line.split("=", 1)
        if key not in ALLOWED[section]:
            raise RuntimeError(f"{path}:{number}: unknown key {key} in [{section}]")
        sections[section].append((key, value))
    return sections


def check(root: Path) -> None:
    for relative in UNITS:
        path = root / relative
        if not path.is_file():
            raise RuntimeError(f"missing exact unit: {relative}")
        sections = parse_unit(path)
        unit_keys = [key for key, _ in sections.get("Unit", [])]
        service_keys = [key for key, _ in sections.get("Service", [])]
        if unit_keys.count("RefuseManualStart") != 1:
            raise RuntimeError(f"{relative}: RefuseManualStart must occur once in [Unit]")
        if "RefuseManualStart" in service_keys:
            raise RuntimeError(f"{relative}: RefuseManualStart misplaced in [Service]")
        if "ConditionPathIsRegular" in path.read_text(encoding="utf-8"):
            raise RuntimeError(f"{relative}: unsupported ConditionPathIsRegular restored")


def systemd_verify(root: Path) -> None:
    executable = shutil.which("systemd-analyze")
    if executable is None:
        raise RuntimeError("systemd-analyze is unavailable")
    command = [executable, "verify", "--man=no", *(str(root / item) for item in UNITS)]
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    output = result.stdout
    if result.returncode != 0 or "Unknown key" in output or "Unknown lvalue" in output:
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
        "stage8b-p-r2b-systemd-unit-check: PASS "
        f"units={len(UNITS)} section_aware=true unknown_keys=false "
        f"systemd_analyze={str(args.systemd_analyze).lower()}"
    )


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError) as error:
        raise SystemExit(f"stage8b-p-r2b-systemd-unit-check: FAIL {error}") from error
