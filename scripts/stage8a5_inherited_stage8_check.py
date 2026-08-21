#!/usr/bin/env python3
"""Replay every accepted Stage 8A semantic and negative gate at its own commit."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path


SLICES = (
    ("8A-0", "c949d7f83aa87cf990204a5b8ae66e5ca37c9f1d", "stage8a0-contract-freeze", "stage8a0_check.py", "stage8a0_negative_harness.py", 41),
    ("8A-1", "1ff04154ba4b7a5ee060a73b853ce89bd7442f44", "stage8a1-protected-capability", "stage8a1_check.py", "stage8a1_negative_harness.py", 70),
    ("8A-2", "16180ac4f8eab761b3b055c1f5515f62cd94bfb9", "stage8a2-builder-composition", "stage8a2_check.py", "stage8a2_negative_harness.py", 37),
    ("8A-3", "012c9bfa51c1d6206fbd9a7e1f06f1fc90fdf30d", "stage8a3-endpoint-classifier", "stage8a3_check.py", "stage8a3_negative_harness.py", 44),
    ("8A-4", "4caf07c16ddad021add7cffe6e887165e49e1bf0", "stage8a4-reconciliation-implementation", "stage8a4_implementation_check.py", "stage8a4_implementation_negative_harness.py", 58),
    ("8A-4-design", "6ddf54ef9d7f740dc59cd2450e78301be3d068cb", "stage8a4-durable-composition-design", "stage8a4_durable_composition_design_check.py", "stage8a4_durable_composition_design_negative_harness.py", 38),
    ("8A-4-spec", "dd01253596527d6cff1db11cc32ae3c3348c96a0", "stage8a4-durable-composition-implementation-spec", "stage8a4_durable_composition_implementation_spec_check.py", "stage8a4_durable_composition_implementation_spec_negative_harness.py", 57),
    ("8A-4-I1", "113d2827ef255e8d2c2597a3acb38fe52dd7e52d", "stage8a4-durable-composition-i1", "stage8a4_durable_composition_i1_check.py", "stage8a4_durable_composition_i1_negative_harness.py", 25),
    ("8A-4-I2", "90f46052cc31cea012437eddb59fb7c3ca5c2320", "stage8a4-durable-composition-i2", "stage8a4_durable_composition_i2_check.py", "stage8a4_durable_composition_i2_negative_harness.py", 33),
    ("8A-4-I3", "593ff255ef7826a22e66c9aff6f7ea47acf47644", "stage8a4-durable-composition-i3-r6", "stage8a4_durable_composition_i3_check.py", "stage8a4_durable_composition_i3_negative_harness.py", 95),
    ("8A-4-I4-design", "81727aae1f648f17961177fc9541e2483cbf07f2", "stage8a4-durable-composition-i4-design-r3", "stage8a4_durable_composition_i4_design_check.py", "stage8a4_durable_composition_i4_design_negative_harness.py", 46),
)


def run(repo: Path, command: list[str]) -> dict[str, object]:
    result = subprocess.run(command, cwd=repo, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}): {' '.join(command)}\n{output}")
    return {
        "command": command,
        "exit_code": result.returncode,
        "output_sha256": hashlib.sha256(output.encode()).hexdigest(),
        "last_line": output.strip().splitlines()[-1] if output.strip() else "",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    results: list[dict[str, object]] = []
    with tempfile.TemporaryDirectory(prefix="stage8a5-inherited-") as temp:
        checkout = Path(temp) / "repo"
        subprocess.run(
            ["git", "clone", "--quiet", "--no-hardlinks", "--shared", str(root), str(checkout)],
            check=True,
        )
        for stage, commit, branch, checker, negative, cases in SLICES:
            subprocess.run(["git", "checkout", "--quiet", "-B", branch, commit], cwd=checkout, check=True)
            semantic = run(checkout, ["python3", f"scripts/{checker}"])
            negatives = run(checkout, ["python3", f"scripts/{negative}"])
            results.append({
                "stage": stage,
                "commit": commit,
                "branch": branch,
                "negative_cases": cases,
                "semantic": semantic,
                "negative": negatives,
            })
            print(f"PASS inherited {stage} commit={commit[:7]} negatives={cases}")
    evidence = {
        "schema_version": 1,
        "stage": "8A-5-aggregate-acceptance",
        "accepted_slice_count": len(results),
        "negative_case_count": sum(int(item[5]) for item in SLICES),
        "all_passed": True,
        "results": results,
    }
    encoded = json.dumps(evidence, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded, encoding="utf-8")
    print(f"stage8a5-inherited-stage8-check: PASS slices={len(results)} negatives={evidence['negative_case_count']}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"stage8a5-inherited-stage8-check: FAIL {error}") from error
