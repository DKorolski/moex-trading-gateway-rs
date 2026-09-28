#!/usr/bin/env python3
"""Validate the narrow Stage 8B-P1-e I1 first-boot governance closure."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


ACCEPTED_SOURCE_REF = "4d7ee64730ffba72d21f9b51a2c65e7d1b8a2721"
ACCEPTED_SOURCE_TREE = "9f79fd31c393a1e5b555202e086de290a5819c3f"
PARENT_HOLD_REF = "21fda88fefdcae9ca555697e32c677f8623ba6de"
ACCEPTED_PREDECESSOR = "8360c4701b6abbe75ced988cf8dd2d74487e1846"
HELD_CLOSURE_REF = "3d43c89a481abc61c3f7541d0807c52a813fb39f"
HELD_CLOSURE_TREE = "7002fe68715ebe8e1b9bb31e5c6188b61e4e7976"
ACCEPTANCE_REVIEW_SHA256 = (
    "d1baa8fe5ca0e6acc37948deceb120bde9906e32aeb14adf75cf7981e80737b1"
)
HOLD_REVIEW_SHA256 = (
    "6ebcd63203cd828cc713938cf77238affebfcbb531dde44c560266d9019966ce"
)
SOURCE_PLAN_V2_SHA256 = (
    "2a507577075b8b5315a462ffeee221dd0a7f8a8f61d42516fbdb9346cc3464ca"
)
WIRE_SCHEMA_V2_SHA256 = (
    "e1cbe59f9ac805a8300141d8e2523ed5cf1fee1b9175ef431d053374fb213594"
)
WIRE_SCHEMA_V1_SHA256 = (
    "415a3bdc8d20755b519b20de941afcbeeb56986fc77845f0462cab6c3a302a64"
)
SOURCE_PLAN_V1_SHA256 = (
    "2b8f32db2aadd9a917b526101c7b407918ce4366e1dbed3e12bb96af5831a0bd"
)
CORRECTION_CONTRACT_SHA256 = (
    "8272664bb809535b58585087c40e71feb876a1b55fa68e6a961902e3eb6b72a7"
)
SOURCE_GATE_SHA256 = (
    "232297389c8457f026f66d632165e8dc4323b89a768c3a53727896b1a44737df"
)

PLAN_PATH = Path("docs/stage-8/stage8b-p1e-first-boot-source-plan-v2.json")
CLOSURE_PATH = Path(
    "docs/stage-8/stage8b-p1e-i1-first-boot-governance-closure-v1.json"
)
SCHEMA_V2_PATH = Path(
    "docs/stage-8/stage8b-p1e-first-boot-source-bundle-schema-v2.json"
)
SCHEMA_V1_PATH = Path(
    "docs/stage-8/stage8b-p1e-first-boot-source-bundle-schema-v1.json"
)
PLAN_V1_PATH = Path("docs/stage-8/stage8b-p1e-first-boot-source-plan-v1.json")
CONTRACT_PATH = Path(
    "docs/stage-8/stage8b-p1e-i1-first-boot-correction-contract-v2.md"
)
SOURCE_GATE_PATH = Path("scripts/stage8b_p1e_i1_first_boot_correction_gate.sh")
GOVERNANCE_CHECKER_PATH = Path(
    "scripts/stage8b_p1e_i1_first_boot_governance_check.py"
)
GOVERNANCE_NEGATIVE_PATH = Path(
    "scripts/stage8b_p1e_i1_first_boot_governance_negative_harness.py"
)
GOVERNANCE_GATE_PATH = Path(
    "scripts/stage8b_p1e_i1_first_boot_governance_gate.sh"
)
STATUS_PATH = Path("docs/current-status.md")
SOURCE_PATH = Path(
    "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs"
)
SUPERVISOR_PATH = Path(
    "crates/runtime-durable-service/src/stage8b_p1_supervisor.rs"
)

FIXED_SOURCE_PATH = (
    "/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json"
)
CLOSED_SURFACES = {
    "operational_redis_db0_db15",
    "vps_activation",
    "operational_keys",
    "finam_post_delete",
    "broker_dispatch",
    "runtime_live",
    "real_orders",
}


class GovernanceCheckError(RuntimeError):
    """Raised when the governance closure contract is not satisfied."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code


def require(condition: bool, message: str, code: str = "contract_violation") -> None:
    if not condition:
        raise GovernanceCheckError(code, message)


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise GovernanceCheckError("content_error", f"cannot load {path}: {error}") from error
    require(isinstance(value, dict), f"{path} must contain a JSON object")
    return value


