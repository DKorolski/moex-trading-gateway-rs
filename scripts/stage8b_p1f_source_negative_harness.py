#!/usr/bin/env python3
"""Mutation harness for the Stage 8B-P1-f I source checker."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path
from typing import Callable

import stage8b_p1f_source_check as check


ROOT = Path(__file__).resolve().parents[1]
REQUIRED_PATHS = {
    check.SOURCE,
    check.LIB,
    check.CARGO,
    check.FIRST_BOOT,
    check.DOCUMENT,
    check.INVENTORY,
    check.MATRIX,
    check.STATUS,
    check.ROADMAP,
    check.MULTI_UID,
    check.REDIS_SOURCE,
    check.STALE_DELETE_LUA,
    *check.DESIGN_HASHES.keys(),
}


def replace(root: Path, path: str, old: str, new: str) -> None:
    target = root / path
    text = target.read_text()
    if old not in text:
        raise RuntimeError(f"mutation source missing: {path}: {old}")
    target.write_text(text.replace(old, new, 1))


def mutate_json(root: Path, callback: Callable[[dict], None]) -> None:
    path = root / check.INVENTORY
    value = json.loads(path.read_text())
    callback(value)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def copy_inputs(root: Path) -> None:
    for relative in REQUIRED_PATHS:
        source = ROOT / relative
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def cases() -> list[tuple[str, str, Callable[[Path], None]]]:
    return [
        ("control-root", "source", lambda root: replace(root, check.SOURCE, "/var/lib/moex-finam-p1-paper-control", "/tmp/p1f-control")),
        ("production-root-check", "source", lambda root: replace(root, check.SOURCE, "if unsafe { libc::geteuid() } != 0", "if false")),
        ("transition-uid-check", "source", lambda root: replace(root, check.SOURCE, "if unsafe { libc::geteuid() } != self.expected_uid", "if false")),
        ("nofollow", "source", lambda root: replace(root, check.SOURCE, "libc::O_NOFOLLOW | libc::O_CLOEXEC", "libc::O_CLOEXEC")),
        ("explicit-file-custody", "source", lambda root: replace(root, check.SOURCE, "libc::fchown", "libc::fchmod")),
        ("exclusive-lock", "source", lambda root: replace(root, check.SOURCE, "libc::LOCK_EX | libc::LOCK_NB", "libc::LOCK_EX")),
        ("cloneable-permit", "source", lambda root: replace(root, check.SOURCE, "#[derive(Debug)]\npub struct Stage8bP1fRunPermitV1", "#[derive(Debug, Clone)]\npub struct Stage8bP1fRunPermitV1")),
        ("phase-domain", "source", lambda root: replace(root, check.SOURCE, "SIGNED_PHASE_DOMAIN", "UNSIGNED_PHASE_DOMAIN")),
        ("claim-transaction", "source", lambda root: replace(root, check.SOURCE, "PENDING_CLAIM_FILE", "REMOVED_CLAIM_FILE")),
        ("materialization-transaction", "source", lambda root: replace(root, check.SOURCE, "PENDING_MATERIALIZATION_FILE", "REMOVED_MATERIALIZATION_FILE")),
        ("terminal-transaction", "source", lambda root: replace(root, check.SOURCE, "PENDING_TERMINAL_FILE", "REMOVED_TERMINAL_FILE")),
        ("retained-manifest-binding", "source", lambda root: replace(root, check.SOURCE, "sha256_hex(&retained_manifest_bytes) != event.manifest_sha256", "false")),
        ("freshness-301", "source", lambda root: replace(root, check.SOURCE, "!(0..=300).contains(&age)", "!(0..=301).contains(&age)")),
        ("force-kill-grace", "source", lambda root: replace(root, check.SOURCE, "force_kill_at = stopping_started_at + chrono::Duration::seconds(30)", "force_kill_at = stopping_started_at + chrono::Duration::seconds(60)")),
        ("redis-client", "source", lambda root: (root / check.SOURCE).write_text((root / check.SOURCE).read_text() + "\n// redis::Client\n")),
        ("multi-uid-unlink", "evidence", lambda root: replace(root, check.MULTI_UID, "unlink-authority", "unlink-removed")),
        ("opened-surface", "inventory", lambda root: mutate_json(root, lambda value: value["closed_surfaces"].__setitem__("runtime_live", True))),
        ("design-freeze", "design", lambda root: replace(root, check.r4.INVENTORY, '"mutating_roles_database": 15', '"mutating_roles_database": 0')),
        ("matrix-removal", "matrix", lambda root: replace(root, check.MATRIX, "P1FI-030,closed,P1F-Ib through Ie O0 through O4 P1F-A Redis FINAM runtime-live and real orders remain closed,REQUIRED\n", "")),
    ]


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage8b-p1f-source-positive-") as directory:
        baseline = Path(directory)
        copy_inputs(baseline)
        missing = [path for path in REQUIRED_PATHS if not (baseline / path).is_file()]
        if missing:
            print(f"FAIL infrastructure missing-inputs={missing}")
            return 1
        try:
            check.validate(baseline, verify_lineage=False)
        except Exception as error:
            print(f"FAIL positive-control category=infrastructure error={error}")
            return 1
        source = baseline / check.SOURCE
        source.write_text(source.read_text() + "\n// nonsemantic control accepted\n")
        try:
            check.validate(baseline, verify_lineage=False)
        except Exception as error:
            print(f"FAIL nonsemantic-control category=false-positive error={error}")
            return 1
        print("PASS positive-control")
        print("PASS nonsemantic-control")

    passed = 0
    for name, category, mutate in cases():
        with tempfile.TemporaryDirectory(prefix="stage8b-p1f-source-negative-") as directory:
            root = Path(directory)
            copy_inputs(root)
            before = {path: (root / path).read_bytes() for path in REQUIRED_PATHS}
            mutate(root)
            after = {path: (root / path).read_bytes() for path in REQUIRED_PATHS}
            if before == after:
                print(f"FAIL {name}: mutation was a no-op")
                return 1
            try:
                check.validate(root, verify_lineage=False)
            except (check.CheckFailure, check.r4.CheckFailure) as error:
                print(f"PASS {name} category={category} rejection={error}")
                passed += 1
                continue
            except (OSError, UnicodeDecodeError) as error:
                print(f"FAIL {name}: infrastructure failure was not a semantic rejection: {error}")
                return 1
            print(f"FAIL {name}: checker accepted mutation")
            return 1
    print(f"PASS stage8b-p1f-source-negative-harness {passed}/{len(cases())}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
