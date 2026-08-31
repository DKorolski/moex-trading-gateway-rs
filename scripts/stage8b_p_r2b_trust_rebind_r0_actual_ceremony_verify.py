#!/usr/bin/env python3
"""Run the exact private ceremony verifier and emit a redacted public receipt."""

from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path

import stage8b_p_r2b_trust_rebind_r0_receipt as receipt_contract


ROOT = Path(__file__).resolve().parents[1]
CEREMONY_ENV = "STAGE8B_R2B_TRUST_REBIND_CEREMONY_DIR"
OUTPUT_ENV = "STAGE8B_R2B_TRUST_REBIND_RECEIPT_OUT"


def run(*arguments: str) -> str:
    return subprocess.check_output(arguments, cwd=ROOT, text=True).strip()


def main() -> None:
    ceremony = os.environ.get(CEREMONY_ENV)
    output_text = os.environ.get(OUTPUT_ENV)
    if not ceremony:
        raise SystemExit("stage8b-p-r2b-trust-rebind-r0-actual-verify: FAIL missing local ceremony environment")
    if not output_text:
        raise SystemExit("stage8b-p-r2b-trust-rebind-r0-actual-verify: FAIL missing receipt output environment")
    output = Path(output_text)
    if output.exists() or not output.parent.is_dir():
        raise SystemExit("stage8b-p-r2b-trust-rebind-r0-actual-verify: FAIL unsafe receipt output")

    source_ref = run("git", "rev-parse", "HEAD")
    verifier_hash = receipt_contract.verifier_source_sha256(ROOT)
    verified_at = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    environment = os.environ.copy()
    environment["STAGE8B_R2B_TRUST_REBIND_SOURCE_REF"] = source_ref
    environment["STAGE8B_R2B_TRUST_REBIND_VERIFIED_AT_UTC"] = verified_at
    environment["STAGE8B_R2B_TRUST_REBIND_VERIFIER_SOURCE_SHA256"] = verifier_hash
    build_environment = environment.copy()
    for name in (
        CEREMONY_ENV,
        OUTPUT_ENV,
        "STAGE8B_R2B_TRUST_REBIND_SOURCE_REF",
        "STAGE8B_R2B_TRUST_REBIND_VERIFIED_AT_UTC",
        "STAGE8B_R2B_TRUST_REBIND_VERIFIER_SOURCE_SHA256",
    ):
        build_environment.pop(name, None)
    build = subprocess.run(
        [
            "cargo",
            "build",
            "--quiet",
            "--locked",
            "--manifest-path",
            "tools/stage8b-readonly-preflight/Cargo.toml",
            "--bin",
            "stage8b-r2b-trust-rebind-key-ceremony-verify",
        ],
        cwd=ROOT,
        env=build_environment,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    if build.returncode != 0:
        raise SystemExit("stage8b-p-r2b-trust-rebind-r0-actual-verify: FAIL verifier build failed")
    completed = subprocess.run(
        [
            str(
                ROOT
                / "tools/stage8b-readonly-preflight/target/debug/"
                "stage8b-r2b-trust-rebind-key-ceremony-verify"
            ),
        ],
        cwd=ROOT,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
        text=True,
    )
    if completed.returncode != 0:
        raise SystemExit("stage8b-p-r2b-trust-rebind-r0-actual-verify: FAIL exact verifier rejected ceremony")
    try:
        receipt = json.loads(completed.stdout)
        if not isinstance(receipt, dict):
            raise ValueError("receipt is not an object")
        receipt_contract.validate_receipt(receipt, ROOT, source_ref)
    except (KeyError, OSError, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2b-trust-rebind-r0-actual-verify: FAIL {error}") from error

    encoded = (json.dumps(receipt, indent=2, ensure_ascii=False) + "\n").encode()
    with tempfile.NamedTemporaryFile(dir=output.parent, prefix=".trust-rebind-receipt-", delete=False) as handle:
        temporary = Path(handle.name)
        handle.write(encoded)
        handle.flush()
        os.fsync(handle.fileno())
    os.chmod(temporary, 0o644)
    os.replace(temporary, output)
    digest = hashlib.sha256(encoded).hexdigest()
    print(
        "stage8b-p-r2b-trust-rebind-r0-actual-verify: PASS "
        f"receipt_sha256={digest} generation=2 bindings=13 account=1 private_path=false"
    )


if __name__ == "__main__":
    main()
