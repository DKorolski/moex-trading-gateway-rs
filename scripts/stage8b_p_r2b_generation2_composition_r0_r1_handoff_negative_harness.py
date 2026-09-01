#!/usr/bin/env python3
"""Mutation coverage for the Generation-2 R0-R1 evidence handoff."""

from __future__ import annotations

import json
import tempfile
import zipfile
from pathlib import Path
from typing import Callable

import stage8b_p_r2b_generation2_composition_r0_r1_handoff_safety_check as safety


Mutation = Callable[[list[tuple[zipfile.ZipInfo, bytes]]], None]


def clone_info(source: zipfile.ZipInfo, name: str | None = None) -> zipfile.ZipInfo:
    target = zipfile.ZipInfo(name or source.filename, date_time=(1980, 1, 1, 0, 0, 0))
    target.create_system = 3
    target.compress_type = zipfile.ZIP_DEFLATED
    target.external_attr = source.external_attr
    return target


def replace(entries: list[tuple[zipfile.ZipInfo, bytes]], name: str, data: bytes) -> None:
    matches = [index for index, (item, _) in enumerate(entries) if item.filename == name]
    if len(matches) != 1:
        raise RuntimeError(f"fixture cardinality drift: {name}")
    index = matches[0]
    entries[index] = (entries[index][0], data)


def remove(entries: list[tuple[zipfile.ZipInfo, bytes]], name: str) -> None:
    before = len(entries)
    entries[:] = [(item, data) for item, data in entries if item.filename != name]
    if len(entries) != before - 1:
        raise RuntimeError(f"fixture cardinality drift: {name}")


def mutate_json(
    entries: list[tuple[zipfile.ZipInfo, bytes]],
    name: str,
    path: tuple[str, ...],
    value: object,
) -> None:
    raw = next(data for item, data in entries if item.filename == name)
    document = json.loads(raw)
    cursor = document
    for key in path[:-1]:
        cursor = cursor[key]
    cursor[path[-1]] = value
    replace(entries, name, (json.dumps(document, indent=2, sort_keys=True) + "\n").encode())


def mutate_marker(entries: list[tuple[zipfile.ZipInfo, bytes]], key: str, value: str) -> None:
    raw = next(data for item, data in entries if item.filename == "handoff-commit.txt")
    lines = raw.decode().splitlines()
    replacement = f"{key}={value}"
    matches = [index for index, line in enumerate(lines) if line.startswith(f"{key}=")]
    if len(matches) != 1:
        raise RuntimeError(f"marker cardinality drift: {key}")
    lines[matches[0]] = replacement
    replace(entries, "handoff-commit.txt", ("\n".join(lines) + "\n").encode())


def mutate_binary(entries: list[tuple[zipfile.ZipInfo, bytes]]) -> None:
    name = next(
        item.filename
        for item, _ in entries
        if item.filename.startswith(f"{safety.ARTIFACT_ROOT}/build-a/")
    )
    raw = next(data for item, data in entries if item.filename == name)
    replace(entries, name, raw[:-1] + bytes([raw[-1] ^ 1]))


def mutate_source(entries: list[tuple[zipfile.ZipInfo, bytes]]) -> None:
    raw = next(data for item, data in entries if item.filename == "README.md")
    replace(entries, "README.md", raw + b"\nmutation\n")


def mutate_gate(entries: list[tuple[zipfile.ZipInfo, bytes]]) -> None:
    raw = next(data for item, data in entries if item.filename == safety.GATE)
    replace(entries, safety.GATE, raw.replace(b"PASS", b"FAIL", 1))


def mutate_manifest(entries: list[tuple[zipfile.ZipInfo, bytes]]) -> None:
    mutate_json(entries, safety.MANIFEST, ("entries",), [])


def add_member(
    entries: list[tuple[zipfile.ZipInfo, bytes]],
    name: str,
    data: bytes,
    mode: int = 0o100644,
) -> None:
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.create_system = 3
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = mode << 16
    entries.append((info, data))


