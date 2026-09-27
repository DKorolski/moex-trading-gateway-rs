#!/usr/bin/env python3
"""Apply the one accepted test-only chronology repair to detached Stage 7B replay."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path


STAGE8A5_REF = "bf58b47fdef8af774a4107455dfcc6204e594283"
STAGE7B_REF = "a1044e0dbe324c722b637498ca80ffafd9f0cbee"
NORMALIZED_REFS = tuple(
    sorted(
        (
            "10e357825a701193d964975bb5769bd0745d4986",
            "2b6371adb905654e0ddd8b6714159bcef737b577",
            "2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64",
            "8418cfb63ecee6702bf8a2873592b7cad1e711ee",
            "8d4c1f437c02cfb023aa75fb4a411b9394d2d293",
            STAGE7B_REF,
            STAGE8A5_REF,
            "e0bf9b7d9eb209e19b875f199511a493ddcd0da9",
            "e10d8fb0f9e095a849b1e56779a0597606d22111",
            "ec71791563a933889eb825f6f8f0846915ba6415",
        )
    )
)
REPAIR_SOURCE_REF = "e7ae487f9897be297bd9fabcee9ffad302e6dd3e"
SOURCE_PATH = Path("crates/strategy-runtime-core/src/stage5d_persistence.rs")
PRE_REPAIR_SHA256 = "90ab3f9253c0b96fee8ea2c2aeb5e0eb9b0e4c99a3dfb0c404648b588c205eb2"
POST_REPAIR_SHA256 = "a8caa83eacd4337c8562d24560c18cdd23d1f48f23115b5ec4c7b7561d38b6be"
DIFF_SHA256 = "4b1f165f785ff72336002d8bfa90bd8535f9e6087a8769347afd72cc2ea07a83"
OLD_EPOCH = 1_790_000_000
NEW_EPOCH = 4_102_444_800
OLD_BLOCK = b'''        envelope.persisted_at_ts_utc =
            DateTime::<Utc>::from_timestamp(1_790_000_000, 0).expect("operational persisted ts");
'''
NEW_BLOCK = b'''        // Source-owned operational fixtures may stamp the current riskgate
        // session. Keep the persisted boundary at the accepted chronology
        // ceiling so this restart matrix cannot expire with wall-clock time.
        envelope.persisted_at_ts_utc =
            DateTime::<Utc>::from_timestamp(4_102_444_800, 0).expect("operational persisted ts");
'''


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def run(root: Path, *args: str) -> str:
    return subprocess.check_output(args, cwd=root, text=True).strip()


def expected_evidence(normalized_refs: tuple[str, ...] = NORMALIZED_REFS) -> dict[str, object]:
    return {
        "schema_version": 1,
        "result": "PASS",
        "normalization_kind": "accepted-test-only-chronology-fixture-repair",
        "accepted_stage8a5_ref": STAGE8A5_REF,
        "detached_stage7b_ref": STAGE7B_REF,
        "normalized_source_refs": list(normalized_refs),
        "repair_source_ref": REPAIR_SOURCE_REF,
        "source_path": SOURCE_PATH.as_posix(),
        "pre_repair_sha256": PRE_REPAIR_SHA256,
        "post_repair_sha256": POST_REPAIR_SHA256,
        "diff_sha256": DIFF_SHA256,
        "replacement_count": 1,
        "old_persisted_at_epoch": OLD_EPOCH,
        "new_persisted_at_epoch": NEW_EPOCH,
        "test_cfg_proven": True,
        "current_tree_production_modified": False,
        "redis_opened": False,
        "finam_opened": False,
        "broker_dispatch_opened": False,
        "runtime_live_opened": False,
        "real_orders_opened": False,
    }


def write_atomic(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, raw_temp = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temp = Path(raw_temp)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temp, path)
    finally:
        if temp.exists():
            temp.unlink()


def verify_evidence(path: Path) -> None:
    actual = json.loads(path.read_text(encoding="utf-8"))
    if actual != expected_evidence():
        raise RuntimeError("temporal compatibility evidence drift")


def read_partial_evidence(path: Path) -> set[str]:
    existing = json.loads(path.read_text(encoding="utf-8"))
    raw_refs = existing.pop("normalized_source_refs", None)
    expected_without_refs = expected_evidence(())
    expected_without_refs.pop("normalized_source_refs")
    if existing != expected_without_refs:
        raise RuntimeError("existing temporal compatibility evidence drift")
    if not isinstance(raw_refs, list) or not all(isinstance(item, str) for item in raw_refs):
        raise RuntimeError("existing normalized ref inventory is invalid")
    observed_refs = set(raw_refs)
    if not observed_refs.issubset(NORMALIZED_REFS):
        raise RuntimeError("existing normalized ref inventory is outside the allowlist")
    return observed_refs


def apply(root: Path, evidence: Path) -> None:
    root = root.resolve()
    source_ref = run(root, "git", "rev-parse", "HEAD")
    if source_ref not in NORMALIZED_REFS:
        raise RuntimeError("detached historical source ref is outside the normalization allowlist")
    source = root / SOURCE_PATH
    data = source.read_bytes()
    digest = sha256(data)
    if digest == PRE_REPAIR_SHA256:
        if data.count(OLD_BLOCK) != 1 or NEW_BLOCK in data:
            raise RuntimeError("chronology repair source cardinality drift")
        cfg_test = data.find(b"#[cfg(test)]\nmod tests")
        repair_at = data.find(OLD_BLOCK)
        if cfg_test < 0 or repair_at <= cfg_test:
            raise RuntimeError("chronology repair is not confined to cfg(test)")
        repaired = data.replace(OLD_BLOCK, NEW_BLOCK, 1)
        if sha256(repaired) != POST_REPAIR_SHA256:
            raise RuntimeError("chronology repair post-image drift")
        write_atomic(source, repaired)
    elif digest != POST_REPAIR_SHA256:
        raise RuntimeError("detached Stage 7B source image drift")

    status = run(root, "git", "status", "--porcelain=v1", "--untracked-files=no")
    if status != f"M {SOURCE_PATH.as_posix()}":
        raise RuntimeError(f"chronology repair dirty-surface drift: {status!r}")
    numstat = run(root, "git", "diff", "--numstat", "--", SOURCE_PATH.as_posix())
    if numstat != f"4\t1\t{SOURCE_PATH.as_posix()}":
        raise RuntimeError(f"chronology repair diff cardinality drift: {numstat!r}")
    diff = subprocess.check_output(
        ["git", "diff", "--", SOURCE_PATH.as_posix()], cwd=root
    )
    if sha256(diff) != DIFF_SHA256:
        raise RuntimeError("chronology repair diff digest drift")
    subprocess.run(["git", "diff", "--check", "--", SOURCE_PATH.as_posix()], cwd=root, check=True)
    evidence = evidence.resolve()
    observed_refs: set[str] = set()
    if evidence.is_file():
        observed_refs.update(read_partial_evidence(evidence))
    observed_refs.add(source_ref)
    write_atomic(
        evidence,
        (
            json.dumps(expected_evidence(tuple(sorted(observed_refs))), indent=2, sort_keys=True)
            + "\n"
        ).encode(),
    )


def restore(root: Path, evidence: Path) -> None:
    root = root.resolve()
    source_ref = run(root, "git", "rev-parse", "HEAD")
    if source_ref not in NORMALIZED_REFS:
        raise RuntimeError("detached historical source ref is outside the restoration allowlist")
    observed_refs = read_partial_evidence(evidence.resolve())
    if source_ref not in observed_refs:
        raise RuntimeError("detached historical source ref is absent from compatibility evidence")
    source = root / SOURCE_PATH
    data = source.read_bytes()
    if sha256(data) != POST_REPAIR_SHA256:
        raise RuntimeError("chronology restoration pre-image drift")
    status = run(root, "git", "status", "--porcelain=v1", "--untracked-files=no")
    if status != f"M {SOURCE_PATH.as_posix()}":
        raise RuntimeError(f"chronology restoration dirty-surface drift: {status!r}")
    diff = subprocess.check_output(["git", "diff", "--", SOURCE_PATH.as_posix()], cwd=root)
    if sha256(diff) != DIFF_SHA256:
        raise RuntimeError("chronology restoration diff digest drift")
    restored = data.replace(NEW_BLOCK, OLD_BLOCK, 1)
    if restored.count(OLD_BLOCK) != 1 or NEW_BLOCK in restored:
        raise RuntimeError("chronology restoration source cardinality drift")
    if sha256(restored) != PRE_REPAIR_SHA256:
        raise RuntimeError("chronology restoration post-image drift")
    write_atomic(source, restored)
    clean_status = run(root, "git", "status", "--porcelain=v1", "--untracked-files=no")
    if clean_status:
        raise RuntimeError(f"chronology restoration left a dirty worktree: {clean_status!r}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--verify-evidence-only", action="store_true")
    parser.add_argument("--restore", action="store_true")
    args = parser.parse_args()
    if args.verify_evidence_only and args.restore:
        raise RuntimeError("--verify-evidence-only and --restore are mutually exclusive")
    if args.verify_evidence_only:
        if args.root is not None:
            raise RuntimeError("--root is forbidden with --verify-evidence-only")
        verify_evidence(args.evidence.resolve())
    elif args.restore:
        if args.root is None:
            raise RuntimeError("--root is required for chronology restoration")
        restore(args.root, args.evidence)
    else:
        if args.root is None:
            raise RuntimeError("--root is required for repair application")
        apply(args.root, args.evidence)
    print("current-tree-stage8a5-replay-compat: PASS")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        raise SystemExit(f"current-tree-stage8a5-replay-compat: FAIL {error}") from error
