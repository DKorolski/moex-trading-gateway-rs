#!/usr/bin/env python3
"""Generate M3c release-profile evidence without enabling order endpoints."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def run_text(command: list[str], cwd: Path) -> tuple[int, str, str]:
    completed = subprocess.run(command, cwd=cwd, text=True, capture_output=True)
    return completed.returncode, completed.stdout.strip(), completed.stderr.strip()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def package_version(root: Path, package_name: str) -> str | None:
    code, stdout, _stderr = run_text(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"], root
    )
    if code != 0:
        return None
    metadata = json.loads(stdout)
    for package in metadata.get("packages", []):
        if package.get("name") == package_name:
            return package.get("version")
    return None


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate source-bound M3c release-profile evidence."
    )
    parser.add_argument(
        "--source-archive",
        required=True,
        type=Path,
        help="Clean handoff archive to bind into the evidence report.",
    )
    parser.add_argument(
        "--output",
        default=Path("reports/m3c-order-endpoint-gate/release-profile-evidence.json"),
        type=Path,
        help="Release-profile evidence JSON output path.",
    )
    parser.add_argument(
        "--package",
        default="broker-cli",
        help="Cargo package to build in release profile.",
    )
    args = parser.parse_args()

    root = repo_root()
    source_archive = (root / args.source_archive).resolve()
    output = (root / args.output).resolve()

    if not source_archive.exists():
        print(f"source archive does not exist: {source_archive}", file=sys.stderr)
        return 2

    git_code, source_commit_full_sha, git_stderr = run_text(
        ["git", "rev-parse", "HEAD"], root
    )
    if git_code != 0:
        print(git_stderr, file=sys.stderr)
        return git_code

    scan_code, scan_stdout, scan_stderr = run_text(
        ["bash", "scripts/forbidden_surface_scan.sh"], root
    )
    scan_script = root / "scripts/forbidden_surface_scan.sh"

    cargo_command = ["cargo", "build", "--release", "-p", args.package]
    build_code, _build_stdout, build_stderr = run_text(cargo_command, root)

    binary_name = args.package.replace("-", "_")
    target_binary = root / "target" / "release" / args.package
    if not target_binary.exists():
        alt_binary = root / "target" / "release" / binary_name
        target_binary = alt_binary if alt_binary.exists() else target_binary

    cargo_version_code, cargo_version, _ = run_text(["cargo", "--version"], root)
    rustc_version_code, rustc_version, _ = run_text(["rustc", "--version"], root)

    evidence = {
        "m3c_step": "M3c-21",
        "release_profile_evidence": True,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": source_commit_full_sha,
        "source_archive_name": source_archive.name,
        "source_archive_sha256": sha256_file(source_archive),
        "build_profile": "release",
        "built_packages": [args.package],
        "package_versions": {args.package: package_version(root, args.package)},
        "cargo_command": cargo_command,
        "exit_code": build_code,
        "cargo_version": cargo_version if cargo_version_code == 0 else None,
        "rustc_version": rustc_version if rustc_version_code == 0 else None,
        "target_binary_present": target_binary.exists(),
        "target_binary_sha256": sha256_file(target_binary)
        if target_binary.exists() and build_code == 0
        else None,
        "forbidden_surface_scan": {
            "status": "Ok" if scan_code == 0 else "Failed",
            "exit_code": scan_code,
            "script_path": "scripts/forbidden_surface_scan.sh",
            "script_sha256": sha256_file(scan_script),
            "stdout": scan_stdout,
        },
        "endpoint_calls_allowed": False,
        "marker_constructible": False,
        "real_post_delete_added": False,
        "real_order_endpoint_enabled": False,
        "command_consumer_enabled": False,
        "runtime_live_attachment": False,
        "live_ready": False,
        "stop_sltp_bracket_enabled": False,
    }

    if build_code != 0:
        evidence["build_stderr_tail"] = build_stderr[-4000:]

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
    evidence_sha256 = sha256_file(output)
    output.with_suffix(output.suffix + ".sha256").write_text(
        f"{evidence_sha256}  {output.name}\n"
    )

    print(json.dumps({"output": str(output), "sha256": evidence_sha256}, indent=2))
    return build_code or scan_code


if __name__ == "__main__":
    raise SystemExit(main())
