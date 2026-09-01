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


def mutate_all_text(root: Path, path: Path, old: str, new: str) -> None:
    target = root / path
    text = target.read_text(encoding="utf-8")
    if old not in text:
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
        "inherited-failure-replay-source",
        lambda r: mutate_json(r, checker.CONTRACT, ("inherited_failure_replay_proof", "source_ref"), "0" * 40),
    ),
    (
        "native-fault-matrix-falsely-required",
        lambda r: mutate_json(r, checker.CONTRACT, ("proof_requirements", "native_fault_injection_required"), True),
    ),
    (
        "native-stale-replay-falsely-required",
        lambda r: mutate_json(r, checker.CONTRACT, ("proof_requirements", "native_stale_replay_attempt_required"), True),
    ),
    (
        "docker-image-id-drift",
        lambda r: mutate_json(r, checker.CONTRACT, ("container_image", "image_id"), "sha256:" + "0" * 64),
    ),
    (
        "docker-image-rebuild-opened",
        lambda r: mutate_json(r, checker.CONTRACT, ("container_image", "rebuild_under_same_tag_allowed"), True),
    ),
    (
        "ceremony-verifier-binary-drift",
        lambda r: mutate_json(r, checker.CONTRACT, ("ceremony_verifier", "linux_amd64_sha256"), "0" * 64),
    ),
    (
        "phase2-manifest-unit-not-installed",
        lambda r: mutate_text(
            r, checker.CONTAINER_RUNNER,
            '  install -m 0644 "$repo_root/deploy/stage8b-r2a5/stage8b-r2a8-current-manifest-issuer.service" /etc/systemd/system/\n', "",
        ),
    ),
    (
        "phase2-source-adapter-unit-not-installed",
        lambda r: mutate_text(
            r, checker.CONTAINER_RUNNER,
            '  install -m 0644 "$repo_root/deploy/stage8b-r2a5/stage8b-r2a7-source-adapter.service" /etc/systemd/system/\n', "",
        ),
    ),
    (
        "root-terminal-durability-lost",
        lambda r: mutate_text(r, checker.CONTAINER_RUNNER, "install -o root -g root -m 0400", "install -o root -g root -m 0600"),
    ),
    (
        "review-archive-sha-not-recomputed",
        lambda r: mutate_text(r, checker.REVIEW_ARCHIVE, "actual_archive_sha256 = digest(archive_path)", "actual_archive_sha256 = expected_archive_sha256"),
    ),
    (
        "ceremony-private-der-tempfile",
        lambda r: mutate_text(r, checker.CEREMONY_PREFLIGHT, '"""Metadata-only custody preflight', 'import tempfile\n"""Metadata-only custody preflight'),
    ),
    (
        "host-negative-handoff-artifact-root-missing",
        lambda r: mutate_text(r, checker.HOST_NEGATIVE, 'handoff-evidence/linux-amd64/exact-binaries', 'missing/exact-binaries'),
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
    (
        "fixed-ceremony-source-path-drift",
        lambda r: mutate_text(r, checker.NATIVE_RUNNER, "/run/stage8b-g2-ceremony-source", "/tmp/stage8b-g2-ceremony-source"),
    ),
    (
        "early-cleanup-trap-removed",
        lambda r: mutate_text(r, checker.NATIVE_RUNNER, "trap global_custody_cleanup EXIT", "# removed global custody trap"),
    ),
    (
        "host-swap-check-removed",
        lambda r: mutate_text(r, checker.NATIVE_RUNNER, 'host_swap_entries="$(swapon --show --noheadings 2>/dev/null)"', 'host_swap_entries=""'),
    ),
    (
        "host-swap-accepted",
        lambda r: mutate_text(r, checker.NATIVE_RUNNER, '[[ -z "$host_swap_entries" ]]', ': # swap accepted'),
    ),
    (
        "fixed-source-one-filesystem-removed",
        lambda r: mutate_text(r, checker.NATIVE_RUNNER, 'rm -rf --one-file-system -- "$fixed_ceremony_root"', 'rm -rf -- "$fixed_ceremony_root"'),
    ),
    (
        "cleanup-failure-not-fatal",
        lambda r: mutate_text(r, checker.NATIVE_RUNNER, "destroy_fixed_ceremony_source || status=1", "destroy_fixed_ceremony_source || true"),
    ),
    (
        "container-cleanup-failure-not-fatal",
        lambda r: mutate_text(r, checker.NATIVE_RUNNER, "remove_proof_container || status=1", "remove_proof_container || true"),
    ),
    (
        "cleanup-exit-status-not-preserved",
        lambda r: mutate_text(r, checker.NATIVE_RUNNER, 'exit "$status"', "exit 0"),
    ),
    (
        "proof-container-name-rebindable",
        lambda r: mutate_text(r, checker.NATIVE_RUNNER, "readonly proof_container=stage8b-g2-native-proof-r2", 'proof_container="${STAGE8B_G2_RUN_LABEL}"'),
    ),
    (
        "container-visible-swap-check-removed",
        lambda r: mutate_text(r, checker.NATIVE_RUNNER, 'container_swap_entries="$(docker exec', 'removed_container_swap_entries="$(docker exec'),
    ),
    (
        "container-visible-swap-accepted",
        lambda r: mutate_text(r, checker.NATIVE_RUNNER, '[[ "$container_swap_entries" = 0 ]]', ': # container swap accepted'),
    ),
    (
        "swap-custody-evidence-removed",
        lambda r: mutate_all_text(r, checker.NATIVE_RUNNER, "swap-custody-preflight.json", "removed-swap-custody-preflight.json"),
    ),
    (
        "host-swap-contract-opened",
        lambda r: mutate_json(r, checker.CONTRACT, ("proof_requirements", "host_swap_entries_required"), 1),
    ),
    (
        "container-swap-contract-opened",
        lambda r: mutate_json(r, checker.CONTRACT, ("proof_requirements", "container_visible_swap_entries_required"), 1),
    ),
    (
        "host-attestation-age-opened",
        lambda r: mutate_json(r, checker.CONTRACT, ("proof_requirements", "host_attestation_max_age_seconds"), 86400),
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
