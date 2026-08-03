#!/usr/bin/env python3
"""Twelve fail-closed mutations for Stage 5G-e-c."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage5g_ec_check as checker

ROOT = Path(__file__).resolve().parents[1]
PATHS = tuple(str(path) for path in checker.FILES.values())


def mutate(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    source = path.read_text()
    if old not in source:
        raise RuntimeError(f"missing mutation anchor: {old}")
    path.write_text(source.replace(old, new, 1))


def must_fail(label: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix="stage5g-ec-negative-") as raw:
        root = Path(raw)
        for relative in PATHS:
            destination = root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        mutation(root)
        try:
            checker.validate(root, check_git=False)
        except (checker.CheckFailure, ValueError, KeyError, json.JSONDecodeError):
            print(f"PASS {label}")
            return
        raise SystemExit(f"FAIL mutation escaped e-c checker: {label}")


def main() -> int:
    restart = str(checker.FILES["restart"])
    stage5d = str(checker.FILES["stage5d"])
    order = str(checker.FILES["order"])
    lib = str(checker.FILES["lib"])
    descriptor = str(checker.FILES["descriptor"])
    cases = (
        ("retain-source", lambda r: mutate(r, restart, "drop(source);", "let _retained = source;")),
        ("parallel-package", lambda r: mutate(r, restart, "let bytes = stage5d_export_canonical_restart_bytes_with_stage5g_extension(", "let bytes = ad_hoc_stage5g_restart_json(")),
        ("stage5d-decode-bypass", lambda r: mutate(r, restart, "let decoded = stage5d_decode_canonical_restart_bytes_requiring_stage5g(bytes)?;", "let decoded = serde_json_decode_without_stage5d(bytes)?;")),
        ("fresh-runtime-bypass", lambda r: mutate(r, restart, "stage5d_reconstruct_runtime_from_clean_restart(decoded, fresh_runtime)?;", "reuse_source_runtime(decoded, fresh_runtime)?;")),
        ("missing-extension-accepted", lambda r: mutate(r, stage5d, ".ok_or(Stage5dEnvelopeValidationError::RequiredFieldEmpty)?", ".unwrap_or_default()")),
        ("extension-checksum-removed", lambda r: mutate(r, stage5d, "fn validate_stage5g_extension_pair(&self)", "fn removed_extension_pair_validation(&self)")),
        ("replay-validation-removed", lambda r: mutate(r, restart, "crate::validate_stage5g_timer_checkpoint(&projection.checkpoint)", "accept_unvalidated_checkpoint(&projection.checkpoint)")),
        ("missing-lifecycle-test", lambda r: mutate(r, order, "stage5ge_c_unsupported_lifecycle_kind_fails_closed", "removed_unsupported_lifecycle_test")),
        ("missing-decimal-test", lambda r: mutate(r, order, "stage5ge_c_exact_decimal_representation_survives_byte_roundtrip", "removed_decimal_test")),
        ("missing-source-move-proof", lambda r: mutate(r, lib, "moved_source_cannot_be_reused", "removed_source_move_witness")),
        ("open-stage5g-f", lambda r: mutate(r, descriptor, '"stage5g_f": false', '"stage5g_f": true')),
        ("reduce-lifecycle-set", lambda r: mutate(r, descriptor, '    "new_package_awaiting"', '    "timer_ready"')),
    )
    for label, mutation in cases:
        must_fail(label, mutation)
    print(f"stage5g-ec-negative-harness: PASS {len(cases)}/{len(cases)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
