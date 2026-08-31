#!/usr/bin/env python3
"""Issue one public Generation-2 helper acceptance without exporting custody."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs/stage-8"
CEREMONY_ENV = "STAGE8B_R2B_TRUST_REBIND_CEREMONY_DIR"
OUTPUT = DOCS / "stage8b-p-r2b-generation2-accepted-helper-authority.json"
HELPER_SHA = DOCS / "stage8b-p-r2b-generation2-accepted-helper-sha256.txt"
PREDECESSOR_HELPER = DOCS / "stage8b-p-r2a5-accepted-helper-authority.json"
TRUST = DOCS / "stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json"


def lower_hex(value: object, length: int) -> bool:
    return (
        isinstance(value, str)
        and len(value) == length
        and all(character in "0123456789abcdef" for character in value)
    )


def main() -> None:
    ceremony = os.environ.get(CEREMONY_ENV)
    if not ceremony:
        raise SystemExit("stage8b-generation2-helper-acceptance: FAIL missing local ceremony environment")
    if OUTPUT.exists():
        raise SystemExit("stage8b-generation2-helper-acceptance: FAIL output already exists")
    helper_sha256 = HELPER_SHA.read_text(encoding="utf-8").strip()
    predecessor = json.loads(PREDECESSOR_HELPER.read_text(encoding="utf-8"))
    trust = json.loads(TRUST.read_text(encoding="utf-8"))
    effect_sha256 = predecessor["effect_build_identity_sha256"]
    if not lower_hex(helper_sha256, 64) or not lower_hex(effect_sha256, 64):
        raise SystemExit("stage8b-generation2-helper-acceptance: FAIL public digest grammar")
    if trust["helper_acceptance_key"]["generation"] != 2:
        raise SystemExit("stage8b-generation2-helper-acceptance: FAIL trust generation")

    environment = os.environ.copy()
    environment["STAGE8B_R2B_GENERATION2_HELPER_SHA256"] = helper_sha256
    environment["STAGE8B_R2B_GENERATION2_EFFECT_SHA256"] = effect_sha256
    build_environment = environment.copy()
    for name in (
        CEREMONY_ENV,
        "STAGE8B_R2B_GENERATION2_HELPER_SHA256",
        "STAGE8B_R2B_GENERATION2_EFFECT_SHA256",
    ):
        build_environment.pop(name, None)
    completed = subprocess.run(
        [
            "cargo",
            "build",
            "--quiet",
            "--locked",
            "--manifest-path",
            "tools/stage8b-readonly-preflight/Cargo.toml",
            "--bin",
            "stage8b-r2b-generation2-helper-acceptance-issuer",
        ],
        cwd=ROOT,
        env=build_environment,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    if completed.returncode != 0:
        raise SystemExit("stage8b-generation2-helper-acceptance: FAIL issuer build")
    issuer = (
        ROOT
        / "tools/stage8b-readonly-preflight/target/debug/"
        "stage8b-r2b-generation2-helper-acceptance-issuer"
    )
    issued = subprocess.run(
        [str(issuer)],
        cwd=ROOT,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=False,
        text=True,
    )
    if issued.returncode != 0:
        raise SystemExit("stage8b-generation2-helper-acceptance: FAIL exact issuer rejected ceremony")
    try:
        authority = json.loads(issued.stdout)
    except json.JSONDecodeError as error:
        raise SystemExit("stage8b-generation2-helper-acceptance: FAIL invalid public output") from error
    expected = {
        "schema_version",
        "stage",
        "revision",
        "status",
        "helper_executable_sha256",
        "effect_build_identity_sha256",
        "valid_from_utc",
        "valid_until_utc",
        "acceptance_key_id",
        "signature_ed25519_hex",
    }
    if (
        set(authority) != expected
        or authority["helper_executable_sha256"] != helper_sha256
        or authority["effect_build_identity_sha256"] != effect_sha256
        or authority["acceptance_key_id"] != trust["helper_acceptance_key"]["key_id"]
        or not lower_hex(authority["signature_ed25519_hex"], 128)
    ):
        raise SystemExit("stage8b-generation2-helper-acceptance: FAIL public authority drift")
    # Preserve the Rust struct field order used by the signed preimage.  JSON
    # object order is not semantically significant to the Rust verifier, but
    # retaining this order also lets an independent checker reconstruct the
    # preimage directly from the reviewed public file.
    encoded = (json.dumps(authority, indent=2) + "\n").encode()
    with tempfile.NamedTemporaryFile(
        dir=OUTPUT.parent,
        prefix=".generation2-helper-acceptance-",
        delete=False,
    ) as handle:
        temporary = Path(handle.name)
        handle.write(encoded)
        handle.flush()
        os.fsync(handle.fileno())
    os.chmod(temporary, 0o644)
    os.replace(temporary, OUTPUT)
    print(
        "stage8b-generation2-helper-acceptance: PASS "
        f"authority_sha256={hashlib.sha256(encoded).hexdigest()} generation=2 "
        "private_path=false authorization=NOT_ISSUED"
    )


if __name__ == "__main__":
    main()
