#!/usr/bin/env python3
"""Executable-aware fail-closed checker for Stage 8B-P R2B Proposal R2."""

from __future__ import annotations

import csv
import hashlib
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "docs/stage-8"
HEX64 = re.compile(r"[0-9a-f]{64}")
HEX128 = re.compile(r"[0-9a-f]{128}")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def load(relative: str) -> object:
    return json.loads((ROOT / relative).read_text(encoding="utf-8"))


def text(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def exact_hash(value: object) -> bool:
    return isinstance(value, str) and HEX64.fullmatch(value) is not None


def main() -> None:
    authority = load("docs/stage-8/stage8b-p-r2b-proposal-authority.json")
    runtime_path = ROOT / "docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json"
    runtime_bytes = runtime_path.read_bytes()
    runtime = json.loads(runtime_bytes)
    build = load("docs/stage-8/stage8b-p-r2b-r2-build-evidence.json")
    acceptance = load("docs/stage-8/stage8b-p-r2b-helper-acceptance-authority.json")
    helper_sha = text("docs/stage-8/stage8b-p-r2b-accepted-helper-sha256.txt").strip()
    proposal = text("docs/stage-8/STAGE8B_P_R2B_PROPOSAL_2026-08-27.md")
    status = text("docs/current-status.md")
    adapter = text("crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs")
    gateway_lib = text("crates/finam-gateway/src/lib.rs")
    gateway_cargo = text("crates/finam-gateway/Cargo.toml")
    producer_bin = text("crates/finam-gateway/src/bin/stage8b-r2a8-production-intake-producer.rs")
    writer_bin = text("crates/finam-gateway/src/bin/stage8b-r2a8-production-current-source-writer.rs")
    launcher = text("tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs")
    old_launcher = text("tools/stage8b-readonly-preflight/src/bin/stage8b-r2a5-launcher.rs")
    helper_lib = text("tools/stage8b-readonly-preflight/src/lib.rs")
    pipeline = text("tools/stage8b-readonly-preflight/src/r2a3.rs")
    finalizer = text("tools/stage8b-readonly-preflight/src/r2a5.rs")

    require(authority["schema_version"] == 1, "schema drift")
    require(authority["stage"] == "Stage 8B-P R2B" and authority["revision"] == "R2", "stage/revision drift")
    require(authority["artifact_kind"] == "design_only_readonly_preflight_proposal", "artifact kind drift")
    require(authority["status"] == "PROPOSAL_ONLY_NOT_AUTHORIZED", "proposal status opened")
    require(authority["authorization_status"] == "NOT_ISSUED", "R2B authorization issued")
    require(authority["accepted_predecessor"]["source_ref"] == "5b2079d7d524d2fa6f084f44f961c4b5958c042a", "predecessor drift")

    capability = authority["proposed_capability"]
    require(capability["one_shot"] and capability["operation_choices"] == ["PLACE", "CANCEL"], "one-shot drift")
    require(capability["selection_count"] == 1, "selection count drift")
    require(not capability["background_loop"] and not capability["unattended_execution"], "unattended surface opened")
    require(capability["result_may_influence_execution"] is False, "evidence became authority")

    network = authority["network_contract"]
    require(network["scheme"] == "https" and network["exact_host"] == "api.finam.ru", "endpoint drift")
    require(network["outbound_destinations"] == ["api.finam.ru:443"], "allowlist drift")
    for field in ("dns_or_ip_rebinding_allowed", "redirects_allowed", "proxy_allowed", "automatic_retries_allowed", "order_post_allowed", "order_delete_allowed", "arbitrary_request_allowed"):
        require(network[field] is False, f"network closure drift: {field}")
    require(network["request_timeout_seconds"] == 10 and network["minimum_broker_get_interval_ms"] == 250, "timing drift")

    sequence = [
        "stage8b-r2a8-production-intake-producer",
        "stage8b-r2a8-production-current-source-writer",
        "stage8b-r2a8-current-manifest-issuer",
        "stage8b-r2a7-source-adapter",
        "stage8b-r2a5-authority-producer",
        "stage8b-r2a5-authority-issuer",
        "stage8b-r2a5-package-issuer",
        "stage8b-r2b-launcher",
        "accepted-stage8b-readonly-preflight",
    ]
    composition = authority["production_composition"]
    require(composition["exact_executable_sequence"] == sequence, "exact sequence drift")
    expected_cardinality = {name: (11 if name in {"stage8b-r2a5-authority-producer", "stage8b-r2a5-authority-issuer"} else 1) for name in sequence}
    require(composition["exact_invocation_cardinality"] == expected_cardinality, "cardinality drift")
    require(runtime["revision"] == "R2" and runtime["exact_executable_sequence"] == sequence, "runtime sequence drift")
    require(runtime["authorization_status"] == "NOT_ISSUED", "runtime contract authorized")
    require(runtime["closed_surfaces"] == {"order_post": False, "order_delete": False, "broker_dispatch": False, "redis_live_consumer": False, "runtime_live": False, "real_orders": False}, "runtime closed surfaces drift")
    embedded = composition["embedded_runtime_composition_contract"]
    require(embedded["sha256"] == hashlib.sha256(runtime_bytes).hexdigest(), "runtime contract binding drift")
    require(embedded["contains_executable_hashes"] is False and embedded["hash_cycle_prevented"] is True, "hash cycle drift")

    query = authority["freshness_and_validation"]["query_policy"]
    require(query["window_start_semantics"] == "request_requested_at_minus_window_inclusive", "start boundary drift")
    require(query["window_end_semantics"] == "request_requested_at_exclusive", "end boundary drift")
    require(runtime["query_policy"]["window_end_semantics"] == "request_requested_at_exclusive", "runtime end boundary drift")
    require(query["trades_query_parameter_names"] == ["limit", "interval.start_time", "interval.end_time"], "query names drift")
    require(query["trades_limit"] == 1000 and query["trades_window_ms"] == 86400000, "query bounds drift")
    require(query["pagination"] == "single_page_no_cursor" and query["full_page_means_incomplete"], "pagination drift")
    for marker in ('pub const TRADES_LIMIT: usize = 1_000;', 'pub const TRADES_WINDOW_MS: i64 = 24 * 60 * 60 * 1_000;', '"interval.start_time"', '"interval.end_time"', 'if parsed.trades.len() >= TRADES_LIMIT'):
        require(marker in helper_lib, f"query implementation drift: {marker}")

    producer = authority["production_intake_producer"]
    require(producer["executable"] == sequence[0] and producer["uid"] == producer["gid"] == 8094, "intake producer identity drift")
    require(producer["upstream_owner_component"] == "Stage8a1OperationalAuthorityIssuer::issue_current_sources", "intake upstream owner drift")
    require("Stage7B recovery-ready owner" in producer["upstream_owner_boundary"], "intake upstream boundary absent")
    for field in ("caller_arguments_allowed", "caller_json_allowed", "caller_readiness_allowed", "caller_broker_truth_allowed", "caller_broker_readiness_allowed", "caller_timestamps_allowed", "network_access_allowed", "finam_credential_access_allowed", "runtime_live_authority"):
        require(producer[field] is False, f"intake producer boundary opened: {field}")
    for field in ("atomic_write", "file_fsync", "directory_fsync"):
        require(producer[field] is True, f"intake producer durability absent: {field}")
    require("std::env::args_os().len() != 1" in producer_bin, "intake producer accepts arguments")
    require("run_stage8b_r2a8_production_intake_producer" in producer_bin, "intake producer detached")
    producer_body = adapter.split("pub fn run_stage8b_r2a8_production_intake_producer", 1)[1].split("pub(crate) fn publish_stage8b_r2a8_trusted_current_source_from_owner", 1)[0]
    for marker in ("read_fixed_regular_file(", "validate_production_writer_intake(", "atomic_write_fixed("):
        require(marker in producer_body, f"intake producer composition missing: {marker}")
    require("reqwest" not in producer_body and "PRODUCTION_CREDENTIALS" not in producer_body, "intake producer gained network/credentials")

    writer = authority["production_current_source_writer"]
    require(writer["executable"] == sequence[1] and writer["uid"] == writer["gid"] == 8095, "writer identity drift")
    for field in ("caller_arguments_allowed", "caller_paths_allowed", "caller_snapshots_allowed", "network_access_allowed", "finam_credential_access_allowed", "redis_access_allowed", "runtime_live_authority"):
        require(writer[field] is False, f"writer boundary opened: {field}")
    require("std::env::args_os().len() != 1" in writer_bin, "writer accepts arguments")
    require("pub(crate) fn publish_stage8b_r2a8_trusted_current_source_from_owner(" in adapter, "owner seam absent")
    require("publish_stage8b_r2a8_trusted_current_source_from_owner," not in gateway_lib, "owner seam re-exported")
    for target in ("stage8b-r2a8-production-intake-producer", "stage8b-r2a8-production-current-source-writer"):
        require(f'name = "{target}"' in gateway_cargo, f"Cargo target absent: {target}")

    admission = authority["r2b_launcher_and_admission"]
    require(admission["launcher"] == "stage8b-r2b-launcher", "launcher authority drift")
    require(admission["launcher_uid"] == admission["launcher_gid"] == 0, "launcher privilege drift")
    require(admission["nonce_registry_owner"] == "root:root" and admission["nonce_registry_mode"] == "0700", "nonce custody drift")
    require(admission["sealed_receipt_fd"] == 3 and admission["privilege_drop_uid"] == admission["privilege_drop_gid"] == 8301, "receipt/identity drift")
    require(admission["helper_executable_fd_min"] == 4, "helper/receipt descriptor separation drift")
    require(admission["helper_execution"] == "open_once_O_NOFOLLOW_hash_then_fexecve_same_fd", "fd-bound helper execution drift")
    require(admission["supplementary_groups_after_drop"] == [] and admission["helper_capabilities"] == [], "helper privilege opened")
    require(not admission["helper_can_write_nonce_registry"] and not admission["helper_can_delete_nonce_marker"], "helper nonce authority opened")
    for marker in ("open_accepted_helper", "O_NOFOLLOW", "F_DUPFD_CLOEXEC", "R2B_ADMISSION_RECEIPT_FD + 1", "prepare_r2b_privileged_admission", "memfd_create", "F_ADD_SEALS", "record_r2b_helper_started", "setgroups", "setgid", "setuid", "fexecve"):
        require(marker in launcher, f"R2B launcher missing {marker}")
    require(launcher.index("open_accepted_helper(&accepted)") < launcher.index("prepare_r2b_privileged_admission(&accepted)"), "helper hash is not checked before nonce admission")
    require("stage8b-p-r2a5-accepted-helper-sha256.txt" in old_launcher, "historical R2A5 launcher mutated")
    require("stage8b-p-r2b-accepted-helper-sha256.txt" in launcher, "R2B helper pin absent")
    require("consume_sealed_r2b_admission_receipt" in finalizer, "helper receipt validation absent")
    for state in ("AdmissionRequested", "AdmissionMarkerCreated", "AdmissionDurable", "HelperStarted"):
        require(state in finalizer, f"admission state absent: {state}")
    run_body = finalizer.split("pub async fn run_r2b_one_shot()", 1)[1].split("fn controlled_client_from_fixed_files", 1)[0]
    require("claim_nonce(" not in run_body, "UID 8301 helper still claims nonce")
    require(run_body.index("consume_sealed_r2b_admission_receipt") < run_body.index("execute_r2a3_pipeline_preserving_attempts"), "receipt validation order drift")

    require(exact_hash(helper_sha), "accepted helper SHA malformed")
    require(acceptance["revision"] == "R2" and acceptance["helper_executable_sha256"] == helper_sha, "helper acceptance binding drift")
    require(acceptance["status"] == "ACCEPTED_HELPER_ONLY_R2B_NOT_ISSUED", "helper acceptance opened R2B")
    require(exact_hash(acceptance["authority_commitment_sha256"]), "acceptance commitment malformed")
    require(HEX128.fullmatch(acceptance["signature_ed25519_hex"]) is not None, "acceptance signature malformed")
    require(set(acceptance["signature_ed25519_hex"]) != {"0"}, "acceptance signature placeholder")
    require("embedded_helper_acceptance_signature_and_hash_are_valid" in launcher, "acceptance signature executable test absent")

    taxonomy = authority["evidence_contract"]["terminal_categories"]
    expected_taxonomy = ["SUCCESS", "AUTH_SESSION_FAILURE", "AUTH_DETAILS_FAILURE", "NETWORK_CONNECT_FAILURE", "TIMEOUT", "HTTP_NON_200", "RESPONSE_TOO_LARGE", "RESPONSE_BODY_FAILURE", "DTO_DECODE_FAILURE", "FRESHNESS_INVALID", "BROKER_TRUTH_INCOMPLETE", "CONTRACT_DRIFT", "INTERNAL_INVARIANT_FAILURE"]
    require(taxonomy == expected_taxonomy, "terminal taxonomy drift")
    for variant in ("AuthSessionFailure", "AuthDetailsFailure", "NetworkConnectFailure", "Timeout", "HttpNon200", "ResponseTooLarge", "ResponseBodyFailure", "DtoDecodeFailure", "FreshnessInvalid", "BrokerTruthIncomplete", "ContractDrift", "InternalInvariantFailure"):
        require(variant in finalizer, f"typed terminal outcome absent: {variant}")
    for marker in ("pub status: Option<u16>", "pub observed_body_length: Option<usize>", "pub configured_body_cap: usize", "pub body_overflow: bool", "pub response_stage_error: bool", "response_stage_failure_preserves_status_length_cap_and_overflow"):
        require(marker in pipeline, f"response-stage evidence absent: {marker}")
    attempt_schema = authority["evidence_contract"]["request_attempt_schema"]
    require(attempt_schema["status_required_after_response"] and attempt_schema["observed_body_length_and_cap_required_after_body_read"] and attempt_schema["body_overflow_flag_required"], "response evidence authority drift")

    hashes = composition["production_linux_amd64_sha256"]
    require(set(hashes) == set(sequence) and all(exact_hash(value) for value in hashes.values()), "production hash inventory drift")
    require(hashes["accepted-stage8b-readonly-preflight"] == helper_sha, "launcher/helper embedded SHA mismatch")
    require(build["stage"] == "Stage 8B-P R2B Proposal R2" and build["run_count"] == 2, "R2 build evidence drift")
    require(build["authorization_status"] == "NOT_ISSUED", "build evidence authorized R2B")
    require(build["fixture_dependencies_in_production"] is False, "production build depends on fixtures")
    for name in ("stage8b-r2a8-production-intake-producer", "stage8b-r2a8-production-current-source-writer", "stage8b-r2b-launcher", "accepted-stage8b-readonly-preflight"):
        record = build["production_binaries"][name]
        require(record["build_a_sha256"] == record["build_b_sha256"] == hashes[name] and record["reproducible"] is True, f"reproducibility drift: {name}")
    for name, record in build["production_binaries"].items():
        require(name in hashes and record["build_a_sha256"] == record["build_b_sha256"] == hashes[name] and record["reproducible"] is True, f"production build binding drift: {name}")
    for name, expected in build["inherited_accepted_production_binaries"].items():
        require(name in hashes and hashes[name] == expected, f"inherited production binding drift: {name}")
    controlled_hashes = composition["controlled_qualification_linux_amd64_sha256"]
    require(set(build["controlled_qualification_binaries"]) == set(controlled_hashes), "controlled build inventory drift")
    for name, record in build["controlled_qualification_binaries"].items():
        require(record["build_a_sha256"] == record["build_b_sha256"] == controlled_hashes[name] and record["reproducible"] is True, f"controlled build binding drift: {name}")
    for field in ("controlled_place_regression", "controlled_cancel_regression", "linux_terminal_evidence_test", "linux_production_custody_rehearsal"):
        require(build[field] == "PASS", f"build regression evidence drift: {field}")
    for relative, expected in build["source_sha256"].items():
        require(hashlib.sha256((ROOT / relative).read_bytes()).hexdigest() == expected, f"source binding drift: {relative}")
    for field in ("finam_network_accessed", "finam_credentials_accessed", "order_post_sent", "order_delete_sent", "redis_live_accessed", "runtime_live_entered"):
        require(build[field] is False, f"build evidence opened {field}")

    require(all(value is False for value in authority["closed_surfaces"].values()), "closed surface opened")
    require(authority["issuance_preconditions"]["r2b_authorization"] == "NOT_ISSUED", "issuance state drift")
    for marker in ("Stage 8B-P R2B Proposal R2", "NOT_ISSUED", "FINAM network access", "POST/DELETE", "runtime-live"):
        require(marker in status, f"status stale: {marker}")
    for marker in ("Proposal R2", "root-owned durable", "end boundary is exclusive", "does not authorize or perform FINAM network access"):
        require(marker in proposal, f"proposal narrative drift: {marker}")

    with (BASE / "STAGE8B_P_R2B_PROPOSAL_ACCEPTANCE_MATRIX_2026-08-27.csv").open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 50, "acceptance row count drift")
    require([row["id"] for row in rows] == [f"R2B-P-{index:03d}" for index in range(1, 51)], "acceptance IDs drift")
    require(all(row["status"] == "PASS" for row in rows), "acceptance row not PASS")

    print("stage8b-p-r2b-proposal-check: PASS revision=R2 rows=50 intake_producer=true launcher_helper_bound=true privileged_admission=true end_exclusive=true response_evidence=true authorization=NOT_ISSUED network=false post_delete=false runtime_live=false")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        raise SystemExit(f"stage8b-p-r2b-proposal-check: FAIL {error}")
