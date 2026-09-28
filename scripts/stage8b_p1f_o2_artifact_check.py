#!/usr/bin/env python3
"""Validate the immutable Stage 8B-P1F O2 execution artifact."""

from __future__ import annotations

import csv
import hashlib
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
ARTIFACT = ROOT / "docs/stage-8/stage8b-p1f-o2-execution-artifact.json"
MATRIX = ROOT / "docs/stage-8/stage8b-p1f-o2-artifact-acceptance-matrix.csv"
IMPLEMENTATION_REF = "9e7f44d119cac63e33d5d5437ff972e49922c797"
CONTRACT_REF = "a9f8fe30a45752c943f9e399775322d83fcd8a36"
PUBLIC_KEY = "8ed71461f37c51d6239db25aeac1e709f250bd16d6d7bb6ba117b716cebf4802"
ACCOUNT_HASH = "e14dde9c8231a28b065789a2deba6f65804c1cad7dd3aa98776f2515e1ad671f"
ACCOUNT_ALIAS = "finam-paper-primary"
SOURCE_SENTINEL = "f" * 64
TEMPLATE_ACCOUNT_SENTINEL = "INJECT_FROM_ACCOUNT_CREDENTIAL"
EXPECTED_MATRIX_IDS = [f"O2A-{index:03d}" for index in range(1, 31)]


class ArtifactError(ValueError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ArtifactError(message)


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        require(key not in value, f"duplicate JSON key: {key}")
        value[key] = item
    return value


def load_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(), object_pairs_hook=strict_object)
    require(isinstance(value, dict), f"JSON root is not an object: {path}")
    return value


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canonical_digest(value: Any) -> str:
    raw = json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode()
    return hashlib.sha256(raw).hexdigest()


def parse_time(value: str) -> datetime:
    require(value.endswith("Z"), "timestamp is not canonical UTC")
    return datetime.fromisoformat(value[:-1] + "+00:00")


def validate_document(document: dict[str, Any], *, verify_files: bool = True) -> None:
    require(document.get("schema_version") == 1, "schema drift")
    require(document.get("domain") == "moex.stage8b.p1f.o2.execution-artifact.v1", "domain drift")
    require(document.get("status") == "REVIEW_CANDIDATE_EXECUTION_NOT_AUTHORIZED", "status drift")
    require(document.get("accepted_contract_ref") == CONTRACT_REF, "contract ref drift")
    require(document.get("implementation_ref") == IMPLEMENTATION_REF, "implementation ref drift")
    require(document.get("execution_authorized") is False, "execution was authorized")
    require(document.get("target_mutation_performed") is False, "target mutation was claimed")

    authority = document["authority"]
    require(authority["key_id"] == "stage8b-p1f-o2-authority-generation-1", "authority key id drift")
    require(authority["public_key_ed25519_hex"] == PUBLIC_KEY, "authority public key drift")
    require(authority["public_key_sha256"] == hashlib.sha256(bytes.fromhex(PUBLIC_KEY)).hexdigest(), "authority digest drift")
    require(authority["private_key_in_artifact"] is False, "private authority key included")

    account = document["account_boundary"]
    require(account == {
        "broker_account_id_in_artifact": False,
        "broker_account_id_sha256": ACCOUNT_HASH,
        "broker_neutral_account_alias": ACCOUNT_ALIAS,
        "credential_path": "/run/credentials/moex-finam-p1f-o2-materializer.service/finam-account.id",
        "token_path": "/run/credentials/moex-finam-p1f-o2-materializer.service/finam-readonly.token",
    }, "account boundary drift")

    roles = document["roles"]
    require(len(roles) == 5 and len({item["id"] for item in roles}) == 5, "five-role inventory drift")
    require({item["id"] for item in roles} == {"offline-signer", "root-guardian", "get-materializer", "systemd-runner", "evidence-collector"}, "role identity drift")
    require({item["binary"] for item in roles} == {"stage8b-p1f-o2-materializer", "stage8b-p1f-o2-operator"}, "binary role mapping drift")

    build = document["build"]
    require(build["platform"] == "linux/amd64", "build platform drift")
    require(build["rust_image"] == "rust@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922", "build image drift")
    require(build["cargo_args"] == ["build", "--locked", "--release", "-p", "finam-gateway", "--bin", "stage8b-p1f-o2-materializer", "-p", "runtime-durable-service", "--bin", "stage8b-p1f-o2-operator"], "build command drift")
    binaries = build["binaries"]
    require(len(binaries) == 2, "binary inventory drift")
    for binary in binaries:
        require(binary["sha256"] and len(binary["sha256"]) == 64, "binary hash missing")
        require(isinstance(binary["size"], int) and binary["size"] > 1_000_000, "binary size invalid")
        require(binary["elf_machine"] == "x86-64", "binary architecture drift")

    public_inputs = document["public_inputs"]
    for name, item in public_inputs.items():
        require(set(item) == {"path", "sha256"}, f"public input shape drift: {name}")
        if verify_files:
            path = ROOT / item["path"]
            require(path.is_file(), f"public input missing: {name}")
            require(digest(path) == item["sha256"], f"public input hash drift: {name}")

    sources = document["source_files"]
    for item in sources:
        if verify_files:
            require(digest(ROOT / item["path"]) == item["sha256"], f"source hash drift: {item['path']}")
    units = document["units"]
    require({item["unit"] for item in units} == {"moex-finam-p1f-o2-materializer.service", "moex-finam-p1-paper-o2-bootstrap-runner.service"}, "unit inventory drift")
    for item in units:
        if verify_files:
            require(digest(ROOT / item["path"]) == item["sha256"], f"unit hash drift: {item['unit']}")

    guard = document["get_guard"]
    require(guard["base_url"] == "https://api.finam.ru", "GET base URL drift")
    require(guard["methods"] == ["GET"] and guard["route_kinds"] == ["account", "account-orders", "asset-params", "asset-schedule", "m1-bars"], "GET allowlist drift")
    require(guard["redirects"] is False and guard["system_proxy"] is False, "GET redirect/proxy opened")
    require(guard["write_methods"] is False and guard["orders_snapshot_is_read_only_truth"] is True, "GET/write scope drift")

    supervisor = document["supervision"]
    require(supervisor["poll_interval_ms"] == 250, "poll interval drift")
    require(supervisor["command_timeout_seconds"] == 5, "command timeout drift")
    require(supervisor["stop_grace_seconds"] == 30, "stop grace drift")
    require(supervisor["systemctl_exit_is_stopped_proof"] is False, "systemctl exit became proof")
    require(supervisor["stopped_proof"] == ["no-job", "main-pid-zero", "control-pid-zero", "inactive-or-failed", "empty-cgroup"], "stopped proof drift")
    require(supervisor["pending_terminal_replay"] == "byte-exact-state-reason-timestamp", "terminal replay drift")
    require(supervisor["restart_grants_new_grace"] is False, "restart grace opened")

    commands = document["commands"]
    require(commands["installation_and_execution_require_separate_acceptance"] is True, "command authorization drift")
    require(commands["runner_start"] == ["/usr/bin/systemctl", "start", "moex-finam-p1-paper-o2-bootstrap-runner.service"], "runner command drift")
    require(commands["evidence"] == ["/usr/local/libexec/moex/stage8b-p1f-o2-operator", "collect-evidence"], "evidence command drift")

    excluded = set(document["excluded"])
    require({"private-o2-authority-key", "finam-readonly-token", "raw-finam-account-id", "lifecycle-credential", "fresh-finam-response-bytes", "signed-time-bounded-authority-documents"} <= excluded, "secret exclusion drift")
    require(all(value is False for value in document["closed_surfaces"].values()), "closed surface opened")


