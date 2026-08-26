#!/usr/bin/env python3
"""Mutation matrix for the R2A5 source-truth closure checker."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MUTATIONS = [
    ("production", "production-issued", '"authorization_status": "NOT_ISSUED"', '"authorization_status": "ISSUED"'),
    ("status", "real-credential-open", '"real_finam_credentials": false', '"real_finam_credentials": true'),
    ("status", "typed-operator-decision-dropped", '"typed_operator_decision_required_in_r2b": true', '"typed_operator_decision_required_in_r2b": false'),
    ("source", "manual-publication", '"manual_or_operator_publication_allowed": false', '"manual_or_operator_publication_allowed": true'),
    ("source", "wrong-root", "/var/lib/moex-trading/operational-authorities", "/tmp/manual-authorities"),
    ("source", "future-budget-expanded", '"max_future_skew_ms":250', '"max_future_skew_ms":5000'),
    ("helper-sha", "helper-unfrozen", None, "0" * 64),
    ("helper-authority", "helper-authority-replaced", "fdfc0311152fadf6f241331745dbf284d02701667375fb731f044ce0fb47f608", "1" * 64),
    ("trust", "helper-package-key-conflated", "stage8b-r2a5-production-helper-acceptance-v1", "stage8b-r2a5-production-package-authorization-v1"),
    ("r2a3", "source-timestamp-field-removed", "pub source_observed_at_utc: DateTime<Utc>", "pub legacy_observed_at_utc: DateTime<Utc>"),
    ("r2a3", "producer-before-source-check-removed", "signed.produced_at_utc < signed.source_observed_at_utc", "false"),
    ("r2a3", "source-receipt-binding-removed", "signed.source_observed_at_utc != signed.receipt.observed_at_utc", "false"),
    ("r2a3", "skew-laundering", "runtime.push(signed.receipt.observed_at_utc)", "runtime.push(signed.produced_at_utc)"),
    ("r2a5", "freshness-check-removed", "validate_source_freshness(source, source_observed_at_utc, produced_at)", "Ok(())"),
    ("r2a5", "issuer-source-time-substitution", "source_observed_at_utc: snapshot.source_observed_at_utc", "source_observed_at_utc: snapshot.produced_at_utc"),
    ("r2a5", "issuer-producer-time-substitution", "produced_at_utc: snapshot.produced_at_utc", "produced_at_utc: snapshot.source_observed_at_utc"),
    ("r2a5", "package-helper-check-removed", "draft.helper_executable_sha256 != accepted_helper.helper_executable_sha256", "false"),
    ("r2a5", "helper-self-check-removed", "if executable_sha256 != accepted_helper.helper_executable_sha256", "if false"),
    ("adapter", "adapter-private", "pub fn publish_stage8b_r2a5_operational_sources(", "pub(crate) fn publish_stage8b_r2a5_operational_sources("),
    ("adapter", "atomic-create-new-removed", ".create_new(true)\n            .mode(0o640)", ".create(true)\n            .mode(0o640)"),
    ("adapter", "nofollow-removed", ".custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)\n            .open(&temporary)", ".custom_flags(libc::O_CLOEXEC)\n            .open(&temporary)"),
    ("launcher", "launcher-hash-check-removed", "ACCEPTED_SHA256.trim()", '"0"'),
    ("producer-unit", "manual-intermediate-restored", "/var/lib/moex-trading/operational-authorities", "/var/lib/moex-trading/stage8b/r2a5/authoritative-stores"),
    ("rehearsal", "cancel-removed", "PLACE CANCEL", "PLACE"),
]


def main() -> None:
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2a5-negative-") as temp:
        repo = Path(temp) / "repo"
        for directory in ("docs/stage-8", "deploy/stage8b-r2a5", "tools/stage8b-readonly-preflight", "crates/finam-gateway/src"):
            shutil.copytree(ROOT / directory, repo / directory, ignore=shutil.ignore_patterns("target"))
        (repo / "scripts").mkdir()
        for name in ("stage8b_p_r2a5_review_closure_check.py", "stage8b_p_r2a5_linux_rehearsal.sh"):
            shutil.copy2(ROOT / "scripts" / name, repo / "scripts" / name)
        targets = {
            "production": repo / "docs/stage-8/stage8b-p-r2a5-authority.json",
            "status": repo / "docs/stage-8/stage8b-p-r2a5-status.json",
            "source": repo / "docs/stage-8/stage8b-p-r2a5-source-adapter-authority.json",
            "helper-sha": repo / "docs/stage-8/stage8b-p-r2a5-accepted-helper-sha256.txt",
            "helper-authority": repo / "docs/stage-8/stage8b-p-r2a5-accepted-helper-authority.json",
            "trust": repo / "docs/stage-8/stage8b-p-r2a5-production-trust-manifest.json",
            "r2a3": repo / "tools/stage8b-readonly-preflight/src/r2a3.rs",
            "r2a5": repo / "tools/stage8b-readonly-preflight/src/r2a5.rs",
            "adapter": repo / "crates/finam-gateway/src/stage8a1_execution_capability.rs",
            "launcher": repo / "tools/stage8b-readonly-preflight/src/bin/stage8b-r2a5-launcher.rs",
            "producer-unit": repo / "deploy/stage8b-r2a5/stage8b-r2a5-producer@.service",
            "rehearsal": repo / "scripts/stage8b_p_r2a5_linux_rehearsal.sh",
        }
        checker = repo / "scripts/stage8b_p_r2a5_review_closure_check.py"
        for target_name, name, old, new in MUTATIONS:
            target = targets[target_name]
            original = target.read_text()
            if old is None:
                target.write_text(new + "\n")
            else:
                if old not in original:
                    raise SystemExit(f"stage8b-p-r2a5-negative: FAIL setup {name}")
                target.write_text(original.replace(old, new, 1))
            result = subprocess.run(["python3", str(checker), "--root", str(repo)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            target.write_text(original)
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r2a5-negative: FAIL accepted {name}")
            passed += 1
            print(f"PASS {name}")
    print(f"stage8b-p-r2a5-negative: PASS {passed}/{len(MUTATIONS)}")


if __name__ == "__main__":
    main()
