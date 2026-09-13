#!/usr/bin/env python3
"""Build the immutable Stage 8B-P1-e I1 transaction V5 source handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1e_i1_transaction_v5_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-paper-shadow-resumption"
ACCEPTED_PREDECESSOR = "21eaf01916f2da5eaacb191b4d7339a8101070ad"
EXPECTED_CHANGED = {
    "crates/runtime-durable-service/src/lib.rs",
    "crates/runtime-durable-service/src/recovery.rs",
    "crates/runtime-durable-service/src/stage8b_p1_bootstrap.rs",
    "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs",
    "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5g_clean_restart.rs",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    "docs/current-status.md",
    "docs/stage-8/stage8b-p1e-i1-transaction-v5-implementation.md",
    "scripts/make_stage8b_p1e_i1_transaction_v5_handoff.py",
    "scripts/stage8b_p1e_i1_transaction_v5_check.py",
    "scripts/stage8b_p1e_i1_transaction_v5_gate.sh",
    "scripts/stage8b_p1e_i1_transaction_v5_handoff_safety_check.py",
    "scripts/stage8b_p1e_i1_transaction_v5_negative_harness.py",
}


def git(*args: str) -> bytes:
    return subprocess.check_output(["git", *args], cwd=ROOT)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def main() -> None:
    if git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1e-i1-transaction-v5-handoff: FAIL dirty worktree")
    branch = git("branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p1e-i1-transaction-v5-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != ACCEPTED_PREDECESSOR:
        raise SystemExit(
            "stage8b-p1e-i1-transaction-v5-handoff: FAIL "
            f"parent={source_parent} expected={ACCEPTED_PREDECESSOR}"
        )
    changed = set(
        filter(
            None,
            git("diff", "--name-only", ACCEPTED_PREDECESSOR, source_ref, "--")
            .decode()
            .splitlines(),
        )
    )
    if changed != EXPECTED_CHANGED:
        raise SystemExit(
            "stage8b-p1e-i1-transaction-v5-handoff: FAIL changed paths "
            f"missing={sorted(EXPECTED_CHANGED - changed)} unexpected={sorted(changed - EXPECTED_CHANGED)}"
        )

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p1e_i1_transaction_v5_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    gate_marker = b"PASS stage8b-p1e-i1-transaction-v5-gate"
    if gate.returncode != 0 or gate_marker not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    short_ref = source_ref[:7]
    archive_name = (
        f"moex-trading-project-{short_ref}-stage8b-p1e-i1-transaction-v5-source-review-package.zip"
    )
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    manifest_document = json.loads(manifest)
    manifest_document.update(
        {
            "schema_version": 3,
            "stage": "Stage 8B-P1-e I1 transaction V5 source",
            "source_tree": source_tree,
            "source_branch": branch,
        }
    )
    manifest = (json.dumps(manifest_document, indent=2, sort_keys=True) + "\n").encode()
    evidence = {
        "schema_version": 1,
        "stage": "Stage 8B-P1-e I1 transaction V5 source",
        "status": "SOURCE_REVIEW_CANDIDATE",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "worktree_clean": True,
        "pushed_to_origin": False,
        "transaction_contract_version": 5,
        "receipt_schema_version": 2,
        "marker_schema_version": 4,
        "classifications": 15,
        "crash_hooks": 10,
        "post_seal_recovery_actions": 4,
        "negative_cases": 18,
        "changed_paths": sorted(changed),
        "deferred": [
            "pre-seal administrative recovery commands",
            "deployable owner loop",
            "process signal/panic/restart matrix",
        ],
        "closed_surfaces": {
            "operational_redis_db0_db15": False,
            "vps_activation": False,
            "operational_credentials": False,
            "finam_post_delete": False,
            "broker_dispatch": False,
            "runtime_live": False,
            "real_orders": False,
        },
        "gate_sha256": sha256(gate.stdout),
        "manifest_sha256": sha256(manifest),
    }
    evidence_bytes = (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode()
    marker = (
        "stage=Stage 8B-P1-e I1 transaction V5 source\n"
        f"source_short_ref={short_ref}\n"
        f"source_ref={source_ref}\n"
        f"source_parent={source_parent}\n"
        f"source_tree={source_tree}\n"
        f"branch={branch}\n"
        f"accepted_predecessor={ACCEPTED_PREDECESSOR}\n"
        f"archive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest,
        safety.COMMIT_RAW: git("cat-file", "commit", source_ref),
        safety.EVIDENCE: evidence_bytes,
        safety.GATE: gate.stdout,
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(
        archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for entry in entries:
            archive.writestr(
                common.zip_info(entry["path"], entry["mode"]),
                git("show", f"{source_ref}:{entry['path']}"),
            )
        for name, raw in sorted(additions.items()):
            archive.writestr(common.zip_info(name), raw)

    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    sha_path = archive_path.with_suffix(".zip.sha256")
    safety_path = archive_path.with_suffix(".zip.safety.json")
    sha_path.write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    safety_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(
        f"archive={archive_path}\nsha256={digest}\nsafety={safety_path}\n"
        f"source_ref={source_ref}\nsource_tree={source_tree}\n"
        "stage8b-p1e-i1-transaction-v5-handoff: PASS"
    )


if __name__ == "__main__":
    main()
