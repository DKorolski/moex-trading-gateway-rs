#!/usr/bin/env python3
"""Fail-closed checker for the Stage 8B-P1-f R4 rollback-boundary design."""

from __future__ import annotations

import csv
import hashlib
import json
import subprocess
from pathlib import Path
from typing import Any

import stage8b_p1f_r3_design_check as r3


ROOT = Path(__file__).resolve().parents[1]
BASE = "811ebe8ce22291311bd63fbf0cc5ff723261b758"
BASE_TREE = "82170aace5e735237ae2191e6d3cf04cb2681d19"
REVIEW_SHA256 = "585872992198d80a0f05e76a791303c272b016903e9fbba1bcaf1132cf0365b9"
DOCUMENT = r3.DOCUMENT
INVENTORY = r3.INVENTORY
MATRIX = r3.MATRIX
MODELS = r3.MODELS
TARGET = r3.TARGET
STATUS = r3.STATUS
ROADMAP = r3.ROADMAP
MATRIX_SHA256 = "3107b84010949be4cc5b464069f4ccc653d9ab6657b97796b8bcb8d103600d41"
MODELS_SHA256 = "867cf023e66bab80322363a5998b7b2464b5e862b32c5e7c7c950449aa29ee69"
DOCUMENT_SHA256 = "e67f6be9c36caed6975c131a01356062d8eb52fc7a66e4f24256c6b05232b990"

ALLOWED_CHANGES = {
    DOCUMENT, INVENTORY, MATRIX, MODELS, STATUS, ROADMAP,
    "scripts/make_stage8b_p1f_r4_design_handoff.py",
    "scripts/stage8b_p1f_r4_design_check.py",
    "scripts/stage8b_p1f_r4_design_gate.sh",
    "scripts/stage8b_p1f_r4_design_handoff_safety_check.py",
    "scripts/stage8b_p1f_r4_design_negative_harness.py",
}

