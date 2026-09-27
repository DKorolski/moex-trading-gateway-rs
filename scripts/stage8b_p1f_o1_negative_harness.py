#!/usr/bin/env python3
"""Mutation controls for the Stage 8B-P1-f O1 package contract."""

from __future__ import annotations

import copy
import sys

import stage8b_p1f_o1_check as check


def mutate(path: tuple[str, ...], value: object):
    document = check.read_json(check.ROOT / check.SPEC)
    cursor = document
    for key in path[:-1]:
        cursor = cursor[key]  # type: ignore[index]
    cursor[path[-1]] = value  # type: ignore[index]
    return document


def main() -> None:
    original = check.read_json(check.ROOT / check.SPEC)
    cases = [
        ("execution-authorized", mutate(("execution", "authorized"), True)),
        ("fresh-o0-removed", mutate(("execution", "requires_fresh_o0_preflight"), False)),
        ("daemon-reload-opened", mutate(("execution", "daemon_reload_allowed"), True)),
        ("unit-start-opened", mutate(("execution", "start_allowed"), True)),
        ("operator-material-added", mutate(("bundle_contract", "operator_material_included"), True)),
        ("credential-added", mutate(("bundle_contract", "credential_included"), True)),
        ("runtime-source-drift", mutate(("runtime_binary", "source_ref"), "0" * 40)),
        ("build-image-drift", mutate(("runtime_binary", "rust_image"), "rust:latest")),
        ("target-drift", mutate(("target", "ipv4"), "127.0.0.1")),
        ("fixed-install-drift", mutate(("accepted_fixed_install", "source_ref"), "0" * 40)),
        ("redis-opened", mutate(("closed_surfaces", "redis_db15_or_db0_mutation"), True)),
        ("real-orders-opened", mutate(("closed_surfaces", "real_orders"), True)),
    ]
    passed = 0
    for name, candidate in cases:
        try:
            check.validate_spec(candidate)
        except check.CheckError:
            passed += 1
            print(f"PASS {name}")
        else:
            print(f"FAIL mutation survived: {name}")
            raise SystemExit(1)
    check.validate_spec(copy.deepcopy(original))
    print(f"PASS stage8b-p1f-o1-negative-harness {passed}/{len(cases)}")


if __name__ == "__main__":
    main()