def validate_files() -> None:
    policy = load_json(ROOT / "docs/stage-8/stage8b-p1f-o2-materialization-policy.json")
    require(policy == {
        "schema_version": 1,
        "domain": "stage8b-p1f-o2-materialization-policy-v1",
        "account_id_sha256": ACCOUNT_HASH,
        "account_alias": ACCOUNT_ALIAS,
        "venue_symbol": "IMOEXF@RTSX",
        "bars_start_utc": "2025-09-27T00:00:00Z",
        "bars_end_utc": "2026-10-31T00:00:00Z",
    }, "materialization policy drift")
    require(180 * 86400 <= (parse_time(policy["bars_end_utc"]) - parse_time(policy["bars_start_utc"])).total_seconds() <= 400 * 86400, "bars range drift")

    source = load_json(ROOT / "docs/stage-8/stage8b-p1f-o2-source-template.json")
    require(source["schema_version"] == 2 and source["domain"] == "moex.stage8b.p1e.first-boot-source-bundle.v2", "source template identity drift")
    require(source["broker_truth"] == {"account_id": TEMPLATE_ACCOUNT_SENTINEL}, "source account sentinel drift")
    require(source["operational_identity_sha256"] == "9b3572618c2540b32e7fd2fb256b5604fd48aec74b7de63a1e01d498b4b962c0", "source operational identity drift")
    sessions = source["history_coverage"]["sessions"]
    require(len(sessions) == 121, "source session count drift")
    dates = [item["session_date"] for item in sessions]
    require(dates == sorted(set(dates)) and dates[-1] == "2026-09-25", "source session ordering drift")
    previous_close = 0
    for session in sessions:
        local_date = datetime.fromisoformat(session["session_date"]).date()
        require(local_date.weekday() < 5, "weekend session admitted")
        require(len(session["windows"]) == 3, "session window inventory drift")
        for window in session["windows"]:
            first = window["first_close_time_utc"]
            last = window["last_close_time_utc"]
            require(first > previous_close and first % 600 == 0 and last % 600 == 0 and first <= last, "session window chronology drift")
            require(datetime.fromtimestamp(first + 3 * 3600, timezone.utc).date() == local_date, "session first close Moscow date drift")
            require(datetime.fromtimestamp(last + 3 * 3600, timezone.utc).date() == local_date, "session last close Moscow date drift")
            previous_close = last

    config = load_json(ROOT / "docs/stage-8/stage8b-p1f-o2-supervisor-template.json")
    require(config["first_boot_source_bundle_sha256"] == SOURCE_SENTINEL, "source hash sentinel drift")
    bootstrap = config["bootstrap"]
    require(bootstrap["account_id"] == ACCOUNT_ALIAS, "broker-neutral account alias drift")
    identity = {
        "broker_id": bootstrap["broker_id"],
        "strategy_instance_id": bootstrap["strategy_id"],
        "deployment_id": bootstrap["deployment_id"],
        "deployment_generation": bootstrap["deployment_generation"],
        "gateway_instance_id": bootstrap["gateway_instance_id"],
        "instrument_map_fingerprint_sha256": bootstrap["instrument_map_fingerprint_sha256"],
        "market_data_generation": bootstrap["market_data_generation"],
        "command_consumer_generation": bootstrap["command_consumer_generation"],
        "stage8a4_writer_issuer_public_key_hex": bootstrap["stage8a4_writer_issuer_public_key_hex"],
    }
    require(canonical_digest(identity) == source["operational_identity_sha256"], "operational identity cross-binding drift")
    require((ROOT / "docs/stage-8/stage8b-p1f-o2-authority-public-key.hex").read_text() == PUBLIC_KEY + "\n", "public key file drift")

    get_source = (ROOT / "crates/broker-finam/src/o2_readonly.rs").read_text()
    for token in [
        ".get(url)",
        'method != "GET"',
        "redirect(Policy::none())",
        "no_proxy()",
        "Stage8bP1fO2GetRouteV1::AccountOrders",
    ]:
        require(token in get_source, f"GET source token missing: {token}")
    for token in ["Method::POST", "Method::PUT", "Method::PATCH", "Method::DELETE"]:
        require(token not in get_source, f"write method in GET source: {token}")

    materializer = (ROOT / "crates/finam-gateway/src/stage8b_p1f_o2_materializer.rs").read_text()
    require(f'pub const STAGE8B_P1F_O2_ACCOUNT_ALIAS: &str = "{ACCOUNT_ALIAS}";' in materializer, "account alias source drift")
    require('"account_id": source_account_alias' in materializer, "raw account durable projection restored")

    runner = (ROOT / "crates/runtime-durable-service/src/stage8b_p1f_o2_systemd.rs").read_text()
    for token in ["COMMAND_TIMEOUT", "StdDuration::from_secs(5)", "POLL_INTERVAL", "StdDuration::from_millis(250)", "stop_kill_and_prove", "resume_stopping_phase"]:
        require(token in runner, f"runner token missing: {token}")


