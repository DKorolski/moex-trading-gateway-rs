#!/usr/bin/env python3
"""Local source review gate. No VPS, Redis provisioning or FINAM calls."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/stage8b-p1f-no-riskgate-source"
COMMANDS = [
    ["cargo", "fmt", "--all", "--", "--check"],
    ["cargo", "clippy", "-p", "strategy-runtime-core", "-p", "runtime-durable-service",
     "-p", "finam-gateway", "--lib", "--bins", "--features",
     "finam-gateway/stage8b-r2a7-source-adapter", "--", "-D", "warnings"],
    ["cargo", "test", "-p", "strategy-runtime-core", "--lib", "--tests"],
    ["cargo", "test", "-p", "runtime-durable-service", "--lib", "--", "--test-threads=1"],
    ["cargo", "test", "-p", "finam-gateway", "--lib"],
    ["cargo", "test", "-p", "finam-gateway", "--bin", "stage8b-p1f-o2-materializer"],
    ["git", "diff", "--check"],
]


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def source_inventory():
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT
    ).decode().split("\0")
    return {name: sha256((ROOT / name).read_bytes()) for name in sorted(set(names)) if name}


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    before = source_inventory()
    env = dict(os.environ, RUST_MIN_STACK="33554432", CARGO_NET_OFFLINE="true")
    # Do not inherit optional operational Redis test endpoints from a shell.
    for key in list(env):
        if "REDIS" in key:
            env.pop(key)
    records = []
    for index, command in enumerate(COMMANDS):
        log_name = "{:02d}.txt".format(index)
        print("RUN " + " ".join(command), flush=True)
        with (OUTPUT / log_name).open("wb") as log:
            result = subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        raw = (OUTPUT / log_name).read_bytes()
        records.append({"command": command, "exit_code": result.returncode,
                        "log": log_name, "sha256": sha256(raw)})
        print("{} {}".format("PASS" if result.returncode == 0 else "FAIL", log_name), flush=True)
        if result.returncode:
            print(raw.decode(errors="replace")[-6000:], flush=True)
            break
    # This reports the old authority's actual status; it does NOT refresh its
    # pins, suppress CI, or call source acceptance a governance acceptance.
    authority = subprocess.run(["python3", "scripts/current_tree_authority_check.py"],
                               cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (OUTPUT / "current-tree-authority.txt").write_bytes(authority.stdout)
    unchanged = before == source_inventory()
    passed = unchanged and len(records) == len(COMMANDS) and all(r["exit_code"] == 0 for r in records)
    document = {
        "schema_version": 1, "scope": "BO-only no-riskgate source review only",
        "source_inventory": before, "source_unchanged_during_gate": unchanged,
        "commands": records, "source_gate_passed": passed,
        "authority_check_exit_code": authority.returncode,
        "authority_log_sha256": sha256(authority.stdout),
        "governance_rebind": "PENDING_SOURCE_ACCEPTANCE",
        "independent_acceptance": "NOT_CLAIMED", "operational_activation": False,
        "vps_changed": False, "finam_order_writes": False,
    }
    (OUTPUT / "result.json").write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
    print("SOURCE_GATE={}; GOVERNANCE_REBIND=PENDING_SOURCE_ACCEPTANCE".format("PASS" if passed else "FAIL"))
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
