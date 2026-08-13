#!/usr/bin/env python3
"""Mutation harness for the Stage 7B-d-b atomic settlement boundary."""
from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECK = Path("scripts/stage7b_d_b_check.py")
DESCRIPTOR = Path("docs/stage-7/stage7b-d-entry-descriptor.json")
AGGREGATE = Path("docs/stage-7/stage7b-entry-descriptor.json")
OWNERSHIP = Path("docs/stage-7/stage7b-d-row-ownership.json")
SETTLEMENT = Path("crates/runtime-durable-service/src/recovery/redis_settlement.rs")
RECOVERY = Path("crates/runtime-durable-service/src/recovery.rs")
BRIDGE = Path("crates/runtime-command-bridge/src/lib.rs")
MANIFEST = Path("crates/runtime-durable-service/Cargo.toml")
PROOF_GENERATOR = Path("scripts/stage7b_proof_map.py")

COPY_PATHS = (
    Path("Cargo.lock"),
    MANIFEST,
    RECOVERY,
    SETTLEMENT,
    BRIDGE,
    Path("docs/current-status.md"),
    Path("docs/roadmap.md"),
    Path("docs/reviewer-onboarding-and-roadmap.md"),
    Path("docs/stage-7/stage7b-d-b-implementation.md"),
    Path("docs/stage-7/stage7b-d-a-implementation.md"),
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
        raise SystemExit(f"stage7b-d-b-negative: fixture token absent: {path}: {old}")
    source = source.replace(old, new) if all_matches else source.replace(old, new, 1)
    path.write_text(source)


CASES = (
    ("close-d-b", lambda root: mutate_json(root / DESCRIPTOR, "stage7b_d_b_open", False)),
    ("accept-d-b-without-review", lambda root: mutate_json(root / DESCRIPTOR, "stage7b_d_b_acceptance_pending", False)),
    ("open-d-c", lambda root: mutate_json(root / DESCRIPTOR, "stage7b_d_c_open", True)),
    ("close-b052-b053", lambda root: mutate_json(root / DESCRIPTOR, "b052_b053_implemented", True)),
    ("implemented-count-drift", lambda root: mutate_json(root / DESCRIPTOR, "implemented_count", 62)),
    ("attach-command-consumer", lambda root: mutate_json(root / DESCRIPTOR, "redis_consumer_attached", True)),
    ("disable-reviewed-settlement", lambda root: mutate_json(root / DESCRIPTOR, "redis_settlement_enabled", False)),
    ("disable-xack", lambda root: mutate_json(root / DESCRIPTOR, "xack_enabled", False)),
    ("overclaim-cross-process-exactly-once", lambda root: mutate_json(root / DESCRIPTOR, "cross_process_exactly_once_claimed", True)),
    ("open-runtime-live", lambda root: mutate_json(root / DESCRIPTOR, "runtime_live", True)),
    ("open-finam-post-delete", lambda root: mutate_json(root / DESCRIPTOR, "finam_post_delete", True)),
    ("negative-count-drift", lambda root: mutate_json(root / DESCRIPTOR, "d_b_negative_case_count", 25)),
    ("payload-in-stable-entry-key", lambda root: replace(root / SETTLEMENT, "let stable = format!(", "let payload_fingerprint = \"forbidden\";\n        let stable = format!(")),
    ("drop-entry-fingerprint-conflict", lambda root: replace(root / SETTLEMENT, "STAGE7B_CONFLICT_ENTRY_FINGERPRINT", "STAGE7B_ENTRY_FINGERPRINT_IGNORED", all_matches=True)),
    ("drop-request-canonical-conflict", lambda root: replace(root / SETTLEMENT, "STAGE7B_CONFLICT_REQUEST_MARKER", "STAGE7B_REQUEST_MARKER_IGNORED", all_matches=True)),
    ("drop-new-settlement-pel-check", lambda root: replace(root / SETTLEMENT, "STAGE7B_SOURCE_NOT_PENDING", "STAGE7B_SOURCE_ASSUMED_PENDING", all_matches=True)),
    ("committed-retry-after-pel", lambda root: replace(root / SETTLEMENT, "local existing_entry = redis.call('GET', entry_marker)", "local committed_entry = redis.call('GET', entry_marker)")),
    ("second-best-effort-xadd", lambda root: replace(root / SETTLEMENT, "redis.call('XACK', source, group, entry_id)", "redis.call('XADD', output, '*', 'payload', selected_payload)\nredis.call('XACK', source, group, entry_id)")),
    ("drop-xack", lambda root: replace(root / SETTLEMENT, "redis.call('XACK', source, group, entry_id)", "-- XACK removed")),
    ("merge-ack-and-poison-authority", lambda root: replace(root / SETTLEMENT, "pub(super) struct Stage7bPoisonDlqAuthorized", "pub(super) struct Stage7bRedisAckSettlementPlan")),
    ("clone-poison-authority", lambda root: replace(root / SETTLEMENT, "pub(super) struct Stage7bPoisonDlqAuthorized", "#[derive(Clone)]\npub(super) struct Stage7bPoisonDlqAuthorized")),
    ("publish-raw-payload-field", lambda root: replace(root / SETTLEMENT, "redacted_payload_sha256: &'a str,", "raw_payload: &'a str,")),
    ("drop-poison-checkpoint-binding", lambda root: replace(root / SETTLEMENT, "observation.stage6_checkpoint_sha256 != current_stage6_checkpoint_sha256", "false")),
    ("open-xautoclaim-consumer", lambda root: replace(root / SETTLEMENT, "#[cfg(test)]", "const FORBIDDEN_D_C: &str = \"XAUTOCLAIM\";\n\n#[cfg(test)]")),
    ("add-finam-http-dependency", lambda root: replace(root / MANIFEST, "redis.workspace = true", "redis.workspace = true\nreqwest = \"0.12\"")),
    ("premature-b052-proof", lambda root: replace(root / PROOF_GENERATOR, '"B-057": (', '"B-052": (')),
    ("request-marker-uses-dynamic-authority", lambda root: replace(root / SETTLEMENT, "marker['terminal_request_ack_identity'] ~= terminal_request_ack_identity", "marker['canonical_ack_fingerprint'] ~= authority_fp")),
    ("stable-request-identity-includes-seal", lambda root: replace(root / RECOVERY, "terminal_ack_schema: u16,", "terminal_ack_schema: u16,\n        seal_generation: u64,")),
    ("restore-free-form-poison-reason", lambda root: replace(root / SETTLEMENT, "current_stage6_checkpoint_sha256: &str,", "poison_reason: &str,\n    current_stage6_checkpoint_sha256: &str,")),
    ("bypass-canonical-poison-evidence", lambda root: replace(root / SETTLEMENT, "evidence: Stage7aPermanentPoisonEvidence,", "evidence: &[u8],")),
    ("valid-command-mints-poison", lambda root: replace(root / BRIDGE, "Ok(_) => return Err(Stage7aBridgeError::NotPermanentPoison),", "Ok(_) => (Stage7aDlqReason::InvalidJson, payload),")),
    ("clone-permanent-poison-evidence", lambda root: replace(root / BRIDGE, "pub struct Stage7aPermanentPoisonEvidence", "#[derive(Clone)]\npub struct Stage7aPermanentPoisonEvidence")),
    ("unrelated-success-clears-unresolved", lambda root: replace(root / SETTLEMENT, "self.unresolved_settlement_keys.remove(settlement_key);", "self.unresolved_settlement_keys.clear();")),
    ("failure-not-recorded-by-entry", lambda root: replace(root / SETTLEMENT, "self.unresolved_settlement_keys\n                    .insert(settlement_key.to_string());", "// unresolved settlement key dropped", all_matches=True)),
)


def main() -> None:
    descriptor = json.loads((ROOT / DESCRIPTOR).read_text())
    expected = descriptor.get("d_b_negative_case_count")
    if len(CASES) != expected:
        raise SystemExit(
            "stage7b-d-b-negative: FAIL descriptor/case-count drift "
            f"descriptor={expected} actual={len(CASES)}"
        )
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage7b-d-b-negative-{name}-") as tmp:
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
                raise SystemExit(f"stage7b-d-b-negative: FAIL mutation survived: {name}")
            print(f"PASS {name}")
    print(f"stage7b-d-b-negative: PASS cases={len(CASES)}")


if __name__ == "__main__":
    main()
