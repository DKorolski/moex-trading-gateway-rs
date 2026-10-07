#!/usr/bin/env python3
"""Narrow successor: reuse sparse packing/smoke with explicit accepted V4 pins.

Historical sparse scripts are unchanged. This process-local binding selects only
the accepted timestamp tree; it is not a configurable build/authority override.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import uuid
import zipfile

import stage8b_p1f_o2_sparse_artifact as base
import stage8b_p1f_o2_v4_timestamp_build as source

ROOT = Path(__file__).resolve().parents[1]
base.source = source
base.P = "artifact-v4-timestamp/"
base.PROBE = "scripts/fixtures/stage8b-o2-v4-timestamp-release-probe.rs"
base.REVIEW = "docs/stage-8/reviews/REVIEW_f3b3499_O2_V4_TIMESTAMP_RU.txt"
base.REVIEW_SHA = "09235ba3ea44b43b0011d8f7dabd051b1e27e8b4d3e2f0258644b9383689a247"
# Deterministic outputs of the fractional probe; unchanged calendar/policy
# digests remain inherited. These are fixtures, never operational inputs.
base.FIXTURE_SHA = dict(base.FIXTURE_SHA) | {
    "mixed-foreign-policy.json": "7a5bca8fe72023886ca84b3efd528509fee71f1c808da0931b5e29e2e362e2ee",
    "mixed-legacy-fingerprint.json": "fabede9c93b5aef7c324aae608d16ed49bad8ed9de6dd2b3bcdaeefc761833dc",
    "mixed-missing-policy.json": "4c8c4e2a071ed266c781c615ffac820d4ce807acf8b1f315c5ffaa2230518210",
    "mixed-schema1.json": "ad55391bd44d89b1abc3bd342c5b4dd5477e0c9c916ac86cc061acae23dc8fd7",
    "probe-result.json": "d030b5d4b0bafeb2d882e61a042ec9a29440eba6e2a38334b59ee791c36a7af5",
    "source-v4-fixture.json": "6603e2a94341c2f0dba5528ff1036f638f4bc410524902873ed206fc3d5bb105",
    "supervisor-fixture.json": "2a3553d7e0372fb7b8bfedc43efcdcec448a02a44d16c14416bd8268015cdd33",
}

TESTS = [
    ("assembly", ["-p", "finam-gateway", "--lib", "observed_v4_"], 2,
     ["observed_v4_assembly_preserves_subsecond_receipt_chronology",
      "observed_v4_parser_failure_keeps_typed_safe_diagnostic_and_collection_progress"]),
    ("staged-cli", ["-p", "finam-gateway", "--bin", "stage8b-p1f-o2-materializer", "observed_tests::"], 4,
     ["observed_staged_consumer_accepts_real_history_only_with_explicit_protected_config"]),
    ("parser-recovery", ["-p", "runtime-durable-service", "--lib",
      "stage8b_p1e_first_boot_source::observed::tests::"], 8,
     ["observed_v4_subsecond_chronology_and_freshness_stay_fail_closed",
      "observed_v4_transaction_seal_restart_and_marker_bound_historical_recovery"]),
    ("guardian", ["-p", "runtime-durable-service", "--lib",
      "stage8b_p1f_guardian::tests::observed_o2_materialization_selects_v4_and_recovers_the_same_transaction"], 1,
     ["observed_o2_materialization_selects_v4_and_recovers_the_same_transaction"]),
]
base.QUALIFIED = (*base.QUALIFIED, "timestamp-tests.json", *(f"{t[0]}.log" for t in TESTS))
ORIGINAL_CHECK = base.check


def validate_log(raw, expected, names):
    matches = re.findall(rb"test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;", raw)
    base.require(matches and tuple(map(int, matches[-1])) == (expected, 0), "release test count")
    for name in names:
        base.require(re.search(rb"test [^\n]*::" + name.encode() + rb" \.\.\. ok", raw),
                     "missing selected release test: " + name)


def qualify(build, output):
    # Docker needs existing mountpoints below the read-only source bind.
    # Empty directories do not change any exported Git blob; all fixture writes
    # happen in disposable container tmpfs, never in the host source tree.
    for crate in ("finam-gateway", "runtime-durable-service"):
        scratch = build / "source/crates" / crate / "target"
        base.require(not scratch.is_symlink(), "scratch mountpoint symlink")
        scratch.mkdir(exist_ok=True)
        base.require(not any(scratch.iterdir()), "scratch mountpoint not empty")
    print("RUN exact release-library probe and inherited ELF smoke", flush=True)
    base.qualify(build, output)
    records = []
    before = {n: base.sha((build / "target/release" / n).read_bytes()) for n in source.builder.BINS}
    for label, args, count, names in TESTS:
        name = "stage8b-v4-tests-" + uuid.uuid4().hex[:12]
        command = ["docker", "run", "--rm", "--name", name, "--network", "none",
            "--platform", "linux/amd64", "--ulimit", "core=0",
            "--tmpfs", "/src/crates/finam-gateway/target:rw,nosuid,nodev,mode=0755",
            "--tmpfs", "/src/crates/runtime-durable-service/target:rw,nosuid,nodev,mode=0755",
            "--mount", f"type=bind,src={build / 'source'},dst=/src,readonly",
            "--mount", f"type=bind,src={build / 'target'},dst=/target",
            "--mount", f"type=bind,src={build / 'cargo-home'},dst=/cargo-home",
            "-w", "/src", "-e", "CARGO_HOME=/cargo-home", "-e", "CARGO_TARGET_DIR=/target",
            "-e", "CARGO_NET_OFFLINE=true", "-e", "CARGO_INCREMENTAL=0",
            "-e", "CARGO_TERM_COLOR=never", "-e", "RUST_MIN_STACK=33554432",
            source.IMAGE, "cargo", "test", "--locked", "--offline", "--release",
            *args, "--", "--test-threads=1"]
        print("RUN Linux release tests " + label, flush=True)
        try:
            done = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=1200)
            code, raw, timed_out = done.returncode, done.stdout, False
        except subprocess.TimeoutExpired as error:
            subprocess.run(["docker", "rm", "-f", name], capture_output=True, timeout=30, check=True)
            code, raw, timed_out = 124, error.stdout or b"", True
        (output / (label + ".log")).write_bytes(raw)
        records.append(dict(id=label, command=command, exit_code=code, timeout=timed_out,
                            log=label + ".log", sha256=base.sha(raw)))
        (output / "timestamp-tests.json").write_bytes(base.encoded(dict(
            compiled_ref=source.SOURCE_REF, records=records, binaries_before=before)))
        base.require(code == 0, "release gate failed: " + label)
        validate_log(raw, count, names)
        print("PASS Linux release tests " + label, flush=True)
    after = {n: base.sha((build / "target/release" / n).read_bytes()) for n in source.builder.BINS}
    base.require(before == after, "qualification changed ELFs")
    (output / "timestamp-tests.json").write_bytes(base.encoded(dict(
        compiled_ref=source.SOURCE_REF, records=records, binaries_before=before,
        binaries_after=after, result="PASS", finam_contact=False, redis_contact=False)))


def check(path):
    result = ORIGINAL_CHECK(path)
    q = base.P + "qualification/"
    with zipfile.ZipFile(path) as z:
        authority = "docs/stage-8/gov-ci-1-authority.json"
        before = json.loads(z.read(base.P + "build-preimages/" + authority))
        after = json.loads(z.read(authority))
        field = "governance_control_plane_manifest"
        base.require({k: v for k, v in before.items() if k != field}
                     == {k: v for k, v in after.items() if k != field}, "authority non-control delta")
        old, new = before[field]["entries"], after[field]["entries"]
        base.require(old.keys() == new.keys() and {k for k in old if old[k] != new[k]}
                     == {"docs/current-status.md", "docs/roadmap.md"}, "authority status-only delta")
        evidence = json.loads(z.read(q + "timestamp-tests.json"))
        build = json.loads(z.read(base.P + "build.json"))
        base.require(evidence["compiled_ref"] == source.SOURCE_REF and evidence["result"] == "PASS"
            and evidence["finam_contact"] is False and evidence["redis_contact"] is False, "release evidence")
        hashes = {b["name"]: b["sha256"] for b in build["binaries"]}
        base.require(evidence["binaries_before"] == evidence["binaries_after"] == hashes, "release ELF continuity")
        base.require(len(evidence["records"]) == len(TESTS), "release gate inventory")
        for record, (label, args, count, names) in zip(evidence["records"], TESTS):
            base.require(record["id"] == label and record["exit_code"] == 0
                         and record["timeout"] is False and record["log"] == label + ".log", "release gate result")
            command = record["command"]
            base.require(command[:4] == ["docker", "run", "--rm", "--name"]
                and command[4].startswith("stage8b-v4-tests-")
                and command[5:11] == ["--network", "none", "--platform", "linux/amd64", "--ulimit", "core=0"]
                and command.count(source.IMAGE) == 1 and "--privileged" not in command
                and not any("docker.sock" in arg for arg in command), "release isolation")
            base.require(command[11:15] == [
                "--tmpfs", "/src/crates/finam-gateway/target:rw,nosuid,nodev,mode=0755",
                "--tmpfs", "/src/crates/runtime-durable-service/target:rw,nosuid,nodev,mode=0755"],
                "release disposable scratch mounts")
            expected_tail = ["cargo", "test", "--locked", "--offline", "--release", *args, "--", "--test-threads=1"]
            base.require(command[command.index(source.IMAGE)+1:] == expected_tail, "release test selection")
            raw = z.read(q + record["log"])
            base.require(base.sha(raw) == record["sha256"], "release log hash")
            validate_log(raw, count, names)
        probe = json.loads(z.read(q + "probe-result.json"))
        for name in ("staged_exact_bytes_preserved", "truncated_capture_rejected", "staged_truncated_capture_rejected"):
            base.require(probe[name] is True, "fractional release probe " + name)
        base.require(probe["captured_at_utc"] == "2026-10-02T04:10:03.123456789Z"
                     and probe["receipt_received_at_utc"] == "2026-10-02T04:10:03.123456788Z", "exact ns witness")
    return result | dict(timestamp_release_tests=15, fractional_staged_probe=True,
                         source_authority_accept_ref=source.SOURCE_REF)


# The inherited packager calls this enhanced check before writing sidecars.
base.check = check


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("qualify", "package", "check"))
    parser.add_argument("path", type=Path)
    parser.add_argument("--build", type=Path)
    parser.add_argument("--proof", type=Path)
    args = parser.parse_args()
    if args.action == "check":
        print(base.encoded(check(args.path.resolve())).decode())
    elif args.action == "qualify":
        qualify(args.build.resolve(), args.path.resolve())
    else:
        base.package(args.build.resolve(), args.proof.resolve(), args.path.resolve())
