#!/usr/bin/env python3
"""Fail-closed checker for the Stage 8B-P1-f R3 design correction."""

from __future__ import annotations

import csv
import hashlib
import json
import re
import subprocess
from pathlib import Path
from typing import Any

import stage8b_p1f_r2_design_check as r2


ROOT = Path(__file__).resolve().parents[1]
BASE = "eeac091635f102fa5b7dc9db3564c214e2efefe0"
BASE_TREE = "6efd543c9cf8ca496c92d37cebca0df528c3698e"
REVIEW_SHA256 = "cdb156224dc76349b20a710d043bbf069c5a38e4231546e831f723cf105a2fb6"
DOCUMENT = r2.DOCUMENT
INVENTORY = r2.INVENTORY
MATRIX = r2.MATRIX
MODELS = r2.MODELS
TARGET = r2.TARGET
STATUS = r2.STATUS
ROADMAP = r2.ROADMAP
MATRIX_SHA256 = "9f7c61cd485fe0081147441205d0ae3a9dc5dea0fa3c1f1e31ac28a23cfa0f77"
MODELS_SHA256 = "bc50d2e23acb6623c96c9102b85b085536337df0f8c0093cb5d104c7eff0314b"
DOCUMENT_SHA256 = "0526cd4d5f2977c3e5ee4e7abbc94ac508c7ea9266a9538b7f846e5ad7ac5bde"

ALLOWED_CHANGES = {
    DOCUMENT, INVENTORY, MATRIX, MODELS, STATUS, ROADMAP,
    "scripts/make_stage8b_p1f_r3_design_handoff.py",
    "scripts/stage8b_p1f_r3_design_check.py",
    "scripts/stage8b_p1f_r3_design_gate.sh",
    "scripts/stage8b_p1f_r3_design_handoff_safety_check.py",
    "scripts/stage8b_p1f_r3_design_negative_harness.py",
}

