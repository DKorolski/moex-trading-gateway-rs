#!/usr/bin/env python3
"""Mutation harness for the Stage 7B-d-a Redis-free authority boundary."""
from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECK = Path("scripts/stage7b_d_a_check.py")
DESCRIPTOR = Path("docs/stage-7/stage7b-d-entry-descriptor.json")
AGGREGATE = Path("docs/stage-7/stage7b-entry-descriptor.json")
OWNERSHIP = Path("docs/stage-7/stage7b-d-row-ownership.json")
RECOVERY = Path("crates/runtime-durable-service/src/recovery.rs")
MANIFEST = Path("crates/runtime-durable-service/Cargo.toml")
PROOF_GENERATOR = Path("scripts/stage7b_proof_map.py")

COPY_PATHS = (
    Path("Cargo.lock"),
    MANIFEST,
    Path("crates/runtime-durable-service/src/lib.rs"),
    RECOVERY,
    Path("crates/strategy-runtime-core/src/lib.rs"),
    Path("crates/strategy-runtime-core/src/stage5g_order_position.rs"),
    Path("crates/strategy-runtime-core/src/stage6d_live_core.rs"),
    Path("docs/current-status.md"),
    Path("docs/roadmap.md"),
    Path("docs/reviewer-onboarding-and-roadmap.md"),
    Path("docs/stage-7/stage7b-d-a-implementation.md"),
    Path("docs/stage-7/stage7b-d-design-r1-review-closure.md"),
    DESCRIPTOR,
    AGGREGATE,
    OWNERSHIP,
    Path("docs/stage-7/stage7b-acceptance-proof-map.json"),
    CHECK,
    PROOF_GENERATOR,
)


def mutate_json(path: Path, key: str, value: object) -> None:
    document = json.loads(path.read_text())
    document[key] = value
    path.write_text(json.dumps(document, indent=2) + "\n")


def replace(path: Path, old: str, new: str, *, all_matches: bool = False) -> None:
    source = path.read_text()
    if old not in source:
        raise SystemExit(f"stage7b-d-a-negative: fixture token absent: {path}: {old}")
    source = source.replace(old, new) if all_matches else source.replace(old, new, 1)
    path.write_text(source)


