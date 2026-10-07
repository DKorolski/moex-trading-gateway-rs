#!/usr/bin/env python3
"""Bounded terminal-abort source gate and immutable ZIP, never remote execution."""
import argparse
import json
from pathlib import Path
import sys
import zipfile

import current_tree_authority_check as authority
import stage8b_p1f_nrg01_test_gate as bounded
import stage8b_p1f_o2_recovery_review as handoff

ROOT = Path(__file__).resolve().parents[1]
BASE = "d866cf992a963fbd76c4532aae14088f6e6046ee"
handoff.BASE = BASE
PREFIX = "crates/runtime-durable-service/src/"
PRODUCTION = {PREFIX + p for p in (
    "stage8b_p1f_guardian.rs", "stage8b_p1f_guardian/materialization_abort.rs",
    "stage8b_p1f_guardian/tests/abort_tests.rs", "stage8b_p1f_o2_systemd.rs",
    "stage8b_p1f_o2_systemd/terminal_abort.rs", "lib.rs", "bin/stage8b-p1f-o2-operator.rs")}
ALLOWED = PRODUCTION | {
    "docs/current-status.md", "docs/roadmap.md",
    "docs/stage-8/stage8b-p1f-o2-external-mode-correction.md",
    "docs/stage-8/stage8b-p1f-o2-materialization-abort.md",
    "docs/stage-8/stage8b-p1f-o2-abort-preservation.json",
    "scripts/stage8b_p1f_o2_abort_review.py",
}
TEST = ["cargo", "test", "--locked", "--offline", "-p", "runtime-durable-service", "--all-features"]
COMMANDS = [
    ("fmt", ["cargo", "fmt", "--all", "--check"], 120),
    ("guardian-debug", TEST + ["--lib", "stage8b_p1f_guardian::tests::", "--", "--test-threads=1", "--nocapture"], 1200),
    ("systemd-debug", TEST + ["--lib", "stage8b_p1f_o2_systemd::", "--", "--test-threads=1"], 300),
    ("guardian-release", TEST + ["--release", "--lib", "stage8b_p1f_guardian::tests::", "--", "--test-threads=1", "--nocapture"], 1800),
    ("doctests", TEST + ["--doc"], 300),
    ("clippy", ["cargo", "clippy", "--locked", "--offline", "-p", "runtime-durable-service", "--all-targets", "--all-features", "--", "-D", "warnings"], 1200),
    ("diff", ["git", "diff", "--check", BASE], 60),
]