def validate_matrix() -> None:
    with MATRIX.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require([row["id"] for row in rows] == EXPECTED_MATRIX_IDS, "acceptance matrix identity drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "acceptance matrix requirement drift")


def validate_git_scope() -> None:
    head = subprocess.check_output(("git", "rev-parse", "HEAD"), cwd=ROOT, text=True).strip()
    require(subprocess.run(("git", "merge-base", "--is-ancestor", IMPLEMENTATION_REF, head), cwd=ROOT).returncode == 0, "implementation is not an ancestor")
    production = [
        "crates/broker-finam/src/o2_readonly.rs",
        "crates/finam-gateway/src/bin/stage8b-p1f-o2-materializer.rs",
        "crates/finam-gateway/src/stage8b_p1f_o2_materializer.rs",
        "crates/runtime-durable-service/src/bin/stage8b-p1f-o2-operator.rs",
        "crates/runtime-durable-service/src/stage8b_p1f_o2_systemd.rs",
        "deploy/stage8b-p1e/moex-finam-p1f-o2-materializer.service",
        "deploy/stage8b-p1e/moex-finam-p1-paper-o2-bootstrap-runner.service",
    ]
    changed = subprocess.check_output(("git", "diff", "--name-only", IMPLEMENTATION_REF, head, "--", *production), cwd=ROOT, text=True).splitlines()
    require(not changed, f"production drift after implementation ref: {changed}")


def main() -> None:
    try:
        document = load_json(ARTIFACT)
        validate_document(document)
        validate_files()
        validate_matrix()
        if os.environ.get("STAGE8B_O2_SKIP_GIT") != "1":
            validate_git_scope()
    except (OSError, KeyError, TypeError, json.JSONDecodeError, ArtifactError, subprocess.CalledProcessError) as error:
        print(f"stage8b-p1f-o2-artifact-check: FAIL {error}")
        raise SystemExit(1) from error
    print("PASS stage8b-p1f-o2-artifact-check rows=30 execution=false")


if __name__ == "__main__":
    main()
