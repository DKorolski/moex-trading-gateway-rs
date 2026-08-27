#!/usr/bin/env python3
"""Adversarial mutations for the R2A7 production-reader checker."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FILES = (
    "docs/stage-8/stage8b-p-r2a7-status.json",
    "docs/stage-8/stage8b-p-r2a7-build-evidence.json",
    "crates/finam-gateway/Cargo.toml",
    "crates/finam-gateway/src/bin/stage8b-r2a7-source-adapter.rs",
    "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs",
    "crates/finam-gateway/src/stage8a1_execution_capability.rs",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    "deploy/stage8b-r2a5/stage8b-r2a7-source-adapter.service",
    "scripts/stage8b_p_r2a7_linux_rehearsal.sh",
    "scripts/stage8b_p_r2a7_review_closure_check.py",
)
TARGETS = {name: path for name, path in zip(("status", "build", "cargo", "binary", "adapter", "composition", "runtime", "service", "rehearsal", "checker"), FILES)}
MUTATIONS = (
    ("status", "open-network", '"network": false', '"network": true'),
    ("status", "controlled-as-production", '"production_domain_accepted": false', '"production_domain_accepted": true'),
    ("cargo", "fixture-production-feature", 'stage8b-r2a7-source-adapter = []', 'stage8b-r2a7-source-adapter = ["strategy-runtime-core/stage5g-artifact-fixtures"]'),
    ("cargo", "fixture-required-feature", 'required-features = ["stage8b-r2a7-source-adapter"]', 'required-features = ["stage8b-r2a7-controlled-qualification"]'),
    ("binary", "missing-production-caller", "run_stage8b_r2a7_source_adapter(mode)", 'panic!("missing production caller")'),
    ("binary", "add-network", "use finam_gateway::", "use reqwest as _;\nuse finam_gateway::"),
    ("adapter", "alternate-root", 'const PRODUCTION_STAGE7B_PARENT: &str = "/var/lib/moex-trading/stage7b";', 'const PRODUCTION_STAGE7B_PARENT: &str = "/tmp/operator-selected";'),
    ("adapter", "remove-manifest-hmac", "|| !commitment_key.stage8b_r2a7_verify_reader_manifest_hmac_sha256(", "|| stage8b_r2a7_manifest_hmac_skipped("),
    ("adapter", "remove-domain-verification", "verify_published_domain(&layout.output_root", "skip_published_domain_verification(&layout.output_root"),
    ("runtime", "allow-duplicate", "if candidates.next().is_some()", "if false"),
    ("runtime", "allow-terminal", "== crate::Stage6DispatchSafetyStateV1::ReconciliationRequired\n                && request.final_disposition().is_none()", "== crate::Stage6DispatchSafetyStateV1::ReconciliationRequired\n                && true"),
    ("runtime", "allow-stale-dispatch", "request.dispatch_attempt_count() == 1", "request.dispatch_attempt_count() >= 1"),
    ("composition", "remove-provenance", "attach_stage8b_r2a7_record_provenance(output_root, evidence, adapter_domain)", "skip_stage8b_r2a7_record_provenance(output_root, evidence, adapter_domain)"),
    ("service", "controlled-production-service", "--one-shot-production", "--one-shot-controlled-place"),
    ("service", "enable-network", "RestrictAddressFamilies=AF_UNIX", "RestrictAddressFamilies=AF_UNIX AF_INET"),
    ("build", "build-drift", '"build_b_sha256":', '"build_b_sha256": "0", "discarded_build_b_sha256":'),
    ("build", "fixture-evidence", '"fixture_dependencies": false', '"fixture_dependencies": true'),
    ("rehearsal", "drop-controlled-provenance", "grep -Fq '\"adapter_domain\":\"controlled_qualification\"' \"/tmp/stage8b-r2a7-$operation.json\"", "grep -Fq '\"adapter_domain\":\"production\"' \"/tmp/stage8b-r2a7-$operation.json\""),
)


def main() -> None:
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2a7-negative-") as temporary:
        base = Path(temporary) / "base"
        for relative in FILES:
            destination = base / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        for label, name, old, new in MUTATIONS:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            target = case / TARGETS[label]
            text = target.read_text()
            if text.count(old) != 1:
                raise SystemExit(f"stage8b-p-r2a7-negative: FAIL setup {name} count={text.count(old)}")
            target.write_text(text.replace(old, new, 1))
            result = subprocess.run(["python3", str(case / "scripts/stage8b_p_r2a7_review_closure_check.py")], cwd=case, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r2a7-negative: FAIL accepted {name}")
            passed += 1
    print(f"stage8b-p-r2a7-negative: PASS {passed}/{len(MUTATIONS)}")


if __name__ == "__main__":
    main()
