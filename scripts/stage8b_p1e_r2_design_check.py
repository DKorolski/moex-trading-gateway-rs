#!/usr/bin/env python3
"""Fail-closed checker for the Stage 8B-P1-e R2 corrected design."""

from __future__ import annotations

import csv
import hashlib
import io
import json
import pathlib
import re
import subprocess


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "693fab351f099b5f16ebb73d4956918b34d8ea1e"
ACCEPTED_P1D4 = "c2a9e1246dfdd59f3a6297268de907dedcb19903"
R1_REVIEW_SHA256 = "eb451af76c9b4030705d900ae0b96fabf62452dde47810e4b007cd4410d58cca"

DESIGN = ROOT / "docs/stage-8/stage8b-p1e-deployable-supervisor-design-r2.md"
ACCEPTANCE = ROOT / "docs/stage-8/stage8b-p1e-deployable-supervisor-r2-acceptance-matrix.csv"
OUTER = ROOT / "docs/stage-8/stage8b-p1e-restart-continuation-matrix-v2.csv"
OPERATIONAL = ROOT / "docs/stage-8/stage8b-p1e-operational-continuation-matrix-v2.csv"
TRANSACTION = ROOT / "docs/stage-8/stage8b-p1e-first-boot-transaction-v1.json"
REDIS_POLICY = ROOT / "docs/stage-8/stage8b-p1e-redis-runtime-policy-v1.json"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1e-deployable-supervisor-r2-design-evidence.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"
RECOVERY_SOURCE = ROOT / "crates/runtime-durable-service/src/recovery.rs"

CONTRACT_HASHES = {
    "first_boot_transaction_v1_sha256": "d3f5f8a3aabacf4b5640deae0ce6d87fcf9bc10947367f45676fc9587e474f59",
    "redis_runtime_policy_v1_sha256": "a3657dfbd10743f93478c1727e118b996c3ad9db5e71986e4378912ab3fdc6f7",
    "restart_outer_matrix_v2_file_sha256": "79936494cc20c460f7a4249ca759fc43da3cca0a31c51d1ad39c26d3a03b79df",
    "operational_continuation_matrix_v2_file_sha256": "8160c7a072d2aa10ea1d75677d7a20efd65b76f7098901ee812028e606444739",
}

INHERITED_R1_HASHES = {
    "docs/stage-8/stage8b-p1e-deployable-supervisor-design-r1.md": "d5cad4652b5134def8ceeb8a2b27ef478253a1b714431bbe3b9e257c64784597",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r1-acceptance-matrix.csv": "b937e5c2434c665914cc1e27db12aee5f0669cb96e8db9764ca3d1791a938c0c",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r1-design-evidence.json": "c7b137480de6cdbe5b17dd0bbf1c39ccce35fab31b01b19f50280c7d3a3a0bcb",
    "docs/stage-8/stage8b-p1e-runtime-profile-v1.json": "ec50ba663aaae3bd2922a3166d1344fe580a5dc38fbf903de4e321308c8ad22a",
    "docs/stage-8/stage8b-p1e-first-boot-source-bundle-schema-v1.json": "415a3bdc8d20755b519b20de941afcbeeb56986fc77845f0462cab6c3a302a64",
    "docs/stage-8/stage8b-p1e-first-boot-source-plan-v1.json": "2b8f32db2aadd9a917b526101c7b407918ce4366e1dbed3e12bb96af5831a0bd",
    "docs/stage-8/stage8b-p1e-redis-deployment-manifest-v1.json": "3c9ddebe1395e5fdcf306be67988698fd2f1a20e1ae8c19e376e16027956ebbc",
    "docs/stage-8/stage8b-p1e-telemetry-contract-v1.json": "fdd2adf7f4c9fbcfcf922a9146f76bafe257a44138242953fc113e1001f96af1",
    "docs/stage-8/stage8b-p1e-supervisor-event-matrix-v1.csv": "37c4f423f80ebba281964f571511ff23c74f3a5b2355b4e7afb78d36d880bcae",
}

EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-design-r2.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r2-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r2-design-evidence.json",
    "docs/stage-8/stage8b-p1e-first-boot-transaction-v1.json",
    "docs/stage-8/stage8b-p1e-redis-runtime-policy-v1.json",
    "docs/stage-8/stage8b-p1e-restart-continuation-matrix-v2.csv",
    "docs/stage-8/stage8b-p1e-operational-continuation-matrix-v2.csv",
    "scripts/stage8b_p1e_r2_design_check.py",
    "scripts/stage8b_p1e_r2_design_negative_harness.py",
    "scripts/stage8b_p1e_r2_design_gate.sh",
    "scripts/make_stage8b_p1e_r2_design_handoff.py",
    "scripts/stage8b_p1e_r2_design_handoff_safety_check.py",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def canonical_json_sha256(value: object) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
    return hashlib.sha256(encoded).hexdigest()


def text_sha256(value: str) -> str:
    return hashlib.sha256(value.encode()).hexdigest()


def file_sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def csv_rows(value: str) -> list[dict[str, str]]:
    return list(csv.DictReader(io.StringIO(value)))


def changed_files() -> set[str]:
    tracked = subprocess.run(
        ["git", "diff", "--name-only", BASE], cwd=ROOT, check=True,
        text=True, capture_output=True,
    ).stdout.splitlines()
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT,
        check=True, text=True, capture_output=True,
    ).stdout.splitlines()
    return set(tracked) | set(untracked)


def restart_variants(source: str) -> list[str]:
    body = source.split("pub enum Stage7bRestartOutcome {", 1)[1].split("\n}", 1)[0]
    return re.findall(r"^    ([A-Za-z][A-Za-z0-9]+)(?:\(|,)", body, flags=re.MULTILINE)


