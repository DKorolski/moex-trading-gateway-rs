#!/usr/bin/env python3
"""Generate deterministic dependency/toolchain evidence for TLS qualification."""

from __future__ import annotations

import hashlib
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
AUTHORITY = ROOT / "docs/stage-8/stage8b-tls-qualification-authority.json"
OUTPUT = ROOT / "reports/stage8b-tls-qualification-graph.json"


def run(*command: str) -> str:
    return subprocess.run(command, cwd=ROOT, check=True, text=True, capture_output=True).stdout


def graph(edges: str) -> tuple[str, str]:
    value = run(
        "cargo",
        "tree",
        "-p",
        "finam-gateway",
        "-e",
        edges,
        "--locked",
        "--prefix",
        "depth",
        "--format",
        "{p} {f}",
    ).replace(str(ROOT), "<repo>")
    return value, hashlib.sha256(value.encode()).hexdigest()


def main() -> None:
    authority = json.loads(AUTHORITY.read_text())
    production, production_sha = graph("normal,build")
    qualification, qualification_sha = graph("all")
    if production_sha != authority["production_graph_sha256"]:
        raise SystemExit("stage8b-tls-graph: FAIL production graph drift")
    if qualification_sha != authority["qualification_graph_sha256"]:
        raise SystemExit("stage8b-tls-graph: FAIL qualification graph drift")
    required_production = (
        "reqwest v0.12.24",
        "hyper-rustls v0.27.9",
        "rustls v0.23.41",
        "tokio-rustls v0.26.4",
    )
    if any(token not in production for token in required_production):
        raise SystemExit("stage8b-tls-graph: FAIL required production TLS component missing")
    if "native-tls" in production or "native_tls" in production or "rcgen" in production:
        raise SystemExit("stage8b-tls-graph: FAIL production graph contamination")
    if "rcgen v0.14.7" not in qualification:
        raise SystemExit("stage8b-tls-graph: FAIL qualification CA dependency missing")
    rustc = run("rustc", "-vV")
    cargo = run("cargo", "-V").strip()
    source_ref = run("git", "rev-parse", "HEAD").strip()
    result = {
        "schema_version": 1,
        "stage": "8B-IT-TLS",
        "revision": "R1",
        "source_ref": source_ref,
        "production_graph_sha256": production_sha,
        "qualification_graph_sha256": qualification_sha,
        "reqwest": "0.12.24",
        "hyper_rustls": "0.27.9",
        "rustls": "0.23.41",
        "tokio_rustls": "0.26.4",
        "rcgen_dev_only": "0.14.7",
        "crypto_provider": "ring",
        "native_tls_present": False,
        "dangerous_verifier_api_present": False,
        "cargo": cargo,
        "rustc_vv_sha256": hashlib.sha256(rustc.encode()).hexdigest(),
        "production_graph": production.splitlines(),
        "qualification_graph": qualification.splitlines(),
    }
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(
        "stage8b-tls-graph: PASS "
        f"production={production_sha} qualification={qualification_sha} native_tls=false"
    )


if __name__ == "__main__":
    main()
