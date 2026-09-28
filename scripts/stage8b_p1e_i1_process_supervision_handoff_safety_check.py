#!/usr/bin/env python3
"""Validate the immutable I1 process-supervision review handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

import stage8b_p1e_i1_process_supervision_check as source_check
import stage8b_p1e_i1a_handoff_safety_check as common


PARENT = "e2ce44206e49c3927fbb42bb252c25eecd1000de"
BRANCH = "stage8b-paper-shadow-resumption"
STAGE = "Stage 8B-P1-e I1 process supervision matrix"
MARKER = "handoff-commit.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
COMMIT_RAW = "handoff-evidence/source-commit.raw"
EVIDENCE = "handoff-evidence/stage8b-p1e-i1-process-supervision-evidence.json"
LOGS = {
    "source_gate": "handoff-evidence/stage8b-p1e-i1-process-supervision-source-gate.log",
    "runtime_process": "handoff-evidence/stage8b-p1e-i1-process-supervision-runtime-process.log",
    "runtime_lib": "handoff-evidence/stage8b-p1e-i1-process-supervision-runtime-lib.log",
    "runtime_redis_integration": "handoff-evidence/stage8b-p1e-i1-process-supervision-runtime-redis-integration.log",
    "runtime_writer_integration": "handoff-evidence/stage8b-p1e-i1-process-supervision-runtime-writer-integration.log",
    "runtime_doc": "handoff-evidence/stage8b-p1e-i1-process-supervision-runtime-doc.log",
    "core_full": "handoff-evidence/stage8b-p1e-i1-process-supervision-core-full.log",
}
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, *LOGS.values()}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def parse_marker(raw: bytes) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in raw.decode().splitlines():
        require("=" in line, "invalid marker line")
        key, value = line.split("=", 1)
        require(bool(key) and bool(value) and key not in values, "invalid marker field")
        values[key] = value
    require(
        set(values)
        == {"stage", "source_short_ref", "source_ref", "source_parent", "source_tree", "branch", "archive_name"},
        "marker inventory drift",
    )
    return values


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        by_name = {item.filename: item for item in infos}
        require(len(names) == len(set(names)), "duplicate archive member")
        require(not (GENERATED - set(names)), "generated evidence member missing")
        for item in infos:
            common.validate_member_name(item.filename)
            mode = (item.external_attr >> 16) & 0o177777
            require(mode & 0o170000 != 0o120000, f"symlink member: {item.filename}")
            require(mode & 0o170000 in {0, 0o100000}, f"special member: {item.filename}")
            parts = PurePosixPath(item.filename).parts
            require(not any(part == ".env" for part in parts), f"secret member: {item.filename}")
            require(
                not item.filename.endswith((".pem", ".key", ".ed25519", ".sqlite", ".sqlite3")),
                f"secret/runtime member: {item.filename}",
            )

        files = {name: archive.read(name) for name in names}
        marker = parse_marker(files[MARKER])
        require(marker["stage"] == STAGE, "stage mismatch")
        require(marker["source_parent"] == PARENT, "source parent mismatch")
        require(marker["branch"] == BRANCH, "branch mismatch")
        require(marker["archive_name"] == Path(path).name, "archive name mismatch")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit hash mismatch")
        commit_lines = commit_raw.decode().splitlines()
        require(commit_lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {PARENT}" in commit_lines, "commit parent mismatch")

        manifest = json.loads(files[MANIFEST])
        require(manifest["schema_version"] == 2, "manifest schema mismatch")
        require(manifest["source_ref"] == marker["source_ref"], "manifest source mismatch")
        entries = manifest["entries"]
        require(manifest["entry_count"] == len(entries), "manifest count mismatch")
        tracked: set[str] = set()
        payloads: dict[str, bytes] = {}
        for entry in entries:
            name = entry["path"]
            require(name not in tracked and name in files, f"tracked member mismatch: {name}")
            raw = files[name]
            mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
            require(entry["size"] == len(raw), f"size mismatch: {name}")
            require(entry["sha256"] == sha256(raw), f"digest mismatch: {name}")
            require(entry["mode"] == mode, f"mode mismatch: {name}")
            tracked.add(name)
            payloads[name] = raw
        require(set(names) - tracked == GENERATED, "generated member inventory mismatch")
        require(common.build_tree_oid(entries, payloads) == marker["source_tree"], "tree reconstruction mismatch")

        evidence = json.loads(files[EVIDENCE])
        require(evidence["source_ref"] == marker["source_ref"], "evidence source mismatch")
        require(evidence["source_parent"] == PARENT, "evidence parent mismatch")
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree mismatch")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        require(
            evidence["status"] == "SOURCE_CORRECTION_REVIEW_CANDIDATE",
            "evidence status mismatch",
        )
        require(evidence["production_process_composition"] is True, "process composition not declared")
        require(evidence["process_matrix"]["case_count"] == 14, "process case count mismatch")
        require(
            all(value is True for key, value in evidence["process_matrix"].items() if key != "case_count"),
            "process matrix case not proven",
        )
        require(
            all(evidence["ordinary_run_admission"].values()),
            "ordinary-run admission evidence incomplete",
        )
        require(
            evidence["terminal_exit_mapping"]
            == {
                "authenticated_before_deadline": 0,
                "authenticated_at_or_after_deadline": 72,
                "unexpected_authenticated_stop_without_intent": 70,
                "signal_task_failure": 73,
                "owner_panic_precedence": 70,
                "restart_required": 67,
            },
            "terminal exit mapping drift",
        )
        require(
            evidence["corrected_predecessor_lifecycle"]
            == {
                "fresh_v5_export": "P1BootstrapReady",
                "historical_v5_export": "P1BootstrapReady",
                "authenticated_adoption_phase": "P1SemanticReady",
                "adoption_predicate_version": 2,
                "legacy_predicate_v1_timer_ready_rejected": True,
                "legacy_automatic_migration": False,
                "continuous_v5_market_witness_complete": True,
                "continuous_v5_witness_uses_intent_injection_or_redis_reset": False,
                "cancel_production_ingress": "authenticated durable restart outcome",
                "isolated_injected_cancel_fixture_counted_as_continuous": False,
            },
            "corrected predecessor lifecycle disclosure drift",
        )
        require(
            evidence["signed_market_authority"]
            == {
                "fixture_signed_envelope": True,
                "production_schedule_reader": True,
                "fresh_schedule_read_total": 1,
                "v4_restart_before_effect": True,
                "exact_publication_marker_revalidated": True,
                "committed_recovery_schedule_read_total": 0,
                "post_truth_already_acknowledged": True,
                "initial_adoption_marker_receipt_immutable": True,
                "legacy_v4_none_authority_used": False,
            },
            "signed Market authority evidence drift",
        )
        require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")
        require(set(evidence["commands"]) == set(LOGS), "command inventory mismatch")
        for name, log_path in LOGS.items():
            record = evidence["commands"][name]
            raw = files[log_path]
            require(record["log_path"] == log_path, f"log path mismatch: {name}")
            require(record["log_sha256"] == sha256(raw), f"log digest mismatch: {name}")
            require(record["log_bytes"] == len(raw), f"log size mismatch: {name}")
            require(record["exit_code"] == 0, f"nonzero evidence exit: {name}")
            require(b"exit_code=0" in raw, f"exit status missing from log: {name}")
            require(marker["source_ref"].encode() in raw, f"source ref missing from log: {name}")
            require(marker["source_tree"].encode() in raw, f"source tree missing from log: {name}")

        require(
            b"PASS stage8b-p1e-i1-process-supervision-gate" in files[LOGS["source_gate"]],
            "source gate marker missing",
        )
        for token in (
            b"production_run_signals_cover_admission_attach_and_s06_without_effects",
            b"production_run_cancels_server_processed_redis_attach_and_s06_requests",
            b"common_supervisor_maps_noncooperative_owner_grace_expiry_to_72",
            b"production_v5_bootstrap_runs_continuous_market_lifecycle_and_readmits_exactly",
            b"process_wrapper_preserves_coordinator_boundary_exit_classes",
            b"ordinary_run_admission_rejects_post_seal_frontiers_without_mutation",
            b"stage8b-p1e-i1-process-supervision-negative-harness 77/77",
            b"stage8b-p1e-i1-transaction-v5-negative-harness 27/27",
        ):
            require(token in files[LOGS["source_gate"]], f"correction gate witness missing: {token!r}")
        require(b"5 passed; 0 failed" in files[LOGS["runtime_process"]], "five inherited process cases not retained")
        for name in (
            "runtime_lib",
            "runtime_redis_integration",
            "runtime_writer_integration",
            "runtime_doc",
            "core_full",
        ):
            raw = files[LOGS[name]]
            require(b"test result: ok." in raw, f"test success missing: {name}")
            require(b"test result: FAILED" not in raw, f"test failure present: {name}")

        with tempfile.TemporaryDirectory(prefix="stage8b-p1e-process-") as directory:
            root = Path(directory)
            for name, raw in payloads.items():
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(raw)
            source_check.validate_content(source_check.load_content(root))

        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": marker["source_ref"],
            "source_parent": PARENT,
            "source_tree": marker["source_tree"],
            "actual_logs_verified": len(LOGS),
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1e_i1_process_supervision_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, UnicodeDecodeError, ValueError, KeyError, TypeError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-i1-process-supervision-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1e-i1-process-supervision-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
