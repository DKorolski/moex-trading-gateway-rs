#!/usr/bin/env python3
"""Tamper matrix for the Generation-2 backup/restore immutable handoff."""

from __future__ import annotations

import copy
import json
import sys
import tempfile
import zipfile
from pathlib import Path
from typing import Callable

import stage8b_p_r2b_generation2_backup_restore_r0_handoff_safety_check as safety


Mutation = Callable[[dict[str, bytes]], None]


def mutate_json(
    members: dict[str, bytes], name: str, keys: tuple[str, ...], replacement: object
) -> None:
    value = json.loads(members[name])
    cursor = value
    for key in keys[:-1]:
        cursor = cursor[key]
    cursor[keys[-1]] = replacement
    members[name] = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def mutate_marker(members: dict[str, bytes], key: str, replacement: str) -> None:
    lines = members["handoff-commit.txt"].decode().splitlines()
    members["handoff-commit.txt"] = (
        "\n".join(replacement if line.startswith(f"{key}=") else line for line in lines) + "\n"
    ).encode()


def flip_signature(members: dict[str, bytes], name: str) -> None:
    value = json.loads(members[name])
    signature = value["signature_ed25519_hex"]
    value["signature_ed25519_hex"] = ("0" if signature[0] != "0" else "1") + signature[1:]
    members[name] = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def add_member(members: dict[str, bytes], name: str, data: bytes) -> None:
    members[name] = data


CASES: tuple[tuple[str, Mutation], ...] = (
    ("final-source-ref", lambda m: mutate_json(m, safety.EVIDENCE, ("source_ref",), "0" * 40)),
    ("operation-source-ref", lambda m: mutate_json(m, safety.EVIDENCE, ("operation_source_ref",), "0" * 40)),
    ("operation-source-tree", lambda m: mutate_marker(m, "operation_source_tree", "operation_source_tree=" + "0" * 40)),
    ("archive-name", lambda m: mutate_marker(m, "archive_name", "archive_name=wrong.zip")),
    ("gate-body", lambda m: m.__setitem__(safety.GATE, m[safety.GATE] + b"tamper\n")),
    ("manifest-body", lambda m: m.__setitem__(safety.MANIFEST, m[safety.MANIFEST] + b"\n")),
    ("restore-receipt-digest", lambda m: mutate_json(m, safety.EVIDENCE, ("restore_receipt_sha256",), "0" * 64)),
    ("destruction-receipt-digest", lambda m: mutate_json(m, safety.EVIDENCE, ("destruction_receipt_sha256",), "0" * 64)),
    ("restore-signature", lambda m: flip_signature(m, safety.RESTORE)),
    ("destruction-signature", lambda m: flip_signature(m, safety.DESTRUCTION)),
    ("backup-status", lambda m: mutate_json(m, safety.EVIDENCE, ("encrypted_backup", "status"), "PENDING")),
    ("ciphertext-claimed-present", lambda m: mutate_json(m, safety.EVIDENCE, ("encrypted_backup", "included_in_handoff"), True)),
    ("private-identity-claimed-present", lambda m: mutate_json(m, safety.EVIDENCE, ("private_material", "recovery_identity_in_handoff"), True)),
    ("generation-active", lambda m: mutate_json(m, safety.EVIDENCE, ("closed_surfaces", "generation_2_active"), True)),
    ("authorization-issued", lambda m: mutate_json(m, safety.EVIDENCE, ("authorization",), "ISSUED")),
    ("finam-open", lambda m: mutate_json(m, safety.EVIDENCE, ("closed_surfaces", "finam_network"), True)),
    ("authority-active", lambda m: mutate_json(m, safety.AUTHORITY, ("activation", "generation_2_active"), True)),
    ("ciphertext-member", lambda m: add_member(m, "handoff-evidence/unexpected.tar.age", b"ciphertext")),
    ("identity-member", lambda m: add_member(m, "handoff-evidence/recovery.agekey", b"identity")),
    ("ceremony-seed-member", lambda m: add_member(m, "issuer-private-keys/x/key.ed25519", b"seed")),
    ("private-identity-value", lambda m: add_member(m, "handoff-evidence/leak.txt", b"AGE-SECRET-" + b"KEY-test")),
    ("primary-path", lambda m: add_member(m, "handoff-evidence/path.txt", b"/Users/" + b"denisq/.config/moex-trading/stage8b/r2b-trust-rebind-generation-" + b"2-20260830")),
    ("recovery-path", lambda m: add_member(m, "handoff-evidence/path.txt", b"/Users/" + b"denisq/Documents/moex-trading-ceremony-" + b"secret")),
    ("external-volume-path", lambda m: add_member(m, "handoff-evidence/path.txt", b"/Volumes/" + b"TRAN" + b"SCEND")),
    ("unexpected-generated-member", lambda m: add_member(m, "handoff-evidence/unexpected.txt", b"public")),
    ("missing-destruction-receipt", lambda m: m.pop(safety.DESTRUCTION)),
)


def write_mutation(source: Path, destination: Path, mutation: Mutation) -> None:
    with zipfile.ZipFile(source) as archive:
        infos = archive.infolist()
        members = {item.filename: archive.read(item.filename) for item in infos}
        original = {item.filename for item in infos}
    mutation(members)
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for info in infos:
            if info.filename in members:
                archive.writestr(copy.copy(info), members[info.filename])
        for name in sorted(set(members) - original):
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, members[name])


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(
            "usage: stage8b_p_r2b_generation2_backup_restore_r0_handoff_negative_harness.py ARCHIVE"
        )
    source = Path(sys.argv[1]).resolve()
    safety.check(str(source))
    passed = 0
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-g2-backup-handoff-{name}-") as temporary:
            candidate = Path(temporary) / source.name
            write_mutation(source, candidate, mutation)
            try:
                safety.check(str(candidate))
            except (KeyError, OSError, ValueError, zipfile.BadZipFile, json.JSONDecodeError):
                passed += 1
                print(f"PASS {name}")
                continue
            raise SystemExit(f"stage8b-generation2-backup-handoff-negative: FAIL accepted={name}")
    print(f"stage8b-generation2-backup-handoff-negative: PASS cases={passed}/{len(CASES)}")


if __name__ == "__main__":
    main()
