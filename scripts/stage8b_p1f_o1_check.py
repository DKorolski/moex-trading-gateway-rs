#!/usr/bin/env python3
"""Fail-closed source/package-contract gate for Stage 8B-P1-f O1."""

from __future__ import annotations

import csv
import hashlib
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BASE = "9c0560b46dc54132fd65e80a6e3ce89ba13d7832"
BRANCH = "stage8b-paper-shadow-resumption"
ACCEPTED_O0 = "98148b80dacddf44c58204c1af9403bb6b47f8d3"
ACCEPTED_IE = "940377ab2bd406be31547200ca0b8cc3bb0f3e22"
ACCEPTED_IE_TREE = "cb7af50342bf9a96b896cda4ca5ad51b067edca3"
FIXED_INSTALL = "7f2e876c4cad7a3a4a0fa10a1eb5202e58202d2f"
FIXED_INSTALL_TREE = "056227feea871b7f5be684b931f58eb1772346bb"
O0_REVIEW = "FINAM_P1F_O0_SOURCE_EVIDENCE_ACCEPT_98148b8_2026-09-27.md"
O0_REVIEW_SHA256 = "a357514bf2d36ae2a47276da268d87bbd785ee65d00fcb86dcf6f57421061498"
RUST_IMAGE = "rust@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922"
SPEC = "docs/stage-8/stage8b-p1f-o1-non-activating-provisioning.json"
DOC = "docs/stage-8/stage8b-p1f-o1-non-activating-provisioning.md"
MATRIX = "docs/stage-8/stage8b-p1f-o1-acceptance-matrix.csv"
INSTALLER = "scripts/stage8b_p1e_i1_fixed_install.py"
IDENTITY = "docs/stage-8/stage8b-p1e-deployment-identity-v2.json"
PAYLOADS = (
    "deploy/stage8b-p1e/moex-finam-p1-paper.service",
    "deploy/stage8b-p1e/moex-finam-p1-paper-bootstrap.service",
    "deploy/stage8b-p1e/moex-finam-p1-paper-bootstrap-recover@.service",
    "deploy/stage8b-p1e/moex-finam-p1-paper.sysusers",
    "deploy/stage8b-p1e/moex-finam-p1-paper.tmpfiles",
)
ALLOWED_CHANGES = {
    "docs/current-status.md",
    "docs/roadmap.md",
    DOC,
    SPEC,
    MATRIX,
    "scripts/stage8b_p1f_o1_check.py",
    "scripts/stage8b_p1f_o1_negative_harness.py",
    "scripts/make_stage8b_p1f_o1_handoff.py",
    "scripts/stage8b_p1f_o1_handoff_safety_check.py",
}


class CheckError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckError(message)


def git(*args: str, root: Path = ROOT) -> bytes:
    return subprocess.check_output(("git", *args), cwd=root)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def strict_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(path: Path) -> dict[str, object]:
    value = json.loads(path.read_text(), object_pairs_hook=strict_object)
    require(isinstance(value, dict), "spec must be an object")
    return value


