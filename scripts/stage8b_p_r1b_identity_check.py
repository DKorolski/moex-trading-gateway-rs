#!/usr/bin/env python3
"""Fail-closed checker for Stage 8B-P R1B endpoint/run identity."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
D = ROOT / "docs/stage-8"
AUTHORITY = D / "stage8b-p-r1b-authorization-authority.json"
NETWORK = D / "stage8b-p-r1b-network-endpoint-authority.json"
RUN = D / "stage8b-p-r1b-run-identity-authority.json"
R1A = D / "stage8b-p-r1a-authorization-authority.json"
SOURCE = ROOT / "crates/finam-gateway/src/stage8b_no_send.rs"
DESIGN = D / "STAGE8B_P_R1B_IDENTITY_CORRECTION_2026-08-25.md"
MATRIX = D / "STAGE8B_P_R1B_ACCEPTANCE_MATRIX_2026-08-25.csv"
NEGATIVE = D / "STAGE8B_P_R1B_NEGATIVE_INVENTORY_2026-08-25.md"

MAIN_REF = "16a59bca74f94881c70d9fa39bbdf1c357e65f95"
R1A_REF = "f922ad65f7221488fcfc591d641b822f635b1993"
R1A_SHA = "4894355b730174b4e4f48fae60a2940f4f3fddbd1f7c6d43acf1f77b64f93ded"
NETWORK_SHA = "ec6f4b643e2cdc9b6b2cd531ce67bcb1cb52f3158298dece3b0ee371fd43d247"
RUN_SHA = "2fe373e4e2f68229a0df7356cfb5e9c256b63aeab0ad61d94f6ca8319315d468"
SOURCE_SHA = "716093652a0526e20d7fdcc72ac15f434cf7bc692091ef630e119b91bb72635b"
ENDPOINT_DOMAIN = "stage8b-i-r2-endpoint-identity-v1"
RUN_DOMAIN = "stage8b-p-r1b-accepted-run-identity-v1"
ENCODING = "digest_parts_v1_domain_raw_then_each_part_u64be_length_and_raw_bytes"
ACCOUNT_GOLDEN = "60106309bd530bd0cec76c3fa78fa4b7004ef34e44447fb7cd78fdda87444435"
RENDERER_GOLDEN = "24bc99b8e794ad85e7c83be7bd7d56cbc7568a01acdd4728785c2de600429d62"
PLACE_ENDPOINT = "84e170daa63dad57d1d88258daa00205b20f13cae7122074cbb0a5af77450b48"
CANCEL_ENDPOINT = "f00cff30e4b4c3d001fe09d8bea8dcdd46639475c906f70d5b1c49b855d2e78a"
PLACE_RUN = "bd3e7964ba2252252f10fc9a13677f9f2274a2c53ea12cfca39d927a0166410c"
CANCEL_RUN = "cb2778482d55b494884cd04589173853679505cff4ff5124451e0f1632e183e4"


def require(value: bool, message: str) -> None:
    if not value:
        raise SystemExit(f"stage8b-p-r1b-identity-check: FAIL {message}")


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def digest_parts(domain: str, parts: list[str]) -> str:
    digest = hashlib.sha256()
    digest.update(domain.encode("ascii"))
    for value in parts:
        data = value.encode("ascii")
        digest.update(len(data).to_bytes(8, "big"))
        digest.update(data)
    return digest.hexdigest()


def swap(values: list[str], left: int, right: int) -> list[str]:
    result = values.copy()
    result[left], result[right] = result[right], result[left]
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--no-git", action="store_true")
    args = parser.parse_args()

    for path in (AUTHORITY, NETWORK, RUN, R1A, SOURCE, DESIGN, MATRIX, NEGATIVE):
        require(path.is_file(), f"missing {path.relative_to(ROOT)}")
    authority = json.loads(AUTHORITY.read_text())
    network = json.loads(NETWORK.read_text())
    run = json.loads(RUN.read_text())

    require(authority.get("schema_version") == 1, "authority schema drift")
    require(authority.get("stage") == "8B-P" and authority.get("revision") == "R1B", "stage/revision drift")
    require(authority.get("status") == "design_only_identity_correction_candidate", "status drift")
    require(authority.get("branch") == "stage8b-p-authorization-r1", "branch drift")
    require(authority.get("lineage") == {
        "r1a_candidate_ref": R1A_REF,
        "r1a_authority_sha256": R1A_SHA,
        "accepted_main_ref": MAIN_REF,
        "inherited_negative_mutations": 98,
    }, "lineage drift")
    require(sha(R1A) == R1A_SHA, "R1A authority content drift")

    identities = authority.get("identity_authorities", {})
    require(identities.get("network_endpoint_path") == NETWORK.relative_to(ROOT).as_posix(), "network authority path drift")
    require(identities.get("network_endpoint_sha256") == NETWORK_SHA == sha(NETWORK), "network authority digest drift")
    require(identities.get("accepted_run_path") == RUN.relative_to(ROOT).as_posix(), "run authority path drift")
    require(identities.get("accepted_run_sha256") == RUN_SHA == sha(RUN), "run authority digest drift")
    require(identities.get("caller_selected_identity_authority_allowed") is False, "caller identity authority opened")
    require(identities.get("missing_or_modified_identity_authority_fails_closed") is True, "identity fail-close weakened")

    require(network.get("schema_version") == 1 and network.get("revision") == "R1B", "network schema drift")
    qualified = network.get("qualified_implementation", {})
    require(qualified.get("source_path") == SOURCE.relative_to(ROOT).as_posix(), "qualified source path drift")
    require(qualified.get("source_sha256") == SOURCE_SHA == sha(SOURCE), "qualified source digest drift")
    require(qualified.get("function") == "compose_endpoint_identity" and qualified.get("digest_function") == "digest_parts", "qualified function drift")
    require(qualified.get("execution_build_identity_sha256") == "ca330934df540de69b52d0463a665a1ab0ff89fa13eeb663b162763cc6bc83a0", "qualified build drift")

    source = SOURCE.read_text()
    for token in (
        'fn compose_endpoint_identity(', 'b"stage8b-i-r2-endpoint-identity-v1"',
        'pair.0,', 'pair.1,', 'account.binding_sha256.as_bytes(),',
        'endpoint_renderer_sha256.as_bytes(),', 'digest.update((part.len() as u64).to_be_bytes());',
    ):
        require(token in source, f"qualified source token missing: {token}")

    require(network.get("transport") == {
        "tls_required": True, "scheme": "https", "exact_host": "api.finam.ru",
        "redirects_allowed": False, "proxy_allowed": False,
        "alternate_host_allowed": False, "arbitrary_request_api_allowed": False,
        "automatic_transport_retry_allowed": False,
    }, "network transport drift")
    operations = network.get("operations", {})
    require(operations == {
        "PLACE": {"method": "POST", "route_template_id": "PlaceOrderV1", "route_template": "/v1/accounts/{account_id}/orders"},
        "CANCEL": {"method": "DELETE", "route_template_id": "CancelOrderV1", "route_template": "/v1/accounts/{account_id}/orders/{order_id}"},
    }, "operation endpoint drift")
    require(network.get("accepted_endpoint_renderer_sha256") == RENDERER_GOLDEN, "renderer drift")
    endpoint = network.get("endpoint_identity", {})
    require(endpoint.get("algorithm") == "sha256" and endpoint.get("domain_utf8") == ENDPOINT_DOMAIN, "endpoint domain drift")
    require(endpoint.get("encoding") == ENCODING, "endpoint encoding drift")
    require(endpoint.get("parts_in_exact_order") == ["method_ascii", "route_template_id_ascii", "keyed_account_binding_lower_hex_ascii", "endpoint_renderer_sha256_lower_hex_ascii"], "endpoint part order drift")
    require(endpoint.get("operation_as_extra_component_allowed") is False, "extra endpoint operation component opened")
    require(endpoint.get("nul_delimited_encoding_allowed") is False, "NUL endpoint encoding opened")
    require(endpoint.get("component_reordering_allowed") is False, "endpoint reorder opened")
    require(endpoint.get("length_prefix_width_bits") == 64 and endpoint.get("length_prefix_byte_order") == "big_endian", "endpoint length prefix drift")

    goldens = network.get("golden_vectors", {})
    require(goldens == {
        "keyed_account_binding_sha256": ACCOUNT_GOLDEN,
        "endpoint_renderer_sha256": RENDERER_GOLDEN,
        "place_endpoint_identity_sha256": PLACE_ENDPOINT,
        "cancel_endpoint_identity_sha256": CANCEL_ENDPOINT,
    }, "endpoint golden inventory drift")
    require(digest_parts(ENDPOINT_DOMAIN, ["POST", "PlaceOrderV1", ACCOUNT_GOLDEN, RENDERER_GOLDEN]) == PLACE_ENDPOINT, "PLACE endpoint golden mismatch")
    require(digest_parts(ENDPOINT_DOMAIN, ["DELETE", "CancelOrderV1", ACCOUNT_GOLDEN, RENDERER_GOLDEN]) == CANCEL_ENDPOINT, "CANCEL endpoint golden mismatch")

    require(run.get("schema_version") == 1 and run.get("revision") == "R1B", "run schema drift")
    require(run.get("network_endpoint_authority_path") == NETWORK.relative_to(ROOT).as_posix(), "run network path drift")
    require(run.get("network_endpoint_authority_sha256") == NETWORK_SHA, "run network digest drift")
    identity = run.get("run_identity", {})
    require(identity.get("algorithm") == "sha256" and identity.get("domain_utf8") == RUN_DOMAIN, "run domain drift")
    require(identity.get("encoding") == ENCODING, "run encoding drift")
    require(identity.get("part_encoding") == "exact_validated_ascii_value_bytes_without_field_name_or_normalization", "run part encoding drift")
    require(identity.get("length_prefix_width_bits") == 64 and identity.get("length_prefix_byte_order") == "big_endian", "run length prefix drift")
    require(identity.get("operation_discriminator_included") is True, "operation discriminator omitted")
    require(identity.get("run_identity_field_included_in_own_preimage") is False, "run identity self-reference opened")
    require(identity.get("computed_and_verified_not_caller_asserted") is True, "caller run assertion opened")

    r1a_manifest = json.loads(R1A.read_text()).get("canonical_manifest", {})
    expected_common = [field for field in r1a_manifest.get("common_required_fields", []) if field != "run_identity_sha256"]
    require(identity.get("common_fields_in_exact_order_excluding_run_identity") == expected_common, "common run field order drift")
    require(identity.get("place_fields_in_exact_order") == r1a_manifest.get("place_required_fields"), "PLACE run field order drift")
    require(identity.get("cancel_fields_in_exact_order") == r1a_manifest.get("cancel_required_fields"), "CANCEL run field order drift")

    rules = run.get("canonical_value_rules", {})
    require(rules.get("all_values_are_ascii_strings") is True and rules.get("unicode_normalization_or_implicit_coercion_allowed") is False, "run text canonicalization drift")
    require(rules.get("lower_sha256_regex") == "^[0-9a-f]{64}$" and rules.get("source_ref_regex") == "^[0-9a-f]{40}$", "identity regex drift")
    require(rules.get("unsigned_generation_regex") == "^(0|[1-9][0-9]*)$", "generation grammar drift")
    require(rules.get("positive_generation_required_for") == ["account_key_generation_id", "stage7b_seal_generation", "durable_budget_generation", "kill_switch_generation"], "positive generation inventory drift")
    require(rules.get("expiry_regex") == r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3}Z$", "expiry grammar drift")
    require(rules.get("approved_pre_run_position_regex") == r"^(0|-?[1-9][0-9]*)(\.[0-9]*[1-9])?$", "position grammar drift")
    require(rules.get("operation_values") == ["PLACE", "CANCEL"] and rules.get("quantity") == "1", "operation/quantity drift")
    require(rules.get("unknown_missing_or_variant_irrelevant_fields_fail_closed") is True, "run field fail-close weakened")

    for operation, expected_digest, expected_endpoint in (
        ("PLACE", PLACE_RUN, PLACE_ENDPOINT), ("CANCEL", CANCEL_RUN, CANCEL_ENDPOINT),
    ):
        vector = run.get("golden_vectors", {}).get(operation, {})
        manifest = vector.get("manifest_without_run_identity_sha256", {})
        fields = expected_common + identity.get(operation.lower() + "_fields_in_exact_order", [])
        require(set(manifest) == set(fields), f"{operation} golden field inventory drift")
        require(all(isinstance(manifest[field], str) and manifest[field].isascii() for field in fields), f"{operation} non-ASCII/non-string value")
        require(manifest.get("operation") == operation, f"{operation} discriminator drift")
        require(manifest.get("endpoint_identity_sha256") == expected_endpoint, f"{operation} endpoint binding drift")
        require(manifest.get("network_policy_sha256") == NETWORK_SHA, f"{operation} network binding drift")
        require(manifest.get("freshness_budget_authority_sha256") == "6f50b6e11292c2493c07fca11ad4e257190dad9941cb85ab6b8177091576d00f", f"{operation} freshness binding drift")
        require(manifest.get("execution_build_identity_sha256") == "ca330934df540de69b52d0463a665a1ab0ff89fa13eeb663b162763cc6bc83a0", f"{operation} build binding drift")
        for field, value in manifest.items():
            if field.endswith("_sha256") or field.endswith("_fingerprint"):
                require(re.fullmatch(r"[0-9a-f]{64}", value) is not None, f"{operation} invalid hash {field}")
        for field in rules.get("positive_generation_required_for", []):
            require(re.fullmatch(r"[1-9][0-9]*", manifest[field]) is not None, f"{operation} noncanonical generation {field}")
        require(re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3}Z", manifest["run_expires_at_utc"]) is not None, f"{operation} expiry drift")
        require(re.fullmatch(r"(0|-?[1-9][0-9]*)(\.[0-9]*[1-9])?", manifest["approved_pre_run_position"]) is not None, f"{operation} position drift")
        calculated = digest_parts(RUN_DOMAIN, [manifest[field] for field in fields])
        require(vector.get("run_identity_sha256") == expected_digest == calculated, f"{operation} run golden mismatch")
        if operation == "PLACE":
            require(manifest.get("quantity") == "1", "PLACE quantity drift")
        else:
            require(manifest.get("cancel_target_strategy_request_id") == manifest.get("strategy_request_id"), "CANCEL request mismatch")
            require(manifest.get("cancel_target_durable_client_order_id") == manifest.get("durable_client_order_id"), "CANCEL client mismatch")

    validation = run.get("validation", {})
    for key in ("computed_digest_exact_match_required", "bound_field_mutation_with_old_digest_fails_closed", "alternate_field_order_or_serialization_fails_closed", "operation_discriminator_omission_fails_closed", "endpoint_body_freshness_build_mutation_with_old_digest_fails_closed", "golden_vectors_required"):
        require(validation.get(key) is True, f"run validation weakened: {key}")
    require(validation.get("caller_asserted_unverified_digest_allowed") is False, "unverified run digest opened")

    correction = authority.get("endpoint_correction", {})
    require(correction.get("qualified_domain") == ENDPOINT_DOMAIN and correction.get("place_golden_sha256") == PLACE_ENDPOINT and correction.get("cancel_golden_sha256") == CANCEL_ENDPOINT, "endpoint correction summary drift")
    require(correction.get("operation_extra_component_allowed") is False and correction.get("nul_delimited_encoding_allowed") is False and correction.get("production_source_change_required") is False, "endpoint correction boundary drift")
    run_correction = authority.get("run_identity_correction", {})
    require(run_correction.get("domain") == RUN_DOMAIN and run_correction.get("encoding") == ENCODING, "run correction summary drift")
    require(run_correction.get("place_golden_sha256") == PLACE_RUN and run_correction.get("cancel_golden_sha256") == CANCEL_RUN, "run correction goldens drift")
    for key in ("operation_discriminator_bound", "common_and_variant_field_order_bound", "run_identity_self_excluded", "computed_and_verified_not_caller_asserted", "endpoint_network_freshness_process_boot_build_body_bound"):
        require(run_correction.get(key) is True, f"run correction weakened: {key}")
    require(all(authority.get("retained_r1a_contract", {}).values()), "R1A retained contract weakened")

    authorization = authority.get("authorization", {})
    require(authorization.get("status") == "NOT_ISSUED", "authorization issued")
    for key in ("exact_run_manifest_present", "account_credential_used", "broker_readonly_get_sent", "operator_arm_issued", "dispatch_attempt_recorded", "transport_entered", "finam_post_delete_sent", "broker_effect", "stage8b_p_open", "stage8b_xe_open"):
        require(authorization.get(key) is False, f"authorization surface opened: {key}")
    require(authorization.get("next_if_independently_accepted") == "Stage8B-P-R2 operator-selected GET-only preflight", "next step drift")
    require(len(authority.get("closed_surfaces", {})) == 14 and all(authority["closed_surfaces"].values()), "closed surface opened")

    with MATRIX.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 40 and [row["id"] for row in rows] == [f"P1B-{index:03d}" for index in range(1, 41)], "acceptance matrix drift")
    require(all(row.get("status") == "PASS" for row in rows), "acceptance matrix not green")
    require(len(re.findall(r"^\d+\. ", NEGATIVE.read_text(), flags=re.MULTILINE)) == 36, "negative inventory drift")
    require(authority.get("acceptance_rows") == 40 and authority.get("r1b_negative_mutations") == 36 and authority.get("inherited_negative_mutations") == 98 and authority.get("total_negative_mutations") == 134, "coverage count drift")
    for phrase in ("authorization `NOT_ISSUED`", "There is no separate operation component", "run_identity_sha256` is computed", "PLACE and CANCEL full golden", "R1B and R2 cannot issue an arm"):
        require(phrase in DESIGN.read_text(), f"design statement missing: {phrase}")

    if not args.no_git:
        require(subprocess.run(["git", "merge-base", "HEAD", MAIN_REF], cwd=ROOT, check=True, text=True, capture_output=True).stdout.strip() == MAIN_REF, "accepted main not ancestor")
        changed = subprocess.run(["git", "diff", "--name-only", MAIN_REF, "--", "Cargo.toml", "Cargo.lock", "crates", "config", ".github/workflows"], cwd=ROOT, check=True, text=True, capture_output=True).stdout.splitlines()
        require(not changed, f"production/config/workflow drift: {changed}")

    print("stage8b-p-r1b-identity-check: PASS rows=40 endpoint_goldens=2 run_goldens=2 new_negatives=36 inherited=98 total=134 authorization=NOT_ISSUED broker_get=false arm=false transport=false finam=false")


if __name__ == "__main__":
    main()
