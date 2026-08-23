#!/usr/bin/env python3
"""Create an immutable exact-commit Stage 8B TLS qualification handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import tempfile
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_tls_handoff_safety_check as safety

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
GATE_LOG = ROOT / "reports/stage8b-tls-gate.log"
GRAPH_REPORT = ROOT / "reports/stage8b-tls-qualification-graph.json"
BRANCH = "stage8b-tls-qualification"
BASE = "14e01a9f838080e196ece5945a7796f2bd2600bc"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-tls-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-tls-handoff: FAIL branch={branch}")
    full_ref = run("git", "rev-parse", "HEAD").decode().strip()
    if full_ref != run("git", "rev-parse", "@{upstream}").decode().strip():
        raise SystemExit("stage8b-tls-handoff: FAIL exact commit not pushed upstream")
    if run("git", "merge-base", full_ref, BASE).decode().strip() != BASE:
        raise SystemExit("stage8b-tls-handoff: FAIL predecessor drift")
    short_ref = run("git", "rev-parse", "--short=7", "HEAD").decode().strip()
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    OUTPUT.mkdir(parents=True, exist_ok=True)

    if not GATE_LOG.is_file() or not GRAPH_REPORT.is_file():
        raise SystemExit("stage8b-tls-handoff: FAIL exact-commit evidence missing")
    gate = GATE_LOG.read_bytes()
    graph = GRAPH_REPORT.read_bytes()
    if b"stage8b-tls-gate: PASS revision=R1 rows=50 negatives=40" not in gate:
        raise SystemExit("stage8b-tls-handoff: FAIL stale or incomplete gate log")
    if f"current-tree-ci-gate: PASS source_ref={full_ref} ".encode() not in gate:
        raise SystemExit("stage8b-tls-handoff: FAIL gate is not exact-commit bound")
    graph_value = json.loads(graph)
    if graph_value.get("source_ref") != full_ref:
        raise SystemExit("stage8b-tls-handoff: FAIL graph evidence is not exact-commit bound")

    manifest, entries = common.source_manifest(full_ref)
    evidence = (
        json.dumps(
            {
                "schema_version": 1,
                "stage": "8B-IT-TLS",
                "revision": "R1",
                "source_ref": full_ref,
                "source_short_ref": short_ref,
                "archive_name": archive_name,
                "branch": branch,
                "accepted_predecessor_ref": BASE,
                "accepted_it_r3_replayed": True,
                "acceptance_rows": 50,
                "negative_mutations": 40,
                "focused_tls_tests": 5,
                "controlled_tls_qualified": True,
                "valid_ca_hostname": True,
                "http2_alpn_negotiated": True,
                "wrong_ca_rejected": True,
                "wrong_hostname_rejected": True,
                "expired_rejected": True,
                "not_yet_valid_rejected": True,
                "timeout_classified": True,
                "response_loss_classified": True,
                "automatic_retry_disabled": True,
                "native_tls_absent": True,
                "canonical_full_regression": True,
                "gate_sha256": sha256(gate),
                "graph_evidence_sha256": sha256(graph),
                "manifest_sha256": sha256(manifest),
                "production_endpoint_authority": False,
                "finam_credentials": False,
                "real_finam_effect": False,
                "redis_execution": False,
                "broker_dispatch": False,
                "runtime_live": False,
                "stage8b_p": False,
                "stage8b_xe": False,
                "stage12": False,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    ).encode()
    additions = {
        "handoff-commit.txt": (
            f"source_short_ref={short_ref}\nsource_ref={full_ref}\narchive_name={archive_name}\n"
        ).encode(),
        safety.GATE: gate,
        safety.GRAPH: graph,
        safety.EVIDENCE: evidence,
        safety.MANIFEST: manifest,
    }
    with tempfile.TemporaryDirectory(prefix="stage8b-tls-handoff-"):
        with zipfile.ZipFile(
            archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
        ) as archive:
            for entry in entries:
                archive.writestr(
                    common.zip_info(entry["path"], entry["mode"]),
                    run("git", "show", f"{full_ref}:{entry['path']}"),
                )
            for name, data in sorted(additions.items()):
                archive.writestr(common.zip_info(name), data)
    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(
        f"{digest}  {archive_name}\n", encoding="utf-8"
    )
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={full_ref}\n"
        "stage8b-tls-handoff: PASS"
    )


if __name__ == "__main__":
    main()
