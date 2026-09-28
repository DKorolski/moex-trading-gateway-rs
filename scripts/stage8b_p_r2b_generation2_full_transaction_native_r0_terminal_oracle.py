#!/usr/bin/env python3
"""Bind exact Generation-2 request evidence to preserved redacted raw inputs."""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

import stage8b_p_r2b_generation2_composition_r0_r1_terminal_oracle as accepted


ORACLE_ID = "EXACT_TYPED_ROOT_TERMINAL_EVIDENCE_WITH_RAW_INPUT_BINDING"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_files(terminal_path: Path, helper_log_path: Path) -> dict[str, object]:
    proof = accepted.validate_files(terminal_path, helper_log_path)
    failed_attempt = proof["failed_attempt"]
    if not isinstance(failed_attempt, dict):
        raise ValueError("failed request attempt missing")
    if failed_attempt.get("error_category") == "TIMEOUT" and failed_attempt.get("timeout_stage") != "request":
        raise ValueError('request TIMEOUT requires timeout_stage == "request"')
    proof.update(
        {
            "oracle": ORACLE_ID,
            "accepted_base_oracle": accepted.ORACLE_ID,
            "raw_redacted_root_terminal_sha256": digest(terminal_path),
            "redacted_helper_journal_sha256": digest(helper_log_path),
            "timeout_stage_exact_request_enforced": True,
        }
    )
    return proof


def main() -> None:
    if len(sys.argv) != 4:
        raise SystemExit("usage: native_terminal_oracle TERMINAL_JSON REDACTED_HELPER_LOG OUTPUT_JSON")
    terminal_path, helper_log_path, output_path = map(Path, sys.argv[1:])
    if output_path.exists() or not output_path.parent.is_dir():
        raise SystemExit("stage8b-generation2-full-transaction-terminal-oracle: FAIL unsafe output")
    try:
        proof = validate_files(terminal_path, helper_log_path)
    except (KeyError, OSError, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-generation2-full-transaction-terminal-oracle: FAIL {error}") from error
    output_path.write_text(json.dumps(proof, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(
        "stage8b-generation2-full-transaction-terminal-oracle: PASS "
        f"request={accepted.EXPECTED_METHOD}:{accepted.EXPECTED_ROUTE}:{accepted.EXPECTED_ORDINAL} "
        f"error={proof['failed_attempt']['error_category']} raw_inputs_bound=true"
    )


if __name__ == "__main__":
    main()
