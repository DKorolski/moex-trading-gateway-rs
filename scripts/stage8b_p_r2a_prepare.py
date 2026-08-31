#!/usr/bin/env python3
"""Validate an exact R2 selection and emit a redacted no-GET plan.

This helper intentionally has no execution mode and imports no network/process
library. R2B owns any separately accepted GET-only invocation.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
RUN_AUTHORITY = ROOT / "docs/stage-8/stage8b-p-r1b-run-identity-authority.json"
R2A_AUTHORITY = ROOT / "docs/stage-8/stage8b-p-r2a-readonly-preflight-authority.json"
MAX_SELECTION_BYTES = 256 * 1024


class SelectionError(ValueError):
    pass


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise SelectionError(f"duplicate key: {key}")
        result[key] = value
    return result


def read_regular(path: Path, maximum: int) -> bytes:
    if not path.is_absolute():
        raise SelectionError("selection and executable paths must be absolute")
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
        raise SelectionError("input must be a single-link regular file")
    if before.st_size > maximum:
        raise SelectionError("input exceeds size limit")
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    try:
        opened = os.fstat(descriptor)
        identity = (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns)
        if identity != (opened.st_dev, opened.st_ino, opened.st_size, opened.st_mtime_ns):
            raise SelectionError("input identity changed during open")
        data = bytearray()
        while len(data) <= maximum:
            chunk = os.read(descriptor, min(65536, maximum + 1 - len(data)))
            if not chunk:
                break
            data.extend(chunk)
        after = os.fstat(descriptor)
        path_after = path.lstat()
        if len(data) != before.st_size or identity != (
            after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns
        ) or identity != (
            path_after.st_dev, path_after.st_ino, path_after.st_size, path_after.st_mtime_ns
        ):
            raise SelectionError("input identity drifted during read")
        return bytes(data)
    finally:
        os.close(descriptor)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def digest_parts(domain: str, values: list[str]) -> str:
    digest = hashlib.sha256()
    digest.update(domain.encode("ascii"))
    for value in values:
        encoded = value.encode("ascii")
        digest.update(len(encoded).to_bytes(8, "big"))
        digest.update(encoded)
    return digest.hexdigest()


def is_lower_sha256(value: Any) -> bool:
    return isinstance(value, str) and len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def validate_manifest(manifest: Any, run: dict[str, Any]) -> str:
    if not isinstance(manifest, dict):
        raise SelectionError("selection must be an object")
    operation = manifest.get("operation")
    if operation not in ("PLACE", "CANCEL"):
        raise SelectionError("operation must be PLACE or CANCEL")
    identity = run["run_identity"]
    common = identity["common_fields_in_exact_order_excluding_run_identity"]
    variant = identity["place_fields_in_exact_order"] if operation == "PLACE" else identity["cancel_fields_in_exact_order"]
    expected = set(common + variant + ["run_identity_sha256"])
    if set(manifest) != expected:
        raise SelectionError("selection field inventory mismatch")
    if any(not isinstance(manifest[key], str) or not manifest[key].isascii() for key in expected):
        raise SelectionError("all selection values must be ASCII strings")
    if operation == "PLACE" and (
        manifest["instrument"] != "IMOEXF@RTSX"
        or manifest["order_type"] != "ORDER_TYPE_LIMIT"
        or manifest["time_in_force"] != "TIME_IN_FORCE_DAY"
        or manifest["quantity"] != "1"
    ):
        raise SelectionError("PLACE policy mismatch")
    for key in expected:
        if key.endswith("_sha256") and not is_lower_sha256(manifest[key]):
            raise SelectionError(f"invalid digest: {key}")
    computed = digest_parts(identity["domain_utf8"], [manifest[key] for key in common + variant])
    if manifest["run_identity_sha256"] != computed:
        raise SelectionError("run identity mismatch")
    return operation


def validate_executable(path: Path, expected_sha256: str) -> None:
    data = read_regular(path, 64 * 1024 * 1024)
    if sha256(data) != expected_sha256:
        raise SelectionError("qualified executable digest mismatch")


def self_test(run: dict[str, Any]) -> None:
    for operation in ("PLACE", "CANCEL"):
        golden = run["golden_vectors"][operation]
        manifest = dict(golden["manifest_without_run_identity_sha256"])
        manifest["run_identity_sha256"] = golden["run_identity_sha256"]
        if validate_manifest(manifest, run) != operation:
            raise SelectionError("golden operation mismatch")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--selection", type=Path)
    parser.add_argument("--qualified-executable", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    run = json.loads(RUN_AUTHORITY.read_text(), object_pairs_hook=unique_object)
    authority = json.loads(R2A_AUTHORITY.read_text(), object_pairs_hook=unique_object)
    self_test(run)
    if args.self_test and not any((args.selection, args.qualified_executable, args.output)):
        print("stage8b-p-r2a-prepare: PASS self_test=2/2 broker_get=false authorization=NOT_ISSUED")
        return
    if not all((args.selection, args.qualified_executable, args.output)):
        raise SelectionError("selection, qualified executable and output are required together")

    selection_bytes = read_regular(args.selection, MAX_SELECTION_BYTES)
    selection = json.loads(selection_bytes, object_pairs_hook=unique_object)
    operation = validate_manifest(selection, run)
    expected_executable = authority["qualified_executable"]["executable_sha256"]
    validate_executable(args.qualified_executable, expected_executable)
    plan = {
        "schema_version": 1,
        "stage": "8B-P",
        "revision": "R2A",
        "status": "VALIDATED_NO_GET_PLAN",
        "operation": operation,
        "selection_sha256": sha256(selection_bytes),
        "run_identity_sha256": selection["run_identity_sha256"],
        "qualified_executable_sha256": expected_executable,
        "max_get_requests": 4,
        "broker_get_sent": False,
        "credential_used": False,
        "operator_arm_issued": False,
        "dispatch_attempt_recorded": False,
        "effect_transport_entered": False,
        "finam_post_delete_sent": False,
        "authorization_status": "NOT_ISSUED",
        "next": "independent R2A acceptance then explicit R2B operator decision",
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n")
    print("stage8b-p-r2a-prepare: PASS plan_only=true broker_get=false authorization=NOT_ISSUED")


if __name__ == "__main__":
    try:
        main()
    except (OSError, KeyError, UnicodeError, json.JSONDecodeError, SelectionError) as error:
        raise SystemExit(f"stage8b-p-r2a-prepare: FAIL {error}")
