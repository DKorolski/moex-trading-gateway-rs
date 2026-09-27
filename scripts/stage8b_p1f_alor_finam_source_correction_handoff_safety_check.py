#!/usr/bin/env python3
"""Validate an immutable ALOR→FINAM source-correction handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

import stage8b_p1e_i1a_handoff_safety_check as common


STAGE = "Stage 8B-P1-f ALOR-FINAM source correction"
BRANCH = "stage8b-paper-shadow-resumption"
BASELINE = "1090de48cd7f216ce7868cfc5c141208579d7d33"
REVIEW_SHA256 = "8a3096d8b0640c22a902a07937233066702dba9db59f0798a1c8d9e48fa9e9f3"
PREFIX = "handoff-evidence/"
MARKER = "handoff-commit.txt"
MANIFEST = PREFIX + "source-tree-manifest.json"
COMMIT_RAW = PREFIX + "source-commit.raw"
EVIDENCE = PREFIX + "stage8b-p1f-alor-finam-source-correction-handoff.json"
GATE = PREFIX + "stage8b-p1f-alor-finam-source-correction-gate.txt"
MULTI_UID = PREFIX + "stage8b-p1f-alor-finam-linux-multi-uid-custody.txt"
REVIEW = PREFIX + "reviews/FINAM_be20447_SOURCE_AND_O2_REVIEW_HOLD_2026-09-27.md"
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, GATE, MULTI_UID, REVIEW}
REQUIRED_TRACKED = {
    "docs/stage-8/stage8b-p1f-alor-finam-freeze-intake-2026-09-27.json",
    "docs/stage-8/stage8b-p1f-alor-finam-source-correction-2026-09-27.md",
    "docs/stage-8/stage8b-p1f-alor-finam-source-correction-evidence-2026-09-27.json",
    "docs/stage-8/stage8b-p1f-alor-finam-parity-guide-2026-09-27.md",
    "fixtures/stage8b-p1f-parity/imoexf_raw_10m_msk_utc.csv",
    "fixtures/stage8b-p1f-parity/baseline07_python_reference_trades.csv",
    "fixtures/stage8b-p1f-parity/candidate09_python_reference_trades.csv",
    "scripts/stage8b_p1f_alor_finam_compare_rounds.py",
    "scripts/stage8b_p1f_alor_finam_compare_rounds_test.py",
    "scripts/stage8b_p1f_alor_finam_source_correction_check.py",
    "scripts/stage8b_p1f_alor_finam_source_correction_gate.sh",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/strategy-runtime-core/src/stage5g_clean_restart.rs",
    "crates/strategy-runtime-core/src/stage8b_p1e_first_boot.rs",
    "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs",
    "crates/runtime-durable-service/src/stage8b_p1f_guardian.rs",
    "crates/runtime-durable-service/src/stage8b_p1f_o2_systemd.rs",
    "deploy/stage8b-p1e/moex-finam-p1-paper-o2-bootstrap-runner.service",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def validate_member_name(name: str) -> None:
    path = PurePosixPath(name)
    require(
        bool(name)
        and not name.startswith("/")
        and "\\" not in name
        and all(part not in {"", ".", ".."} for part in path.parts),
        f"unsafe archive member: {name!r}",
    )
    require(
        not any(
            part in {".git", "target", "tmp", "__pycache__", "__MACOSX"}
            for part in path.parts
        ),
        f"forbidden archive path: {name}",
    )
    require(
        not (len(path.parts) >= 2 and path.parts[:2] == ("reports", "handoff")),
        f"nested handoff path: {name}",
    )
    basename = path.name
    require(
        basename != ".env"
        and not (basename.startswith(".env.") and basename != ".env.example"),
        f"secret-bearing env member: {name}",
    )
    require(
        not basename.endswith(
            (".pem", ".key", ".ed25519", ".sqlite", ".sqlite3", ".rdb")
        ),
        f"secret/runtime artifact: {name}",
    )


def parse_marker(raw: bytes) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in raw.decode().splitlines():
        require(bool(line) and "=" in line, "invalid marker line")
        key, value = line.split("=", 1)
        require(bool(key) and bool(value) and key not in result, "invalid marker item")
        result[key] = value
    require(
        set(result)
        == {
            "stage",
            "source_short_ref",
            "source_ref",
            "source_parent",
            "source_tree",
            "branch",
            "baseline_ref",
            "archive_name",
        },
        "marker inventory drift",
    )
    return result


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        by_name = {item.filename: item for item in infos}
        require(len(names) == len(set(names)), "duplicate archive members")
        require(not (GENERATED - set(names)), "missing generated evidence")
        require(not (REQUIRED_TRACKED - set(names)), "missing source-correction members")
        for item in infos:
            validate_member_name(item.filename)
            mode = (item.external_attr >> 16) & 0o177777
            require(mode & 0o170000 != 0o120000, f"symlink member: {item.filename}")
            require(
                mode & 0o170000 in {0, 0o100000, 0o040000},
                f"special member: {item.filename}",
            )

        files = {name: archive.read(name) for name in names}
        marker = parse_marker(files[MARKER])
        require(marker["stage"] == STAGE, "stage mismatch")
        require(marker["branch"] == BRANCH, "branch mismatch")
        require(marker["baseline_ref"] == BASELINE, "baseline mismatch")
        require(marker["archive_name"] == PurePosixPath(path).name, "archive-name mismatch")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit object mismatch")
        commit_lines = commit_raw.decode().splitlines()
        require(commit_lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(
            f"parent {marker['source_parent']}" in commit_lines,
            "commit parent mismatch",
        )

        manifest = json.loads(files[MANIFEST])
        require(manifest["schema_version"] == 2, "manifest schema mismatch")
        require(manifest["source_ref"] == marker["source_ref"], "manifest source mismatch")
        entries = manifest["entries"]
        require(manifest["entry_count"] == len(entries), "manifest count mismatch")
        tracked: set[str] = set()
        payloads: dict[str, bytes] = {}
        for entry in entries:
            name = entry["path"]
            require(name not in tracked and name in files, f"manifest member mismatch: {name}")
            raw = files[name]
            mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
            require(len(raw) == entry["size"], f"manifest size mismatch: {name}")
            require(sha256(raw) == entry["sha256"], f"manifest digest mismatch: {name}")
            require(mode == entry["mode"], f"manifest mode mismatch: {name}")
            tracked.add(name)
            payloads[name] = raw
        require(set(names) - tracked == GENERATED, "generated member inventory mismatch")
        require(common.build_tree_oid(entries, payloads) == marker["source_tree"], "tree reconstruction mismatch")

        evidence = json.loads(files[EVIDENCE])
        require(evidence["source_ref"] == marker["source_ref"], "evidence source mismatch")
        require(evidence["source_parent"] == marker["source_parent"], "evidence parent mismatch")
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree mismatch")
        require(evidence["baseline_ref"] == BASELINE, "evidence baseline mismatch")
        require(evidence["migration_target"] == "baseline07_bo_only", "target mismatch")
        require(evidence["parity_rounds"] == {"actual": 38, "expected": 38}, "parity count mismatch")
        require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        require(evidence["gate_sha256"] == sha256(files[GATE]), "gate digest mismatch")
        require(evidence["multi_uid_sha256"] == sha256(files[MULTI_UID]), "multi-UID digest mismatch")
        require(sha256(files[REVIEW]) == REVIEW_SHA256, "review digest mismatch")
        require(b"PASS stage8b-p1f-multi-uid root-transition-and-custody" in files[MULTI_UID], "multi-UID custody evidence missing")

        gate = files[GATE]
        for expected in (
            b"PASS stage8b-p1f-alor-finam-source-correction",
            b"Ran 6 tests",
            b"frozen_baseline07_replay_matches_all_38_alor_rounds ... ok",
            b"disabled_live_mr_never_claims_owner_and_later_breakout_remains_eligible ... ok",
            b"same_day_eod_blocks_new_entries_but_preserves_exit ... ok",
            b"canonical_m10_keeps_close_bound_identity_but_uses_start_model_label ... ok",
            b"exact_runtime_profile_builds_real_fingerprint ... ok",
            b"first_boot_start_labels_survive_restart_and_admit_adjacent_canonical_m10 ... ok",
            b"controlled_stop_proof_adapter_covers_proof_kill_then_proof_and_timeout ... ok",
            b"custody_policy_has_no_service_uid_write_bit ... ok",
            b"PASS stage8b-p1f-alor-finam-source-correction-gate",
        ):
            require(expected in gate, f"gate marker missing: {expected!r}")

        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": marker["source_ref"],
            "source_tree": marker["source_tree"],
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1f_alor_finam_source_correction_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (
        OSError,
        UnicodeDecodeError,
        ValueError,
        KeyError,
        TypeError,
        zipfile.BadZipFile,
        json.JSONDecodeError,
    ) as error:
        print(f"stage8b-p1f-alor-finam-source-correction-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print(
        "PASS stage8b-p1f-alor-finam-source-correction-handoff-safety "
        + json.dumps(result, sort_keys=True)
    )


if __name__ == "__main__":
    main()
