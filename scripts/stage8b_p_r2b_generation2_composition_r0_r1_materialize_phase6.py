#!/usr/bin/env python3
"""Materialize Generation-2 Phase-6 with the exact typed R1A terminal oracle."""

from __future__ import annotations

import hashlib
import os
import sys
from pathlib import Path

import stage8b_p_r2b_generation2_composition_r0_materialize_phase6 as r0


ROOT = Path(__file__).resolve().parents[1]
R0_MATERIALIZER = ROOT / "scripts/stage8b_p_r2b_generation2_composition_r0_materialize_phase6.py"
R0_MATERIALIZER_SHA256 = "b12803593ef80409846aa55d9e7e20ce52c45ab887bc77a1d1bfaadac0a7d6e0"
ORACLE = "scripts/stage8b_p_r2b_generation2_composition_r0_r1_terminal_oracle.py"

R1_REPLACEMENTS = (
    (
        "Stage 8B-P R2B Generation-2 Composition Rebuild R0",
        "Stage 8B-P R2B Generation-2 Composition Rebuild R0-R1 Exact Phase-6 Evidence Closure",
        1,
    ),
    (
        '''grep -Eq 'NETWORK_CONNECT_FAILURE|AUTH_SESSION_FAILURE' "$terminal_file"
journalctl -u "$supervisor_unit" --no-pager > /run/r0-r1a-supervisor.log
grep -Fq 'stage8b-r2b-helper: identity-verified' /run/r0-r1a-supervisor.log
grep -Fq 'stage8b-r2b-helper: receipt-verified' /run/r0-r1a-supervisor.log
grep -Fq 'stage8b-r2b-helper: authority-verified' /run/r0-r1a-supervisor.log
grep -Fq 'stage8b-r2b-helper: credentials-loaded' /run/r0-r1a-supervisor.log''',
        '''journalctl -u "$supervisor_unit" --no-pager > /run/r0-r1a-supervisor.log
request_boundary_proof=/run/stage8b-generation2-r0-r1-request-boundary-proof.json
python3 "$repo_root/scripts/stage8b_p_r2b_generation2_composition_r0_r1_terminal_oracle.py" \
  "$terminal_file" /run/r0-r1a-supervisor.log "$request_boundary_proof"''',
        1,
    ),
    (
        'python3 - "$evidence_path" "$helper_sha256" "$launcher_sha256" "$supervisor_exit" "$terminal_file" <<\'PY\'',
        'python3 - "$evidence_path" "$helper_sha256" "$launcher_sha256" "$supervisor_exit" "$terminal_file" "$request_boundary_proof" <<\'PY\'',
        1,
    ),
    (
        "terminal=pathlib.Path(sys.argv[5])",
        "terminal=pathlib.Path(sys.argv[5])\nrequest_boundary_proof=json.loads(pathlib.Path(sys.argv[6]).read_text())",
        1,
    ),
    (
        ' "actual_read_attempts":True,',
        ' "actual_read_attempts":request_boundary_proof["actual_read_attempts"],',
        1,
    ),
    (
        ' "terminal_evidence_sha256":hashlib.sha256(terminal.read_bytes()).hexdigest(),',
        ' "terminal_evidence_sha256":hashlib.sha256(terminal.read_bytes()).hexdigest(),\n "request_boundary_proof":request_boundary_proof,',
        1,
    ),
    (
        "stage8b-generation2-composition-r0-phase6-rehearsal",
        "stage8b-generation2-composition-r0-r1-phase6-rehearsal",
        2,
    ),
)


def materialized_text() -> str:
    if hashlib.sha256(R0_MATERIALIZER.read_bytes()).hexdigest() != R0_MATERIALIZER_SHA256:
        raise ValueError("accepted R0 materializer drift")
    source = r0.BASE.read_bytes()
    if hashlib.sha256(source).hexdigest() != r0.BASE_SHA256:
        raise ValueError("accepted Phase-6 base drift")
    text = source.decode()
    for old, new, count in r0.REPLACEMENTS:
        if text.count(old) != count:
            raise ValueError(f"R0 replacement cardinality drift: {old!r}")
        text = text.replace(old, new)
    for old, new, count in R1_REPLACEMENTS:
        if text.count(old) != count:
            raise ValueError(f"R0-R1 replacement cardinality drift: {old!r}")
        text = text.replace(old, new)
    forbidden = (
        "stage8b-p-r2a5-production-trust-manifest.json",
        "stage8b-p-r2a5-production-account-key-manifest.json",
        "stage8b-p-r2a5-accepted-helper-authority.json",
        "stage8b-p-r2b-accepted-helper-sha256.txt",
        "generation-1.hex",
        "grep -Eq 'NETWORK_CONNECT_FAILURE|AUTH_SESSION_FAILURE'",
        '"actual_read_attempts":True',
    )
    if any(marker in text for marker in forbidden):
        raise ValueError("legacy Generation-1 or category-only evidence residue")
    required = (
        ORACLE,
        "EXACT_TYPED_ROOT_TERMINAL_EVIDENCE",
        'request_boundary_proof["actual_read_attempts"]',
        '"request_boundary_proof":request_boundary_proof',
    )
    # The oracle ID is emitted by the invoked parser rather than duplicated in
    # the shell. Its source path is the stable materialized-script binding.
    if required[0] not in text or any(marker not in (ROOT / ORACLE).read_text() for marker in required[1:2]):
        raise ValueError("exact typed oracle binding missing")
    if any(marker not in text for marker in required[2:]):
        raise ValueError("derived request-attempt evidence binding missing")
    return text


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: R0_R1_MATERIALIZER OUTPUT")
    output = Path(sys.argv[1])
    if output.exists() or not output.parent.is_dir():
        raise SystemExit("stage8b-generation2-r0-r1-materializer: FAIL unsafe output")
    try:
        text = materialized_text()
    except (OSError, ValueError) as error:
        raise SystemExit(f"stage8b-generation2-r0-r1-materializer: FAIL {error}") from error
    output.write_text(text, encoding="utf-8")
    os.chmod(output, 0o700)
    print(
        "stage8b-generation2-r0-r1-materializer: PASS "
        "base=accepted-r0 generation=2 oracle=exact-typed category_only=false "
        "authorization=NOT_ISSUED"
    )


if __name__ == "__main__":
    main()