def require_hash(root: Path, path: Path, expected: str) -> None:
    absolute = root / path
    require(absolute.is_file(), f"missing required file: {path}", "content_error")
    require(
        sha256_file(absolute) == expected,
        f"immutable hash drift: {path}",
        "immutable_hash",
    )


def require_closed_surfaces(value: Any, owner: str) -> None:
    require(isinstance(value, dict), f"{owner}.closed_surfaces must be an object")
    require(set(value) == CLOSED_SURFACES, f"{owner}.closed_surfaces inventory drift")
    require(
        all(item is False for item in value.values()),
        f"{owner} opens an operational or live surface",
    )


def check_content(root: Path) -> None:
    root = root.resolve()

    for path in (
        PLAN_PATH,
        CLOSURE_PATH,
        SCHEMA_V2_PATH,
        SCHEMA_V1_PATH,
        PLAN_V1_PATH,
        CONTRACT_PATH,
        SOURCE_GATE_PATH,
        GOVERNANCE_CHECKER_PATH,
        GOVERNANCE_NEGATIVE_PATH,
        GOVERNANCE_GATE_PATH,
        STATUS_PATH,
        SOURCE_PATH,
        SUPERVISOR_PATH,
    ):
        require(
            (root / path).is_file(),
            f"missing required file: {path}",
            "content_error",
        )

    require_hash(root, PLAN_PATH, SOURCE_PLAN_V2_SHA256)
    require_hash(root, SCHEMA_V2_PATH, WIRE_SCHEMA_V2_SHA256)
    require_hash(root, SCHEMA_V1_PATH, WIRE_SCHEMA_V1_SHA256)
    require_hash(root, PLAN_V1_PATH, SOURCE_PLAN_V1_SHA256)
    require_hash(root, CONTRACT_PATH, CORRECTION_CONTRACT_SHA256)
    require_hash(root, SOURCE_GATE_PATH, SOURCE_GATE_SHA256)

    plan = load_json(root / PLAN_PATH)
    require(plan.get("schema_version") == 2, "active source plan schema must be v2")
    require(
        plan.get("domain") == "moex.stage8b.p1e.first-boot-source-plan.v2",
        "active source plan domain drift",
    )
    require(
        plan.get("status") == "SOURCE_ACCEPTED_GOVERNANCE_CLOSURE_REQUIRED",
        "active source plan status drift",
    )
    require(plan.get("accepted_source_ref") == ACCEPTED_SOURCE_REF, "source ref drift")
    require(
        plan.get("accepted_source_tree") == ACCEPTED_SOURCE_TREE,
        "accepted source tree drift",
    )
    require(
        plan.get("source_acceptance_review_sha256") == ACCEPTANCE_REVIEW_SHA256,
        "source acceptance review binding drift",
    )
    require(
        plan.get("production_facade") == "build_stage8b_p1_first_boot_source_v1",
        "compatibility facade drift",
    )

    wire = plan.get("wire_contract")
    require(isinstance(wire, dict), "wire_contract must be an object")
    require(wire.get("active_schema_version") == 2, "wire v2 is not active")
    require(
        wire.get("active_domain")
        == "moex.stage8b.p1e.first-boot-source-bundle.v2",
        "active wire domain drift",
    )
    require(
        wire.get("active_schema_sha256") == WIRE_SCHEMA_V2_SHA256,
        "active wire schema hash drift",
    )
    require(wire.get("legacy_v1_wire_accepted") is False, "legacy v1 wire reopened")
    require(
        wire.get("historical_schema_sha256") == WIRE_SCHEMA_V1_SHA256,
        "historical wire schema hash drift",
    )
    require(
        wire.get("historical_plan_sha256") == SOURCE_PLAN_V1_SHA256,
        "historical source plan hash drift",
    )

    source_bundle = plan.get("source_bundle")
    require(isinstance(source_bundle, dict), "source_bundle must be an object")
    require(source_bundle.get("fixed_path") == FIXED_SOURCE_PATH, "fixed path drift")
    require(
        source_bundle.get("fixed_path_name_is_api_compatibility_not_wire_version")
        is True,
        "fixed path v1 compatibility meaning is not sealed",
    )
    require(source_bundle.get("path_override_allowed") is False, "path override opened")

    history = plan.get("history")
    require(isinstance(history, dict), "history must be an object")
    require(
        history.get("coverage_authority")
        == "config-bound-explicit-session-windows-v1",
        "history coverage authority drift",
    )
    require(
        history.get("coverage_sessions_sha256_required") is True,
        "history session-window digest is not required",
    )
    require(
        history.get("exact_window_close_timestamp_equality_required") is True,
        "history exact window equality is not required",
    )
    require(
        history.get("universal_exchange_bar_count_assumed") is False,
        "universal exchange bar count assumption reopened",
    )

    candidate = plan.get("candidate")
    require(isinstance(candidate, dict), "candidate must be an object")
    require(
        candidate.get("canonical_builder") == "build_stage8b_p1_canonical_m10",
        "candidate canonical builder drift",
    )
    require(
        candidate.get("canonical_parser") == "parse_stage8b_p1_canonical_m10",
        "candidate canonical parser drift",
    )
    require(candidate.get("exact_source_m1_records") == 10, "candidate M1 cardinality drift")
    require(
        candidate.get("validated_semantic_id_only_crosses_runtime_boundary") is True,
        "unvalidated candidate identity can cross the runtime boundary",
    )
    require_closed_surfaces(plan.get("closed_surfaces"), "source plan")

    closure = load_json(root / CLOSURE_PATH)
    require(closure.get("schema_version") == 1, "closure schema version drift")
    require(
        closure.get("domain")
        == "moex.stage8b.p1e.i1.first-boot-governance-closure.v1",
        "closure domain drift",
    )
    require(
        closure.get("status")
        == "GOVERNANCE_HARNESS_CORRECTION_REVIEW_CANDIDATE",
        "closure must remain a harness-correction review candidate",
    )

    accepted = closure.get("accepted_source")
    require(isinstance(accepted, dict), "accepted_source must be an object")
    require(accepted.get("source_ref") == ACCEPTED_SOURCE_REF, "closure source ref drift")
    require(accepted.get("source_tree") == ACCEPTED_SOURCE_TREE, "closure source tree drift")
    require(accepted.get("parent_hold_ref") == PARENT_HOLD_REF, "HOLD lineage drift")
    require(
        accepted.get("accepted_predecessor") == ACCEPTED_PREDECESSOR,
        "accepted predecessor drift",
    )
    require(
        accepted.get("review_sha256") == ACCEPTANCE_REVIEW_SHA256,
        "closure review hash drift",
    )
    require(accepted.get("verdict") == "SOURCE ACCEPT", "source verdict drift")

    active = closure.get("active_contracts")
    require(isinstance(active, dict), "active_contracts must be an object")
    require(active.get("source_plan_sha256") == SOURCE_PLAN_V2_SHA256, "plan pin drift")
    require(active.get("wire_schema_sha256") == WIRE_SCHEMA_V2_SHA256, "schema pin drift")
    require(
        active.get("correction_contract_sha256") == CORRECTION_CONTRACT_SHA256,
        "correction contract pin drift",
    )
    require(active.get("source_gate_sha256") == SOURCE_GATE_SHA256, "source gate pin drift")
    require(active.get("wire_schema_version") == 2, "closure active wire version drift")
    require(
        active.get("wire_domain") == "moex.stage8b.p1e.first-boot-source-bundle.v2",
        "closure active wire domain drift",
    )
    require(active.get("fixed_source_path") == FIXED_SOURCE_PATH, "closure fixed path drift")
    require(active.get("legacy_v1_wire_accepted") is False, "closure reopens v1 wire")

    historical = closure.get("historical_contracts")
    require(isinstance(historical, dict), "historical_contracts must be an object")
    require(
        historical.get("wire_schema_v1_sha256") == WIRE_SCHEMA_V1_SHA256,
        "closure historical schema pin drift",
    )
    require(
        historical.get("source_plan_v1_sha256") == SOURCE_PLAN_V1_SHA256,
        "closure historical plan pin drift",
    )
    require(
        historical.get("role") == "HISTORICAL_REVIEW_EVIDENCE_ONLY",
        "historical v1 artifacts gained active authority",
    )

    acceptance = closure.get("source_acceptance")
    require(isinstance(acceptance, dict), "source_acceptance must be an object")
    require(
        acceptance.get("P1-FB01_candidate_identity") == "CLOSED",
        "P1-FB01 not closed",
    )
    require(
        acceptance.get("P1-FB02_history_completeness")
        == "CLOSED_WITH_EXPLICIT_WINDOW_AUTHORITY",
        "P1-FB02 closure drift",
    )
    require(
        acceptance.get("P2-FB03_fixed_path_loader_tests") == "CLOSED",
        "P2-FB03 not closed",
    )
    for key in (
        "transaction_v5_accepted",
        "receipt_v2_and_provenance_accepted",
        "deployable_i1_accepted",
    ):
        require(acceptance.get(key) is False, f"premature acceptance: {key}")

    next_slice = closure.get("next_authorized_slice")
    require(isinstance(next_slice, dict), "next_authorized_slice must be an object")
    require(
        next_slice.get("name") == "transaction V5 / receipt V2 / provenance binding",
        "next authorized slice drift",
    )
    require(
        next_slice.get("crash_and_adoption_tests_required") is True,
        "crash/adoption requirement removed",
    )
    require(
        next_slice.get("deployable_owner_loop_authorized") is False,
        "deployable owner loop opened prematurely",
    )
    require_closed_surfaces(closure.get("closed_surfaces"), "closure")

    correction = closure.get("governance_harness_correction")
    require(
        isinstance(correction, dict),
        "governance_harness_correction must be an object",
    )
    require(
        correction.get("held_closure_ref") == HELD_CLOSURE_REF,
        "held closure ref drift",
    )
    require(
        correction.get("held_closure_tree") == HELD_CLOSURE_TREE,
        "held closure tree drift",
    )
    require(correction.get("review_verdict") == "HOLD", "HOLD verdict drift")
    require(
        correction.get("review_sha256") == HOLD_REVIEW_SHA256,
        "HOLD review hash drift",
    )
    required_artifacts = {
        "checker": GOVERNANCE_CHECKER_PATH,
        "negative_harness": GOVERNANCE_NEGATIVE_PATH,
        "aggregate_gate": GOVERNANCE_GATE_PATH,
    }
    artifacts = correction.get("artifacts")
    require(isinstance(artifacts, dict), "correction artifacts must be an object")
    require(
        set(artifacts) == set(required_artifacts),
        "correction artifact inventory drift",
    )
    for name, expected_path in required_artifacts.items():
        entry = artifacts.get(name)
        require(isinstance(entry, dict), f"correction artifact {name} must be an object")
        require(entry.get("path") == expected_path.as_posix(), f"{name} path drift")
        require(
            entry.get("sha256") == sha256_file(root / expected_path),
            f"{name} hash drift",
        )
    controls = correction.get("required_controls")
    require(isinstance(controls, dict), "required_controls must be an object")
    require(
        controls
        == {
            "missing_git_authority_is_infrastructure": True,
            "noop_mutation_fails_harness": True,
            "positive_fixture_same_check_path": True,
            "semantic_mutation_expected_code_required": True,
        },
        "governance harness controls drift",
    )
    require(
        correction.get("semantic_mutation_cases") == 15,
        "semantic mutation case count drift",
    )
    require(
        correction.get("rust_or_cargo_changed") is False,
        "governance correction claims a Rust/Cargo change",
    )

    source_text = (root / SOURCE_PATH).read_text(encoding="utf-8")
    for token in (
        "STAGE8B_P1E_FIRST_BOOT_SOURCE_SCHEMA_VERSION: u16 = 2",
        '"moex.stage8b.p1e.first-boot-source-bundle.v2"',
        "build_stage8b_p1_first_boot_source_v1",
        "build_stage8b_p1_canonical_m10",
        "parse_stage8b_p1_canonical_m10",
        "validated_candidate_semantic_id_sha256",
    ):
        require(token in source_text, f"accepted source token missing: {token}")
    supervisor_text = (root / SUPERVISOR_PATH).read_text(encoding="utf-8")
    require(FIXED_SOURCE_PATH in supervisor_text, "supervisor fixed source path drift")

    status_text = (root / STATUS_PATH).read_text(encoding="utf-8")
    for token in (
        "4d7ee64",
        "SOURCE ACCEPT",
        "governance closure",
        "Transaction V5/receipt/provenance",
        "complete deployable I1 is not yet accepted",
        "FINAM POST/DELETE",
        "runtime-live",
        "real orders remain closed",
    ):
        require(token in status_text, f"current status marker missing: {token}")

def check_repository(repository_root: Path) -> None:
    repository_root = repository_root.resolve()
    try:
        tree = subprocess.run(
            ["git", "rev-parse", f"{ACCEPTED_SOURCE_REF}^{{tree}}"],
            cwd=repository_root,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError) as error:
        raise GovernanceCheckError(
            "infrastructure",
            "accepted source commit is not locally verifiable",
        ) from error
    require(
        tree == ACCEPTED_SOURCE_TREE,
        "accepted source Git tree does not match closure",
        "provenance",
    )


def check(root: Path, repository_root: Path | None = None) -> None:
    """Check content and Git provenance, optionally using a trusted checkout."""

    check_content(root)
    check_repository(root if repository_root is None else repository_root)


def main() -> int:
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path.cwd()
    try:
        check(root)
    except GovernanceCheckError as error:
        print(
            "FAIL stage8b-p1e-i1-first-boot-governance-check "
            f"code={error.code}: {error}",
            file=sys.stderr,
        )
        return 1
    print(
        "PASS stage8b-p1e-i1-first-boot-governance-check "
        "source=4d7ee64 wire=v2 transaction_v5=false receipt_v2=false "
        "deployable_i1=false operational_redis=false finam_write=false live=false"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