UNCHANGED_DIGESTS = dict(r3.UNCHANGED_DIGESTS)
CHANGED_DIGESTS = {
    "freshness_contract": r3.CHANGED_DIGESTS["freshness_contract"],
    "artifact_contracts": r3.CHANGED_DIGESTS["artifact_contracts"],
    "operator_authority": "9de14e85683412b50353c00e0fd2ca4d86d25f2eb755c12cd3d885a4296848e0",
    "phase_lifecycle": "060a8f5f2b07847c3ac03e2319eb52d7f8b2f856f543963bd65d5379f21241bf",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def digest(value: Any) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def read_json(root: Path, relative: str) -> dict[str, Any]:
    try:
        value = json.loads((root / relative).read_text(), object_pairs_hook=r3.r2.strict_object)
    except (OSError, json.JSONDecodeError) as error:
        raise CheckFailure(f"cannot read {relative}: {error}") from error
    require(type(value) is dict, f"{relative} must contain an object")
    return value


def validate_lineage(root: Path) -> None:
    try:
        subprocess.run(["git", "merge-base", "--is-ancestor", BASE, "HEAD"], cwd=root,
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        changed = set(subprocess.check_output(
            ["git", "diff", "--name-only", BASE, "--"], cwd=root, text=True).splitlines())
        changed |= set(subprocess.check_output(
            ["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True).splitlines())
    except (OSError, subprocess.CalledProcessError) as error:
        raise CheckFailure(f"cannot verify P1-f R4 lineage: {error}") from error
    require(changed == ALLOWED_CHANGES, "P1-f R4 changed-path inventory drift")


def validate_custody(value: dict[str, Any]) -> None:
    custody = value["phase_lifecycle"]["authority_custody"]
    expected_keys = {
        "control_root", "control_root_custody", "authority_root",
        "authority_root_custody", "runtime_state_unchanged", "single_writer",
        "history_head_path", "event_path_template", "manifest_directory_template",
        "receipt_custody", "manifest_binding", "claim_transaction",
        "consumed_history", "partial_history_failure", "same_active_restart",
        "new_manifest", "rollback_trust_boundary", "permitted_restore_operations",
        "coherent_full_rollback", "genesis_protocol", "control_loss_rebind",
        "implementation_evidence",
    }
    require(type(custody) is dict and set(custody) == expected_keys,
            "authority custody key inventory drift")
    require(custody["control_root"] == "/var/lib/moex-finam-p1-paper-control",
            "authority control root drift")
    require("parent /var/lib root:root 0755" in custody["control_root_custody"]
            and "root:moex-p1-paper 0750" in custody["control_root_custody"],
            "authority parent custody drift")
    require("cannot create unlink rename or substitute" in custody["authority_root_custody"],
            "service mutation veto drift")
    require("never reconstruct Unclaimed" in custody["partial_history_failure"]
            and "trusted control root retained" in custody["partial_history_failure"],
            "partial history disposition drift")
    require("outside every permitted" in custody["rollback_trust_boundary"]
            and "root operator" in custody["rollback_trust_boundary"],
            "rollback trust boundary drift")
    require(custody["control_root"] in custody["permitted_restore_operations"]
            and "fail before mutation" in custody["permitted_restore_operations"],
            "restore exclusion drift")
    require("not claimed to be locally detectable" in custody["coherent_full_rollback"]
            and "quarantines normal guardian" in custody["coherent_full_rollback"],
            "coherent rollback limit drift")
    require("original deadline" in custody["same_active_restart"]
            and "next authority sequence" in custody["new_manifest"],
            "restart/new-manifest custody drift")

    genesis = custody["genesis_protocol"]
    require(type(genesis) is dict and set(genesis) == {
        "domain", "command", "manifest_binds", "offline_registry", "local_steps",
        "activation_certificate", "claim_admission", "crash_recovery", "repeat_attempt",
    }, "genesis protocol key inventory drift")
    require(genesis["domain"] == "moex.stage8b.p1f.authority-genesis.v1",
            "genesis domain drift")
    require(genesis["command"].startswith("root guardian initialize-authority only")
            and "ordinary phase claim never creates" in genesis["command"],
            "ordinary claim gained genesis authority")
    require(genesis["manifest_binds"] == [
        "installation_instance_sha256", "target_host_key", "control_root",
        "authority_generation", "genesis_head_sha256", "ceremony_nonce",
        "not_before_utc", "expires_at_utc",
    ], "genesis manifest binding drift")
    require("outside VPS" in genesis["offline_registry"]
            and "never reissues" in genesis["offline_registry"],
            "offline genesis registry drift")
    require("claims remain blocked" in genesis["local_steps"]
            and "fsync" in genesis["local_steps"], "genesis local transaction drift")
    require("marks generation Activated" in genesis["activation_certificate"]
            and all(item in genesis["activation_certificate"]
                    for item in ("generation", "genesis head", "installation", "host")),
            "genesis activation certificate drift")
    require("Activated certificate" in genesis["claim_admission"]
            and "sequence 1" in genesis["claim_admission"],
            "pre-activation claim admission drift")
    require("same genesis transaction" in genesis["crash_recovery"]
            and "permanently rejected" in genesis["crash_recovery"],
            "genesis crash/replay drift")
    require("absence after prior activation is control-state loss" in genesis["repeat_attempt"],
            "lost authority treated as fresh genesis")

    rebind = custody["control_loss_rebind"]
    require(type(rebind) is dict and set(rebind) == {
        "triggers", "normal_start", "required_authority", "generation_rule",
        "current_stage_authorized",
    }, "control-loss rebind key inventory drift")
    require(type(rebind["triggers"]) is list and len(rebind["triggers"]) == 4
            and "declared whole-host snapshot restore" in rebind["triggers"]
            and "suspected coherent control-state rollback" in rebind["triggers"],
            "control-loss trigger inventory drift")
    require("quarantined" in rebind["normal_start"]
            and "no ordinary claim" in rebind["normal_start"],
            "control-loss quarantine drift")
    require("separate independently reviewed" in rebind["required_authority"],
            "rebind review boundary drift")
    require("distinct generation" in rebind["generation_rule"]
            and "old generation" in rebind["generation_rule"],
            "rebind generation rule drift")
    require(rebind["current_stage_authorized"] is False,
            "authority rebind self-authorized")

    operator = value["operator_authority"]
    require(operator["ordinary_claim_cannot_create_genesis"] is True
            and operator["control_state_loss_requires_new_generation_rebind"] is True,
            "operator genesis/rebind boundary drift")
    require("restore tooling rejects control-root overlap" in custody["implementation_evidence"]
            and "repeated genesis" in custody["implementation_evidence"],
            "implementation evidence boundary drift")


def validate_inventory(root: Path) -> dict[str, Any]:
    value = read_json(root, INVENTORY)
    require(set(value) == r3.r2.TOP_LEVEL_KEYS, "design top-level key inventory drift")
    require(value["schema_version"] == 5 and type(value["schema_version"]) is int,
            "design schema drift")
    require(value["stage"] == "Stage 8B-P1-f R4 isolated operational acceptance design correction",
            "stage drift")
    require(value["status"] == "DESIGN_CORRECTION_REVIEW_CANDIDATE_NO_ACTIVATION",
            "design self-accepted")
    r3.r2.exact(value["correction_parent"], {
        "commit": BASE, "tree": BASE_TREE, "review_sha256": REVIEW_SHA256,
    }, "correction parent")
    require(value["target_baseline"] == TARGET and value["model_fixtures"] == MODELS,
            "design path binding drift")
    for key, expected in UNCHANGED_DIGESTS.items():
        require(digest(value[key]) == expected, f"unchanged {key} drift")
    phases = list(r3.r2.r1.PHASES)
    phases[0] = dict(phases[0], requires_prior_acceptance="P1F-R4-design")
    r3.r2.exact(value["phases"], phases, "phases")
    validate_custody(value)
    r3.validate_redis(root, value)
    for key, expected in CHANGED_DIGESTS.items():
        require(digest(value[key]) == expected, f"R4 {key} contract drift")
    return value


def validate_models(root: Path) -> None:
    require(sha256(root / MODELS) == MODELS_SHA256, "model fixture bytes drift")
    value = read_json(root, MODELS)
    require(value["schema_version"] == 4
            and value["domain"] == "moex.stage8b.p1f.operational-model-fixtures.v4"
            and value["status"] == "DESIGN_MODEL_ONLY_NO_OPERATION",
            "model header drift")
    cases = value["cases"]
    require([row.get("id") for row in cases] == [f"P1F-M{index:02}" for index in range(1, 46)],
            "model case inventory drift")
    require(all(type(row) is dict and set(row) == {"id", "area", "given", "event", "expected"}
                and all(type(item) is str and item for item in row.values()) for row in cases),
            "model case schema drift")
    expected = {row["id"]: row["expected"] for row in cases}
    require("ordinary duplicate" in expected["P1F-M40"]
            and "sequence and predecessor mismatch" in expected["P1F-M41"]
            and "cannot be claimed again" in expected["P1F-M42"],
            "duplicate/partial rollback models drift")
    require("not claimed" in expected["P1F-M43"]
            and "quarantines" in expected["P1F-M43"]
            and "new-generation" in expected["P1F-M43"],
            "coherent rollback model drift")
    require("ordinary phase claim never creates genesis" in expected["P1F-M44"]
            and "Repeated genesis".lower() in expected["P1F-M45"].lower()
            and "control-state loss" in expected["P1F-M45"],
            "genesis model drift")
    require("original deadline" in expected["P1F-M38"],
            "same-Active restart model drift")


def validate_matrix(root: Path) -> None:
    require(sha256(root / MATRIX) == MATRIX_SHA256, "acceptance matrix bytes drift")
    with (root / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(rows and list(rows[0]) == ["id", "area", "requirement", "status"],
            "matrix header drift")
    require([row["id"] for row in rows] == [f"P1F-R4-{index:03}" for index in range(1, 79)],
            "matrix row inventory drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "optional acceptance row")


def validate_documents(root: Path) -> None:
    require(sha256(root / DOCUMENT) == DOCUMENT_SHA256, "design document bytes drift")
    document = (root / DOCUMENT).read_text()
    status = (root / STATUS).read_text()
    roadmap = (root / ROADMAP).read_text()
    for fragment in (
        "Stage 8B-P1-f R4", BASE, "trusted non-rollback authority",
        "not** claimed", "quarantined", "Genesis is a separate one-time operation",
        "ordinary claims remain blocked", "45 positive/fail-closed cases",
        "opens only `P1F-I`",
    ):
        require(fragment in document, f"design document fragment missing: {fragment}")
    require("active P1-f R4 design/checker correction candidate" in status
            and "grants no operational authority" in status,
            "current status R4 boundary missing")
    require("P1-f R0 through R3 were held" in roadmap
            and "P1F-I remains closed until R4\nacceptance" in roadmap,
            "roadmap R4 hold boundary missing")


def validate(root: Path = ROOT, *, verify_lineage: bool = True) -> None:
    if verify_lineage:
        validate_lineage(root)
    r3.r2.r1.r0.validate_target(root)
    validate_inventory(root)
    validate_models(root)
    validate_matrix(root)
    validate_documents(root)


def main() -> int:
    try:
        validate()
    except (CheckFailure, r3.CheckFailure, r3.r2.CheckFailure, r3.r2.r1.CheckFailure,
            r3.r2.r1.r0.CheckFailure, OSError, UnicodeDecodeError) as error:
        print(f"stage8b-p1f-r4-design-check: FAIL {error}")
        return 1
    print("stage8b-p1f-r4-design-check: PASS rows=78 models=45 artifacts=9 "
          "scripts=8 operations=10 traces=2 routes=6 roles=8 phases=7 activation=false")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
