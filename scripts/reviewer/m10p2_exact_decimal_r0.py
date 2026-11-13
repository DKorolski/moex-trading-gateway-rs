#!/usr/bin/env python3
"""Offline CLI regression for the a7b2c636 M10 exact-decimal contract.

Only the explicitly supplied CLI is executed. Package files are read-only;
all generated manifests, inputs, SQLite files and evidence go to --output.
No broker, network, installed runtime or repository mutation is involved.
"""

import argparse
import copy
import hashlib
import json
import re
import subprocess
import sys
from decimal import Decimal, localcontext
from pathlib import Path


def load_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def json_bytes(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode()


def write_json(path, value):
    path.write_bytes(json_bytes(value) + b"\n")


def run(command):
    return subprocess.run(command, capture_output=True, text=True, timeout=90)


def compact_states(export):
    result = []
    for state in export.get("states", []):
        if not state.get("arm", "").startswith("IMOEXF/"):
            continue
        model = state.get("model", {}).get("model", {}).get("state", {})
        result.append({
            "arm": state["arm"],
            "ready": state["ready"],
            "bars_processed": state["bars_processed"],
            "observation": model.get("observation"),
            "pending": model.get("pending"),
        })
    return result


def prepare(package, output, implementation, refresh):
    base = package / "evidence" / "local-integration"
    manifest = load_json(base / "manifest.json")
    candles = load_json(base / "candles.json")
    original_impl = manifest["implementation_sha256"]
    if refresh:
        manifest["implementation_sha256"] = implementation
    elif original_impl != implementation:
        raise ValueError(
            "Package implementation differs from the CLI. For an explicitly "
            "reviewed R1 binary, use --refresh-implementation; the default never "
            "silently changes the implementation identity."
        )

    sparse_manifest = copy.deepcopy(manifest)
    sparse_manifest["generation"] = "reviewer-exact-decimal-sparse"
    for seed in sparse_manifest["seeds"]:
        seed["history"] = []
    target = next(s for s in sparse_manifest["seeds"]
                  if s["binding"]["instrument"] == "IMOEXF")
    target["binding"]["construction"] = "sparse_m1_v1"
    # Hash uses the Rust Binding struct's serialization field order.
    binding = target["binding"]
    ordered_binding = {k: binding[k] for k in (
        "instrument", "provider_id", "catalog_sha256", "basis", "tick",
        "lot", "construction")}
    binding_sha = hashlib.sha256(json_bytes(ordered_binding)).hexdigest()
    first = copy.deepcopy(next(c for c in candles if c["instrument"] == "IMOEXF"))
    first["source_sequence"] = 1
    first["binding_sha256"] = binding_sha
    flat = dict(open="100.1", high="100.1", low="100.1", close="100.1", volume="1")
    cases = []

    def add_sparse(name, volumes, volume, kind="defect", prices=None):
        candle = copy.deepcopy(first)
        candle["data"] = dict(flat, volume=volume)
        minutes = []
        for index, item in enumerate(volumes):
            data = dict(flat, volume=item)
            if prices is not None:
                for field in ("open", "high", "low", "close"):
                    data[field] = prices[0]
                    candle["data"][field] = prices[1]
            minutes.append({"start_ms": candle["start_ms"] + index * 60000,
                            "data": data})
        candle["construction"] = {
            "kind": "sparse_m1_v1", "raw_sha256": "a" * 64,
            "range_from_ms": candle["start_ms"], "range_to_ms": candle["end_ms"],
            "declared_complete_response": True, "actual_m1": minutes,
        }
        cases.append((name, "manifest-sparse", [candle], kind))

    add_sparse("control-sparse", ["0.1", "0.2"], "0.3", "positive")
    add_sparse("parse-rounding-large", ["10000000000000000000000000000.1"],
               "10000000000000000000000000000")
    add_sparse("add-rounding-large", ["4000000000000000000000000000.1"] * 2,
               "8000000000000000000000000000")
    add_sparse("parse-scale-large", ["100000000000000000000.0000000001"],
               "100000000000000000000.00000000")
    scientific_100_1 = "0.00000000000000000000000001001e28"
    add_sparse("scientific-volume", [scientific_100_1], "100")
    add_sparse("scientific-ohlc", ["1"], "1", prices=(scientific_100_1, "100"))

    off_manifest = copy.deepcopy(manifest)
    off_manifest["generation"] = "reviewer-exact-decimal-offtick"
    off_seed = next(s for s in off_manifest["seeds"]
                    if s["binding"]["instrument"] == "IMOEXF")
    if Decimal(off_seed["binding"]["tick"]) != Decimal("0.01"):
        raise ValueError("Expected accepted fixture tick0.01; input fixture changed")
    off_input = copy.deepcopy(next(c for c in candles if c["instrument"] == "IMOEXF"))
    for field in ("open", "high", "low", "close"):
        off_input["data"][field] = "0.0000000000000000000000000100001e28"
    cases.append(("scientific-offtick", "manifest-offtick", [off_input], "defect"))
    plain_input = copy.deepcopy(off_input)
    for field in ("open", "high", "low", "close"):
        plain_input["data"][field] = "100.001"
    cases.append(("control-plain-offtick", "manifest-offtick", [plain_input], "negative"))

    clock_manifest = copy.deepcopy(manifest)
    clock_manifest["generation"] = "reviewer-ms-clock-appendix"
    first_two = [copy.deepcopy(c) for c in candles if c["instrument"] == "IMOEXF"][:2]
    epoch = first_two[1]["end_ms"]
    for candle in first_two:
        candle["received_ms"] = epoch + 10
        candle["durable_ms"] = epoch + 20
    first_two[0]["source_processed_ms"] = epoch + 900
    first_two[1]["source_processed_ms"] = epoch + 901
    cases.append(("control-clock", "manifest-clock", copy.deepcopy(first_two), "positive"))
    first_two[1]["source_processed_ms"] = epoch + 100
    cases.append(("appendix-clock-regression", "manifest-clock", first_two, "informational"))

    manifests = {"manifest-sparse": sparse_manifest, "manifest-offtick": off_manifest,
                 "manifest-clock": clock_manifest}
    for name, value in manifests.items():
        write_json(output / (name + ".json"), value)
    for name, _, value, _ in cases:
        write_json(output / (name + ".json"), value)
    return cases, manifests, original_impl


def exact_math(candles):
    first = candles[0]
    construction = first["construction"]
    if construction["kind"] == "sparse_m1_v1":
        minutes = construction["actual_m1"]
        with localcontext() as context:
            context.prec = 100
            exact = {
                "open": Decimal(minutes[0]["data"]["open"]),
                "high": max(Decimal(m["data"]["high"]) for m in minutes),
                "low": min(Decimal(m["data"]["low"]) for m in minutes),
                "close": Decimal(minutes[-1]["data"]["close"]),
                "volume": sum((Decimal(m["data"]["volume"]) for m in minutes), Decimal(0)),
            }
            return {
                "exact_aggregate": {k: str(v) for k, v in exact.items()},
                "declared_aggregate": first["data"],
                "mathematically_equal": all(v == Decimal(first["data"][k])
                                             for k, v in exact.items()),
            }
    if len(candles) == 2:
        times = [c["source_processed_ms"] for c in candles]
        return {"processed_ms": times, "delta_ms": times[1] - times[0],
                "projected_seconds": [t // 1000 for t in times],
                "scope": "Nonblocking clarification; no new raw-ms gate required by this review"}
    with localcontext() as context:
        context.prec = 100
        value = Decimal(first["data"]["close"])
        return {"exact_close": str(value), "tick": "0.01",
                "exact_tick_remainder": str(value % Decimal("0.01"))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--package", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--expect", choices=("candidate", "reject"), default="candidate",
                        help="candidate reproduces R0 acceptance; reject asserts all defect inputs fail for R1")
    parser.add_argument("--refresh-implementation", action="store_true",
                        help="Explicitly bind generated manifests to the supplied binary's implementation hash")
    args = parser.parse_args()
    binary, package, output = (p.resolve() for p in (args.binary, args.package, args.output))
    if output.exists() and any(output.iterdir()):
        parser.error("--output must be absent or empty; existing evidence is never overwritten")
    output.mkdir(parents=True, exist_ok=True)
    impl_run = run([str(binary), "implementation"])
    implementation = impl_run.stdout.strip()
    if impl_run.returncode or not re.fullmatch(r"[0-9a-f]{64}", implementation):
        raise ValueError("CLI implementation query failed: " + impl_run.stderr)
    cases, manifests, package_impl = prepare(package, output, implementation,
                                            args.refresh_implementation)
    results, failures = [], []
    for name, manifest_name, candles, kind in cases:
        manifest_path = output / (manifest_name + ".json")
        database = output / (name + ".sqlite")
        command = [str(binary), "replay", str(manifest_path), str(database),
                   str(output / (name + ".json"))]
        replay = run(command)
        (output / (name + ".stdout.json")).write_text(replay.stdout, encoding="utf-8")
        (output / (name + ".stderr.txt")).write_text(replay.stderr, encoding="utf-8")
        expected = (True if kind == "positive" else False if kind == "negative" else
                    args.expect == "candidate" if kind == "defect" else None)
        row = {"case": name, "kind": kind, "command": command,
               "expected_acceptance": expected, "returncode": replay.returncode,
               "accepted": replay.returncode == 0, "stderr": replay.stderr,
               "exact_math": exact_math(candles)}
        if expected is not None and row["accepted"] != expected:
            failures.append(name + ": unexpected admission result")
        reopen = run([str(binary), "export", str(manifest_path), str(database)])
        (output / (name + ".reopen.stdout.json")).write_text(reopen.stdout, encoding="utf-8")
        (output / (name + ".reopen.stderr.txt")).write_text(reopen.stderr, encoding="utf-8")
        row["reopen_returncode"] = reopen.returncode
        if reopen.returncode:
            failures.append(name + ": failed to reopen resulting journal")
        else:
            export = json.loads(reopen.stdout)
            row["revision"] = export["revision"]
            row["input_count"] = len(export["inputs"])
            row["result_count"] = len(export["results"])
            row["imoexf_states"] = compact_states(export)
            # All defect and negative cases contain one input. Rejection must
            # leave no committed input, output, marks or model progress.
            wanted_revision = len(candles) if row["accepted"] else 0
            if kind != "informational" and (row["revision"] != wanted_revision or
                row["input_count"] != wanted_revision or
                row["result_count"] != wanted_revision * 2):
                failures.append(name + ": incorrect durable frontier after admission/rejection")
        results.append(row)
    report = {
        "schema": "reviewer.m10-exact-decimal.v1", "expected_mode": args.expect,
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "binary_implementation_sha256": implementation,
        "package_implementation_sha256": package_impl,
        "manifest_implementation_refreshed": args.refresh_implementation,
        "result": "PASS" if not failures else "FAIL", "failures": failures,
        "cases": results,
        "scope": "Synthetic local source review; no operational or observed-session claim",
    }
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"result": report["result"], "expected_mode": args.expect,
                      "case_count": len(results), "failures": failures,
                      "report": str(output / "report.json")}, indent=2))
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
