#!/usr/bin/env python3
"""Create the immutable I1 aggregate-readiness review handoff."""

from __future__ import annotations

import json
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import make_stage8b_p1e_i1_owner_loop_handoff as runner
import stage8b_p1e_i1_aggregate_readiness_check as aggregate_check
import stage8b_p1e_i1_aggregate_readiness_handoff_safety_check as safety


ROOT = runner.ROOT
OUTPUT = runner.OUTPUT
DOWNLOADS = Path("/Users/denisq/Downloads")
REVIEW_SOURCES = {
    "handoff-evidence/reviews/FINAM_P1e_I0_R2_REVIEW_afda87a_2026-09-11.md": DOWNLOADS / "FINAM_P1e_I0_R2_REVIEW_afda87a_2026-09-11.md",
    "handoff-evidence/reviews/FINAM_P1e_FOUNDATION_R2_I1A_R1_REVIEW_130e9a4_2026-09-12.md": DOWNLOADS / "FINAM_P1e_FOUNDATION_R2_I1A_R1_REVIEW_130e9a4_2026-09-12.md",
    "handoff-evidence/reviews/FINAM_P1e_I1A_R2_REVIEW_aa24e84_2026-09-12.md": DOWNLOADS / "FINAM_P1e_I1A_R2_REVIEW_aa24e84_2026-09-12.md",
    "handoff-evidence/reviews/FINAM_P1e_I1A_SOURCE_CORRECTION_REVIEW_8360c47_2026-09-13.md": DOWNLOADS / "FINAM_P1e_I1A_SOURCE_CORRECTION_REVIEW_8360c47_2026-09-13.md",
    "handoff-evidence/reviews/FINAM_I1_FIRST_BOOT_CORRECTION_REVIEW_4d7ee64_2026-09-13.md": DOWNLOADS / "FINAM_I1_FIRST_BOOT_CORRECTION_REVIEW_4d7ee64_2026-09-13.md",
    "handoff-evidence/reviews/FINAM_I1_GOVERNANCE_CORRECTION_REVIEW_21eaf01_2026-09-13.md": DOWNLOADS / "FINAM_I1_GOVERNANCE_CORRECTION_REVIEW_21eaf01_2026-09-13.md",
    "handoff-evidence/reviews/FINAM_I1_TRANSACTION_V5_CORRECTION_REVIEW_5e2e157_2026-09-14.md": DOWNLOADS / "FINAM_I1_TRANSACTION_V5_CORRECTION_REVIEW_5e2e157_2026-09-14.md",
    "handoff-evidence/reviews/FINAM_I1_PRE_SEAL_RECOVERY_CORRECTION_REVIEW_a655da9_2026-09-14.md": DOWNLOADS / "FINAM_I1_PRE_SEAL_RECOVERY_CORRECTION_REVIEW_a655da9_2026-09-14.md",
    "handoff-evidence/reviews/FINAM_I1_GENERATED_MARKET_CORRECTION_REVIEW_ff6639e_2026-09-17.md": DOWNLOADS / "FINAM_I1_GENERATED_MARKET_CORRECTION_REVIEW_ff6639e_2026-09-17.md",
    "handoff-evidence/reviews/FINAM_I1_CANCEL_COMPOSITION_REVIEW_cc1f02c_2026-09-17.md": DOWNLOADS / "FINAM_I1_CANCEL_COMPOSITION_REVIEW_cc1f02c_2026-09-17.md",
    "handoff-evidence/reviews/FINAM_I1_DAY_EXPIRY_CORRECTION_REVIEW_a667938_2026-09-18.md": DOWNLOADS / "FINAM_I1_DAY_EXPIRY_CORRECTION_REVIEW_a667938_2026-09-18.md",
    "handoff-evidence/reviews/FINAM_I1_COMMITTED_CANCEL_DAY_EXPIRY_REVIEW_efe56a9_2026-09-18.md": DOWNLOADS / "FINAM_I1_COMMITTED_CANCEL_DAY_EXPIRY_REVIEW_efe56a9_2026-09-18.md",
    "handoff-evidence/reviews/FINAM_I1_OWNER_LOOP_CORRECTION_REVIEW_e2ce442_2026-09-18.md": DOWNLOADS / "FINAM_I1_OWNER_LOOP_CORRECTION_REVIEW_e2ce442_2026-09-18.md",
    "handoff-evidence/reviews/FINAM_I1_PROCESS_ACCEPTANCE_1086b8d_2026-09-23.md": DOWNLOADS / "FINAM_I1_PROCESS_ACCEPTANCE_1086b8d_2026-09-23.md",
}


