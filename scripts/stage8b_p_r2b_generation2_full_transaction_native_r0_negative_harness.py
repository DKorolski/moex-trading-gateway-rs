#!/usr/bin/env python3
"""Targeted mutations for the Generation-2 full transaction rebind R0."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path
from typing import Callable

import stage8b_p_r2b_generation2_full_transaction_native_r0_check as checker


Mutation = Callable[[Path], None]


def mutate_json(root: Path, path: Path, keys: tuple[object, ...], value: object) -> None:
    document = json.loads((root / path).read_text(encoding="utf-8"))
    cursor = document
    for key in keys[:-1]:
        cursor = cursor[key]
    cursor[keys[-1]] = value
    (root / path).write_text(
        json.dumps(document, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def mutate_unit(root: Path) -> None:
    contract = json.loads((root / checker.CONTRACT).read_text(encoding="utf-8"))
    relative = Path(next(iter(contract["unit_file_sha256"])))
    with (root / relative).open("a", encoding="utf-8") as handle:
        handle.write("\n# mutation\n")


def mutate_text(root: Path, path: Path, old: str, new: str) -> None:
    target = root / path
    text = target.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise RuntimeError(f"mutation marker drift: {path} {old!r}")
    target.write_text(text.replace(old, new), encoding="utf-8")


CASES: tuple[tuple[str, Mutation], ...] = (
    (
        "accepted-composition-ref",
        lambda r: mutate_json(r, checker.CONTRACT, ("accepted_composition_r0_r1", "source_ref"), "0" * 40),
    ),
    (
        "accepted-composition-archive",
        lambda r: mutate_json(r, checker.CONTRACT, ("accepted_composition_r0_r1", "archive_sha256"), "0" * 64),
    ),
    (
        "legacy-contract-binding",
        lambda r: mutate_json(r, checker.CONTRACT, ("inherited_transaction", "sha256"), "0" * 64),
    ),
    (
        "typed-preflight-binding",
        lambda r: mutate_json(r, checker.CONTRACT, ("inherited_preflight", "request_oracle"), "CATEGORY_ONLY"),
    ),
    ("phase-count", lambda r: mutate_json(r, checker.CONTRACT, ("phase_count",), 5)),
    (
        "service-count",
        lambda r: mutate_json(r, checker.CONTRACT, ("service_invocation_count",), 30),
    ),
    (
        "phase-order",
        lambda r: mutate_json(r, checker.CONTRACT, ("phases", 1, "predecessor_target"), None),
    ),
    (
        "source-order",
        lambda r: mutate_json(r, checker.CONTRACT, ("source_instances", 0), "durable_micro_budget"),
    ),
    ("unit-content", mutate_unit),
    (
        "unit-binding",
        lambda r: mutate_json(
            r,
            checker.CONTRACT,
            ("unit_file_sha256", next(iter(checker.legacy.load(checker.ROOT, checker.LEGACY_CONTRACT)["unit_file_sha256"]))),
            "0" * 64,
        ),
    ),
    (
        "upstream-binary",
        lambda r: mutate_json(
            r,
            checker.CONTRACT,
            ("production_linux_amd64_sha256", checker.UPSTREAM_NAMES[0]),
            "0" * 64,
        ),
    ),
    (
        "generation2-binary",
        lambda r: mutate_json(
            r,
            checker.CONTRACT,
            ("production_linux_amd64_sha256", "stage8b-r2b-launcher"),
            "0" * 64,
        ),
    ),
    (
        "production-rebuild-claimed",
        lambda r: mutate_json(
            r,
            checker.CONTRACT,
            ("binary_lineage", "phase3_phase6", "production_binaries_rebuilt_by_this_stage"),
            True,
        ),
    ),
    (
        "generation-authority",
        lambda r: mutate_json(r, checker.CONTRACT, ("generation2_public_authority", "generation"), 1),
    ),
    (
        "generation-active",
        lambda r: mutate_json(r, checker.CONTRACT, ("closed_surfaces", "generation_2_active"), True),
    ),
    (
        "authorization-issued",
        lambda r: mutate_json(r, checker.CONTRACT, ("closed_surfaces", "authorization_issued"), True),
    ),
    (
        "native-not-required",
        lambda r: mutate_json(r, checker.CONTRACT, ("proof_requirements", "native_execution_required"), False),
    ),
    (
        "qemu-allowed",
        lambda r: mutate_json(r, checker.CONTRACT, ("proof_requirements", "qemu_emulation_allowed"), True),
    ),
    (
        "production-host-allowed",
        lambda r: mutate_json(r, checker.CONTRACT, ("proof_requirements", "production_account_host_allowed"), True),
    ),
    (
        "sensitive-cotenant-allowed",
        lambda r: mutate_json(r, checker.CONTRACT, ("proof_requirements", "sensitive_cotenant_allowed"), True),
    ),
    (
        "network-opened",
        lambda r: mutate_json(r, checker.CONTRACT, ("proof_requirements", "container_network_mode"), "bridge"),
    ),
    (
        "finam-opened",
        lambda r: mutate_json(r, checker.CONTRACT, ("proof_requirements", "finam_network_allowed"), True),
    ),
    (
        "timeout-stage-not-exact",
        lambda r: mutate_json(
            r,
            checker.CONTRACT,
            ("proof_requirements", "timeout_stage_exact_request_required"),
            False,
        ),
    ),
    (
        "raw-terminal-optional",
        lambda r: mutate_json(
            r,
            checker.CONTRACT,
            ("proof_requirements", "raw_redacted_root_terminal_required"),
            False,
        ),
    ),
    (
        "journal-optional",
        lambda r: mutate_json(
            r,
            checker.CONTRACT,
            ("proof_requirements", "redacted_helper_journal_required"),
            False,
        ),
    ),
    (
        "second-run-optional",
        lambda r: mutate_json(r, checker.CONTRACT, ("proof_requirements", "clean_second_run_required"), False),
    ),
    (
        "private-export-allowed",
        lambda r: mutate_json(r, checker.CONTRACT, ("proof_requirements", "private_material_export_allowed"), True),
    ),
    (
        "cleanup-optional",
        lambda r: mutate_json(r, checker.CONTRACT, ("proof_requirements", "post_proof_uninstall_required"), False),
    ),
    (
        "next-step-opened",
        lambda r: mutate_json(r, checker.CONTRACT, ("next_allowed_step",), "REAL_READONLY_RUN"),
    ),
    (
        "authority-predecessor",
        lambda r: mutate_json(r, checker.AUTHORITY, ("accepted_predecessor", "source_ref"), "0" * 40),
    ),
    (
        "authority-artifact-binding",
        lambda r: mutate_json(r, checker.AUTHORITY, ("artifacts", checker.CONTRACT.as_posix()), "0" * 64),
    ),
    (
        "authority-negative-count",
        lambda r: mutate_json(r, checker.AUTHORITY, ("static_rebind", "negative_cases"), 38),
    ),
    (
        "authority-eligible-host-removed",
        lambda r: mutate_json(
            r,
            checker.AUTHORITY,
            ("host_assessment", "eligible_disposable_linux_amd64_host_identified"),
            False,
        ),
    ),
    (
        "authority-broker-vps-falsely-eligible",
        lambda r: mutate_json(r, checker.AUTHORITY, ("host_assessment", "known_broker_vps_native_proof_eligible"), True),
    ),
    (
        "authority-execution-falsely-started",
        lambda r: mutate_json(r, checker.AUTHORITY, ("execution_state", "native_container_created"), True),
    ),
    (
        "authority-generation-active",
        lambda r: mutate_json(r, checker.AUTHORITY, ("activation", "generation_2_active"), True),
    ),
    (
        "authority-issued",
        lambda r: mutate_json(r, checker.AUTHORITY, ("activation", "authorization"), "ISSUED"),
    ),
    (
        "authority-finam-opened",
        lambda r: mutate_json(r, checker.AUTHORITY, ("closed_surfaces", "external_finam_network"), True),
    ),
    (
        "authority-next-step-opened",
        lambda r: mutate_json(r, checker.AUTHORITY, ("next_allowed_step",), "CREATE_CONTAINER"),
    ),
    (
        "authority-ceremony-exported",
        lambda r: mutate_json(r, checker.AUTHORITY, ("ceremony_custody", "private_material_exported"), True),
    ),
    (
        "proof-tool-hash",
        lambda r: mutate_json(
            r,
            checker.CONTRACT,
            ("proof_tool_linux_amd64_sha256", "stage8b-r2a5-controlled-layout"),
            "0" * 64,
        ),
    ),
    (
        "runner-network-opened",
        lambda r: mutate_text(r, checker.NATIVE_RUNNER, "--network none", "--network host"),
    ),
    (
        "container-runner-execstart-substitution",
        lambda r: mutate_text(
            r,
            checker.CONTAINER_RUNNER,
            "\ninstall_payload() {",
            "\n# ExecStart= proof bypass\ninstall_payload() {",
        ),
    ),
)


def main() -> None:
    passed = 0
    required = checker.contract_required_paths(checker.ROOT)
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-g2-full-native-{name}-") as temporary:
            root = Path(temporary)
            for relative in required:
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(checker.ROOT / relative, target)
            mutation(root)
            try:
                checker.check_contract(root)
            except (KeyError, OSError, RuntimeError, ValueError, json.JSONDecodeError):
                passed += 1
                print(f"PASS {name}")
                continue
            raise SystemExit(
                f"stage8b-generation2-full-transaction-native-r0-negative: FAIL accepted={name}"
            )
    print(
        "stage8b-generation2-full-transaction-native-r0-negative: "
        f"PASS cases={passed}/{len(CASES)}"
    )


if __name__ == "__main__":
    main()
