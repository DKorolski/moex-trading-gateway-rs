#!/usr/bin/env python3
"""Mutation checks for the Stage 7B-d design authority."""
from __future__ import annotations

import json
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts/stage7b_d_design_check.py"

CASES = (
    ("allow-mutable-extractor", "descriptor", "mutable_recovered_extractor_allowed", True),
    ("drop-seal-barrier", "descriptor", "seal_before_ack_xack_required", False),
    ("drop-disk-seal-revalidation", "descriptor", "on_disk_seal_revalidation_required", False),
    ("drop-atomic-ack", "descriptor", "atomic_ack_xack_required", False),
    ("drop-atomic-dlq", "descriptor", "atomic_dlq_xack_required", False),
    ("execution-identity-marker", "descriptor", "settlement_marker_transport_only", False),
    ("memory-ack-authority", "descriptor", "process_memory_ack_restart_authority", True),
    ("merge-freshness", "descriptor", "source_claim_freshness_independent", False),
    ("abort-stale-ready", "descriptor", "explicit_task_abort_clears_readiness", False),
    ("premature-redis", "descriptor", "redis_consumer_attached", True),
    ("premature-xack", "descriptor", "xack_enabled", True),
    ("exactly-once-overclaim", "descriptor", "cross_process_exactly_once_claimed", True),
    ("remove-lua-primitive", "design", "one reviewed Lua primitive", ""),
    ("remove-response-loss", "design", "response loss", ""),
    ("remove-legacy-isolation", "design", "Legacy SQLite/M3", ""),
    ("ack-authority-not-linear", "descriptor", "settlement_authorization_linear", False),
    ("ack-authority-serializable", "descriptor", "settlement_authorization_serializable", True),
    ("ack-authority-input-reconstructible", "descriptor", "settlement_authorization_reconstructible_from_input", True),
    ("drop-exact-request-binding", "descriptor", "settlement_authorization_exact_request_bound", False),
    ("drop-seal-generation-binding", "descriptor", "settlement_authorization_seal_generation_bound", False),
    ("drop-checkpoint-binding", "descriptor", "settlement_authorization_checkpoint_bound", False),
    ("drop-ack-fingerprint-binding", "descriptor", "settlement_authorization_payload_fingerprint_bound", False),
    ("drop-transport-entry-binding", "descriptor", "transport_plan_entry_bound", False),
    ("mint-ack-before-finalized-sealed", "descriptor", "ack_requires_finalized_and_sealed", False),
    ("merge-ack-poison-authority", "descriptor", "separate_ack_and_poison_capabilities", False),
    ("drop-zero-stage6-poison-proof", "descriptor", "poison_requires_zero_stage6_mutation", False),
    ("fake-seal-for-poison", "descriptor", "poison_no_stage6_seal_advance", False),
    ("settle-held-state", "descriptor", "holds_never_dlq_or_xack", False),
    ("fingerprint-in-stable-key", "descriptor", "stable_settlement_key_excludes_payload_fingerprint", False),
    ("drop-stored-fingerprint", "descriptor", "marker_value_contains_payload_fingerprint", False),
    ("repeat-xadd-on-exact-retry", "descriptor", "same_key_same_fingerprint_idempotent", False),
    ("overwrite-conflicting-fingerprint", "descriptor", "same_key_different_fingerprint_conflict", False),
    ("drop-request-canonical-marker", "descriptor", "request_canonical_ack_marker", False),
    ("second-canonical-on-duplicate", "descriptor", "post_publication_duplicate_semantics", False),
    ("settle-conflicting-duplicate", "descriptor", "conflicting_duplicate_never_settles", False),
    ("drop-new-settlement-pel-check", "descriptor", "new_settlement_requires_expected_pel", False),
    ("require-pel-on-committed-retry", "descriptor", "marker_retry_does_not_require_pel", False),
    ("validate-after-first-write", "descriptor", "lua_validates_before_first_write", False),
    ("allow-cross-slot-settlement", "descriptor", "single_hash_slot_required", False),
    ("blind-seal-generation-retry", "descriptor", "ambiguous_seal_commit_requires_reread", False),
    ("overclaim-redis-rollback", "descriptor", "redis_response_loss_scope_explicit", False),
    ("close-b052-b053-in-da", "descriptor", "d_a_rows_exclude_b052_b053", False),
    ("omit-b052-b053-from-dc", "descriptor", "d_c_closes_b052_b053", False),
    ("open-implementation-before-accept", "descriptor", "implementation_open_after_design_acceptance", False),
)


def main() -> None:
    for name, kind, key, replacement in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage7b-d-design-negative-{name}-") as tmp:
            clone = Path(tmp) / "repo"
            subprocess.run(
                ["git", "clone", "--quiet", "--no-hardlinks", str(ROOT), str(clone)],
                check=True,
            )
            subprocess.run(
                ["git", "checkout", "--quiet", ROOT.resolve().as_posix()],
                cwd=clone,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            ) if False else None
            # Copy the current design worktree because the harness also runs before commit.
            for relative in (
                "docs/stage-7/stage7b-d-entry-descriptor.json",
                "docs/stage-7/stage7b-entry-descriptor.json",
                "docs/stage-7/stage7b-c-entry-descriptor.json",
                "docs/stage-7/stage7b-d-design.md",
                "docs/stage-7/stage7b-d-row-ownership.json",
                "docs/stage-7/stage7b-acceptance-proof-map.json",
                "docs/stage-7/TZ_STAGE7B_D_DESIGN_R1_IMPLEMENTATION_CONTRACT_2026-08-13.md",
                "docs/stage-7/STAGE7B_D_DESIGN_R1_ACCEPTANCE_MATRIX_2026-08-13.csv",
                "scripts/stage7b_proof_map.py",
                "scripts/stage7b_d_design_check.py",
            ):
                source = ROOT / relative
                target = clone / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
            if kind == "descriptor":
                path = clone / "docs/stage-7/stage7b-d-entry-descriptor.json"
                value = json.loads(path.read_text())
                value[key] = replacement
                path.write_text(json.dumps(value, indent=2) + "\n")
            else:
                path = clone / "docs/stage-7/stage7b-d-design.md"
                text = path.read_text()
                if key not in text:
                    raise SystemExit(f"stage7b-d-design-negative: fixture token absent: {key}")
                path.write_text(re.sub(re.escape(key), replacement, text, flags=re.IGNORECASE))
            result = subprocess.run(
                ["python3", str(clone / CHECK.relative_to(ROOT))],
                cwd=clone,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage7b-d-design-negative: FAIL mutation survived: {name}")
            print(f"PASS {name}")
    print(f"stage7b-d-design-negative: PASS cases={len(CASES)}")


if __name__ == "__main__":
    main()
