#!/usr/bin/env python3
"""Fail-closed source check for the I1 production telemetry composition slice."""

from __future__ import annotations

import argparse
import csv
import hashlib
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BASE = "896ad1b2f85ea47a59212001eb713befaea26832"
BASE_TREE = "a696daa513f27d6c476ae18cfef8e005647dcf4f"
TELEMETRY_CONTRACT_SHA256 = "fdd2adf7f4c9fbcfcf922a9146f76bafe257a44138242953fc113e1001f96af1"
DOCUMENT = "docs/stage-8/stage8b-p1e-i1-telemetry-composition.md"
MATRIX = "docs/stage-8/stage8b-p1e-i1-telemetry-composition-acceptance-matrix.csv"
STATUS = "docs/current-status.md"
PROCESS = "crates/runtime-durable-service/src/stage8b_p1e_process.rs"
SUPERVISOR = "crates/runtime-durable-service/src/stage8b_p1_supervisor.rs"
SEMANTIC = "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs"
RECOVERY = "crates/runtime-durable-service/src/recovery.rs"
LIB = "crates/runtime-durable-service/src/lib.rs"
CONTRACT = "docs/stage-8/stage8b-p1e-telemetry-contract-v1.json"

ALLOWED_CHANGES = {
    LIB,
    RECOVERY,
    SEMANTIC,
    SUPERVISOR,
    PROCESS,
    DOCUMENT,
    MATRIX,
    STATUS,
    "scripts/stage8b_p1e_i1_process_supervision_check.py",
    "scripts/stage8b_p1e_i1_telemetry_composition_check.py",
    "scripts/stage8b_p1e_i1_telemetry_composition_negative_harness.py",
    "scripts/stage8b_p1e_i1_telemetry_composition_gate.sh",
    "scripts/stage8b_p1e_i1_telemetry_composition_handoff_safety_check.py",
    "scripts/make_stage8b_p1e_i1_telemetry_composition_handoff.py",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def section(text: str, start: str, end: str) -> str:
    begin = text.find(start)
    finish = text.find(end, begin + len(start))
    require(begin >= 0 and finish > begin, f"section missing: {start}")
    return text[begin:finish]


def require_order(text: str, tokens: tuple[str, ...], label: str) -> None:
    cursor = -1
    for token in tokens:
        cursor = text.find(token, cursor + 1)
        require(cursor >= 0, f"{label}: missing or reordered {token}")


def load_content(root: Path = ROOT) -> dict[str, str]:
    paths = {
        "lib": LIB,
        "recovery": RECOVERY,
        "semantic": SEMANTIC,
        "supervisor": SUPERVISOR,
        "process": PROCESS,
        "document": DOCUMENT,
        "status": STATUS,
    }
    return {name: (root / path).read_text() for name, path in paths.items()}


def validate_lineage(root: Path) -> None:
    try:
        actual_tree = subprocess.check_output(
            ["git", "rev-parse", f"{BASE}^{{tree}}"], cwd=root, text=True
        ).strip()
        require(actual_tree == BASE_TREE, "accepted predecessor tree drift")
        subprocess.run(
            ["git", "merge-base", "--is-ancestor", BASE, "HEAD"],
            cwd=root,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        changed = set(
            subprocess.check_output(
                ["git", "diff", "--name-only", BASE, "--"], cwd=root, text=True
            ).splitlines()
        )
        changed.update(
            subprocess.check_output(
                ["git", "ls-files", "--others", "--exclude-standard"],
                cwd=root,
                text=True,
            ).splitlines()
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise CheckFailure(f"cannot verify accepted predecessor: {error}") from error
    require(changed == ALLOWED_CHANGES, f"changed-path inventory drift: {sorted(changed ^ ALLOWED_CHANGES)}")


def validate_matrix(root: Path) -> None:
    try:
        with (root / MATRIX).open(newline="") as handle:
            rows = list(csv.DictReader(handle))
    except OSError as error:
        raise CheckFailure(f"cannot read acceptance matrix: {error}") from error
    require(rows and list(rows[0]) == ["id", "area", "requirement", "status"], "matrix header drift")
    require([row["id"] for row in rows] == [f"I1TEL-{index:03}" for index in range(1, 21)], "matrix inventory drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "optional matrix row introduced")


def validate_content(content: dict[str, str]) -> None:
    supervisor = content["supervisor"]
    process = content["process"]
    recovery = content["recovery"]
    semantic = content["semantic"]
    lib = content["lib"]
    document = content["document"]
    status = content["status"]

    publisher_struct = section(
        supervisor,
        "pub struct Stage8bP1eTelemetryPublisherV1 {",
        "/// Linear result of S05.",
    )
    require_order(
        publisher_struct,
        (
            "    connection: ConnectionManager,",
            "    health_stream: String,",
            "    readiness_stream: String,",
        ),
        "restricted publisher fields",
    )
    for forbidden in ("transport", "xack", "claim", "command", "consumer"):
        require(forbidden not in publisher_struct.lower(), f"publisher gained forbidden field: {forbidden}")

    publisher_impl = section(
        supervisor,
        "impl Stage8bP1eTelemetryPublisherV1 {",
        "async fn publish_stage8b_p1e_telemetry_v1(",
    )
    require(publisher_impl.count("pub async fn") == 2, "publisher public method inventory drift")
    for token in ("pub async fn publish_health(", "pub async fn publish_readiness("):
        require(token in publisher_impl, f"publisher method missing: {token}")
    require("&self.readiness_stream, payload" in publisher_impl, "readiness stream binding missing")
    for forbidden in ("xack", "xread", "xautoclaim", "publish_command", "delconsumer"):
        require(forbidden not in publisher_impl.lower(), f"publisher gained lifecycle authority: {forbidden}")

    redis_writer = section(
        supervisor,
        "async fn publish_stage8b_p1e_telemetry_v1(",
        "#[cfg(test)]\npub(crate) async fn stage8b_p1e_test_redis_control_v1",
    )
    require_order(
        redis_writer,
        (
            'redis::cmd("XADD")',
            '.arg(stream)',
            '.arg("NOMKSTREAM")',
            '.arg("MAXLEN")',
            '.arg("=")',
            '.arg(STAGE8B_P1E_TELEMETRY_RETENTION)',
            '.arg("*")',
            '.arg("payload")',
            '.arg(payload)',
        ),
        "exact telemetry XADD",
    )
    require(redis_writer.count('.arg("payload")') == 1, "telemetry field/value arity drift")
    require("MKSTREAM" not in redis_writer.replace("NOMKSTREAM", ""), "implicit stream creation introduced")
    for token in ("boot_id: [u8; 16]", "boot_id: self.boot_id"):
        require(token in supervisor, f"real boot identity binding missing: {token}")
    require("Stage8bP1eTelemetryPublisherV1" in lib, "restricted publisher export missing")

    seal_helper = section(
        recovery,
        "fn stage8b_p1e_current_seal(&self)",
        "/// Authenticates the exact restart package",
    )
    for token in (
        "Self::Ready(owner)",
        "Self::P1d4GeneratedMarketTruthCommitted(owner)",
        "Self::P1d3CancelContinuationPending(owner)",
        "Self::P1eScheduleBindingCommitted(owner)",
        "Self::Blocked(_)",
        "pub(crate) fn stage8b_p1e_telemetry_seal_v1(",
    ):
        require(token in seal_helper, f"durable telemetry seal route missing: {token}")
    for token in (
        "pub(crate) fn stage8b_p1e_telemetry_seal_v1(",
        "pub(crate) fn stage8b_p1e_validate_telemetry_readiness_v1(",
        "self.stage7.validate_composite_readiness(commitment_key)",
    ):
        require(token in semantic, f"durable readiness bridge missing: {token}")

    reporter = section(
        process,
        "struct Stage8bP1eTelemetryReporterV1 {",
        "impl Stage8bP1eProductionTelemetryStateV1 {",
    )
    for token in (
        "tokio::sync::mpsc::Sender",
        "self.sender.try_send(snapshot)",
        "self.request_telemetry_failure()",
        "Stage8bP1eShutdownCauseV1::TelemetryFailure",
        "if *state == previous",
        "fn update_ready(",
        "fn update_lifecycle_pending(",
        "fn update_draining(",
        "fn finish_stopped(",
        "fn finish_degraded(",
    ):
        require(token in reporter, f"reporter invariant missing: {token}")
    require(reporter.count("self.request_telemetry_failure();") == 2, "reporter failure path inventory drift")
    require(".send(" not in reporter, "reporter may block the lifecycle owner")

    publisher_task = section(
        process,
        "async fn run_stage8b_p1e_production_telemetry_v1(",
        "#[cfg(test)]\nasync fn stage8b_p1e_telemetry_test_barrier_v1(",
    )
    require_order(
        publisher_task,
        (
            "interval_at(tokio::time::Instant::now() + period, period)",
            "MissedTickBehavior::Skip",
            "publish_stage8b_p1e_production_snapshot_v1",
            "if state.terminal",
            "receiver.recv()",
            "interval.tick()",
        ),
        "bounded periodic telemetry task",
    )
    require("latch.request(Stage8bP1eShutdownIntentV1::new(" in publisher_task, "write failure is not fail closed")

    production_owner = section(
        process,
        "async fn run_stage8b_p1e_production_owner_v1(",
        "fn process_failure_class(",
    )
    require_order(
        production_owner,
        (
            "admit_stage8b_p1e_ordinary_run_v1(",
            ".stage8b_p1e_telemetry_seal_v1()",
            "attach_stage8b_p1e_verified_redis(&attach_plan)",
            ".clean_stale_zero_pending_consumers(&consumer_name)",
            ".telemetry_publisher()",
            "acquire_stage8b_p1e_startup_owner_v1(attachable, session)",
            "let initial_pel_count = startup",
            "Stage8bP1eRedisScheduleReader::connect(&redis_url)",
            "tokio::sync::mpsc::channel(32)",
            "tokio::spawn(run_stage8b_p1e_production_telemetry_v1(",
            "run_stage8b_p1e_process_owner_with_telemetry_v1(",
            "telemetry_reporter.update_draining()",
            "telemetry_reporter.finish_stopped()",
        ),
        "production telemetry composition order",
    )
    for token in (
        "boot_id: settings.boot_id",
        "operational_identity_sha256",
        "deployment_generation",
        "runtime_config_fingerprint_sha256",
        "durable_seal_generation",
        "durable_commitment_sha256",
        "consumer_name",
        "pel_count: initial_pel_count",
    ):
        require(token in production_owner, f"initial production identity missing: {token}")
    for token in (
        "paper_only: true",
        "finam_transport_attached: false",
        "broker_network_dispatch_attached: false",
        "runtime_live: false",
        "real_orders: false",
    ):
        require(process.count(token) >= 2, f"closed telemetry flag missing: {token}")
    require("LiveReady" not in process, "live readiness introduced")

    for token in (
        "telemetry.update_ready(",
        "telemetry.update_lifecycle_pending(",
        "telemetry.update_draining()",
        "fn settle_stage8b_p1e_production_telemetry_v1(",
        "if telemetry_succeeded",
        "Err(error) => Err(error)",
        "Stage8bP1eProcessErrorV1::TelemetryFailed.exit_code(), 71",
        "Some(71) => Err(Stage8bP1eProcessErrorV1::TelemetryFailed)",
        "Stage8bP1eShutdownCauseV1::ExternalSignal",
    ):
        require(token in process, f"owner/failure telemetry invariant missing: {token}")
    require(process.count("telemetry.update_ready(") == 2, "ready transition inventory drift")
    require(process.count("telemetry.update_lifecycle_pending(") == 3, "degraded transition inventory drift")
    require(process.count("telemetry.update_draining()") == 2, "draining transition inventory drift")

    for test in (
        "production_telemetry_publishes_all_transitions_in_order",
        "production_telemetry_missing_stream_fails_closed_without_creation",
        "production_telemetry_periodically_republishes_current_snapshot",
        "telemetry_reporter_backpressure_is_fail_closed_and_first_wins",
        "production_telemetry_settlement_preserves_first_wins_failure_precedence",
        "production_process_publishes_ready_drain_and_stop_telemetry",
    ):
        require(test in process, f"telemetry executable evidence missing: {test}")

    for fragment in (
        "SOURCE REVIEW CANDIDATE — I1 NOT CLOSED",
        "write-only `Stage8bP1eTelemetryPublisherV1`",
        "Starting/PaperReady/Degraded/Draining/Stopped",
        "XADD <fixed-stream> NOMKSTREAM MAXLEN = 4096",
        "fixed-path installation and systemd material",
        "No operational installation or service start is authorized here",
    ):
        require(fragment in document, f"telemetry document fragment missing: {fragment}")
    for fragment in (
        "I1 telemetry composition candidate",
        "source review candidate, not acceptance",
        "Operational Redis, VPS activation",
    ):
        require(fragment in status, f"current status fragment missing: {fragment}")


def validate(root: Path = ROOT, verify_lineage: bool = True) -> None:
    if verify_lineage:
        validate_lineage(root)
    contract_digest = hashlib.sha256((root / CONTRACT).read_bytes()).hexdigest()
    require(contract_digest == TELEMETRY_CONTRACT_SHA256, "accepted telemetry contract drift")
    validate_matrix(root)
    validate_content(load_content(root))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-lineage", action="store_true")
    arguments = parser.parse_args()
    try:
        validate(ROOT, verify_lineage=not arguments.skip_lineage)
    except (OSError, UnicodeDecodeError, CheckFailure) as error:
        print(f"stage8b-p1e-i1-telemetry-composition-check: FAIL {error}")
        return 1
    print("stage8b-p1e-i1-telemetry-composition-check: PASS rows=20 closed_surfaces=9")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
