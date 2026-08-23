#!/usr/bin/env python3
"""Static fail-closed checker for Stage 8B-P preconditions R1."""

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
A = D / "stage8b-p-preconditions-authority.json"
C = D / "stage8b-p-finam-contract-snapshot-2026-08-23.json"
B = D / "stage8b-p-build-identity-2026-08-23.json"
G = D / "stage8b-p-governance-observation-2026-08-23.json"
BASELINE = D / "stage8a0-finam-contract-snapshot-2026-08-14.json"
DESIGN = D / "STAGE8B_P_PRECONDITIONS_REFRESH_2026-08-23.md"
MATRIX = D / "STAGE8B_P_PRECONDITIONS_ACCEPTANCE_MATRIX_2026-08-23.csv"
NEGATIVE = D / "STAGE8B_P_PRECONDITIONS_NEGATIVE_INVENTORY_2026-08-23.md"
SOURCE = ROOT / "crates/finam-gateway/src/stage8b_no_send.rs"
WORKSPACE = ROOT / "Cargo.toml"
FINAM_MANIFEST = ROOT / "crates/finam-gateway/Cargo.toml"
LOCK = ROOT / "Cargo.lock"
CI = ROOT / ".github/workflows/ci.yml"
CHECKER = ROOT / "scripts/stage8b_p_preconditions_check.py"
HARNESS = ROOT / "scripts/stage8b_p_preconditions_negative_harness.py"
CONTRACT_REFRESH = ROOT / "scripts/stage8b_p_contract_refresh.py"
BUILD_REPRO = ROOT / "scripts/stage8b_p_build_repro.py"
GATE = ROOT / "scripts/stage8b_p_preconditions_gate.sh"
MAKER = ROOT / "scripts/make_stage8b_p_preconditions_handoff.py"
SAFETY = ROOT / "scripts/stage8b_p_preconditions_handoff_safety_check.py"


def require(value: bool, message: str) -> None:
    if not value:
        raise SystemExit(f"stage8b-p-preconditions-check: FAIL {message}")