def validate(
    design: str,
    acceptance_text: str,
    outer_text: str,
    operational_text: str,
    transaction: dict[str, object],
    redis_policy: dict[str, object],
    evidence: dict[str, object],
    status: str,
    roadmap: str,
    recovery_source: str,
) -> None:
    for token in (
        "Status: R2 design-only review candidate",
        ACCEPTED_P1D4,
        BASE,
        R1_REVIEW_SHA256,
        "HOLD / SUPERSEDED BY R2",
        "authenticated incomplete-bootstrap ceremony",
        "renameat2(..., RENAME_NOREPLACE)",
        "CommittedRootResponseLost",
        "Stage8bP1FirstBootProvenanceV1",
        "before Redis contact",
        "54-row operational continuation matrix v2",
        "Stage8bP1eCompositeClassifierV2",
        "UnexpectedAlreadyAcknowledgedSource",
        "There is no catch-all route to `Ready`",
        "P1D4C-031/032",
        "P1D4C-036/037",
        "P1D4C-041/042",
        "P1D4C-047",
        "P1D4C-049",
        "must commit/reread exact `S_cancel_recovered`",
        "same delivery is processed in the same ownership invocation",
        "claim_idle_ms              30000",
        "30000 + 5000 + 4*2000 = 43000 <= 60000",
        "RestartPreventExitStatus",
        "pending count ascending",
        "0 < child_pid <= u32::MAX",
        "does not authorize that implementation",
    ):
        require(token in design, f"missing R2 design invariant: {token}")

    acceptance = csv_rows(acceptance_text)
    require(len(acceptance) == 48, f"R2 acceptance rows drifted: {len(acceptance)}")
    require(list(acceptance[0]) == ["id", "area", "requirement", "status"], "R2 acceptance fields drifted")
    require([row["id"] for row in acceptance] == [f"P1ER2-{i:03d}" for i in range(1, 49)], "R2 acceptance IDs drifted")
    require(all(row["status"] == "REQUIRED" for row in acceptance), "R2 acceptance weakened")
    by_id = {row["id"]: row["requirement"] for row in acceptance}
    for row_id, token in {
        "P1ER2-008": "before canonical root creation",
        "P1ER2-016": "fresh authenticated restart from final path",
        "P1ER2-022": "initial durable package and first seal",
        "P1ER2-026": "54 exact local plus PEL plus timer rows",
        "P1ER2-032": "exact S_cancel_recovered",
        "P1ER2-033": "TruthCommitted replacement before XACK",
        "P1ER2-035": "no second fresh read or premature PaperReady",
        "P1ER2-036": "closed exit-67 classifier error",
        "P1ER2-039": "fit inside the global startup deadline",
        "P1ER2-046": "greater-than-16 inventory progresses across boots",
        "P1ER2-048": "remain closed",
    }.items():
        require(token in by_id[row_id], f"R2 acceptance semantic drift: {row_id}")

    outer = csv_rows(outer_text)
    outer_fields = [
        "variant", "redis_attachment", "authenticated_package_phase",
        "expected_source_state", "equivalent_authority_reissue", "starting_boundary",
        "first_legal_transition", "next_owner", "s06r_completion_boundary",
        "xack_legality", "readiness_after_s06r", "shutdown_rule",
    ]
    variants = restart_variants(recovery_source)
    require(len(variants) == 22 and len(set(variants)) == 22, "source restart enum inventory drifted")
    require(len(outer) == 22 and list(outer[0]) == outer_fields, "outer restart matrix shape drifted")
    require([row["variant"] for row in outer] == variants, "outer restart matrix is not exact enum order")
    outer_by = {row["variant"]: row for row in outer}
    require(outer_by["Ready"]["first_legal_transition"] == "classify_pel_and_due_timer_before_fresh_read", "Ready external work discarded")
    require("s_truth_committed" in outer_by["P1d2AckCommitted"]["s06r_completion_boundary"], "P1-d2 ACK stops before truth")
    require("generated_s_truth_committed" in outer_by["P1d4GeneratedMarketAckCommitted"]["s06r_completion_boundary"], "generated ACK stops before truth")
    require("s_truth_committed" in outer_by["P1d3AckCommitted"]["s06r_completion_boundary"], "P1-d3 ACK stops before truth")
    cancel = outer_by["P1d3CancelContinuationPending"]
    require(cancel["starting_boundary"] == "s_terminal_committed_cancel_continuation_pending", "cancel starting boundary drifted")
    require("s_cancel_recovered_committed" in cancel["s06r_completion_boundary"], "cancel recovered boundary generalized")
    require(outer_by["Blocked"]["redis_attachment"] == "forbidden", "Blocked attached Redis")

    operational = csv_rows(operational_text)
    operational_fields = [
        "id", "local_restart_variant", "authenticated_package_phase", "redis_pel_state",
        "timer_state", "starting_boundary", "equivalent_authority_reissue",
        "first_legal_transition", "next_owner", "s06r_completion_boundary",
        "source_disposition", "xack_legality", "fresh_poll_legality",
        "paper_ready_legality", "accepted_proof_cells",
    ]
    require(len(operational) == 54 and list(operational[0]) == operational_fields, "operational matrix shape drifted")
    require([row["id"] for row in operational] == [f"OC{i:02d}" for i in range(1, 55)], "operational IDs drifted")
    require(all(row["local_restart_variant"] in variants for row in operational), "unknown operational restart variant")
    op = {row["id"]: row for row in operational}
    require(op["OC01"]["fresh_poll_legality"] == "one_bounded_s08_poll", "quiescent Ready cannot poll exactly once")
    require(op["OC01"]["paper_ready_legality"] == "only_after_s08_returns_no_delivery", "Ready published before empty poll")
    require(op["OC02"]["accepted_proof_cells"] == "P1D4C-031|P1D4C-032", "Ready zero-intent proof cells drifted")
    require(op["OC03"]["accepted_proof_cells"] == "P1D4C-036|P1D4C-037", "Ready one-intent proof cells drifted")
    require(op["OC04"]["accepted_proof_cells"] == "P1D4C-041|P1D4C-042", "Ready fill proof cells drifted")
    require(op["OC05"]["fresh_poll_legality"] == "forbidden" and op["OC05"]["equivalent_authority_reissue"] == "forbidden", "unclaimable Ready work escaped")
    require(op["OC06"]["first_legal_transition"] == "exit_67_ambiguous_without_dispatch", "ambiguous PEL accepted")
    require(op["OC07"]["accepted_proof_cells"] == "P1D4C-047" and "Day" not in op["OC07"]["first_legal_transition"], "Day expiry row drifted")
    require(op["OC08"]["equivalent_authority_reissue"] == "forbidden" and op["OC08"]["accepted_proof_cells"] == "P1D4C-049", "committed Day expiry replayed")
    require("resolve_exact_source_to_terminal_and_xack_then_reclassify_timer_and_execute_expiry_only_if_still_working_due" == op["OC09"]["first_legal_transition"], "source/timer ordering drifted")
    require(op["OC09"]["s06r_completion_boundary"] == "source_terminal_and_xack_then_exact_timer_not_applicable_or_expiry_terminal", "source/timer completion drifted")
    require(op["OC10"]["fresh_poll_legality"] == "second_fresh_read_forbidden", "S08 second read opened")
    require("s_cancel_recovered" in op["OC28"]["s06r_completion_boundary"], "cancel row lost exact recovered seal")
    for row_id in ("OC15", "OC22", "OC26"):
        require("truth" in op[row_id]["s06r_completion_boundary"], f"ACK row stops before truth: {row_id}")
    for row_id in ("OC32", "OC33", "OC34", "OC35"):
        require(op[row_id]["xack_legality"] == "no_second_xack", f"already-ACK row repeats XACK: {row_id}")
        require("continuous_frontier" in op[row_id]["s06r_completion_boundary"], f"already-ACK row lacks frontier: {row_id}")
    claimable_variants = {row["local_restart_variant"] for row in operational[10:29]}
    not_claimable_variants = {row["local_restart_variant"] for row in operational[35:54]}
    expected_active = set(variants) - {"Ready", "Stage8a4I3Pending", "Blocked"}
    require(claimable_variants == expected_active, "claimable active-variant coverage drifted")
    require(not_claimable_variants == expected_active, "unclaimable active-variant coverage drifted")
    require(all(row["fresh_poll_legality"] == "forbidden" for row in operational[35:54]), "unclaimable row permits fresh read")

    require(transaction.get("schema_version") == 1, "transaction schema drifted")
    require(transaction.get("domain") == "moex.stage8b.p1e.first-boot-transaction.v1", "transaction domain drifted")
    require(transaction.get("bootstrap_entry_rule") == "ordinary-bootstrap-is-allowed-only-for-NoRoot-with-no-marker-no-temp-and-no-receipt-and-attempt-generation-greater-than-authenticated-quarantine-history", "bootstrap entry widened")
    require(transaction.get("next_attempt_rule") == "bootstrap-attempt-generation-must-exceed-every-authenticated-quarantined-transaction-for-this-operational-identity", "bootstrap attempt generation drifted")
    marker = transaction.get("marker_authentication", {})
    require(isinstance(marker, dict) and marker.get("key") == "stage8b-p1-lifecycle.key", "transaction HMAC key drifted")
    require(len(marker.get("required_bindings", [])) == 13, "transaction marker bindings drifted")
    require(transaction.get("marker_update_protocol") == [
        "create-exclusive-temp-mode-0600-no-follow", "write-all", "sync-all-temp",
        "rename-temp-over-marker", "fsync-canonical-parent", "nofollow-reread-and-byte-verify",
    ], "transaction persistence protocol drifted")
    classifications = transaction.get("classifications", [])
    require(isinstance(classifications, list) and len(classifications) == 9, "transaction classifications drifted")
    class_by = {item["classification"]: item for item in classifications}
    require(class_by["RootWithoutJournal"]["sole_legal_action"] == "bootstrap-recover-quarantine-incomplete-root", "root-only cleanup drifted")
    require(class_by["JournalWithoutSeal"]["sole_legal_action"] == "bootstrap-recover-quarantine-incomplete-root", "journal-only cleanup drifted")
    require(class_by["CommittedRootResponseLost"]["sole_legal_action"] == "bootstrap-recover-adopt-committed-root-after-fresh-authenticated-restart", "response-loss adoption drifted")
    require(class_by["UnpublishedMarkerTemp"]["sole_legal_action"].startswith("bootstrap-recover-unlink-exact-nofollow"), "marker-temp response loss drifted")
    require(class_by["QuarantinedIncompleteRoot"]["sole_legal_action"] == "bootstrap-recover-fsync-quarantine-and-move-marker-into-exact-quarantine-directory", "quarantine response loss drifted")
    require(class_by["AdoptedCommittedRoot"]["run_allowed"] is True, "adopted root cannot run")
    require(all(not item["run_allowed"] for name, item in class_by.items() if name != "AdoptedCommittedRoot"), "run opened before adoption")
    quarantine = transaction.get("quarantine_protocol", {})
    require(quarantine.get("recursive_delete_allowed") is False and quarantine.get("automatic_delete_allowed") is False, "unsafe deletion opened")
    require("no-committed-seal" in quarantine.get("preconditions", []), "committed root quarantine opened")
    adoption = transaction.get("adoption_protocol", {})
    require("fresh-restart-from-final-canonical-path-returns-exact-TimerReady-owner" in adoption.get("preconditions", []), "final-path adoption proof missing")
    require("accepted-receipt-as-commit-point" in adoption.get("operation", ""), "adoption commit point drifted")
    provenance = transaction.get("first_boot_provenance", {})
    require(provenance.get("storage") == "hmac-covered-field-of-initial-durable-package-and-first-committed-seal", "durable provenance weakened")
    require(len(provenance.get("required_fields", [])) == 11, "durable provenance fields drifted")
    require(len(transaction.get("required_sigkill_hooks", [])) == 3, "first-boot SIGKILL inventory drifted")

    require(redis_policy.get("domain") == "moex.stage8b.p1e.redis-runtime-policy.v1", "Redis policy domain drifted")
    exact_values = {
        "read_count": 1, "claim_count": 2, "claim_idle_ms": 30000,
        "max_claim_pages": 1, "retention_floor": 4096,
        "redis_operation_timeout_ms": 2000, "fresh_poll_timeout_ms": 1000,
    }
    for key, value in exact_values.items():
        require(redis_policy.get(key) == value, f"Redis runtime value drifted: {key}")
    require(redis_policy.get("ordinary_environment_override_allowed") is False, "Redis environment override opened")
    require(redis_policy.get("unchecked_default_merge_allowed") is False, "Redis unchecked default opened")
    startup = redis_policy.get("startup_claim", {})
    require(startup.get("attempts") == 12 and startup.get("total_deadline_ms") == 60000, "claim budget drifted")
    require(startup.get("maximum_accepted_pel_count") == 1, "PEL ambiguity bound drifted")
    upper = redis_policy["claim_idle_ms"] + startup["maximum_backoff_ms"] + startup["maximum_commands_after_threshold"] * redis_policy["redis_operation_timeout_ms"]
    require(upper == 43000 and upper <= startup["total_deadline_ms"], "claim policy cannot complete within deadline")
    require(startup.get("nonterminal_cursor_after_max_pages") == "exit-67-no-fresh-read", "claim cursor exhaustion falls through")
    fresh = redis_policy.get("fresh_read", {})
    require(fresh.get("maximum_successful_polls_before_readiness_decision") == 1, "fresh poll count widened")
    require(fresh.get("second_fresh_read_before_first_delivery_resolution") is False, "second fresh read opened")
    stale = redis_policy.get("stale_consumer_hygiene", {})
    require(stale.get("maximum_inventory") == 64 and stale.get("examined_per_boot") == 16, "stale inventory bounds drifted")
    require(stale.get("deterministic_order") == "pending-count-ascending-then-idle-ms-descending-then-consumer-name-bytewise-ascending", "stale order drifted")
    require(stale.get("nonzero_pending_delete_allowed") is False, "pending consumer deletion opened")
    systemd = redis_policy.get("systemd_restart", {})
    require(systemd.get("restart_prevent_exit_status") == [64, 66], "restart-prevent classes drifted")
    require(systemd.get("start_limit_interval_sec") == 600 and systemd.get("start_limit_burst") == 5, "finite restart contract drifted")

    actual_hashes = {
        "first_boot_transaction_v1_sha256": canonical_json_sha256(transaction),
        "redis_runtime_policy_v1_sha256": canonical_json_sha256(redis_policy),
        "restart_outer_matrix_v2_file_sha256": text_sha256(outer_text),
        "operational_continuation_matrix_v2_file_sha256": text_sha256(operational_text),
    }
    require(actual_hashes == CONTRACT_HASHES, f"R2 contract hash drift: {actual_hashes}")

    require(evidence.get("stage") == "Stage 8B-P1-e R2 deployable paper supervisor design correction", "evidence stage drifted")
    require(evidence.get("status") == "R2_DESIGN_REVIEW_CANDIDATE", "evidence status drifted")
    require(evidence.get("r1_ref") == BASE and evidence.get("r1_review_sha256") == R1_REVIEW_SHA256, "evidence R1 binding drifted")
    require(evidence.get("inherited_r1_acceptance_rows") == 88 and evidence.get("r2_acceptance_rows") == 48, "evidence acceptance counts drifted")
    require(evidence.get("restart_outer_rows") == 22 and evidence.get("operational_continuation_rows") == 54, "evidence matrix counts drifted")
    require(evidence.get("inherited_r1_negative_cases") == 64 and evidence.get("r2_negative_cases") == 40, "evidence negative counts drifted")
    require(evidence.get("canonical_contracts") == CONTRACT_HASHES, "evidence contract hashes drifted")
    require(evidence.get("first_boot_transaction", {}).get("classification_count") == 9, "evidence transaction classification count drifted")
    require(evidence.get("design_only") is True and evidence.get("implementation_authorized") is False, "source implementation opened")
    require(evidence.get("operational_activation_authorized") is False, "operational activation opened")
    require(all(value is False for value in evidence.get("closed_surfaces", {}).values()), "closed surface opened")
    require(evidence.get("deferred_p2", {}).get("required_bound") == "0 < child_pid <= u32::MAX", "deferred PID bound drifted")

    for token in (
        "P1-e R1 design at `693fab351f099b5f16ebb73d4956918b34d8ea1e` is HOLD",
        "active candidate is P1-e R2 design correction",
        "P1-e source implementation remains unauthorized",
    ):
        require(token in status, f"status drifted: {token}")
    for token in (
        "P1-e R1 design is HOLD",
        "P1-e R2 design correction is the active candidate",
        "Only independent R2 design acceptance may open P1-e source implementation",
    ):
        require(token in roadmap, f"roadmap drifted: {token}")


