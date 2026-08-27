#!/usr/bin/env python3
"""Focused adversarial mutations for the R2A8 closure checker."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FILES = (
    "docs/stage-8/stage8b-p-r2a8-status.json",
    "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs",
    "tools/stage8b-readonly-preflight/src/r2a5.rs",
    "tools/stage8b-readonly-preflight/src/bin/stage8b-r2a5-authority-producer.rs",
    "crates/finam-gateway/src/bin/stage8b-r2a8-current-manifest-issuer.rs",
    "deploy/stage8b-r2a5/stage8b-r2a8-current-manifest-issuer.service",
    "deploy/stage8b-r2a5/stage8b-r2a7-source-adapter.service",
    "scripts/stage8b_p_r2a7_linux_rehearsal.sh",
    "scripts/stage8b_p_r2a8_review_closure_check.py",
)
MUTATIONS = (
    (1, "unsigned-source", ".sign_stage8b_r2a8_current_source_commitment(", ".skip_stage8b_r2a8_signature("),
    (1, "skip-source-validation", "validate_trusted_current_source(&source, mode)?", "skip_trusted_current_source(&source, mode)?"),
    (1, "remove-expiry", "source.expires_at <= now", "false"),
    (1, "normalize-key", "let line = bytes.strip_suffix(b\"\\n\").unwrap_or(bytes);", "let line = bytes.trim_ascii();"),
    (2, "drop-production-domain", "OperationalAdapterDomain::Production,", "OperationalAdapterDomain::ControlledQualification,"),
    (2, "drop-mode-check", "record.adapter_mode != OperationalAdapterMode::OneShotRecoveryReader", "false"),
    (3, "remove-controlled-place", 'Some("--controlled-r2a8-place")', 'Some("--removed-place")'),
    (5, "issuer-network", "RestrictAddressFamilies=AF_UNIX", "RestrictAddressFamilies=AF_UNIX AF_INET"),
    (5, "issuer-cannot-traverse-owner-root", "SupplementaryGroups=m8a8095", "SupplementaryGroups=m8m8096"),
    (6, "remove-issuer-ordering", "Requires=stage8b-r2a8-current-manifest-issuer.service", "Requires=stage8b-r2a7-source-adapter.service"),
    (7, "skip-full-chain", "stage8b-r2a8-full-chain-$operation: PASS", "stage8b-r2a8-short-chain-$operation: PASS"),
    (0, "open-r2b", '"r2b_authorization": "NOT_ISSUED"', '"r2b_authorization": "ISSUED"'),
)


def main() -> None:
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2a8-negative-") as temporary:
        base = Path(temporary) / "base"
        for relative in FILES:
            destination = base / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        for index, name, old, new in MUTATIONS:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            target = case / FILES[index]
            text = target.read_text()
            if text.count(old) < 1:
                raise SystemExit(f"stage8b-p-r2a8-negative: FAIL setup {name}")
            target.write_text(text.replace(old, new))
            result = subprocess.run(
                ["python3", str(case / "scripts/stage8b_p_r2a8_review_closure_check.py")],
                cwd=case,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r2a8-negative: FAIL accepted {name}")
            passed += 1
    print(f"stage8b-p-r2a8-negative: PASS {passed}/{len(MUTATIONS)}")


if __name__ == "__main__":
    main()
