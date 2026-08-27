#!/usr/bin/env python3
"""Adversarial mutation matrix for the R2A6 integration checker."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
FILES = (
    "docs/stage-8/stage8b-p-r2a6-status.json",
    "docs/stage-8/stage8b-p-r2a6-build-evidence.json",
    "crates/finam-gateway/Cargo.toml",
    "crates/finam-gateway/src/bin/stage8b-r2a6-source-adapter.rs",
    "crates/finam-gateway/src/stage8a1_execution_capability.rs",
    "crates/runtime-durable-service/src/recovery.rs",
    "tools/stage8b-readonly-preflight/src/r2a5.rs",
    "tools/stage8b-readonly-preflight/src/r2a2.rs",
    "tools/stage8b-readonly-preflight/src/r2a3.rs",
    "tools/stage8b-readonly-preflight/src/bin/stage8b-r2a5-controlled-layout.rs",
    "deploy/stage8b-r2a5/stage8b-r2a5.sysusers",
    "deploy/stage8b-r2a5/stage8b-r2a6.tmpfiles",
    "deploy/stage8b-r2a5/stage8b-r2a6-source-adapter@.service",
    "scripts/stage8b_p_r2a6_linux_rehearsal.sh",
    "scripts/stage8b_p_r2a6_review_closure_check.py",
)
MUTATIONS = (
    ("status", "open-real-credential", '"real_finam_credentials": false', '"real_finam_credentials": true'),
    ("status", "replace-effect", "677f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06", "0" * 64),
    ("adapter", "remove-owner-composition", "pub fn publish_stage8b_r2a6_operational_sources_from_owner(", "pub fn publish_stage8b_r2a6_operational_sources_from_dto("),
    ("adapter", "remove-euid", "unsafe { libc::geteuid() } != STAGE8B_R2A6_SOURCE_ADAPTER_UID", "false"),
    ("adapter", "remove-output-ownership", "validate_stage8b_r2a6_output_ownership()?;", "let _ownership_skipped = true;"),
    ("binary", "add-network", "use finam_gateway::", "use reqwest as _;\nuse finam_gateway::"),
    ("binary", "remove-real-caller", "run_stage8b_r2a6_controlled_source_adapter(&value)", "panic!(\"synthetic records\")"),
    ("runtime", "remove-cancel-source", "pub fn stage8b_r2a6_cancel_production_test_setup_in(", "pub fn removed_stage8b_r2a6_cancel_production_test_setup_in("),
    ("producer", "accept-root-writer", "R2A6_SOURCE_ADAPTER_UID,\n            false,", "0,\n            false,"),
    ("producer", "remove-run-identity-rebind", "r2a2::recompute_manifest_run_identity(&fields)?", '"0".repeat(64)'),
    ("service", "enable-network", "RestrictAddressFamilies=AF_UNIX", "RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6"),
    ("service", "run-root", "User=m8a8095", "User=root"),
    ("tmpfiles", "group-writable-output", "0755 m8a8095 m8a8095", "0775 m8a8095 m8a8095"),
    ("rehearsal", "producer-before-adapter", '"$ADAPTER_BIN" --controlled-rehearsal', '"$PRODUCER" "$source"\n  "$ADAPTER_BIN" --controlled-rehearsal'),
    ("rehearsal", "remove-manifest-bind", '"$LAYOUT" bind-r2a6', 'true # removed manifest bind'),
    ("rehearsal", "replace-accepted-helper", 'HELPER="$ACCEPTED_R2A5_BIN_DIR/stage8b-readonly-preflight"', 'HELPER="$TOOL_BIN_DIR/stage8b-readonly-preflight"'),
    ("rehearsal", "remove-empty-root-proof", 'test -z "$(find /var/lib/moex-trading/operational-authorities', 'true # removed-empty-root-proof "$(find /var/lib/moex-trading/operational-authorities'),
    ("build", "nonreproducible", '"reproducible": true', '"reproducible": false'),
    ("build", "adapter-digest-drift", '"build_b_sha256":', '"build_b_sha256": "0", "discarded_build_b_sha256":'),
)
TARGET = {
    "status": FILES[0], "build": FILES[1], "binary": FILES[3], "adapter": FILES[4],
    "runtime": FILES[5], "producer": FILES[6], "tmpfiles": FILES[11],
    "service": FILES[12], "rehearsal": FILES[13],
}


def main() -> None:
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2a6-negative-") as temporary:
        base = Path(temporary) / "base"
        for relative in FILES:
            destination = base / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        checker = base / "scripts/stage8b_p_r2a6_review_closure_check.py"
        for label, name, old, new in MUTATIONS:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            target = case / TARGET[label]
            text = target.read_text()
            if text.count(old) != 1:
                raise SystemExit(f"stage8b-p-r2a6-negative: FAIL setup {name} count={text.count(old)}")
            target.write_text(text.replace(old, new, 1))
            result = subprocess.run(
                ["python3", str(case / checker.relative_to(base))],
                cwd=case,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r2a6-negative: FAIL accepted {name}")
            passed += 1
    print(f"stage8b-p-r2a6-negative: PASS {passed}/{len(MUTATIONS)}")


if __name__ == "__main__":
    main()