def validate_spec(value: dict[str, object], root: Path = ROOT) -> None:
    require(value.get("schema_version") == 1, "schema drift")
    require(value.get("status") == "REVIEW_CANDIDATE_PACKAGE_PREPARED_EXECUTION_NOT_AUTHORIZED", "status drift")
    o0 = value.get("accepted_o0")
    require(isinstance(o0, dict), "accepted O0 missing")
    require(o0.get("source_and_evidence_commit") == ACCEPTED_O0, "accepted O0 drift")
    require(o0.get("governance_closure_commit") == BASE, "O0 closure drift")
    require(o0.get("review_sha256") == O0_REVIEW_SHA256, "O0 review digest drift")

    runtime = value.get("runtime_binary")
    require(isinstance(runtime, dict), "runtime binary missing")
    require(runtime.get("source_ref") == ACCEPTED_IE, "runtime source drift")
    require(runtime.get("source_tree") == ACCEPTED_IE_TREE, "runtime tree drift")
    require(runtime.get("rust_image") == RUST_IMAGE, "Rust image drift")
    require(runtime.get("package") == "runtime-durable-service", "package drift")
    require(runtime.get("binary") == "stage8b-p1-paper-supervisor", "binary drift")
    require(runtime.get("target") == "x86_64-unknown-linux-gnu", "target drift")
    require(runtime.get("profile") == "release" and runtime.get("locked") is True, "build mode drift")

    fixed = value.get("accepted_fixed_install")
    require(isinstance(fixed, dict), "fixed install missing")
    require(fixed.get("source_ref") == FIXED_INSTALL and fixed.get("source_tree") == FIXED_INSTALL_TREE, "fixed install authority drift")
    installer = fixed.get("installer")
    require(isinstance(installer, dict) and installer.get("path") == INSTALLER, "installer path drift")
    require(installer.get("sha256") == sha256((root / INSTALLER).read_bytes()), "installer digest drift")
    identity = fixed.get("deployment_identity")
    require(isinstance(identity, dict) and identity.get("path") == IDENTITY, "identity path drift")
    require(identity.get("sha256") == sha256((root / IDENTITY).read_bytes()), "identity digest drift")
    payloads = fixed.get("public_payloads")
    require(isinstance(payloads, list) and len(payloads) == 5, "payload inventory drift")
    require([item.get("path") for item in payloads if isinstance(item, dict)] == list(PAYLOADS), "payload order/path drift")
    for item in payloads:
        require(isinstance(item, dict), "payload entry type")
        path = item["path"]
        require(item.get("sha256") == sha256((root / path).read_bytes()), f"payload digest drift: {path}")
        require(item.get("owner") == "root" and item.get("group") == "root" and item.get("mode") == "0644", f"payload custody drift: {path}")

    target = value.get("target")
    require(isinstance(target, dict), "target missing")
    require(target == {
        "target_id": "stage8b-p1f-isolated-vps-1",
        "hostname": "nektodk1.ispvds.com",
        "ipv4": "45.150.11.252",
        "ssh_ed25519_fingerprint": "SHA256:8fOAPkfvZ61LYmldUBs6A5ZC+BFr6O3yCvUrBAWZsAo",
    }, "target identity drift")
    bundle = value.get("bundle_contract")
    require(isinstance(bundle, dict), "bundle contract missing")
    require(bundle.get("operator_material_included") is False, "operator material opened")
    require(bundle.get("first_boot_material_included") is False, "first boot material opened")
    require(bundle.get("credential_included") is False, "credential opened")
    require(bundle.get("binary_mode") == "0755" and bundle.get("binary_owner") == "root" and bundle.get("binary_group") == "root", "binary custody drift")
    execution = value.get("execution")
    require(isinstance(execution, dict) and execution.get("authorized") is False, "execution opened")
    require(execution.get("requires_fresh_o0_preflight") is True, "fresh O0 removed")
    require(execution.get("daemon_reload_allowed") is False and execution.get("enable_allowed") is False and execution.get("start_allowed") is False, "activation opened")
    closed = value.get("closed_surfaces")
    require(isinstance(closed, dict) and len(closed) == 12 and all(item is False for item in closed.values()), "closed surface opened")


def validate_source(root: Path = ROOT) -> None:
    require(git("rev-parse", f"{BASE}^{{commit}}", root=root).decode().strip() == BASE, "base commit missing")
    require(git("rev-parse", f"{ACCEPTED_IE}^{{tree}}", root=root).decode().strip() == ACCEPTED_IE_TREE, "accepted Ie tree drift")
    require(git("rev-parse", f"{FIXED_INSTALL}^{{tree}}", root=root).decode().strip() == FIXED_INSTALL_TREE, "fixed install tree drift")
    require(subprocess.run(("git", "diff", "--quiet", ACCEPTED_IE, "HEAD", "--", "Cargo.toml", "Cargo.lock", "crates"), cwd=root).returncode == 0, "Rust/Cargo changed after accepted Ie")
    for path in (INSTALLER, IDENTITY, *PAYLOADS):
        accepted = git("show", f"{FIXED_INSTALL}:{path}", root=root)
        require((root / path).read_bytes() == accepted, f"accepted fixed-install material drift: {path}")
    with (root / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 18 and [row["id"] for row in rows] == [f"O1-{index:02d}" for index in range(1, 19)], "acceptance matrix drift")
    document = (root / DOC).read_text()
    for phrase in (
        "EXECUTION NOT AUTHORIZED",
        "performs no SSH connection",
        "remote mutation",
        "do not run `systemctl daemon-reload`, `enable` or `start`",
        "FINAM POST/DELETE",
    ):
        require(phrase in document, f"documentation boundary missing: {phrase}")


def main() -> None:
    try:
        validate_spec(read_json(ROOT / SPEC))
        validate_source()
    except (OSError, subprocess.CalledProcessError, json.JSONDecodeError, KeyError, TypeError, CheckError) as error:
        print(f"stage8b-p1f-o1-check: FAIL {error}")
        raise SystemExit(1) from error
    print("PASS stage8b-p1f-o1-check matrix=18 execution=false remote_mutation=false")


if __name__ == "__main__":
    main()
