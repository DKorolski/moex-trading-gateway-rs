#!/usr/bin/env python3
"""Independent isolated mutation harness for Stage 8B-I."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage8b_i_check.py"
AUTHORITY = Path("docs/stage-8/stage8b-i-authority.json")
FILES = (
    "docs/stage-8/stage8b-i-authority.json",
    "docs/stage-8/STAGE8B_I_IMPLEMENTATION_2026-08-22.md",
    "docs/stage-8/STAGE8B_I_ACCEPTANCE_MATRIX_2026-08-22.csv",
    "docs/stage-8/STAGE8B_I_NEGATIVE_INVENTORY_2026-08-22.md",
    "docs/stage-8/stage8b-spec-authority.json",
    "crates/finam-gateway/src/stage8b_no_send.rs",
    "crates/finam-gateway/src/lib.rs",
    "crates/finam-gateway/Cargo.toml",
    "crates/finam-gateway/src/stage8a1_execution_capability/stage8a2_builder_composition.rs",
    "crates/finam-gateway/src/stage8a1_execution_capability.rs",
    "crates/finam-gateway/src/stage8a3_endpoint_classifier.rs",
    "crates/broker-cli/src/lib.rs",
    "crates/broker-cli/tests/stage8b_i_no_send_facade.rs",
    "scripts/stage8b_i_external_compile_fail.sh",
    "scripts/stage8b_i_closed_surface_check.py",
    "scripts/stage8b_i_gate.sh",
    "scripts/stage8b_i_full_regression.sh",
    "scripts/stage8b_i_handoff_safety_check.py",
    "scripts/make_stage8b_i_handoff.py",
    "Cargo.lock",
)


def mutate_json(key: str, value: object) -> Callable[[Path], None]:
    def apply(root: Path) -> None:
        path = root / AUTHORITY
        payload = json.loads(path.read_text(encoding="utf-8"))
        if key not in payload:
            raise RuntimeError(f"missing authority key: {key}")
        payload[key] = value
        path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return apply


def replace(relative: str, old: str, new: str, *, all_occurrences: bool = False) -> Callable[[Path], None]:
    def apply(root: Path) -> None:
        path = root / relative
        text = path.read_text(encoding="utf-8")
        if old not in text:
            raise RuntimeError(f"missing mutation source in {relative}: {old}")
        path.write_text(text.replace(old, new) if all_occurrences else text.replace(old, new, 1), encoding="utf-8")
    return apply


CASES: tuple[tuple[str, Callable[[Path], None]], ...] = (
    ("s-candidate-drift", mutate_json("accepted_stage8b_s_r3_candidate", "0" * 40)),
    ("s-merge-drift", mutate_json("accepted_stage8b_s_r3_merge", "1" * 40)),
    ("s-tree-drift", mutate_json("accepted_stage8b_s_r3_tree", "2" * 40)),
    ("s-authority-drift", mutate_json("accepted_stage8b_s_authority_sha256", "3" * 64)),
    ("public-facade-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "pub fn invoke_stage8b_operator_once(", "fn removed_stage8b_operator_once(")),
    ("private-root-public", replace("crates/finam-gateway/src/stage8b_no_send.rs", "pub(crate) fn compose_stage8b_effect_authority(", "pub fn compose_stage8b_effect_authority(")),
    ("cli-positive-removed", replace("crates/broker-cli/tests/stage8b_i_no_send_facade.rs", "broker_cli_reaches_only_the_public_redacted_no_send_facade", "removed_positive_fixture")),
    ("compile-private-root-removed", replace("scripts/stage8b_i_external_compile_fail.sh", "check_fail private_root", "check_fail removed_private_root")),
    ("authority-fields-public", mutate_json("authority_fields_private", False)),
    ("authority-traits-open", mutate_json("authority_clone_copy_debug_serde_forbidden", False)),
    ("a2-source-drift", replace("crates/finam-gateway/src/stage8a1_execution_capability/stage8a2_builder_composition.rs", "//! Stage 8A-2", "//! drift\n//! Stage 8A-2")),
    ("builder-bridge-bypassed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "permit.capability.compose_stage8a2_no_send(&mut sink)", "removed_existing_builder_bridge(permit, &mut sink)")),
    ("a3-source-drift", replace("crates/finam-gateway/src/stage8a3_endpoint_classifier.rs", "//! Stage 8A-3", "//! drift\n//! Stage 8A-3")),
    ("classifier-bridge-bypassed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "context.classify(observation)", "removed_classifier_bridge(context, observation)")),
    ("hmac-constructor-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "Hmac::<Sha256>::new_from_slice", "removed_hmac_constructor", all_occurrences=True)),
    ("hmac-suffix-drift", replace("crates/finam-gateway/src/stage8b_no_send.rs", "message.push(0)", "message.push(1)", all_occurrences=True)),
    ("hmac-length-little-endian", replace("crates/finam-gateway/src/stage8b_no_send.rs", "length.to_be_bytes()", "length.to_le_bytes()", all_occurrences=True)),
    ("hmac-key-weakened", mutate_json("hmac_minimum_key_bytes", 16)),
    ("hmac-golden-drift", mutate_json("hmac_golden_digest", "4" * 64)),
    ("hmac-constant-time-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "mac.verify_slice(expected)", "removed_constant_time_verify(expected)")),
    ("zeroization-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "Zeroizing<Vec<u8>>", "Vec<u8>", all_occurrences=True)),
    ("absolute-path-open", mutate_json("absolute_paths_required", False)),
    ("symlink-open", mutate_json("symlink_components_rejected", False)),
    ("hardlink-open", replace("crates/finam-gateway/src/stage8b_no_send.rs", "path_before.nlink() != 1", "false")),
    ("nofollow-open", mutate_json("nofollow_descriptor_open", False)),
    ("identity-recheck-open", mutate_json("descriptor_path_identity_recheck", False)),
    ("path-swap-test-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "package_path_swap_after_open_is_rejected", "removed_path_swap_test")),
    ("manifest-openat-open", mutate_json("manifest_openat_child", False)),
    ("bounded-read-open", mutate_json("bounded_evidence_reads", False)),
    ("arm-o-excl-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "| libc::O_EXCL", "", all_occurrences=True)),
    ("arm-fsync-open", mutate_json("durable_arm_file_and_directory_fsync", False)),
    ("arm-race-test-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "two_processes_cannot_issue_two_arms", "removed_arm_race_test")),
    ("kill-count-reduced", mutate_json("kill_boundary_count", 4)),
    ("crash-count-reduced", mutate_json("crash_window_count", 5)),
    ("restart-prefix-reduced", mutate_json("durable_restart_prefix_count", 5)),
    ("impossible-replay-open", replace("crates/finam-gateway/src/stage8b_no_send.rs", "durable_rehearsal_rejects_impossible_or_corrupt_sequence", "removed_impossible_replay_test")),
    ("closure-count-reduced", mutate_json("closure_class_count", 4)),
    ("automatic-retry-open", mutate_json("automatic_retry_or_resend", True)),
    ("network-send-open", mutate_json("network_send_enabled", True)),
    ("acceptance-count-drift", mutate_json("acceptance_rows", 85)),
    ("builder-before-permit", replace("crates/finam-gateway/src/stage8b_no_send.rs", "let durable_request_sha256 = durable", "let _early = compose_stage8b_private_request_parts_from_stage8a2(capability);\n    let durable_request_sha256 = durable")),
    ("builder-without-permit", replace("crates/finam-gateway/src/stage8b_no_send.rs", "permit: Stage8bExactTransportPermit", "capability: Stage8a1CurrentlyAuthorizedCapability")),
    ("preflight-stores-request-parts", replace("crates/finam-gateway/src/stage8b_no_send.rs", "capability: Stage8a1CurrentlyAuthorizedCapability,", "request_parts: Stage8bApprovedRequestParts,", all_occurrences=False)),
    ("sealed-transition-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "fn commit_stage8b_sealed_attempt(", "fn removed_commit_stage8b_sealed_attempt(")),
    ("permit-transition-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "fn authorize_stage8b_exact_transport_permit(", "fn removed_authorize_stage8b_exact_transport_permit(")),
    ("raw-witness-escape", replace("crates/finam-gateway/src/stage8b_no_send.rs", "struct Stage8bApprovedRequestParts {", "pub struct Stage8bApprovedRequestParts {")),
    ("durable-binding-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", ".into_stage8b_binding_sha256()", ".removed_stage8b_binding_sha256()")),
    ("k2-sources-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "k2_sources: Stage8bK2FreshSources,", "_k2_sources: Stage8bK2FreshSources,")),
    ("build-input-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "build: Stage8bExecutionQualifiedBuild,", "_build: Stage8bExecutionQualifiedBuild,")),
    ("account-input-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "account: Stage8bKeyedAccountBinding,", "_account: Stage8bKeyedAccountBinding,")),
    ("contract-input-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "contract: Stage8bFreshContractAuthority,", "_contract: Stage8bFreshContractAuthority,")),
    ("run-input-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "run: Stage8bAcceptedRunSpec,", "_run: Stage8bAcceptedRunSpec,")),
    ("control-input-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "control: Stage8bK1ControlApproved,", "_control: Stage8bK1ControlApproved,")),
    ("arm-input-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "arm: Stage8bAuthenticatedOperatorArm,", "_arm: Stage8bAuthenticatedOperatorArm,")),
    ("single-owner-open", replace("crates/finam-gateway/src/stage8b_no_send.rs", "if !k2_sources.single_finam_owner", "if false")),
    ("ambiguity-open", replace("crates/finam-gateway/src/stage8b_no_send.rs", "k2_sources.ambiguity_count != 0", "false")),
    ("unresolved-open", replace("crates/finam-gateway/src/stage8b_no_send.rs", "k2_sources.unresolved_lifecycle_count != 0", "false")),
    ("freshness-open", replace("crates/finam-gateway/src/stage8b_no_send.rs", "|| !k2_sources.readiness_fresh", "|| false")),
    ("closure-payload-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "DurableOutcomeRecorded(Stage8bClosureClassification)", "DurableOutcomeRecorded")),
    ("closure-normalized-safe", replace("crates/finam-gateway/src/stage8b_no_send.rs", "Ok(durable)", "Ok(Stage8bClosureClassification::Stage8BClosedSafe)", all_occurrences=True)),
    ("closure-corruption-open", replace("crates/finam-gateway/src/stage8b_no_send.rs", "corrupt_unknown_or_mismatched_closure_payload_fails_closed", "removed_corrupt_closure_fixture")),
    ("legacy-feature-open", replace("crates/finam-gateway/src/stage8b_no_send.rs", '("finam-gateway/m3j16-actual-one-shot".to_string(), false)', '("finam-gateway/m3j16-actual-one-shot".to_string(), true)', all_occurrences=True)),
    ("endpoint-account-unbound", replace("crates/finam-gateway/src/stage8b_no_send.rs", "account.binding_sha256.as_bytes(),", "b\"account-omitted\",", all_occurrences=True)),
    ("arm-authentication-expiry-open", replace("crates/finam-gateway/src/stage8b_no_send.rs", "validate_authenticated_arm_for_k2(&arm, &expected_arm_binding, &k2_sources)?;", "let _ = (&arm, &expected_arm_binding);")),
    ("max-one-budget-open", replace("crates/finam-gateway/src/stage8b_no_send.rs", "k2_sources.max_one_budget_remaining != 1", "false")),
    ("k3-covering-seal-unbound", replace("crates/finam-gateway/src/stage8b_no_send.rs", "k3.seal_sha256.as_bytes(),", "b\"seal-omitted\",")),
    ("k4-exact-attempt-recheck-open", replace("crates/finam-gateway/src/stage8b_no_send.rs", "k4.rechecked_attempt_sha256 != sealed.attempt_sha256", "false")),
    ("k5-reconciliation-transition-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "fn reconcile_stage8b_possible_effect(", "fn removed_reconcile_stage8b_possible_effect(")),
    ("k5-control-binding-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "k5.control_sha256.as_bytes(),", "b\"control-omitted\",")),
    ("exact-closure-publication-removed", replace("crates/finam-gateway/src/stage8b_no_send.rs", "fn publish_stage8b_durable_closure(", "fn removed_publish_stage8b_durable_closure(")),
)


def copy_minimal(root: Path) -> None:
    for relative in FILES:
        source = ROOT / relative
        destination = root / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)


def main() -> None:
    if len(CASES) != 70:
        raise SystemExit(f"stage8b-i-negative: FAIL inventory={len(CASES)} expected=70")
    with tempfile.TemporaryDirectory(prefix="stage8b-i-negative-") as raw:
        base = Path(raw) / "base"
        copy_minimal(base)
        for name, mutation in CASES:
            case = Path(raw) / name
            shutil.copytree(base, case)
            mutation(case)
            result = subprocess.run(
                ["python3", str(CHECKER), "--root", str(case)],
                cwd=ROOT,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8b-i-negative: FAIL {name}")
            print(f"PASS {name}")
    print("stage8b-i-negative: PASS cases=70/70")


if __name__ == "__main__":
    main()
