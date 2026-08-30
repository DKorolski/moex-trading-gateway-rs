#!/usr/bin/env python3
"""Fail-closed checker for Stage 8B-P R2B Implementation R0-R1."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PREDECESSOR = "da83f5922d9e2a9a5a1db3e581d2d9f55d810d81"
AUTHORITY = Path("docs/stage-8/stage8b-p-r2b-implementation-r0-r1-authority.json")
BUILD = Path("docs/stage-8/stage8b-p-r2b-implementation-r0-r1-linux-build-evidence.json")
REHEARSAL = Path("docs/stage-8/stage8b-p-r2b-implementation-r0-r1-linux-rehearsal-evidence.json")
MATRIX = Path("docs/stage-8/STAGE8B_P_R2B_IMPLEMENTATION_R0_R1_ACCEPTANCE_MATRIX_2026-08-30.csv")
CORE = Path("tools/stage8b-readonly-preflight/src/r2a5.rs")
BUILDER = Path("deploy/stage8b-r2b/moex-stage8b-r2b-run-package-draft-builder.service")
SIGNER = Path("deploy/stage8b-r2b/moex-stage8b-r2b-package-issuer.service")
SUPERVISOR = Path("deploy/stage8b-r2b/moex-stage8b-r2b-readonly-supervisor.service")
ARTIFACT_ROOT = Path("reports/stage8b-p-r2b-r0-r1/linux-amd64")


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def values(path: Path, key: str) -> list[str]:
    result = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line.startswith(f"{key}="):
            result.append(line.split("=", 1)[1])
    return result


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(("git", *args), cwd=root, text=True).strip()


def check(root: Path) -> None:
    required = (AUTHORITY, BUILD, REHEARSAL, MATRIX, CORE, BUILDER, SIGNER, SUPERVISOR)
    for relative in required:
        require((root / relative).is_file(), f"missing artifact: {relative}")

    authority = json.loads((root / AUTHORITY).read_text(encoding="utf-8"))
    build = json.loads((root / BUILD).read_text(encoding="utf-8"))
    rehearsal = json.loads((root / REHEARSAL).read_text(encoding="utf-8"))
    require(authority.get("schema_version") == 1, "authority schema drift")
    require(authority.get("stage") == "Stage 8B-P R2B Implementation Package R0-R1", "stage drift")
    require(authority.get("accepted_predecessor") == PREDECESSOR, "predecessor drift")
    require(authority.get("status") == "IMPLEMENTED_NOT_INSTALLED_NOT_ISSUED_REVIEW_REQUIRED", "status drift")
    require(authority.get("scope") == "CREDENTIAL_ISOLATION_AND_LINUX_ARTIFACT_CLOSURE", "scope drift")

    for relative, digest in authority["implementation_artifacts"].items():
        path = root / relative
        require(path.is_file(), f"missing frozen implementation artifact: {relative}")
        require(re.fullmatch(r"[0-9a-f]{64}", digest) is not None, f"bad digest: {relative}")
        require(sha256(path) == digest, f"implementation artifact drift: {relative}")

    builder = root / BUILDER
    require(values(builder, "PrivateNetwork") == ["yes"], "builder network namespace opened")
    require(values(builder, "InaccessiblePaths") == ["/run/credentials/moex-trading/stage8b/r2a5"], "builder credential root visible")
    require(values(builder, "CapabilityBoundingSet") == [""], "builder capability bounding set opened")
    require(values(builder, "AmbientCapabilities") == [""], "builder ambient capability opened")
    require(values(builder, "ReadOnlyPaths") == ["/etc/moex-trading/stage8b/r2a5 /run/moex-trading/stage8b/r2a5 /var/lib/moex-trading/stage8b/r2a5"], "builder read roots drift")
    require(values(builder, "ReadWritePaths") == ["/var/lib/moex-trading/stage8b/r2a5/draft-output"], "builder write scope widened")

    signer = root / SIGNER
    require(values(signer, "PrivateNetwork") == ["yes"], "signer network namespace opened")
    require(values(signer, "LoadCredential") == [], "non-portable signer LoadCredential restored")
    require(values(signer, "BindReadOnlyPaths") == ["/run/credentials/moex-trading/stage8b/r2a5/package-authorization.ed25519:/run/moex-stage8b-r2b-package-issuer/package-authorization.ed25519"], "signer credential projection drift")
    require(values(signer, "InaccessiblePaths") == ["/run/credentials/moex-trading/stage8b/r2a5"], "signer source credentials visible")
    require(values(signer, "CapabilityBoundingSet") == [""], "signer capability bounding set opened")
    require(values(signer, "AmbientCapabilities") == [""], "signer ambient capability opened")
    require(values(signer, "ReadOnlyPaths") == ["/etc/moex-trading/stage8b/r2a5 /run/moex-trading/stage8b/r2a5 /var/lib/moex-trading/stage8b/r2a5"], "signer read roots drift")
    require(values(signer, "ReadWritePaths") == ["/var/lib/moex-trading/stage8b/r2a5/signed-output"], "signer write scope widened")

    supervisor = root / SUPERVISOR
    supervisor_projection = (
        "/run/credentials/moex-trading/stage8b/r2a5/account-id:/run/moex-stage8b-r2b-supervisor/account-id "
        "/run/credentials/moex-trading/stage8b/r2a5/finam-readonly-secret:/run/moex-stage8b-r2b-supervisor/finam-readonly-secret "
        "/run/credentials/moex-trading/stage8b/r2a5/account-binding-keys:/run/moex-stage8b-r2b-supervisor/account-binding-keys"
    )
    require(values(supervisor, "BindReadOnlyPaths") == [supervisor_projection], "supervisor broker projection drift")
    require(values(supervisor, "InaccessiblePaths") == ["/run/credentials/moex-trading/stage8b/r2a5"], "supervisor source credential root visible")
    require(values(supervisor, "CapabilityBoundingSet") == ["CAP_SETUID CAP_SETGID CAP_KILL CAP_DAC_OVERRIDE CAP_FOWNER"], "supervisor capability set drift")
    require(values(supervisor, "AmbientCapabilities") == [""], "supervisor ambient capability opened")

    core = (root / CORE).read_text(encoding="utf-8")
    for marker in (
        'pub const PRODUCTION_DRAFT_ROOT: &str =',
        'pub const PRODUCTION_SIGNED_PACKAGE_ROOT: &str =',
        'pub const PRODUCTION_PACKAGE_SIGNER_CREDENTIALS: &str =',
        'pub const PRODUCTION_SUPERVISOR_CREDENTIALS: &str =',
        'draft_root.join("r2b-run-package.unsigned.json")',
        'signed_package_root.join("r2b-run-package.json")',
        "package_signer_credentials_root()",
        "supervisor_credentials_root()",
    ):
        require(marker in core, f"fixed path marker missing: {marker}")
    require('etc_root.join("r2b-run-package.json")' not in core, "legacy writable config package path restored")
    require("#[cfg(feature = \"stage8b-r2b-controlled-custody\")]" in core, "controlled fallback not compile-time isolated")

    require(build.get("result") == "PASS", "Linux build evidence failed")
    require(build.get("target") == "x86_64-unknown-linux-musl", "Linux target drift")
    require(build.get("clean_target_directories") == 2, "clean build count drift")
    require(build.get("controlled_custody_feature") is False, "controlled feature entered production build")
    require(build.get("default_features") is False, "default features entered production build")
    require(build.get("all_hashes_identical") is True, "Linux build mismatch")
    require(set(build["binaries"]) == {"stage8b-r2b-run-package-draft-builder", "stage8b-r2a5-package-issuer"}, "Linux binary inventory drift")
    for binary, record in build["binaries"].items():
        require(record["reproducible"] is True, f"non-reproducible ELF: {binary}")
        require(record["build_a_sha256"] == record["build_b_sha256"], f"A/B mismatch: {binary}")
        require("ELF 64-bit LSB" in record["file_identity"] and "x86-64" in record["file_identity"], f"ELF identity drift: {binary}")
        for build_name in ("build-a", "build-b"):
            artifact = root / ARTIFACT_ROOT / build_name / binary
            require(artifact.is_file(), f"packaged Linux ELF missing: {build_name}/{binary}")
            require(sha256(artifact) == record[f"{build_name.replace('-', '_')}_sha256"], f"packaged Linux ELF drift: {build_name}/{binary}")

    expected_true = (
        "systemd_pid1", "actual_read_attempts", "credential_canaries_real",
        "builder_effective_capabilities_empty", "builder_write_scope_exact",
        "signer_projected_package_key_readable", "signer_effective_capabilities_empty",
        "signer_write_scope_exact", "supervisor_broker_subset_readable",
        "controlled_builder_executed", "controlled_signer_executed",
        "phase1_failure_blocks_phase2", "producer_failure_blocks_issuers",
        "issuer_failure_blocks_builder", "builder_failure_blocks_signer",
        "signer_failure_blocks_supervisor", "second_transaction_old_output_blocked",
    )
    for key in expected_true:
        require(rehearsal.get(key) is True, f"rehearsal proof missing: {key}")
    for key in (
        "external_network_available", "builder_credential_root_visible",
        "builder_external_network", "signer_source_credential_root_visible",
        "signer_external_network", "supervisor_package_key_readable",
        "supervisor_issuer_keys_readable", "finam_endpoint_called",
        "real_credentials_used", "services_installed_to_production",
    ):
        require(rehearsal.get(key) is False, f"rehearsal closed surface opened: {key}")
    require(rehearsal.get("graph_service_invocations") == 31, "dynamic graph arithmetic drift")
    require(rehearsal.get("native_execution") is True, "dynamic rehearsal was not native")
    require(rehearsal.get("qemu_emulation") is False, "dynamic rehearsal used QEMU")
    require(rehearsal.get("authorization") == "NOT_ISSUED" and rehearsal.get("result") == "PASS", "rehearsal verdict drift")

    rows = list(csv.DictReader((root / MATRIX).read_text(encoding="utf-8").splitlines()))
    require(len(rows) == 30 and len({row["id"] for row in rows}) == 30, "acceptance matrix inventory drift")
    require(all(row["status"] == "pass" for row in rows), "acceptance matrix not green")
    require(authority["authorization"] == "NOT_ISSUED", "R2B authorization opened")
    require(authority["repository_state"] == {
        "installed": False, "enabled": False, "started": False,
        "operator_selected": False, "real_credentials_materialized": False,
    }, "repository state opened")
    require(all(value is False for value in authority["closed_surfaces"].values()), "effect surface opened")

    if (root / ".git").exists():
        require(git(root, "merge-base", "HEAD", PREDECESSOR) == PREDECESSOR, "predecessor lineage drift")
        changed = set(git(root, "diff", "--name-only", PREDECESSOR).splitlines())
        forbidden_prefixes = ("crates/", "config/", "configs/")
        require(not any(path.startswith(forbidden_prefixes) for path in changed), "broker/runtime production source changed")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    check(args.root)
    print("stage8b-p-r2b-implementation-r0-r1-check: PASS credentials=isolated write_scopes=dedicated linux_elf=2x2 graph=31 dynamic_failures=5 replay=blocked authorization=NOT_ISSUED finam=false")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, KeyError, ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"stage8b-p-r2b-implementation-r0-r1-check: FAIL {error}") from error