UNCHANGED_DIGESTS = {
    "accepted_predecessor": "7e7ef327bcf9309fd82b6ebdd38276e1a2ea1d2b93843df85fd4ec995fbab114",
    "isolation": "62aa6a2e4b779072c2d3257f5f48099f536dbeb06ff35c752bb71dbd852f0d2c",
    "session_policy": "812579b3bd32a44369bd29773f9c8b36cf95e64f4a4984bd4c90ff49a737b609",
    "synthetic_phase": "988fa1452462cd8cc1b78fbfd1b764e90f5d4dd4313e60248bac68b908df274e",
    "finam_bars_phase": "f2436cf14c8e8d5a39e271ec512c03936523039743f6b9865b1e5cfe8650a5a5",
    "publisher_continuity": "2bcee049c9ba787d27501c6db871ebf54a46745580c10da72a10050364806139",
    "restart_scenarios": "fc8fa92a7e1fc304fde04b6ac608f5e8efe8d99a2b3aa04c2306f88896bdcdb0",
    "evidence": "18c7a88b32b62a19b84f62bcfaeb7da421f6ab7e0994de8c2433eba7cd687eb9",
    "closed_surfaces": "863648ac1fa2183a8968202b8b00f8a0ddab0a0b530815834ba70f4070ce8797",
}
CHANGED_DIGESTS = {
    "freshness_contract": "b5e5dc25e286a95b7e165ec9de634746998c4089fab9161d04e1f50d5c1d0b98",
    "artifact_contracts": "2c281bb634daadbcd6dbd5a6bca6e0d0de6f56730dec548610b0dba3eae1b5f5",
    "operator_authority": "2bd8e2ac8514d8c0daff9bca49650f89fddd4379418f11e0037c08866d591ffc",
    "phase_lifecycle": "90453317d2bea9a28223ea46e0fb6ae8b72ad9948c41039136662a43204d269a",
}
REDIS_DIGESTS = {
    "scripts": "14854d208116e3beea97452d7373b0a033f45781dbdaf8198d90d9982fa1ff54",
    "source_operations": "d904feed825128e2ce090886ff19e0a4fa4fe0346d7f2d02718b6bd0ed6702f0",
    "public_operation_traces": "e56703067c004bc1ced9306f59aa4b334f18efdc6f4e1d80fd9e1af705ea020c",
    "schedule_route_frontiers": r2.REDIS_CONTRACT_DIGESTS["schedule_route_frontiers"],
    "roles": "fc26557df04f65012b6e1af2f14675359a4855f5bc63da20f401ecc7e61196b8",
    "p0_protection_evidence": "178491c304571349246148035ed6d45209b88cfebc8daa41378c47b79fc73ed5",
    "resource_limits": "b50f6775ba72f6e8a094e2b61454718a9a90cd75db11ab6ef2c3152d0fcc3785",
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
        value = json.loads((root / relative).read_text(), object_pairs_hook=r2.strict_object)
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
        raise CheckFailure(f"cannot verify P1-f R3 lineage: {error}") from error
    require(changed == ALLOWED_CHANGES, "P1-f R3 changed-path inventory drift")


def validate_custody(value: dict[str, Any]) -> None:
    lifecycle = value["phase_lifecycle"]
    custody = lifecycle["authority_custody"]
    expected_keys = {
        "control_root", "control_root_custody", "authority_root",
        "authority_root_custody", "runtime_state_unchanged", "single_writer",
        "history_head_path", "event_path_template", "manifest_directory_template",
        "receipt_custody", "manifest_binding", "claim_transaction",
        "consumed_history", "missing_corrupt_or_rollback", "same_active_restart",
        "new_manifest", "implementation_evidence",
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
    require("state remains moex-p1-paper:moex-p1-paper 0700" in custody["runtime_state_unchanged"]
            and "no phase authority" in custody["runtime_state_unchanged"],
            "runtime state/authority separation drift")
    require(custody["single_writer"].startswith("root P1-f phase guardian only"),
            "authority single-writer drift")
    require("authority_sequence" in custody["manifest_binding"]
            and "predecessor_event_sha256" in custody["manifest_binding"],
            "history predecessor binding drift")
    require("before first effect" in custody["claim_transaction"]
            and "fsync" in custody["claim_transaction"], "claim transaction drift")
    require("permanently consume" in custody["consumed_history"],
            "consumed-manifest history drift")
    require("never reconstruct Unclaimed" in custody["missing_corrupt_or_rollback"]
            and "administrative recovery" in custody["missing_corrupt_or_rollback"],
            "history rollback disposition drift")
    require("original deadline" in custody["same_active_restart"]
            and "next authority sequence" in custody["new_manifest"],
            "restart/new-manifest custody drift")
    for state in ("Claimed", "ReadyForBootstrap"):
        row = next(item for item in value["freshness_contract"]["o2_materialization"]
                   ["identity_chain"] if item["state"] == state)
        require(row["path"].startswith(custody["manifest_directory_template"]),
                f"{state} receipt outside protected authority root")
    require("root:moex-p1-paper 0440" in value["artifact_contracts"][2]["custody"]
            and custody["control_root"] in value["artifact_contracts"][2]["custody"],
            "materialized receipt custody drift")


def validate_redis(root: Path, value: dict[str, Any]) -> None:
    redis = value["redis_capabilities"]
    expected_keys = {
        "enforcement", "mutating_roles_database", "mutating_roles_key_pattern",
        "global_forbidden_commands", "scripts", "source_operations",
        "public_operation_traces", "schedule_route_frontiers", "roles",
        "p0_protection_evidence", "resource_limits",
    }
    require(type(redis) is dict and set(redis) == expected_keys,
            "Redis capability inventory drift")
    r2.exact(redis["global_forbidden_commands"], r2.r1.FORBIDDEN_COMMANDS,
             "forbidden Redis commands")
    require(redis["mutating_roles_database"] == 15
            and redis["mutating_roles_key_pattern"] == "finam_imoexf_paper:{finam-imoexf-p1}:*",
            "Redis DB/key boundary drift")
    scripts = {row["id"]: row for row in redis["scripts"]}
    require(list(scripts) == r2.SCRIPT_IDS, "Redis script inventory drift")
    for script_id, expected_hash in r2.SCRIPT_HASHES.items():
        require(scripts[script_id]["sha256"] == expected_hash,
                f"Redis script hash drift: {script_id}")
    r2.exact(r2.rust_script_hashes(root), r2.SCRIPT_HASHES,
             "production Redis script hashes")
    require("XINFO STREAM exact-command-stream predecessor last-generated-id"
            in scripts["p1d4-command-publication-v1"]["nested_commands"],
            "reserved publication XINFO STREAM capability missing")
    source = (root / "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs").read_text()
    body = re.search(r'const P1D4_COMMAND_PUBLICATION_LUA: &str = r#"(.*?)"#;', source, re.DOTALL)
    require(body is not None and "redis.call('XINFO', 'STREAM', stream)" in body.group(1),
            "production reserved publication XINFO STREAM missing")
    operations = {row["id"]: row for row in redis["source_operations"]}
    require(list(operations) == r2.OPERATION_IDS, "source operation inventory drift")
    require(all(type(row) is dict and set(row) == {
        "id", "role", "source", "command", "required_capabilities", "response_loss",
    } and type(row["required_capabilities"]) is list
        and all(type(item) is str and item for item in row["required_capabilities"])
        for row in operations.values()), "source operation schema drift")
    fresh = operations["fresh-namespace"]
    require(fresh["source"] == "initialize_stage8b_p1_redis_namespace public initializer"
            and "then EVAL namespace-verify-v1" in fresh["command"]
            and "rerun full ordered trace" in fresh["response_loss"],
            "public initializer trace drift")
    traces = {row["id"]: row for row in redis["public_operation_traces"]}
    require(list(traces) == ["initialize-stage8b-p1-redis-namespace",
                             "publish-generated-market-reserved"],
            "public conformance trace inventory drift")
    roles = {row["role"]: row for row in redis["roles"]}
    operation_roles = {
        "synthetic-m10-feeder-or-finam-bars-feeder": [
            "synthetic-m10-feeder", "finam-bars-feeder"
        ],
    }
    for operation in operations.values():
        role_names = operation_roles.get(operation["role"], [operation["role"]])
        require(all(role_name in roles for role_name in role_names),
                f"source operation role missing: {operation['id']}")
        require(all(capability in roles[role_name]["allowed"]
                    for role_name in role_names
                    for capability in operation["required_capabilities"]),
                f"source operation unreachable: {operation['id']}")
    init_trace = traces["initialize-stage8b-p1-redis-namespace"]
    require(init_trace["ordered_steps"] == [
        "EVAL namespace-initialization-v1 exact-2-keys-2-argv",
        "EVAL namespace-verify-v1 exact-same-2-keys-2-argv",
    ], "initializer ordered steps drift")
    require(all(capability in roles["provisioner"]["allowed"]
                for capability in init_trace["required_capabilities"]),
            "provisioner cannot reach complete public initializer")
    reserved = traces["publish-generated-market-reserved"]
    require(all(capability in roles["supervisor"]["allowed"]
                for capability in reserved["required_capabilities"]),
            "supervisor cannot reach reserved publication")
    require("XINFO STREAM exact-command-stream predecessor last-generated-id"
            in reserved["nested_authority"], "reserved trace predecessor authority drift")
    require(not any(item == "EVAL" or "arbitrary" in item for row in redis["roles"]
                    for item in row["allowed"]), "arbitrary EVAL capability opened")
    for key, expected in REDIS_DIGESTS.items():
        require(digest(redis[key]) == expected, f"Redis {key} contract digest drift")


def validate_inventory(root: Path) -> dict[str, Any]:
    value = read_json(root, INVENTORY)
    require(set(value) == r2.TOP_LEVEL_KEYS, "design top-level key inventory drift")
    require(value["schema_version"] == 4 and type(value["schema_version"]) is int,
            "design schema drift")
    require(value["stage"] == "Stage 8B-P1-f R3 isolated operational acceptance design correction",
            "stage drift")
    require(value["status"] == "DESIGN_CORRECTION_REVIEW_CANDIDATE_NO_ACTIVATION",
            "design self-accepted")
    r2.exact(value["correction_parent"], {
        "commit": BASE, "tree": BASE_TREE, "review_sha256": REVIEW_SHA256,
    }, "correction parent")
    require(value["target_baseline"] == TARGET and value["model_fixtures"] == MODELS,
            "design path binding drift")
    for key, expected in UNCHANGED_DIGESTS.items():
        require(digest(value[key]) == expected, f"unchanged {key} drift")
    phases = list(r2.r1.PHASES)
    phases[0] = dict(phases[0], requires_prior_acceptance="P1F-R3-design")
    r2.exact(value["phases"], phases, "phases")
    validate_custody(value)
    validate_redis(root, value)
    for key, expected in CHANGED_DIGESTS.items():
        require(digest(value[key]) == expected, f"R3 {key} contract drift")
    return value


def validate_models(root: Path) -> None:
    require(sha256(root / MODELS) == MODELS_SHA256, "model fixture bytes drift")
    value = read_json(root, MODELS)
    require(value["schema_version"] == 3
            and value["domain"] == "moex.stage8b.p1f.operational-model-fixtures.v3"
            and value["status"] == "DESIGN_MODEL_ONLY_NO_OPERATION",
            "model header drift")
    cases = value["cases"]
    require([row.get("id") for row in cases] == [f"P1F-M{index:02}" for index in range(1, 40)],
            "model case inventory drift")
    require(all(type(row) is dict and set(row) == {"id", "area", "given", "event", "expected"}
                and all(type(item) is str and item for item in row.values()) for row in cases),
            "model case schema drift")
    expected = {row["id"]: row["expected"] for row in cases}
    require("namespace-verify-v1" in expected["P1F-M31"]
            and "complete ordered" in expected["P1F-M32"], "initializer models drift")
    require("XINFO STREAM" in expected["P1F-M34"]
            and "before XADD" in expected["P1F-M35"], "reserved publication models drift")
    require("denies every mutation" in expected["P1F-M36"]
            and "without reconstructing Unclaimed" in expected["P1F-M37"],
            "custody failure models drift")
    require("original deadline" in expected["P1F-M38"]
            and "loser fails before effect" in expected["P1F-M39"],
            "custody continuation models drift")


def validate_matrix(root: Path) -> None:
    require(sha256(root / MATRIX) == MATRIX_SHA256, "acceptance matrix bytes drift")
    with (root / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(rows and list(rows[0]) == ["id", "area", "requirement", "status"],
            "matrix header drift")
    require([row["id"] for row in rows] == [f"P1F-R3-{index:03}" for index in range(1, 73)],
            "matrix row inventory drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "optional acceptance row")


def validate_documents(root: Path) -> None:
    require(sha256(root / DOCUMENT) == DOCUMENT_SHA256, "design document bytes drift")
    document = (root / DOCUMENT).read_text()
    status = (root / STATUS).read_text()
    roadmap = (root / ROADMAP).read_text()
    for fragment in (
        "Stage 8B-P1-f R3", BASE, "/var/lib/moex-finam-p1-paper-control",
        "never reconstructs `Unclaimed`", "namespace-initialization-v1` and then",
        "`XINFO STREAM`", "39 positive/fail-closed cases", "opens only `P1F-I`",
    ):
        require(fragment in document, f"design document fragment missing: {fragment}")
    require("active P1-f R3 design/checker correction candidate" in status
            and "grants no operational authority" in status, "current status R3 boundary missing")
    require("P1-f R0, R1 and R2 were held" in roadmap
            and "P1F-I remains closed until R3\nacceptance" in roadmap,
            "roadmap R3 hold boundary missing")


def validate(root: Path = ROOT, *, verify_lineage: bool = True) -> None:
    if verify_lineage:
        validate_lineage(root)
    r2.r1.r0.validate_target(root)
    validate_inventory(root)
    validate_models(root)
    validate_matrix(root)
    validate_documents(root)


def main() -> int:
    try:
        validate()
    except (CheckFailure, r2.CheckFailure, r2.r1.CheckFailure, r2.r1.r0.CheckFailure,
            OSError, UnicodeDecodeError) as error:
        print(f"stage8b-p1f-r3-design-check: FAIL {error}")
        return 1
    print("stage8b-p1f-r3-design-check: PASS rows=72 models=39 artifacts=9 "
          "scripts=8 operations=10 traces=2 routes=6 roles=8 phases=7 activation=false")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
