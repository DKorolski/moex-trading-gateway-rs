#!/usr/bin/env python3
"""Materialize the accepted Phase-6 rehearsal with exact Generation-2 bindings."""

from __future__ import annotations

import hashlib
import os
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "scripts/stage8b_p_r2b_implementation_r0_r1a_phase6_rehearsal.sh"
BASE_SHA256 = "97ef39c944db607b5cb9a79509922e7bb9737dec8de1fdb9615356cf76763ac7"

REPLACEMENTS = (
    (
        "stage8b-p-r2b-r0-r1a-phase6-rehearsal",
        "stage8b-generation2-composition-r0-phase6-rehearsal",
        2,
    ),
    (
        "docs/stage-8/stage8b-p-r2a5-production-trust-manifest.json",
        "docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json",
        2,
    ),
    (
        "docs/stage-8/stage8b-p-r2a5-production-account-key-manifest.json",
        "docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-account-key-manifest.json",
        1,
    ),
    (
        "docs/stage-8/stage8b-p-r2a5-accepted-helper-authority.json",
        "docs/stage-8/stage8b-p-r2b-generation2-accepted-helper-authority.json",
        1,
    ),
    (
        "docs/stage-8/stage8b-p-r2b-accepted-helper-sha256.txt",
        "docs/stage-8/stage8b-p-r2b-generation2-accepted-helper-sha256.txt",
        1,
    ),
    ("generation-1.hex", "generation-2.hex", 4),
    ("fields['account_key_generation_id']='1'", "fields['account_key_generation_id']='2'", 1),
    (
        "Stage 8B-P R2B Implementation Package R0-R1A",
        "Stage 8B-P R2B Generation-2 Composition Rebuild R0",
        1,
    ),
    (
        "for binary in stage8b-r2b-run-package-draft-builder stage8b-r2a5-package-issuer stage8b-readonly-preflight stage8b-r2b-launcher; do",
        "for binary in stage8b-r2a5-authority-issuer stage8b-r2b-run-package-draft-builder stage8b-r2a5-package-issuer stage8b-readonly-preflight stage8b-r2b-launcher; do",
        1,
    ),
    (
        "for binary in stage8b-r2a5-controlled-layout stage8b-r2a5-authority-producer stage8b-r2a5-authority-issuer; do",
        "for binary in stage8b-r2a5-controlled-layout stage8b-r2a5-authority-producer; do",
        1,
    ),
    (
        '"$controlled_dir/stage8b-r2a5-authority-issuer"',
        '"$production_dir/stage8b-r2a5-authority-issuer"',
        1,
    ),
    (
        'test -s "$signed_root/r2b-run-package.json"\n'
        'test ! -e /etc/moex-trading/stage8b/r2a5/r2b-run-package.json',
        '''test -s "$signed_root/r2b-run-package.json"
python3 - "$signed_root/r2b-run-package.json" \\
  "$repo_root/docs/stage-8/stage8b-p-r2b-generation2-production-authority.json" <<'PY'
import json,pathlib,sys
package=json.loads(pathlib.Path(sys.argv[1]).read_text())
authority=json.loads(pathlib.Path(sys.argv[2]).read_text())
if (
    package.get('authorization_status') != 'ISSUED'
    or package.get('account_key_generation_id') != '2'
    or package.get('trust_manifest_sha256') != authority.get('trust_manifest_sha256')
    or package.get('public_key_set_sha256') != authority.get('public_key_set_sha256')
    or package.get('account_key_manifest_sha256') != authority.get('account_key_manifest_sha256')
    or package.get('source_adapter_authority_sha256') != authority.get('source_adapter_authority_sha256')
):
    raise SystemExit('isolated Generation-2 package composition mismatch')
PY
test ! -e /etc/moex-trading/stage8b/r2a5/r2b-run-package.json''',
        1,
    ),
    (
        ' "authorization":"NOT_ISSUED",',
        ' "generation":2,\n "account_key_generation_id":"2",\n "generation_2_public_composition_selected":True,\n "isolated_rehearsal_package_generation":2,\n "isolated_rehearsal_package_signed":True,\n "production_authorization":"NOT_ISSUED",\n "authorization":"NOT_ISSUED",',
        1,
    ),
)


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: materializer OUTPUT")
    output = Path(sys.argv[1])
    if output.exists() or not output.parent.is_dir():
        raise SystemExit("stage8b-generation2-phase6-materializer: FAIL unsafe output")
    source = BASE.read_bytes()
    if hashlib.sha256(source).hexdigest() != BASE_SHA256:
        raise SystemExit("stage8b-generation2-phase6-materializer: FAIL accepted base drift")
    text = source.decode()
    for old, new, count in REPLACEMENTS:
        if text.count(old) != count:
            raise SystemExit(
                f"stage8b-generation2-phase6-materializer: FAIL replacement cardinality {old!r}"
            )
        text = text.replace(old, new)
    forbidden = (
        "stage8b-p-r2a5-production-trust-manifest.json",
        "stage8b-p-r2a5-production-account-key-manifest.json",
        "stage8b-p-r2a5-accepted-helper-authority.json",
        "stage8b-p-r2b-accepted-helper-sha256.txt",
        "generation-1.hex",
    )
    if any(marker in text for marker in forbidden):
        raise SystemExit("stage8b-generation2-phase6-materializer: FAIL generation-1 residue")
    output.write_text(text, encoding="utf-8")
    os.chmod(output, 0o700)
    print(
        "stage8b-generation2-phase6-materializer: PASS "
        "base=accepted-r0-r1a generation=2 authorization=NOT_ISSUED"
    )


if __name__ == "__main__":
    main()