def load_json(path: pathlib.Path) -> dict[str, object]:
    def reject_duplicates(pairs: list[tuple[str, object]]) -> dict[str, object]:
        result: dict[str, object] = {}
        for key, value in pairs:
            if key in result:
                raise CheckFailure(f"duplicate JSON key in {path}: {key}")
            result[key] = value
        return result
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=reject_duplicates)


def main() -> None:
    try:
        require(changed_files() == EXPECTED_CHANGED, f"R2 scope drift: {sorted(changed_files())}")
        for path, digest in INHERITED_R1_HASHES.items():
            require(file_sha256(ROOT / path) == digest, f"inherited R1 artifact drifted: {path}")
        validate(
            DESIGN.read_text(encoding="utf-8"),
            ACCEPTANCE.read_text(encoding="utf-8"),
            OUTER.read_text(encoding="utf-8"),
            OPERATIONAL.read_text(encoding="utf-8"),
            load_json(TRANSACTION),
            load_json(REDIS_POLICY),
            load_json(EVIDENCE),
            STATUS.read_text(encoding="utf-8"),
            ROADMAP.read_text(encoding="utf-8"),
            RECOVERY_SOURCE.read_text(encoding="utf-8"),
        )
    except (CheckFailure, OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-r2-design-check: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1e-r2-design-scope files=14 inherited_acceptance=88 r2_acceptance=48 restart=22 operational=54 design_only=true")


if __name__ == "__main__":
    main()
