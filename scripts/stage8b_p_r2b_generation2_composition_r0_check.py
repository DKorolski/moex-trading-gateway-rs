#!/usr/bin/env python3
"""Fail-closed checker for Generation-2 Composition Rebuild R0."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
STAGE = "Stage 8B-P R2B Generation-2 Composition Rebuild R0"
ACCEPTED_PREDECESSOR = "3029bab714f8b75daaba3946ed858426515b4165"
ACCEPTED_ARCHIVE_SHA256 = "ee7deefa31dcf6b126408452f4772081ba20999c90ef58cf52df7b873869759f"
ACCEPTED_BACKUP_HASHES = {
    "docs/stage-8/stage8b-p-r2b-generation2-backup-restore-r0-authority.json": "cedb3f2b86b50bde469cf18441b49dc6b4334a7e34bb193138d4f5bdf2d8024e",
    "docs/stage-8/stage8b-p-r2b-generation2-backup-restore-r0-receipt.json": "7a8d8746cfecb9ff1f35537404380bb4035ce07ed05a383ad50e66a188a7775f",
    "docs/stage-8/stage8b-p-r2b-generation2-restore-destruction-r0-receipt.json": "63e5435835f108b6162c030c1fb2aba54f60970e83a73e57d132ac29800a5c95",
}
TRUST = Path("docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json")
ACCOUNT = Path("docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-account-key-manifest.json")
SOURCE_ADAPTER = Path("docs/stage-8/stage8b-p-r2a5-source-adapter-authority.json")
PRODUCTION_AUTHORITY = Path("docs/stage-8/stage8b-p-r2b-generation2-production-authority.json")
HELPER_AUTHORITY = Path("docs/stage-8/stage8b-p-r2b-generation2-accepted-helper-authority.json")
HELPER_PIN = Path("docs/stage-8/stage8b-p-r2b-generation2-accepted-helper-sha256.txt")
BUILD = Path("docs/stage-8/stage8b-p-r2b-generation2-composition-r0-linux-build-evidence.json")
REHEARSAL = Path("docs/stage-8/stage8b-p-r2b-generation2-composition-r0-linux-rehearsal-evidence.json")
AUTHORITY = Path("docs/stage-8/stage8b-p-r2b-generation2-composition-r0-authority.json")
DESIGN = Path("docs/stage-8/STAGE8B_P_R2B_GENERATION2_COMPOSITION_REBUILD_R0_2026-08-31.md")
MATRIX = Path("docs/stage-8/STAGE8B_P_R2B_GENERATION2_COMPOSITION_REBUILD_R0_ACCEPTANCE_MATRIX_2026-08-31.csv")
STATUS = Path("docs/current-status.md")
CORE = Path("tools/stage8b-readonly-preflight/src/r2a5.rs")
LAUNCHER = Path("tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-launcher.rs")
ISSUER = Path("tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-generation2-helper-acceptance-issuer.rs")
ISSUE_SCRIPT = Path("scripts/stage8b_p_r2b_generation2_composition_rebuild_r0_issue_helper.py")
BUILD_SCRIPT = Path("scripts/stage8b_p_r2b_generation2_composition_rebuild_r0_build_linux.sh")
MATERIALIZER = Path("scripts/stage8b_p_r2b_generation2_composition_r0_materialize_phase6.py")
RUNNER = Path("scripts/stage8b_p_r2b_generation2_composition_r0_phase6_runner.sh")
BASE_PHASE6 = Path("scripts/stage8b_p_r2b_implementation_r0_r1a_phase6_rehearsal.sh")
DEFAULT_ARTIFACT_ROOT = Path("reports/stage8b-p-r2b-generation2-composition-r0/linux-amd64")
HANDOFF_ARTIFACT_ROOT = Path("handoff-evidence/linux-amd64")
EXPECTED_TRUST_SHA256 = "dfe61ddb944df042cdf9514f56c14131e4a45bc732435ff89658ceaceb92d4ee"
EXPECTED_ACCOUNT_SHA256 = "206bb41415f5edd9c59aa0d256dea63219fa6e28def2e436b676a4de3d1b52ec"
EXPECTED_PUBLIC_SET_SHA256 = "a1094751e25613d1a9f10b54436f3229fc73774d9135812577978c22a7bb7465"
EXPECTED_AUTHORIZATION_KEY_SHA256 = "c3160a41e54fbeb9de4afe2163260f383fefa3fb531613d9754fc6b911a37c88"
EXPECTED_SOURCE_ADAPTER_SHA256 = "711b7d01552ef3ca48bde1daba13fcb844380e758df097e2757534ccf2952129"
BASE_PHASE6_SHA256 = "97ef39c944db607b5cb9a79509922e7bb9737dec8de1fdb9615356cf76763ac7"
BUILDER_IMAGE = "messense/rust-musl-cross:x86_64-musl@sha256:020ec7f60e63ace4338f8cb492bb2521071d089133732d0fc6a0ecea722b87c5"
PRODUCTION_BINARIES = {
    "stage8b-r2a5-authority-producer",
    "stage8b-r2a5-authority-issuer",
    "stage8b-r2b-run-package-draft-builder",
    "stage8b-r2a5-package-issuer",
    "stage8b-r2a5-helper-acceptance-issuer",
    "stage8b-readonly-preflight",
    "stage8b-r2b-launcher",
}
OPERATION_BINARIES = {"stage8b-r2b-generation2-helper-acceptance-issuer"}
HEX64 = re.compile(r"[0-9a-f]{64}")
HEX40 = re.compile(r"[0-9a-f]{40}")
PRIVATE_PATTERNS = (
    re.compile(rb"AGE-SECRET-KEY-1[0-9A-Z]+"),
    re.compile(rb"/Users/[A-Za-z0-9._-]+/[A-Za-z0-9._ /-]*(?:ceremony|agekey)"),
    re.compile(rb"/Volumes/[A-Za-z0-9._ -]+"),
)


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load(root: Path, relative: Path) -> dict[str, Any]:
    value = json.loads((root / relative).read_text(encoding="utf-8"))
    require(isinstance(value, dict), f"JSON object required: {relative}")
    return value


def exact_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    require(set(value) == expected, f"{label} schema drift")


def resolve_artifact_root(root: Path, requested: Path | None) -> Path:
    if requested is not None:
        candidate = requested if requested.is_absolute() else root / requested
        require(candidate.is_dir(), f"artifact root missing: {requested}")
        return candidate
    candidates = (root / DEFAULT_ARTIFACT_ROOT, root / HANDOFF_ARTIFACT_ROOT)
    existing = [candidate for candidate in candidates if candidate.is_dir()]
    require(len(existing) == 1, f"artifact root must resolve exactly once: {existing}")
    return existing[0]


def verify_helper_acceptance(root: Path, trust: dict[str, Any], helper_sha: str) -> None:
    authority = load(root, HELPER_AUTHORITY)
    order = (
        "schema_version", "stage", "revision", "status", "helper_executable_sha256",
        "effect_build_identity_sha256", "valid_from_utc", "valid_until_utc",
        "acceptance_key_id", "signature_ed25519_hex",
    )
    exact_keys(authority, set(order), "helper acceptance")
    require(list(authority) == list(order), "helper acceptance signed field order drift")
    key = trust["helper_acceptance_key"]
    require(authority["schema_version"] == 1 and authority["stage"] == "8B-P", "helper stage drift")
    require(authority["revision"] == "R2A5" and authority["status"] == "ACCEPTED", "helper status drift")
    require(authority["helper_executable_sha256"] == helper_sha, "helper acceptance digest drift")
    require(authority["acceptance_key_id"] == key["key_id"], "helper acceptance key drift")
    require(authority["valid_from_utc"] == key["valid_from_utc"], "helper validity start drift")
    require(authority["valid_until_utc"] == key["valid_until_utc"], "helper validity end drift")
    require(HEX64.fullmatch(authority["effect_build_identity_sha256"]) is not None, "effect identity drift")
    require(re.fullmatch(r"[0-9a-f]{128}", authority["signature_ed25519_hex"]) is not None, "helper signature grammar")
    unsigned = {name: authority[name] for name in order}
    unsigned["signature_ed25519_hex"] = ""
    preimage = b"stage8b-p-r2a5-helper-acceptance-ed25519-v1\0" + json.dumps(
        unsigned, separators=(",", ":")
    ).encode()
    public_der = bytes.fromhex("302a300506032b6570032100" + key["public_key_ed25519_hex"])
    with tempfile.TemporaryDirectory(prefix="stage8b-g2-helper-verify-") as temporary:
        directory = Path(temporary)
        (directory / "public.der").write_bytes(public_der)
        (directory / "preimage").write_bytes(preimage)
        (directory / "signature").write_bytes(bytes.fromhex(authority["signature_ed25519_hex"]))
        result = subprocess.run(
            ["openssl", "pkeyutl", "-verify", "-pubin", "-inkey", str(directory / "public.der"),
             "-keyform", "DER", "-rawin", "-in", str(directory / "preimage"),
             "-sigfile", str(directory / "signature")],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
        )
    require(result.returncode == 0, "helper acceptance signature invalid")


def check_public_composition(root: Path) -> tuple[dict[str, Any], dict[str, Any], str]:
    trust_path = root / TRUST
    account_path = root / ACCOUNT
    source_path = root / SOURCE_ADAPTER
    require(sha256(trust_path) == EXPECTED_TRUST_SHA256, "Generation-2 trust drift")
    require(sha256(account_path) == EXPECTED_ACCOUNT_SHA256, "Generation-2 account drift")
    require(sha256(source_path) == EXPECTED_SOURCE_ADAPTER_SHA256, "source adapter drift")
    trust = load(root, TRUST)
    account = load(root, ACCOUNT)
    production = load(root, PRODUCTION_AUTHORITY)
    exact_keys(production, {
        "schema_version", "stage", "revision", "authorization_status",
        "authorization_public_key_sha256", "trust_manifest_sha256",
        "public_key_set_sha256", "account_key_manifest_sha256",
        "source_adapter_authority_sha256",
    }, "production authority")
    require(production == {
        "schema_version": 1,
        "stage": "8B-P",
        "revision": "R2B-G2-R0",
        "authorization_status": "NOT_ISSUED",
        "authorization_public_key_sha256": EXPECTED_AUTHORIZATION_KEY_SHA256,
        "trust_manifest_sha256": EXPECTED_TRUST_SHA256,
        "public_key_set_sha256": EXPECTED_PUBLIC_SET_SHA256,
        "account_key_manifest_sha256": EXPECTED_ACCOUNT_SHA256,
        "source_adapter_authority_sha256": EXPECTED_SOURCE_ADAPTER_SHA256,
    }, "production authority value drift")
    require(trust.get("schema_version") == 1 and trust.get("environment") == "production", "trust schema drift")
    require(trust.get("public_key_set_sha256") == EXPECTED_PUBLIC_SET_SHA256, "public set drift")
    require(trust.get("rotation_requires_new_reviewed_package") is True, "rotation policy opened")
    keys = [trust["authorization_key"], trust["helper_acceptance_key"], *trust["source_keys"].values()]
    require(len(trust["source_keys"]) == 11 and len(keys) == 13, "Generation-2 key inventory drift")
    for key in keys:
        require(key["generation"] == 2, f"mixed key generation: {key.get('key_id')}")
        require(hashlib.sha256(bytes.fromhex(key["public_key_ed25519_hex"])).hexdigest() == key["public_key_sha256"], "public key self-hash drift")
    require(trust["authorization_key"]["public_key_sha256"] == EXPECTED_AUTHORIZATION_KEY_SHA256, "authorization key drift")
    exact_keys(account, {"schema_version", "entries"}, "account manifest")
    require(account["schema_version"] == 1 and len(account["entries"]) == 1, "account inventory drift")
    entry = account["entries"][0]
    require(entry["generation_id"] == "2" and entry["relative_key_path"] == "generation-2.hex", "account generation drift")
    helper_sha = (root / HELPER_PIN).read_text(encoding="utf-8").strip()
    require(HEX64.fullmatch(helper_sha) is not None, "helper pin grammar")
    verify_helper_acceptance(root, trust, helper_sha)
    return trust, production, helper_sha


def check_sources(root: Path, helper_sha: str) -> None:
    core = (root / CORE).read_text(encoding="utf-8")
    launcher = (root / LAUNCHER).read_text(encoding="utf-8")
    issuer = (root / ISSUER).read_text(encoding="utf-8")
    issue_script = (root / ISSUE_SCRIPT).read_text(encoding="utf-8")
    materializer = (root / MATERIALIZER).read_text(encoding="utf-8")
    runner = (root / RUNNER).read_text(encoding="utf-8")
    build_script = (root / BUILD_SCRIPT).read_text(encoding="utf-8")
    require('stage8b-p-r2b-generation2-production-authority.json' in core, "production authority include missing")
    require(core.count("validate_generation2_composition(") == 7, "composition validation call inventory drift")
    for marker in (
        "accept_helper_from_fixed_authority", "issue_run_package_from_fixed_draft",
        "build_run_package_draft_at", "issue_from_source_at",
    ):
        require(marker in core, f"composition consumer missing: {marker}")
    require("stage8b-p-r2b-generation2-accepted-helper-sha256.txt" in launcher, "launcher Generation-2 pin missing")
    require("create_generation2_helper_acceptance_authority" in issuer, "offline helper issuer seam missing")
    require("sort_keys=True" not in issue_script, "signed helper field order canonicalization broken")
    require("stderr=subprocess.DEVNULL" in issue_script and "private_path=false" in issue_script, "helper privacy boundary drift")
    require(BASE_PHASE6_SHA256 in materializer and "replacement cardinality" in materializer, "Phase-6 base pin drift")
    require("generation-1.hex" in materializer and "generation-1 residue" in materializer, "Phase-6 residue rejection missing")
    require('--network none' in runner and ':/ceremony:ro' in runner, "Phase-6 network/custody boundary drift")
    require("production_authorization':'NOT_ISSUED'" in runner, "production authorization closure missing")
    require(BUILDER_IMAGE in build_script and "--no-default-features" in build_script, "reproducible build contract drift")
    require(sha256(root / BASE_PHASE6) == BASE_PHASE6_SHA256, "accepted Phase-6 base drift")
    for relative in (ISSUER, ISSUE_SCRIPT, MATERIALIZER, RUNNER, BUILD_SCRIPT):
        data = (root / relative).read_bytes()
        for pattern in PRIVATE_PATTERNS:
            require(pattern.search(data) is None, f"private custody marker exported: {relative}")
    require(helper_sha in (root / HELPER_AUTHORITY).read_text(encoding="utf-8"), "helper public authority mismatch")


def check_build(root: Path, artifact_root: Path, helper_sha: str) -> dict[str, Any]:
    build = load(root, BUILD)
    require(build.get("stage") == STAGE and build.get("result") == "PASS", "build result drift")
    require(HEX40.fullmatch(str(build.get("source_ref"))) is not None, "build source ref drift")
    require(HEX40.fullmatch(str(build.get("source_tree"))) is not None, "build source tree drift")
    require(build.get("container_image") == BUILDER_IMAGE, "builder image drift")
    require(build.get("target") == "x86_64-unknown-linux-musl", "build target drift")
    require(build.get("build_profile") == "release" and build.get("default_features") is False, "build mode drift")
    require(build.get("clean_target_directories") == 2 and build.get("all_hashes_identical") is True, "reproducibility drift")
    require(build.get("production_binary_count") == len(PRODUCTION_BINARIES), "production binary count drift")
    require(build.get("offline_tool_binary_count") == len(OPERATION_BINARIES), "offline tool count drift")
    records = build.get("binaries", {})
    require(set(records) == PRODUCTION_BINARIES | OPERATION_BINARIES, "binary inventory drift")
    for name, record in records.items():
        expected_class = "PRODUCTION" if name in PRODUCTION_BINARIES else "OFFLINE_PUBLIC_AUTHORITY_TOOL"
        require(record.get("classification") == expected_class, f"binary classification drift: {name}")
        left_hash = record.get("build_a_sha256")
        right_hash = record.get("build_b_sha256")
        require(HEX64.fullmatch(str(left_hash)) is not None and left_hash == right_hash, f"binary reproducibility drift: {name}")
        require(record.get("reproducible") is True and "ELF 64-bit LSB" in record.get("file_identity", ""), f"ELF evidence drift: {name}")
        for build_name, expected_hash in (("build-a", left_hash), ("build-b", right_hash)):
            binary = artifact_root / build_name / name
            require(binary.is_file() and sha256(binary) == expected_hash, f"binary artifact drift: {build_name}/{name}")
    require(build.get("helper_sha256") == helper_sha, "build helper pin drift")
    require(records["stage8b-readonly-preflight"]["build_a_sha256"] == helper_sha, "helper binary drift")
    require(build.get("launcher_embeds_exact_helper_sha256") is True, "launcher pin evidence missing")
    require(build.get("generation") == 2 and build.get("authorization") == "NOT_ISSUED", "build generation/status drift")
    return build


def check_rehearsal(root: Path, build: dict[str, Any]) -> dict[str, Any]:
    rehearsal = load(root, REHEARSAL)
    require(rehearsal.get("stage") == STAGE and rehearsal.get("result") == "PASS", "rehearsal result drift")
    require(rehearsal.get("source_ref") == build["source_ref"] and rehearsal.get("source_tree") == build["source_tree"], "rehearsal source binding drift")
    require(rehearsal.get("linux_build_evidence_sha256") == sha256(root / BUILD), "rehearsal build binding drift")
    expected_true = (
        "systemd_pid1", "actual_read_attempts", "credential_canaries_real",
        "controlled_builder_executed", "controlled_signer_executed",
        "production_phase5_phase6_compatibility_proved", "production_launcher_executed",
        "production_helper_executed", "production_helper_projected_credentials_read",
        "production_helper_expected_no_network_terminal", "root_terminal_evidence_published",
        "generation_2_public_composition_selected", "isolated_rehearsal_package_signed",
        "isolated_rehearsal_package_destroyed_with_container",
    )
    for key in expected_true:
        require(rehearsal.get(key) is True, f"rehearsal proof missing: {key}")
    expected_false = (
        "external_network_available", "builder_external_network", "signer_external_network",
        "finam_endpoint_called", "real_credentials_used", "services_installed_to_production",
        "production_credentials_installed",
    )
    for key in expected_false:
        require(rehearsal.get(key) is False, f"rehearsal boundary opened: {key}")
    require(rehearsal.get("container_network_mode") == "none", "rehearsal network mode drift")
    require(rehearsal.get("generation") == 2 and rehearsal.get("account_key_generation_id") == "2", "rehearsal generation drift")
    require(rehearsal.get("isolated_rehearsal_package_generation") == 2, "rehearsal package generation drift")
    require(rehearsal.get("production_authorization") == "NOT_ISSUED" and rehearsal.get("authorization") == "NOT_ISSUED", "production authorization opened")
    return rehearsal


def check_authority(root: Path, build: dict[str, Any], rehearsal: dict[str, Any]) -> None:
    authority = load(root, AUTHORITY)
    require(authority.get("schema_version") == 1 and authority.get("stage") == STAGE, "aggregate authority stage drift")
    require(authority.get("status") == "INDEPENDENT_REVIEW_REQUIRED", "aggregate status drift")
    predecessor = authority.get("accepted_predecessor", {})
    require(predecessor == {"source_ref": ACCEPTED_PREDECESSOR, "archive_sha256": ACCEPTED_ARCHIVE_SHA256, "verdict": "ACCEPTED"}, "accepted predecessor drift")
    immutable = authority.get("accepted_backup_restore", {}).get("immutable_public_artifacts", {})
    require(immutable == ACCEPTED_BACKUP_HASHES, "accepted backup artifact drift")
    composition = authority.get("public_composition", {})
    require(composition.get("generation") == 2 and composition.get("selected_in_source") is True, "composition selection drift")
    require(composition.get("production_authority_sha256") == sha256(root / PRODUCTION_AUTHORITY), "production authority binding drift")
    require(composition.get("helper_acceptance_sha256") == sha256(root / HELPER_AUTHORITY), "helper authority binding drift")
    rebuild = authority.get("production_rebuild", {})
    require(rebuild.get("source_ref") == build["source_ref"] and rebuild.get("source_tree") == build["source_tree"], "rebuild source drift")
    require(rebuild.get("evidence_sha256") == sha256(root / BUILD), "build evidence authority drift")
    require(rebuild.get("all_hashes_identical") is True and rebuild.get("production_binary_count") == 7, "rebuild result drift")
    phase6 = authority.get("phase6_rehearsal", {})
    require(phase6.get("evidence_sha256") == sha256(root / REHEARSAL), "rehearsal authority drift")
    require(phase6.get("generation") == 2 and phase6.get("network_mode") == "none", "phase6 authority drift")
    require(phase6.get("production_authorization") == "NOT_ISSUED", "phase6 authorization drift")
    expected_activation = {
        "generation_2_public_authority_selected": True,
        "production_binaries_rebuilt": True,
        "helper_acceptance_reissued": True,
        "phase6_rehearsal_rebound": True,
        "generation_2_active": False,
        "production_credentials_installed": False,
        "controlled_installation": False,
        "package_authorization": "NOT_ISSUED",
    }
    require(authority.get("activation") == expected_activation, "activation boundary drift")
    expected_closed = {
        "finam_network": False, "auth_service": False, "broker_get": False,
        "http_post_delete": False, "broker_dispatch": False, "redis_live": False,
        "runtime_live": False, "real_orders": False,
    }
    require(authority.get("closed_surfaces") == expected_closed, "closed surface drift")
    require(authority.get("next_allowed_step") == "INDEPENDENT_REVIEW_BEFORE_CONTROLLED_INSTALLATION_OR_AUTHORIZATION", "next-step drift")
    status = (root / STATUS).read_text(encoding="utf-8")
    for marker in (STAGE, "3029bab714f8b75daaba3946ed858426515b4165", "NOT_ISSUED", "Generation 2 remains inactive"):
        require(marker in status, f"status marker missing: {marker}")
    with (root / MATRIX).open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 37 and [row["id"] for row in rows] == [f"G2CR-R0-{index:03d}" for index in range(1, 38)], "acceptance matrix inventory drift")
    require(all(row["expected"] == "PASS" for row in rows), "acceptance matrix result drift")


def check(root: Path, artifact_root: Path | None = None) -> None:
    required = {
        TRUST, ACCOUNT, SOURCE_ADAPTER, PRODUCTION_AUTHORITY, HELPER_AUTHORITY, HELPER_PIN,
        BUILD, REHEARSAL, AUTHORITY, DESIGN, MATRIX, STATUS, CORE, LAUNCHER, ISSUER,
        ISSUE_SCRIPT, BUILD_SCRIPT, MATERIALIZER, RUNNER, BASE_PHASE6,
        *(Path(path) for path in ACCEPTED_BACKUP_HASHES),
    }
    for relative in required:
        require((root / relative).is_file(), f"missing artifact: {relative}")
    for relative, digest in ACCEPTED_BACKUP_HASHES.items():
        require(sha256(root / relative) == digest, f"accepted backup artifact changed: {relative}")
    _, _, helper_sha = check_public_composition(root)
    check_sources(root, helper_sha)
    artifacts = resolve_artifact_root(root, artifact_root)
    build = check_build(root, artifacts, helper_sha)
    rehearsal = check_rehearsal(root, build)
    check_authority(root, build, rehearsal)
    print(
        "stage8b-generation2-composition-r0-check: PASS "
        "generation=2 production_binaries=7 reproducible=true phase6=PASS "
        "active=false authorization=NOT_ISSUED finam=false"
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--artifact-root", type=Path)
    arguments = parser.parse_args()
    check(arguments.root.resolve(), arguments.artifact_root)


if __name__ == "__main__":
    main()