def gate(output):
    ref = handoff.clean_ref()
    changed = set(handoff.git("diff", "--name-only", BASE, ref).decode().splitlines())
    handoff.require(changed == ALLOWED, "unexpected source scope")
    pinned = json.loads((ROOT / authority.AUTHORITY).read_text())["production_code_manifest"]["entries"]
    actual = authority.file_inventory(ROOT, authority.production_files(ROOT))
    drift = {p for p in pinned.keys() | actual.keys() if pinned.get(p) != actual.get(p)}
    handoff.require(drift == PRODUCTION, "unexpected production drift")
    output.mkdir(parents=True, exist_ok=False)
    records, logs = [], {}
    for label, cmd, timeout in COMMANDS:
        print("RUN " + label, flush=True)
        record = bounded.run(cmd, output / label, timeout)
        records.append(record)
        raw = (output / label / "output.txt").read_bytes()
        handoff.require(record["exit_code"] == 0 and not record["deadline_exceeded"], "FAIL " + label)
        if label in ("guardian-debug", "guardian-release"):
            handoff.require(raw.count(b"PASS abort nonmutating negative ") == 28, "negative selection mismatch")
            handoff.require(raw.count(b"PASS abort reopen frontier ") == 15, "reopen selection mismatch")
            handoff.require(b"o2_abort_partial_archive_and_completed_archive_tamper_are_closed" in raw, "archive negatives absent")
            handoff.require(b"PASS exact materialization/temp/receipt replay under umask 0077" in raw, "prevention regression absent")
        if label == "systemd-debug":
            handoff.require(b"abort_preflight_requires_actual_stopped_units_not_asserted_flags" in raw
                            and b"abort_preflight_real_file_inventory_rejects_drift_and_links" in raw, "preflight tests absent")
        if label.endswith(("-debug", "-release")):
            handoff.require(b"test result: ok." in raw and b"0 passed; 0 failed" not in raw, "empty suite")
        logs[label + "/output.txt"] = handoff.digest(raw)
        print("PASS " + label, flush=True)
    label = "authority-pending"
    record = bounded.run([sys.executable, "scripts/current_tree_authority_check.py"], output / label, 60)
    raw = (output / label / "output.txt").read_bytes()
    expected_drift = "production file-count drift" if len(pinned) != len(actual) else "production entry drift"
    handoff.require(record["exit_code"] == 1 and not record["deadline_exceeded"]
                    and raw.strip() == ("current-tree-authority-check: FAIL " + expected_drift).encode(),
                    "unexpected authority status")
    records.append(record)
    logs[label + "/output.txt"] = handoff.digest(raw)
    handoff.require(handoff.clean_ref() == ref, "tree changed during gate")
    summary = dict(stage="O2 October-7 terminal-only materialization abort",
        source_ref=ref, source_tree=handoff.git("rev-parse", "HEAD^{tree}").decode().strip(),
        baseline_ref=BASE, source_gate_passed=True, status="SOURCE_REVIEW_PENDING",
        records=records, commands=COMMANDS, logs_sha256=logs,
        authority_status="ACCEPTED_BASELINE_REBIND_PENDING", authority_drift_paths=sorted(drift),
        merge_ready=False, execution_authorized=False, vps_contacted=False,
        finam_contacted=False, operational_redis_activated=False, o2_status="HOLD",
        limitations=["local Mac tests, not native Linux/root:service artifact qualification",
                     "controlled real-filesystem durable frontiers, not OS SIGKILL",
                     "remote recovery/installation/new phase require separate authorization",
                     "targeted suites, not full historical platform/CI rerun"])
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print("PASS source gate; review and authority rebind pending; O2 HOLD", flush=True)


def package(output):
    ref = handoff.clean_ref()
    summary = json.loads((output / "summary.json").read_text())
    handoff.require(summary["source_ref"] == ref, "evidence/HEAD mismatch")
    manifest, entries = handoff.packaging.source_manifest(ref)
    archive = ROOT / "reports/handoff" / f"moex-trading-project-{ref[:7]}-o2-terminal-abort-review.zip"
    handoff.require(not archive.exists(), "immutable archive exists")
    archive.parent.mkdir(parents=True, exist_ok=True)
    extra = {"handoff-commit.txt": handoff.marker(ref, summary["source_tree"], archive.name),
             "handoff-evidence/source-commit.raw": handoff.git("cat-file", "commit", ref),
             "handoff-evidence/source-tree-manifest.json": manifest,
             "handoff-evidence/summary.json": (output / "summary.json").read_bytes()}
    for name, digest in summary["logs_sha256"].items():
        raw = (output / name).read_bytes()
        handoff.require(handoff.digest(raw) == digest, "evidence changed")
        extra["handoff-evidence/" + name] = raw
    with zipfile.ZipFile(archive, "x", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for entry in entries:
            name = entry["path"]
            handoff.require(name not in extra, "source/evidence collision")
            z.writestr(handoff.packaging.zip_info(name, entry["mode"]), handoff.git("show", ref + ":" + name))
        for name, raw in extra.items():
            z.writestr(handoff.packaging.zip_info(name), raw)
    result = handoff.check_archive(archive)
    handoff.require(handoff.clean_ref() == ref, "tree changed during packaging")
    Path(str(archive) + ".sha256").write_text(result["archive_sha256"] + "  " + archive.name + "\n")
    Path(str(archive) + ".safety.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(dict(result, archive=str(archive)), indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["gate", "package", "check"])
    parser.add_argument("path", type=Path)
    args = parser.parse_args()
    if args.action == "gate":
        gate(args.path.resolve())
    elif args.action == "package":
        package(args.path.resolve())
    else:
        print(json.dumps(handoff.check_archive(args.path.resolve()), indent=2))