def digest(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha(path: Path) -> str:
    return digest(path.read_bytes())


def manifest_aggregate() -> str:
    paths = sorted([ROOT / "Cargo.toml", *ROOT.glob("crates/*/Cargo.toml")])
    projection = "".join(f"{sha(path)} {path.relative_to(ROOT)}\n" for path in paths)
    return digest(projection.encode())


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--no-git", action="store_true")
    args = parser.parse_args()

    for path in (A, C, B, G, BASELINE, DESIGN, MATRIX, NEGATIVE, SOURCE, WORKSPACE, FINAM_MANIFEST, LOCK, CI, CHECKER, HARNESS, CONTRACT_REFRESH, BUILD_REPRO, GATE, MAKER, SAFETY):
        require(path.is_file(), f"missing {path.relative_to(ROOT)}")
    authority = json.loads(A.read_text())
    contract = json.loads(C.read_text())
    build = json.loads(B.read_text())
    governance = json.loads(G.read_text())
    baseline = json.loads(BASELINE.read_text())
    design = DESIGN.read_text()
    ci = CI.read_text()

    require(authority.get("stage") == "8B-P-PRECONDITIONS", "stage drift")
    require(authority.get("revision") == "R1", "revision drift")
    require(authority.get("status") == "design_only_candidate_pending_independent_acceptance", "status drift")
    require(authority.get("branch") == "stage8b-p-preconditions-refresh", "branch drift")
    require(authority.get("accepted_tls_ref") == "6cb179509fad97e8be56e31bb930b2a86caefc6a", "TLS ref drift")
    require(authority.get("accepted_tls_tree") == "4900fd38d741ab24f643acf211e7d1f807d23792", "TLS tree drift")
    require(authority.get("accepted_tls_archive_sha256") == "1066ab44b32451f921f2d3cdd49118471f78b214de7dd848a3273c95e19143b6", "TLS archive drift")
    require(authority.get("accepted_tls_merged_to_main") is True, "accepted merge missing")
    require(authority.get("accepted_tls_tree_identical_after_merge") is True, "tree identity missing")

    expected = {
        "rest_place_order": ("0fc4494e2f06a9bc8aebb10eb0a7de0500b661c9988a9fdfda526364348ff589", 23736),
        "rest_cancel_order": ("595f123796fca321e9027c81ea1dc54d61b85862b9a1031fea73eaa2ef92b63e", 6727),
        "grpc_place_order": ("3df67157308a1add9c27953912bd279ac66e6a715438c5034cec3a5b5d7bca12", 59500),
        "grpc_get_order": ("71cc118c771c9c960594f4e0cc3a0f2466ed76377c8f6e48a87a88d19df74dd8", 45948),
        "rest_get_asset": ("a7292fe5e0948bd926075baba3f1d9f318f380e3531e7d2f5b6698c353f9d6d3", 6421),
        "rest_get_asset_params": ("bb7c07ebadb6b3fdd0ed531ffab64aae91f547687348ffddc02209c46281b98d", 6552),
        "rest_schedule": ("9739d401763845a82c8a401b8e174694a0f6689cc760f54dc3b4792b4c1dd5d7", 5139),
    }
    responses = contract.get("retrieval", {}).get("responses", [])
    require(len(responses) == 7, "official response count drift")
    for response in responses:
        item = expected.get(response.get("name"))
        require(item is not None, "unknown official response")
        require(response.get("http_status") == 200, "official fetch not 200")
        require((response.get("sha256"), response.get("bytes")) == item, f"official response drift: {response.get('name')}")
        require(str(response.get("url", "")).startswith("https://api.finam.ru/docs/"), "non-official URL")
    baseline_values = {x["name"]: (x["sha256"], x["bytes"]) for x in baseline["retrieval"]["responses"]}
    require(baseline_values == expected, "accepted baseline drift")
    comparison = contract.get("comparison", {})
    require(comparison.get("baseline_sha256") == sha(BASELINE), "baseline hash drift")
    require(comparison.get("response_sha256_identical") is True, "hash equality removed")
    require(comparison.get("byte_counts_identical") is True, "byte equality removed")
    require(comparison.get("material_contract_drift") is False, "material contract drift")
    require(contract["retrieval"].get("production_host") == "api.finam.ru", "production host drift")
    require(contract["place_order"].get("method") == "POST" and contract["place_order"].get("path") == "/v1/accounts/{account_id}/orders", "PLACE contract drift")
    require(contract["cancel_order"].get("method") == "DELETE" and contract["cancel_order"].get("path") == "/v1/accounts/{account_id}/orders/{order_id}", "CANCEL contract drift")
    require(contract["initial_effect_policy"].get("automatic_retry") is False, "retry opened")
    require(contract["initial_effect_policy"].get("same_request_resend_after_attempt") is False, "resend opened")
    require(contract.get("contract_p1_status") == "READY_FOR_INDEPENDENT_ACCEPTANCE", "contract status drift")
    require(contract.get("stage8b_p_authorized") is False and contract.get("finam_request_sent") is False, "contract opened execution")
    require(authority["contract_p1"].get("snapshot_sha256") == sha(C), "fresh snapshot hash drift")

    source = build["source"]
    require(source.get("commit") == authority["accepted_tls_ref"], "build source drift")
    require(source.get("tree") == authority["accepted_tls_tree"], "build tree drift")
    require(source.get("archive_sha256") == authority["accepted_tls_archive_sha256"], "build archive drift")
    require(source.get("source_member_manifest_sha256") == "0b503d1f692bad3b5de7dec00d30028943661342a7c828e17321fd5655539d64", "source manifest drift")
    require(source.get("source_unchanged_after_build") is True, "source changed during build")
    build_info = build["build"]
    for token in ("CARGO_NET_OFFLINE=true", "CARGO_INCREMENTAL=0", "SOURCE_DATE_EPOCH=1787497046", "--remap-path-prefix=<canonical-extracted-root>=/stage8b-source", "--remap-path-prefix=<extracted-root>=/stage8b-source", "--release", "--locked", "-p broker-cli"):
        require(token in build_info.get("command", ""), f"build command missing {token}")
    require(build_info.get("network_dependency_fetch") is False, "network build opened")
    require(build_info.get("profile") == "release" and build_info.get("package") == "broker-cli", "build target drift")
    require(build_info.get("target_triple") == "aarch64-apple-darwin", "target drift")
    require(build_info.get("extraction_parent") == "/tmp", "extraction parent drift")
    require(build_info.get("cargo_working_directory") == "<extracted-root>", "Cargo working directory drift")
    require(build_info.get("executable_sha256") == "677f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06", "executable drift")
    require(build_info.get("independent_clean_build_count", 0) >= 2, "insufficient build repetitions")
    require(build_info.get("all_executable_hashes_identical") is True, "build hashes differ")
    cargo = build["cargo"]
    require(cargo.get("workspace_manifest_sha256") == sha(WORKSPACE), "workspace manifest drift")
    require(cargo.get("finam_gateway_manifest_sha256") == sha(FINAM_MANIFEST), "FINAM manifest drift")
    require(cargo.get("cargo_manifests_aggregate_sha256") == manifest_aggregate(), "manifest aggregate drift")
    require(cargo.get("cargo_lock_sha256") == sha(LOCK), "Cargo.lock drift")
    require(cargo.get("resolved_production_graph_sha256") == "e314ac65c4093c9370dec3f21e7aada33f5b57981ae137d8f505820b79c54b70", "production graph drift")
    require(cargo.get("resolved_all_features_graph_sha256") == "f7c4346dd3dcd22491336681846e6d94291d3f5dfef100d082156c141e8f7cc2", "feature graph drift")
    require(cargo.get("unknown_feature_count") == 0, "unknown feature state")
    rustc = build["rustc"]
    require(rustc.get("release") == "1.95.0" and rustc.get("commit_hash") == "59807616e1fa2540724bfbac14d7976d7e4a3860", "rustc identity drift")
    bindings = build["semantic_bindings"]
    require(bindings.get("stage8b_source_sha256") == sha(SOURCE), "Stage 8B source drift")
    require(bindings.get("config_policy_authority_sha256") == sha(D / "stage8b-spec-authority.json"), "policy authority drift")
    require(bindings.get("fresh_api_snapshot_sha256") == sha(C), "build API binding drift")
    projections = {
        "instrument_projection_sha256": '{"symbol":"IMOEXF","venue_symbol":"IMOEXF@RTSX","exchange":"MOEX","quantity":"1","order_types":["LIMIT"],"time_in_force":["DAY"]}',
        "endpoint_renderer_projection_sha256": "stage8b-endpoint-renderer-v1|POST|/v1/accounts/{account_id}/orders|DELETE|/v1/accounts/{account_id}/orders/{order_id}|PlaceOrderV1|CancelOrderV1",
        "request_body_schema_projection_sha256": "stage8b-request-body-schema-v1|symbol|quantity|side|type|time_in_force|limit_price|client_order_id|comment=none",
    }
    for field, value in projections.items():
        require(bindings.get(field) == digest(value.encode()), f"projection drift: {field}")
    require(build.get("build_p1_status") == "READY_FOR_INDEPENDENT_ACCEPTANCE", "build status drift")
    require(build.get("executable_executed") is False and build.get("broker_effect") is False, "build opened execution")

    require(governance.get("observed_head") == authority["accepted_tls_ref"], "governance head drift")
    github = governance["github_public_api"]
    require(github.get("branch_protected") is False, "unverified protection claim")
    require(github.get("ruleset_id") == 20111805 and github.get("ruleset_enforcement") == "disabled", "ruleset drift")
    require(governance.get("mutable_ci_references_observed") == ["actions/checkout@v4", "dtolnay/rust-toolchain@stable"], "mutable CI inventory drift")
    require("uses: actions/checkout@v4" in ci and "uses: dtolnay/rust-toolchain@stable" in ci, "CI observation drift")
    require(governance.get("gov_p1_status") == "PENDING_INDEPENDENT_ACCEPTANCE_OR_RULESET_ENABLEMENT", "GOV-P1 self-accepted")
    require(governance.get("workflow_modified_by_this_slice") is False, "workflow modification claimed")
    require(all(governance["equivalent_reviewed_change_control_candidate"].values()), "equivalent control weakened")

    require(authority["contract_p1"].get("status") == "READY_FOR_INDEPENDENT_ACCEPTANCE", "CONTRACT-P1 authority drift")
    require(authority["build_p1"].get("status") == "READY_FOR_INDEPENDENT_ACCEPTANCE", "BUILD-P1 authority drift")
    gov = authority["gov_p1"]
    require(gov.get("status") == "PENDING_INDEPENDENT_ACCEPTANCE_OR_RULESET_ENABLEMENT", "GOV-P1 authority drift")
    require(gov.get("branch_protection_active") is False and gov.get("ruleset_enforcement_active") is False and gov.get("immutable_action_pins_active") is False, "governance blocker hidden")
    aggregate = authority["aggregate"]
    require(aggregate.get("prerequisite_count") == 3 and aggregate.get("ready_for_independent_acceptance_count") == 2 and aggregate.get("pending_count") == 1, "aggregate counts drift")
    require(aggregate.get("all_prerequisites_accepted") is False and aggregate.get("stage8b_p_open") is False, "P opened")
    require(aggregate.get("next_allowed_action") == "independent_review_of_this_design_only_preconditions_package", "next action drift")
    require(all(authority["closed_surfaces"].values()), "closed surface opened")
    require(authority.get("acceptance_rows") == 36 and authority.get("negative_mutations") == 24, "authority matrix count drift")

    with MATRIX.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 36, "acceptance row count drift")
    require([row["id"] for row in rows] == [f"PPR-{i:03d}" for i in range(1, 37)], "acceptance IDs drift")
    require(all(row["status"] == "PASS" for row in rows), "acceptance matrix not green")
    require(len(re.findall(r"^\d+\. ", NEGATIVE.read_text(), flags=re.MULTILINE)) == 24, "negative inventory drift")
    require("Stage 8B-P remains closed" in design and "GOV-P1 is pending" in design, "boundary docs drift")
    gate_text = GATE.read_text()
    for command in (
        "python3 scripts/current_tree_authority_check.py",
        "python3 scripts/stage8b_tls_qualification_check.py --no-git",
        "python3 scripts/stage8b_p_contract_refresh.py",
        "python3 scripts/stage8b_p_build_repro.py",
        "python3 scripts/stage8b_p_preconditions_check.py",
        "python3 scripts/stage8b_p_preconditions_negative_harness.py",
        "cargo fmt --all -- --check",
        "git diff --check",
    ):
        require(gate_text.count(command) == 1, f"gate command drift: {command}")

    if not args.no_git:
        branch = subprocess.run(["git", "branch", "--show-current"], cwd=ROOT, check=True, text=True, capture_output=True).stdout.strip()
        require(branch == authority["branch"], "branch drift")
        status = subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, check=True, text=True, capture_output=True).stdout.splitlines()
        changed = [line[3:] for line in status if len(line) > 3]
        require(not any(path.startswith(("crates/", "Cargo.toml", "Cargo.lock", ".github/", "config/")) for path in changed), "production/workflow surface changed")

    print("stage8b-p-preconditions-check: PASS revision=R1 rows=36 contract=ready build=ready governance=pending stage8b_p=false")


if __name__ == "__main__":
    main()