CASES: tuple[tuple[str, Mutation], ...] = (
    ("duplicate-member", lambda e: e.append((clone_info(e[0][0]), e[0][1]))),
    ("path-traversal", lambda e: add_member(e, "../escape", b"x")),
    ("symlink", lambda e: add_member(e, "handoff-evidence/link", b"target", 0o120777)),
    (
        "secret-member",
        lambda e: add_member(e, "handoff-evidence/package-authorization.ed25519", b"synthetic"),
    ),
    ("extra-generated-member", lambda e: add_member(e, "handoff-evidence/unbound.txt", b"x")),
    (
        "missing-binary",
        lambda e: remove(
            e,
            next(
                item.filename
                for item, _ in e
                if item.filename.startswith(f"{safety.ARTIFACT_ROOT}/build-b/")
            ),
        ),
    ),
    ("binary-byte", mutate_binary),
    ("tracked-source-byte", mutate_source),
    (
        "marker-source-ref",
        lambda e: mutate_marker(e, "source_ref", "0" * 40),
    ),
    (
        "marker-evidence-source-ref",
        lambda e: mutate_marker(e, "evidence_source_ref", "0" * 40),
    ),
    ("handoff-authorization", lambda e: mutate_json(e, safety.EVIDENCE, ("authorization",), "ISSUED")),
    ("handoff-active", lambda e: mutate_json(e, safety.EVIDENCE, ("generation_2_active",), True)),
    ("handoff-finam", lambda e: mutate_json(e, safety.EVIDENCE, ("finam_endpoint_called",), True)),
    (
        "handoff-category-only",
        lambda e: mutate_json(e, safety.EVIDENCE, ("category_only_oracle",), True),
    ),
    ("handoff-request-ordinal", lambda e: mutate_json(e, safety.EVIDENCE, ("request_ordinal",), 2)),
    ("handoff-request-method", lambda e: mutate_json(e, safety.EVIDENCE, ("request_method",), "GET")),
    (
        "handoff-request-route",
        lambda e: mutate_json(e, safety.EVIDENCE, ("request_route_template",), "/v1/orders"),
    ),
    (
        "handoff-request-error",
        lambda e: mutate_json(e, safety.EVIDENCE, ("request_error_category",), "AUTH_SESSION_FAILURE"),
    ),
    (
        "handoff-production-rebuild",
        lambda e: mutate_json(e, safety.EVIDENCE, ("production_binaries_rebuilt_in_r0_r1",), True),
    ),
    (
        "handoff-private-residue",
        lambda e: mutate_json(e, safety.EVIDENCE, ("private_ceremony_members",), 1),
    ),
    ("gate-result", mutate_gate),
    ("manifest-inventory", mutate_manifest),
)


def main() -> None:
    import sys

    if len(sys.argv) != 2:
        raise SystemExit(
            "usage: stage8b_p_r2b_generation2_composition_r0_r1_handoff_negative_harness.py ARCHIVE"
        )
    source = Path(sys.argv[1]).resolve()
    with zipfile.ZipFile(source) as archive:
        base = [(clone_info(item), archive.read(item.filename)) for item in archive.infolist()]
    passed = 0
    for name, mutation in CASES:
        entries = [(clone_info(item), data) for item, data in base]
        mutation(entries)
        with tempfile.TemporaryDirectory(prefix=f"stage8b-g2-r0-r1-handoff-{name}-") as temporary:
            candidate = Path(temporary) / "candidate.zip"
            with zipfile.ZipFile(
                candidate,
                "w",
                compression=zipfile.ZIP_DEFLATED,
                compresslevel=1,
            ) as archive:
                for item, data in entries:
                    archive.writestr(item, data)
            try:
                safety.check(str(candidate))
            except (
                AssertionError,
                KeyError,
                OSError,
                RuntimeError,
                ValueError,
                zipfile.BadZipFile,
                json.JSONDecodeError,
            ):
                passed += 1
                print(f"PASS {name}")
                continue
            raise SystemExit(
                f"stage8b-generation2-composition-r0-r1-handoff-negative: FAIL accepted={name}"
            )
    print(
        "stage8b-generation2-composition-r0-r1-handoff-negative: "
        f"PASS cases={passed}/{len(CASES)}"
    )


if __name__ == "__main__":
    main()
