#!/usr/bin/env python3
"""Tamper matrix for the immutable Trust Rebind R0-R1 receipt handoff."""

from __future__ import annotations

import copy
import json
import sys
import tempfile
import zipfile
from pathlib import Path
from typing import Callable

import stage8b_p_r2b_trust_rebind_r0_handoff_safety_check as safety


Mutation = Callable[[dict[str, bytes]], None]


def mutate_json(members: dict[str, bytes], name: str, keys: tuple[str, ...], replacement: object) -> None:
    value = json.loads(members[name])
    cursor = value
    for key in keys[:-1]:
        cursor = cursor[key]
    cursor[keys[-1]] = replacement
    members[name] = (json.dumps(value, indent=2) + "\n").encode()


def remove_receipt(members: dict[str, bytes]) -> None:
    del members[safety.RECEIPT]


def remove_receipt_binding(members: dict[str, bytes]) -> None:
    value = json.loads(members[safety.EVIDENCE])
    del value["ceremony_verification_receipt_sha256"]
    members[safety.EVIDENCE] = (json.dumps(value, indent=2) + "\n").encode()


def mark_verifier_not_run(members: dict[str, bytes]) -> None:
    members[safety.GATE] = members[safety.GATE].replace(
        b"actual_ceremony_verifier=PASS", b"actual_ceremony_verifier=NOT_RUN", 1
    )


def drift_signature(members: dict[str, bytes]) -> None:
    value = json.loads(members[safety.RECEIPT])
    signature = value["signature_ed25519_hex"]
    value["signature_ed25519_hex"] = ("0" if signature[0] != "0" else "1") + signature[1:]
    members[safety.RECEIPT] = (json.dumps(value, indent=2) + "\n").encode()


CASES: list[tuple[str, Mutation]] = [
    ("actual-ceremony-verifier-not-run", mark_verifier_not_run),
    ("ceremony-verification-receipt-missing", remove_receipt),
    ("ceremony-receipt-source-ref-drift", lambda m: mutate_json(m, safety.RECEIPT, ("source_ref",), "0" * 40)),
    ("ceremony-receipt-generation-drift", lambda m: mutate_json(m, safety.RECEIPT, ("generation",), 1)),
    ("ceremony-receipt-trust-hash-drift", lambda m: mutate_json(m, safety.RECEIPT, ("trust_manifest_sha256",), "0" * 64)),
    ("ceremony-receipt-account-hash-drift", lambda m: mutate_json(m, safety.RECEIPT, ("account_key_manifest_sha256",), "0" * 64)),
    ("ceremony-receipt-binding-count-drift", lambda m: mutate_json(m, safety.RECEIPT, ("private_public_bindings_verified",), 12)),
    ("ceremony-receipt-private-path-exported", lambda m: mutate_json(m, safety.RECEIPT, ("private_path_recorded",), True)),
    ("hardcoded-binding-count-without-receipt", remove_receipt_binding),
    ("receipt-signature-drift", drift_signature),
]


def write_mutation(source: Path, destination: Path, mutation: Mutation) -> None:
    with zipfile.ZipFile(source) as archive:
        infos = archive.infolist()
        members = {item.filename: archive.read(item.filename) for item in infos}
    mutation(members)
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for info in infos:
            if info.filename not in members:
                continue
            archive.writestr(copy.copy(info), members[info.filename])


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p_r2b_trust_rebind_r0_handoff_negative_harness.py ARCHIVE")
    source = Path(sys.argv[1]).resolve()
    safety.check(str(source))
    passed = 0
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-trust-rebind-receipt-{name}-") as temporary:
            candidate = Path(temporary) / source.name
            write_mutation(source, candidate, mutation)
            try:
                safety.check(str(candidate))
            except (KeyError, OSError, ValueError, zipfile.BadZipFile, json.JSONDecodeError):
                passed += 1
                print(f"PASS {name}")
                continue
            raise SystemExit(f"stage8b-p-r2b-trust-rebind-r0-handoff-negative: FAIL accepted {name}")
    print(f"stage8b-p-r2b-trust-rebind-r0-handoff-negative: PASS {passed}/{len(CASES)}")


if __name__ == "__main__":
    main()