CASES = (
    ("design-unfrozen", lambda root: mutate_json(root / DESCRIPTOR, "stage7b_d_design_frozen", False)),
    ("implementation-not-started", lambda root: mutate_json(root / DESCRIPTOR, "stage7b_d_a_implementation_started", False)),
    ("acceptance-not-pending", lambda root: mutate_json(root / DESCRIPTOR, "stage7b_d_a_acceptance_pending", False)),
    ("open-d-b", lambda root: mutate_json(root / DESCRIPTOR, "stage7b_d_b_open", True)),
    ("open-d-c", lambda root: mutate_json(root / DESCRIPTOR, "stage7b_d_c_open", True)),
    ("close-b052-b053", lambda root: mutate_json(root / DESCRIPTOR, "b052_b053_implemented", True)),
    ("attach-redis-consumer", lambda root: mutate_json(root / DESCRIPTOR, "redis_consumer_attached", True)),
    ("enable-redis-settlement", lambda root: mutate_json(root / DESCRIPTOR, "redis_settlement_enabled", True)),
    ("enable-xack", lambda root: mutate_json(root / DESCRIPTOR, "xack_enabled", True)),
    ("enable-runtime-live", lambda root: mutate_json(root / DESCRIPTOR, "runtime_live", True)),
    ("enable-real-orders", lambda root: mutate_json(root / DESCRIPTOR, "real_orders", True)),
    ("negative-count-drift", lambda root: mutate_json(root / DESCRIPTOR, "d_a_negative_case_count", 27)),
    ("publish-ack-authority", lambda root: replace(root / RECOVERY, "pub(crate) struct Stage7bDurableAckAuthorized", "pub struct Stage7bDurableAckAuthorized")),
    ("clone-ack-authority", lambda root: replace(root / RECOVERY, "pub(crate) struct Stage7bDurableAckAuthorized", "#[derive(Clone)]\npub(crate) struct Stage7bDurableAckAuthorized")),
    ("drop-frontier-refresh", lambda root: replace(root / RECOVERY, "refresh_stage7b_durable_frontier(&mut self.recovered)?;", "/* frontier refresh removed */")),
    ("drop-seal-advance", lambda root: replace(root / RECOVERY, "self.advance_recovery_seal(commitment_key)?;", "/* seal advance removed */")),
    ("ignore-seal-uncertainty", lambda root: replace(root / RECOVERY, "self.seal_commit_uncertain = true;", "self.seal_commit_uncertain = false;", all_matches=True)),
    ("first-recovered-ack-duplicate", lambda root: replace(root / RECOVERY, "None => Stage7bAckPublicationDecision::Canonical", "None => Stage7bAckPublicationDecision::Duplicate")),
    ("add-redis-dependency", lambda root: replace(root / MANIFEST, "chrono.workspace = true", "chrono.workspace = true\nredis = \"0.32\"")),
    ("add-http-dependency", lambda root: replace(root / MANIFEST, "chrono.workspace = true", "chrono.workspace = true\nreqwest = \"0.12\"")),
    ("drop-b044", lambda root: replace(root / RECOVERY, "stage7b_d_a_b044_sigkill_after_accepted_recovers_dispatch_once", "removed_b044")),
    ("drop-b045", lambda root: replace(root / RECOVERY, "stage7b_d_a_b045_sigkill_after_dispatch_never_blind_redispatches", "removed_b045")),
    ("drop-b046", lambda root: replace(root / RECOVERY, "stage7b_d_a_b046_sigkill_during_unknown_effect_requires_reconciliation", "removed_b046")),
    ("drop-b047", lambda root: replace(root / RECOVERY, "stage7b_d_a_b047_sigkill_after_outcome_reconstructs_finalization_and_ack", "removed_b047")),
    ("drop-b048", lambda root: replace(root / RECOVERY, "stage7b_d_a_b048_sigkill_after_finalization_reconstructs_canonical_ack", "removed_b048")),
    ("drop-b050", lambda root: replace(root / RECOVERY, "stage7b_d_a_b050_seal_failure_blocks_authorization_and_readiness", "removed_b050")),
    ("drop-b051", lambda root: replace(root / RECOVERY, "stage7b_d_a_b051_sigkill_after_seal_reconstructs_without_provider", "removed_b051")),
    ("drop-b054", lambda root: replace(root / RECOVERY, "stage7b_d_a_b054_sequential_cancel_survives_restart_and_reseals", "removed_b054")),
    ("premature-b052-proof", lambda root: replace(root / PROOF_GENERATOR, '"B-054": (', '"B-052": (')),
)


def main() -> None:
    descriptor = json.loads((ROOT / DESCRIPTOR).read_text())
    expected = descriptor.get("d_a_negative_case_count")
    if len(CASES) != expected:
        raise SystemExit(
            "stage7b-d-a-negative: FAIL descriptor/case-count drift "
            f"descriptor={expected} actual={len(CASES)}"
        )
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage7b-d-a-negative-{name}-") as tmp:
            clone = Path(tmp) / "repo"
            subprocess.run(
                ["git", "clone", "--quiet", "--no-hardlinks", str(ROOT), str(clone)],
                check=True,
            )
            for relative in COPY_PATHS:
                source = ROOT / relative
                target = clone / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
            mutation(clone)
            result = subprocess.run(
                ["python3", str(clone / CHECK)],
                cwd=clone,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage7b-d-a-negative: FAIL mutation survived: {name}")
            print(f"PASS {name}")
    print(f"stage7b-d-a-negative: PASS cases={len(CASES)}")


if __name__ == "__main__":
    main()
