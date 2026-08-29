#!/usr/bin/env python3
"""Fail-closed checker for Stage 8B-P R2B Implementation Package R0."""

from __future__ import annotations

import csv
import hashlib
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
AUTHORITY = Path("docs/stage-8/stage8b-p-r2b-implementation-r0-authority.json")
EVIDENCE = Path("docs/stage-8/stage8b-p-r2b-implementation-r0-evidence.json")
MATRIX = Path("docs/stage-8/STAGE8B_P_R2B_IMPLEMENTATION_R0_ACCEPTANCE_MATRIX_2026-08-29.csv")
DESIGN = Path("docs/stage-8/STAGE8B_P_R2B_IMPLEMENTATION_PACKAGE_R0_2026-08-29.md")
PREDECESSOR = "ebec9a100c92872134f3de91644cec50e2ed073a"
TARGET_DIR = Path("deploy/stage8b-r2b")
BUILDER_SOURCE = Path("tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-run-package-draft-builder.rs")
CORE_SOURCE = Path("tools/stage8b-readonly-preflight/src/r2a5.rs")


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def assignments(path: Path, key: str) -> list[str]:
    values: list[str] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line.startswith(f"{key}="):
            values.extend(line.split("=", 1)[1].split())
    return values


def check(root: Path) -> None:
    for relative in (AUTHORITY, EVIDENCE, MATRIX, DESIGN, BUILDER_SOURCE, CORE_SOURCE):
        require((root / relative).is_file(), f"missing artifact: {relative}")

    authority = json.loads((root / AUTHORITY).read_text(encoding="utf-8"))
    evidence = json.loads((root / EVIDENCE).read_text(encoding="utf-8"))
    require(authority.get("schema_version") == 1, "authority schema drift")
    require(authority.get("stage") == "Stage 8B-P R2B Implementation Package R0", "stage drift")
    require(authority.get("status") == "IMPLEMENTED_NOT_INSTALLED_NOT_ISSUED_REVIEW_REQUIRED", "status drift")
    require(authority.get("accepted_predecessor") == {
        "stage": "Stage 8B-P R2B Issuance Package R0-R1A1",
        "source_ref": PREDECESSOR,
        "verdict": "ACCEPTED",
    }, "accepted predecessor drift")
    require(authority.get("scope") == "SOURCE_IMPLEMENTATION_ONLY", "scope expansion")

    builder = authority["builder"]
    require(builder["uid"] == 0 and builder["gid"] == 0, "builder identity drift")
    require(builder["caller_arguments_allowed"] is False, "builder caller arguments opened")
    require(builder["network_allowed"] is False, "builder network opened")
    require(builder["signing_key_access"] is False, "builder signing key opened")
    require(builder["finam_credential_access"] is False, "builder FINAM credential opened")
    require(builder["required_receipt_count"] == 11, "receipt count drift")
    require(builder["fixed_output"] == "/var/lib/moex-trading/stage8b/r2a5/r2b-run-package.unsigned.json", "builder output drift")
    require(builder["output_mode"] == "0600" and builder["output_no_replace"] is True, "builder output safety drift")
    require(builder["file_fsync"] is True and builder["directory_fsync"] is True, "builder durability drift")
    require(len(builder["fixed_inputs"]) == 7, "fixed input class count drift")

    signer = authority["signer"]
    require(signer["uid"] == 0 and signer["gid"] == 0, "signer identity drift")
    require(signer["fixed_input"] == builder["fixed_output"], "builder/signer path mismatch")
    require(signer["fixed_output"] == "/etc/moex-trading/stage8b/r2a5/r2b-run-package.json", "signer output drift")
    require(signer["output_no_replace"] is True, "signer overwrite opened")
    require(signer["sole_signing_key_consumer"] is True, "signing custody drift")
    require(signer["revalidates_all_builder_inputs"] is True, "signer validation removed")
    require(signer["may_repair_or_synthesize_draft"] is False, "signer synthesis opened")
    require(signer["requires_builder_success"] is True, "signer ordering opened")

    artifacts = authority["implementation_artifacts"]
    require(len(artifacts) == 16, "implementation artifact count drift")
    for relative, digest in artifacts.items():
        path = root / relative
        require(path.is_file(), f"implementation artifact missing: {relative}")
        require(re.fullmatch(r"[0-9a-f]{64}", digest) is not None, f"bad artifact digest: {relative}")
        require(sha256(path) == digest, f"implementation artifact drift: {relative}")

    transaction = authority["transaction"]
    phases = transaction["phases"]
    require(transaction["phase_count"] == len(phases) == 6, "phase count drift")
    require(transaction["service_invocation_count"] == sum(len(item["services"]) for item in phases) == 31, "service arithmetic drift")
    require([len(item["services"]) for item in phases] == [4, 2, 11, 11, 2, 1], "phase arithmetic drift")
    for key in (
        "all_targets_refuse_manual_start",
        "all_targets_stop_when_unneeded",
        "failed_component_blocks_downstream",
    ):
        require(transaction[key] is True, f"transaction invariant drift: {key}")
    for key in (
        "oneshot_remain_after_exit_allowed",
        "condition_skip_allowed",
        "prior_active_state_may_satisfy_new_run",
    ):
        require(transaction[key] is False, f"transaction invariant opened: {key}")
    require([item["ordinal"] for item in phases] == list(range(1, 7)), "phase ordinal drift")
    require(len({service for phase in phases for service in phase["services"]}) == 31, "duplicate service invocation")

    target_names = [item["target"] for item in phases]
    require(len(set(target_names)) == 6, "duplicate phase target")
    for index, phase in enumerate(phases):
        path = root / TARGET_DIR / phase["target"]
        require(path.is_file(), f"phase target missing: {phase['target']}")
        required = assignments(path, "Requires")
        after = assignments(path, "After")
        expected = list(phase["services"])
        if index:
            require(phase["after_target"] == phases[index - 1]["target"], "authority phase edge drift")
            expected = [phase["after_target"], *expected]
        else:
            require(phase["after_target"] is None, "phase one predecessor opened")
        require(required == expected, f"Requires graph drift: {phase['target']}")
        require(after == expected, f"After graph drift: {phase['target']}")
        text = path.read_text(encoding="utf-8")
        require(text.count("RefuseManualStart=yes") == 1, f"manual start opened: {phase['target']}")
        require(text.count("StopWhenUnneeded=yes") == 1, f"stale active target opened: {phase['target']}")
        require("[Install]" not in text, f"install section opened: {phase['target']}")

    aggregate = root / TARGET_DIR / transaction["aggregate_target"]
    aggregate_text = aggregate.read_text(encoding="utf-8")
    require(assignments(aggregate, "Requires") == [phases[-1]["target"]], "aggregate Requires drift")
    require(assignments(aggregate, "After") == [phases[-1]["target"]], "aggregate After drift")
    require("RefuseManualStart=yes" in aggregate_text and "StopWhenUnneeded=yes" in aggregate_text, "aggregate replay barrier drift")
    require("[Install]" not in aggregate_text, "aggregate install opened")

    all_units = [root / TARGET_DIR / name for name in target_names]
    all_units += [aggregate]
    all_units += [root / relative for relative in artifacts if relative.endswith(".service")]
    for path in all_units:
        text = path.read_text(encoding="utf-8")
        require("RemainAfterExit=" not in text, f"retained oneshot state opened: {path.name}")
        require("ConditionPath" not in text, f"condition skip barrier opened: {path.name}")
        require("WantedBy=" not in text and "RequiredBy=" not in text and "Alias=" not in text, f"unit activation opened: {path.name}")

    builder_unit = (root / TARGET_DIR / "moex-stage8b-r2b-run-package-draft-builder.service").read_text(encoding="utf-8")
    require("User=root" in builder_unit and "Group=root" in builder_unit, "builder unit identity drift")
    require(assignments(root / TARGET_DIR / "moex-stage8b-r2b-run-package-draft-builder.service", "RestrictAddressFamilies") == ["AF_UNIX"], "builder address-family isolation drift")
    require(assignments(root / TARGET_DIR / "moex-stage8b-r2b-run-package-draft-builder.service", "IPAddressDeny") == ["any"], "builder network deny drift")
    require(assignments(root / TARGET_DIR / "moex-stage8b-r2b-run-package-draft-builder.service", "ExecStart") == [builder["executable"]], "builder executable or caller argument drift")
    require("package-authorization.ed25519" not in builder_unit and "/run/credentials" not in builder_unit, "builder signing credential exposed")
    require("FINAM" not in builder_unit and "finam" not in builder_unit, "builder FINAM access marker")
    require("Requires=moex-stage8b-r2b-phase4-authority-issuers.target" in builder_unit, "builder phase barrier removed")

    signer_unit = (root / TARGET_DIR / "moex-stage8b-r2b-package-issuer.service").read_text(encoding="utf-8")
    require(assignments(root / TARGET_DIR / "moex-stage8b-r2b-package-issuer.service", "ExecStart") == [signer["executable"]], "signer executable or caller argument drift")
    require("Requires=moex-stage8b-r2b-run-package-draft-builder.service" in signer_unit, "signer builder barrier removed")
    require("After=moex-stage8b-r2b-run-package-draft-builder.service" in signer_unit, "signer builder ordering removed")
    require(signer["signing_key"] in signer_unit, "signer credential custody drift")

    supervisor = (root / TARGET_DIR / "moex-stage8b-r2b-readonly-supervisor.service").read_text(encoding="utf-8")
    require("Requires=moex-stage8b-r2b-phase5-run-package.target" in supervisor, "supervisor package barrier removed")
    require("After=local-fs.target moex-stage8b-r2b-phase5-run-package.target" in supervisor, "supervisor ordering removed")

    builder_binary = (root / BUILDER_SOURCE).read_text(encoding="utf-8")
    require("build_run_package_draft_from_fixed_inputs()" in builder_binary, "fixed builder entry removed")
    require("std::env::args" not in builder_binary and "clap" not in builder_binary, "builder caller arguments opened")
    core = (root / CORE_SOURCE).read_text(encoding="utf-8")
    for marker in (
        "pub fn build_run_package_draft_from_fixed_inputs()",
        "fn validate_unsigned_draft_inputs(",
        "fn atomic_create_owned_mode(",
        "std::fs::hard_link(&temporary, path)",
        "File::open(parent)?.sync_all()?",
        'state_root.join("r2b-run-package.unsigned.json")',
        'etc_root.join("r2b-run-package.json")',
        'chrono::Duration::seconds(30)',
        "r2a3::validate_signed_authorities",
        "source_generation_commitment(&envelope.receipts)?",
        "load_accepted_helper_authority",
        'etc_root.join("operator-decision.json")',
        'etc_root.join("account-key-manifest.json")',
        "run_package_atomic_create_is_no_replace_and_symlink_safe",
    ):
        require(marker in core, f"implementation semantic marker missing: {marker}")
    create_body = core.split("fn atomic_create_owned_mode(", 1)[1].split("\nfn produce_from_store_at(", 1)[0]
    builder_body = core.split("fn build_run_package_draft_at(", 1)[1].split("\nfn strict_single_line(", 1)[0]
    signer_validation_body = core.split("fn validate_unsigned_draft_inputs(", 1)[1].split("\n/// Builds the unsigned", 1)[0]
    signer_issue_body = core.split("pub fn issue_run_package_from_fixed_draft()", 1)[1].split("\nfn validate_unsigned_draft_inputs(", 1)[0]
    require("std::fs::hard_link(&temporary, path)" in create_body, "atomic create no-replace removed")
    require("std::fs::remove_file(&temporary)?;\n    File::open(parent)?.sync_all()?" in create_body, "atomic create directory fsync removed")
    require("expires_at_utc: now + chrono::Duration::seconds(30)" in builder_body, "builder TTL drift")
    require(".num_seconds()\n            != 30" in signer_validation_body, "signer exact TTL validation removed")
    require("validate_unsigned_draft_inputs(" in signer_issue_body, "signer independent revalidation removed")
    require("load_accepted_helper_authority" in signer_issue_body and "load_accepted_helper_authority" in builder_body, "helper binding removed")
    for marker in (
        "r2a3::validate_signed_authorities",
        "source_generation_commitment(&envelope.receipts)?",
        'etc_root.join("operator-decision.json")',
        'etc_root.join("account-key-manifest.json")',
    ):
        require(marker in builder_body and marker in signer_validation_body, f"builder/signer binding removed: {marker}")
    require(core.count('state_root.join("r2b-run-package.unsigned.json")') == 3, "unsigned package path inventory drift")
    require(core.count('etc_root.join("r2b-run-package.json")') == 2, "signed package path inventory drift")

    # No repository activation source may pull the aggregate target. Its only
    # other appearance is the phase-6 ordering declaration and review/control artifacts.
    for path in (root / "deploy").rglob("*"):
        if not path.is_file() or path == aggregate:
            continue
        text = path.read_text(encoding="utf-8", errors="ignore")
        if transaction["aggregate_target"] in text:
            require(path.name == phases[-1]["target"] and f"Before={transaction['aggregate_target']}" in text, f"aggregate pulled by deploy artifact: {path.relative_to(root)}")
    for search_root in (root / "deploy", root / "scripts", root / ".github"):
        if not search_root.exists():
            continue
        for suffix in ("*.timer", "*.path", "*.socket", "*.preset"):
            for path in search_root.rglob(suffix):
                require(transaction["aggregate_target"] not in path.read_text(encoding="utf-8", errors="ignore"), f"aggregate activated by {path.relative_to(root)}")

    state = authority["repository_state"]
    require(state == {
        "source_artifacts_implemented": True,
        "units_and_targets_implemented": True,
        "installed": False,
        "enabled": False,
        "started": False,
        "operator_selected": False,
        "run_nonce_issued": False,
        "credentials_materialized": False,
        "unsigned_run_package_present": False,
        "signed_run_package_present": False,
    }, "repository state drift")
    require(authority["authorization"] == {"r2b": "NOT_ISSUED", "activation_authority_present": False, "operator_arm_issued": False}, "authorization opened")
    require(authority["closed_surfaces"] == {
        "finam_requests": 0,
        "auth_service_called": False,
        "broker_account_get_sent": False,
        "order_post_sent": False,
        "order_delete_sent": False,
        "redis_live_consumer": False,
        "broker_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
    }, "closed surface opened")

    require(evidence["accepted_predecessor"] == PREDECESSOR, "evidence predecessor drift")
    require(evidence["authorization"] == "NOT_ISSUED", "evidence authorization opened")
    require(evidence["systemd"]["static_parser_result"] == "PASS", "systemd static evidence drift")
    require(evidence["systemd"]["unit_count"] == 18 and evidence["systemd"]["parser_warnings"] == 0, "systemd unit evidence drift")
    linux_verify = evidence["systemd"]["isolated_linux_verify"]
    require(linux_verify["exit_code"] == 0 and linux_verify["verified_units"] == 18, "Linux systemd verify failed")
    require(linux_verify["services_started"] == 0 and linux_verify["result"] == "PASS", "Linux systemd verification opened execution")
    require(evidence["activation_closure"] == {
        "install_manifest_present": False,
        "install_section_present": False,
        "enabled": False,
        "started": False,
        "pulled_by_installed_target": False,
        "timer_path_socket_reference": False,
        "preset_reference": False,
        "operator_selected": False,
        "run_nonce_present": False,
        "credential_present": False,
        "unsigned_package_present": False,
        "signed_package_present": False,
    }, "activation evidence opened")
    for key in ("auth_service_called", "broker_get_sent", "post_delete_sent", "redis_live_consumer", "broker_dispatch", "runtime_live", "real_orders"):
        require(evidence[key] is False, f"evidence closed surface opened: {key}")
    require(evidence["finam_requests"] == 0 and evidence["result"] == "PASS_REVIEW_REQUIRED", "evidence result drift")

    rows = list(csv.DictReader((root / MATRIX).read_text(encoding="utf-8").splitlines()))
    require(len(rows) == 52, "acceptance matrix row count drift")
    require(len({row["id"] for row in rows}) == 52, "duplicate acceptance id")
    require(all(row["status"] == "pass" for row in rows), "acceptance matrix not green")


def main() -> None:
    check(ROOT)
    print("stage8b-p-r2b-implementation-r0-check: PASS phases=6 services=31 artifacts=16 rows=52 installed=false enabled=false started=false authorization=NOT_ISSUED finam_requests=0")


if __name__ == "__main__":
    try:
        main()
    except (KeyError, OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2b-implementation-r0-check: FAIL {error}") from error