def main() -> None:
    status = runner.git("status", "--porcelain", "--untracked-files=all").decode().strip()
    if status:
        raise SystemExit(f"stage8b-p1e-i1-aggregate-handoff: FAIL dirty worktree\n{status}")
    branch = runner.git("branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-p1e-i1-aggregate-handoff: FAIL branch={branch}")
    source_ref = runner.git("rev-parse", "HEAD").decode().strip()
    source_short_ref = source_ref[:7]
    source_parent = runner.git("rev-parse", "HEAD^").decode().strip()
    source_tree = runner.git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != safety.PARENT:
        raise SystemExit(f"stage8b-p1e-i1-aggregate-handoff: FAIL parent={source_parent} expected={safety.PARENT}")
    changed_paths = runner.git("diff", "--name-only", safety.PARENT, source_ref, "--").decode().splitlines()
    if set(changed_paths) != aggregate_check.ALLOWED_CHANGES:
        raise SystemExit("stage8b-p1e-i1-aggregate-handoff: FAIL changed-path inventory")

    reviews: dict[str, bytes] = {}
    for archive_name, local_path in REVIEW_SOURCES.items():
        raw = local_path.read_bytes()
        if runner.sha256(raw) != safety.REVIEWS[archive_name]:
            raise SystemExit(f"stage8b-p1e-i1-aggregate-handoff: FAIL review digest {local_path.name}")
        reviews[archive_name] = raw

    runner.safety = safety
    gate_record, gate_log = runner.run_check(
        "aggregate_readiness_gate",
        ["bash", "scripts/stage8b_p1e_i1_aggregate_readiness_gate.sh"],
        source_ref,
        source_tree,
        None,
    )
    require_clean = runner.git("status", "--porcelain", "--untracked-files=all").decode().strip()
    if runner.git("rev-parse", "HEAD").decode().strip() != source_ref or require_clean:
        raise SystemExit("stage8b-p1e-i1-aggregate-handoff: FAIL source changed during verification")

    archive_name = f"moex-trading-project-{source_short_ref}-stage8b-p1e-i1-aggregate-readiness.zip"
    archive_path = OUTPUT / archive_name
    manifest_raw, entries = common.source_manifest(source_ref)
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": "REVIEW_CANDIDATE_I1_NOT_CLOSED",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "manifest_sha256": runner.sha256(manifest_raw),
        "inventory_sha256": runner.sha256((ROOT / aggregate_check.INVENTORY).read_bytes()),
        "changed_paths": changed_paths,
        "production_source_changed": False,
        "i1_closed": False,
        "open_slices": [
            "telemetry composition",
            "fixed-path installation and systemd material",
            "aggregate I1 acceptance",
        ],
        "closed_surfaces": {
            "operational_redis_db0_db15": False,
            "vps_installation_or_service_start": False,
            "paper_provider_operational_activation": False,
            "finam_post_delete_send": False,
            "broker_dispatch": False,
            "runtime_live": False,
            "real_orders": False,
            "p1f_authorized": False,
        },
        "review_sha256": safety.REVIEWS,
        "gate": gate_record,
        "gate_log_sha256": runner.sha256(gate_log),
    }
    evidence_raw = (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode()
    marker = (
        f"stage={safety.STAGE}\nsource_short_ref={source_short_ref}\nsource_ref={source_ref}\n"
        f"source_parent={source_parent}\nsource_tree={source_tree}\n"
        f"branch={branch}\narchive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest_raw,
        safety.COMMIT_RAW: runner.git("cat-file", "commit", source_ref),
        safety.EVIDENCE: evidence_raw,
        safety.GATE_LOG: gate_log,
        **reviews,
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(
                common.zip_info(entry["path"], entry["mode"]),
                runner.git("show", f"{source_ref}:{entry['path']}"),
            )
        for name, raw in sorted(additions.items()):
            archive.writestr(common.zip_info(name), raw)

    result = safety.check(str(archive_path))
    digest = runner.sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\nstage8b-p1e-i1-aggregate-handoff: PASS")


if __name__ == "__main__":
    main()
