#!/usr/bin/env python3
"""Create the immutable Stage 8B-P1-e I1 aggregate review handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1e_i1_aggregate_acceptance_check as aggregate
import stage8b_p1e_i1_aggregate_acceptance_handoff_safety_check as safety
import stage8b_p1e_i1_aggregate_readiness_handoff_safety_check as readiness_safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
REPORTS = ROOT / "reports/stage8b-p1e-i1-fixed-install"
DOWNLOADS = Path("/Users/denisq/Downloads")
REVIEW_SOURCES = {
    **{
        archive_name: DOWNLOADS / Path(archive_name).name
        for archive_name in readiness_safety.REVIEWS
    },
    "handoff-evidence/reviews/FINAM_I1_TELEMETRY_SOURCE_ACCEPT_b6f6d5b_2026-09-24.md":
        DOWNLOADS / "FINAM_I1_TELEMETRY_SOURCE_ACCEPT_b6f6d5b_2026-09-24.md",
    "handoff-evidence/reviews/FINAM_I1_FIXED_INSTALL_SOURCE_ACCEPT_7f2e876_2026-09-24.md":
        DOWNLOADS / "FINAM_I1_FIXED_INSTALL_SOURCE_ACCEPT_7f2e876_2026-09-24.md",
}


def git(*args: str) -> bytes:
    return subprocess.check_output(("git", *args), cwd=ROOT)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def main() -> None:
    status = git("status", "--porcelain", "--untracked-files=all").decode().strip()
    if status:
        raise SystemExit(f"stage8b-p1e-i1-aggregate-acceptance-handoff: FAIL dirty worktree\n{status}")
    branch = git("branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-p1e-i1-aggregate-acceptance-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != safety.PARENT:
        raise SystemExit("stage8b-p1e-i1-aggregate-acceptance-handoff: FAIL parent drift")
    changed_paths = set(git("diff", "--name-only", safety.PARENT, source_ref, "--").decode().splitlines())
    if changed_paths != aggregate.ALLOWED_CHANGES:
        raise SystemExit("stage8b-p1e-i1-aggregate-acceptance-handoff: FAIL changed paths")

    reviews: dict[str, bytes] = {}
    for archive_name, local_path in REVIEW_SOURCES.items():
        raw = local_path.read_bytes()
        if sha256(raw) != safety.REVIEWS[archive_name]:
            raise SystemExit(f"stage8b-p1e-i1-aggregate-acceptance-handoff: FAIL review digest {local_path.name}")
        reviews[archive_name] = raw
    report_files = {path.name for path in REPORTS.iterdir() if path.is_file()}
    if report_files != safety.REPORT_FILES:
        raise SystemExit("stage8b-p1e-i1-aggregate-acceptance-handoff: FAIL target evidence inventory")

    invocation = ["bash", "scripts/stage8b_p1e_i1_aggregate_acceptance_gate.sh"]
    process = subprocess.Popen(
        invocation,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if process.stdout is None:
        raise SystemExit("stage8b-p1e-i1-aggregate-acceptance-handoff: FAIL gate stdout unavailable")
    output_chunks: list[bytes] = []
    for output_line in iter(process.stdout.readline, b""):
        output_chunks.append(output_line)
        print(output_line.decode(errors="replace"), end="", flush=True)
    returncode = process.wait()
    gate_stdout = b"".join(output_chunks)
    gate_log = (
        f"source_ref={source_ref}\nsource_tree={source_tree}\ninvocation={' '.join(invocation)}\n--- stdout-stderr ---\n".encode()
        + gate_stdout
        + f"\n--- result ---\nexit_code={returncode}\n".encode()
    )
    if returncode != 0:
        raise SystemExit(gate_log.decode(errors="replace"))
    if git("rev-parse", "HEAD").decode().strip() != source_ref or git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1e-i1-aggregate-acceptance-handoff: FAIL source changed during gate")

    short = source_ref[:7]
    archive_name = f"moex-trading-project-{short}-stage8b-p1e-i1-aggregate-acceptance.zip"
    archive_path = OUTPUT / archive_name
    manifest_raw, entries = common.source_manifest(source_ref)
    report_hashes = {name: sha256((REPORTS / name).read_bytes()) for name in sorted(safety.REPORT_FILES)}
    closed_surfaces = {
        "operational_redis_db0": False,
        "operational_redis_db15": False,
        "vps_installation_or_service_start": False,
        "paper_provider_operational_activation": False,
        "finam_post_delete_send": False,
        "broker_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
    }
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": "REVIEW_CANDIDATE_I1_NOT_CLOSED",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "manifest_sha256": sha256(manifest_raw),
        "changed_paths": sorted(changed_paths),
        "production_source_changed": False,
        "i1_closed": False,
        "p1f_authorized": False,
        "closed_surfaces": closed_surfaces,
        "review_sha256": safety.REVIEWS,
        "target_evidence_sha256": report_hashes,
        "gate_log_sha256": sha256(gate_log),
        "next_after_acceptance": "separate P1-f isolated operational acceptance design",
    }
    evidence_raw = (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode()
    marker = (
        f"stage={safety.STAGE}\nsource_short_ref={short}\nsource_ref={source_ref}\n"
        f"source_parent={source_parent}\nsource_tree={source_tree}\nbranch={branch}\narchive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest_raw,
        safety.COMMIT_RAW: git("cat-file", "commit", source_ref),
        safety.EVIDENCE: evidence_raw,
        safety.GATE_LOG: gate_log,
        **reviews,
        **{safety.REPORT_PREFIX + name: (REPORTS / name).read_bytes() for name in safety.REPORT_FILES},
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(common.zip_info(entry["path"], entry["mode"]), git("show", f"{source_ref}:{entry['path']}"))
        for name, raw in sorted(additions.items()):
            archive.writestr(common.zip_info(name), raw)

    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\nstage8b-p1e-i1-aggregate-acceptance-handoff: PASS")


if __name__ == "__main__":
    main()
