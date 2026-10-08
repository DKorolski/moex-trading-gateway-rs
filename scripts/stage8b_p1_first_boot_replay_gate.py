#!/usr/bin/env python3
"""Offline source qualification; no VPS/broker/operational Redis access.

Inherited library tests may start their own disposable loopback Redis.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("public_inputs", type=Path, help="retained public-failure-inputs.stdout")
    parser.add_argument("output", type=Path, help="new evidence directory")
    parser.add_argument("--repro-only", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    inputs = args.public_inputs.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, RUST_MIN_STACK="33554432", CARGO_NET_OFFLINE="true",
               O2_PUBLIC_INPUTS=str(inputs))
    tests = [
        ("retained-source-restart", ["cargo", "test", "--locked", "--offline", "-p",
            "runtime-durable-service", "--all-features", "--lib",
            "retained_o2_first_boot_prepares_and_restarts", "--", "--ignored", "--nocapture"]),
    ]
    if not args.repro_only:
        tests += [
            ("fmt", ["cargo", "fmt", "--all", "--check"]),
            ("core-tests", ["cargo", "test", "--locked", "--offline", "-p",
                "strategy-runtime-core", "--all-features", "--lib", "--", "--test-threads=1"]),
            ("durable-first-boot-tests", ["cargo", "test", "--locked", "--offline", "-p",
                "runtime-durable-service", "--all-features", "--lib",
                "stage8b_p1e_first_boot_source::", "--", "--test-threads=1"]),
            ("clippy", ["cargo", "clippy", "--locked", "--offline", "-p", "strategy-runtime-core",
                "-p", "runtime-durable-service", "--all-features", "--all-targets", "--", "-D", "warnings"]),
        ]
    result = {"scope": "offline-source-only", "input_file_sha256": hashlib.sha256(inputs.read_bytes()).hexdigest(),
              "operational_attempt": False, "vps_contact": False, "gates": []}
    for name, command in tests:
        print(f"RUN {name}", flush=True)
        start = time.monotonic()
        with (output / f"{name}.log").open("wb") as log:
            completed = subprocess.run(command, cwd=root, env=env, stdout=log,
                                       stderr=subprocess.STDOUT, timeout=1800, check=False)
        result["gates"].append({"name": name, "command": command, "exit_code": completed.returncode,
            "elapsed_seconds": round(time.monotonic() - start, 2),
            "log_sha256": hashlib.sha256((output / f"{name}.log").read_bytes()).hexdigest()})
        result["all_passed"] = all(g["exit_code"] == 0 for g in result["gates"])
        (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print(f"{'PASS' if completed.returncode == 0 else 'FAIL'} {name}", flush=True)
        if completed.returncode:
            print((output / f"{name}.log").read_text()[-6000:])
            return completed.returncode
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
