#!/usr/bin/env python3
"""Additive readiness/custody mutations for Stage 8B-P R2A8-R1."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FILES = (
    "docs/stage-8/stage8b-p-r2a8-status.json",
    "docs/stage-8/stage8b-p-r2a8-build-evidence.json",
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
    (0, "status-semantic-persistence", '"composite_readiness_semantics_persisted": true', '"composite_readiness_semantics_persisted": false'),
    (0, "status-writer-fail-closed", '"writer_readiness_fail_closed": true', '"writer_readiness_fail_closed": false'),
    (0, "status-reader-fail-closed", '"reader_readiness_fail_closed": true', '"reader_readiness_fail_closed": false'),
    (2, "trusted-source-v2-removed", "Stage8bR2a8TrustedCurrentSourceV2", "Stage8bR2a8TrustedCurrentSourceLegacy"),
    (2, "manifest-v2-removed", "Stage8bR2a7ReaderManifestV2", "Stage8bR2a7ReaderManifestLegacy"),
    (2, "readiness-field-removed", "pub composite_readiness: Stage8bCompositeReadinessAuthorityV1", "pub composite_timestamp_only: chrono::DateTime<Utc>"),
    (2, "blocked-entry-removed", "pub blocked_entry_ids: Vec<String>", "pub dropped_entry_count: usize"),
    (2, "blocked-request-removed", "pub blocked_request_ids: Vec<StrategyRequestId>", "pub dropped_request_count: usize"),
    (2, "source-domain-drift", "stage8b-r2a8-r1-trusted-current-source-commitment-v2", "stage8b-r2a8-trusted-current-source-commitment-v1"),
    (2, "manifest-domain-drift", "stage8b-r2a8-r1-reader-manifest-commitment-v2", "stage8b-r2a7-reader-manifest-commitment-v1"),
    (2, "writer-admission-removed", "composite_readiness.validate_ready()?;\n    let layout", "let layout"),
    (2, "reader-admission-removed", "manifest.composite_readiness.validate_ready()?;\n    let readiness", "let readiness"),
    (2, "reader-synthesis-restored", "let readiness = manifest.composite_readiness.to_snapshot();", "let readiness = Stage7bCompositeReadinessSnapshot { phase: Stage7bPaperReadinessPhase::PaperReady, reasons: Vec::new(), blocked_entry_ids: Vec::new(), blocked_request_ids: Vec::new(), checked_at: manifest.composite_readiness.checked_at };"),
    (2, "degraded-phase-removed", "Stage8bCompositeReadinessPhaseV1::Degraded", "Stage8bCompositeReadinessPhaseV1::PaperReady"),
    (2, "stopped-phase-removed", "Stage8bCompositeReadinessPhaseV1::Stopped", "Stage8bCompositeReadinessPhaseV1::PaperReady"),
    (2, "consumer-reason-removed", "Stage8bCompositeReadinessReasonV1::ConsumerNotAlive", "Stage8bCompositeReadinessReasonV1::StorageUnavailable"),
    (2, "storage-reason-removed", "Stage8bCompositeReadinessReasonV1::StorageUnavailable", "Stage8bCompositeReadinessReasonV1::SourcePollStale"),
    (2, "source-poll-reason-removed", "Stage8bCompositeReadinessReasonV1::SourcePollStale", "Stage8bCompositeReadinessReasonV1::ClaimScanStale"),
    (2, "claim-scan-reason-removed", "Stage8bCompositeReadinessReasonV1::ClaimScanStale", "Stage8bCompositeReadinessReasonV1::SettlementUnavailable"),
    (2, "settlement-reason-removed", "Stage8bCompositeReadinessReasonV1::SettlementUnavailable", "Stage8bCompositeReadinessReasonV1::DurablePendingEntries"),
    (2, "durable-pending-reason-removed", "Stage8bCompositeReadinessReasonV1::DurablePendingEntries", "Stage8bCompositeReadinessReasonV1::CommandLifecycleBlocked"),
    (2, "command-lifecycle-reason-removed", "Stage8bCompositeReadinessReasonV1::CommandLifecycleBlocked", "Stage8bCompositeReadinessReasonV1::ConsumerNotAlive"),
    (2, "key-gid-drift", "STAGE8B_R2A8_LIFECYCLE_KEY_GID: u32 = 8095", "STAGE8B_R2A8_LIFECYCLE_KEY_GID: u32 = 8096"),
    (2, "key-mode-drift", "STAGE8B_R2A8_LIFECYCLE_KEY_MODE: u32 = 0o640", "STAGE8B_R2A8_LIFECYCLE_KEY_MODE: u32 = 0o644"),
    (2, "key-specific-reader-bypass", "let key_bytes = read_lifecycle_key_file", "let key_bytes = read_fixed_regular_file"),
    (2, "semantic-equality-test-removed", "manifest_binds_readiness_and_preserves_exact_semantics", "manifest_only_checks_ready_boolean"),
    (2, "cross-source-staleness-test-removed", "cross_source_staleness_is_fail_closed", "cross_source_staleness_is_ignored"),
)


def main() -> None:
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2a8-r1-negative-") as temporary:
        base = Path(temporary) / "base"
        for relative in FILES:
            destination = base / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        baseline = subprocess.run(
            ["python3", str(base / "scripts/stage8b_p_r2a8_review_closure_check.py")],
            cwd=base,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        if baseline.returncode != 0:
            raise SystemExit("stage8b-p-r2a8-r1-negative: FAIL baseline")
        for index, name, old, new in MUTATIONS:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            target = case / FILES[index]
            content = target.read_text()
            if content.count(old) < 1:
                raise SystemExit(f"stage8b-p-r2a8-r1-negative: FAIL setup {name}")
            target.write_text(content.replace(old, new))
            result = subprocess.run(
                ["python3", str(case / "scripts/stage8b_p_r2a8_review_closure_check.py")],
                cwd=case,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r2a8-r1-negative: FAIL accepted {name}")
            passed += 1
    print(f"stage8b-p-r2a8-r1-negative: PASS {passed}/{len(MUTATIONS)}")


if __name__ == "__main__":
    main()
