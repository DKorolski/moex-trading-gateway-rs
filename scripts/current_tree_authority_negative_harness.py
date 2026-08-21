#!/usr/bin/env python3
"""Fail-closed semantic and control-plane mutations for GOV-CI-1B."""

from __future__ import annotations

import hashlib
import json
import shutil
import stat
import subprocess
import tempfile
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
AUTHORITY = Path("docs/stage-8/gov-ci-1-authority.json")
CHECKER = ["python3", "scripts/current_tree_authority_check.py"]


def replace(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if old not in text:
        raise RuntimeError(f"mutation source missing in {path}: {old}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def append(path: Path, value: str) -> None:
    path.write_text(path.read_text(encoding="utf-8") + value, encoding="utf-8")


def copy_tree(destination: Path) -> None:
    shutil.copytree(
        ROOT,
        destination,
        ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports", "*.log", "__pycache__"),
    )


def production_files(root: Path) -> list[Path]:
    paths = [root / "Cargo.toml", root / "Cargo.lock"]
    paths.extend(
        path
        for path in (root / "crates").rglob("*")
        if path.is_file() and (path.name == "Cargo.toml" or path.suffix == ".rs")
    )
    return sorted(set(paths), key=lambda path: path.relative_to(root).as_posix())


def refresh_production_manifest(root: Path) -> None:
    authority_path = root / AUTHORITY
    authority = json.loads(authority_path.read_text(encoding="utf-8"))
    entries: dict[str, dict[str, object]] = {}
    for path in production_files(root):
        data = path.read_bytes()
        entries[path.relative_to(root).as_posix()] = {
            "mode": "100755" if path.stat().st_mode & stat.S_IXUSR else "100644",
            "sha256": hashlib.sha256(data).hexdigest(),
            "size": len(data),
        }
    aggregate = hashlib.sha256()
    for name, entry in sorted(entries.items()):
        aggregate.update(
            name.encode()
            + b"\0"
            + str(entry["mode"]).encode()
            + b"\0"
            + str(entry["sha256"]).encode()
            + b"\0"
            + str(entry["size"]).encode()
            + b"\n"
        )
    content = hashlib.sha256()
    for name, entry in sorted(entries.items()):
        content.update(name.encode() + b"\0" + str(entry["sha256"]).encode() + b"\n")
    authority["production_code_manifest"] = {
        "aggregate_sha256": aggregate.hexdigest(),
        "content_sha256": content.hexdigest(),
        "entries": entries,
        "file_count": len(entries),
    }
    authority_path.write_text(json.dumps(authority, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def mutate_closed_flag(root: Path, key: str) -> None:
    path = root / AUTHORITY
    authority = json.loads(path.read_text(encoding="utf-8"))
    authority["closed_surfaces"][key] = True
    path.write_text(json.dumps(authority, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def mutate_replay_ref(root: Path) -> None:
    path = root / AUTHORITY
    authority = json.loads(path.read_text(encoding="utf-8"))
    authority["accepted_stage8a5_replay"]["source_ref"] = "0" * 40
    path.write_text(json.dumps(authority, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def semantic_feature_open(root: Path) -> None:
    replace(
        root / "crates/broker-cli/Cargo.toml",
        "default = []",
        'default = ["m3j16-actual-one-shot"]',
    )
    refresh_production_manifest(root)


def semantic_accessor_open(root: Path, name: str) -> None:
    path = root / "crates/strategy-runtime-core/src/stage6d_live_core.rs"
    replace(path, f"pub fn {name}(&self) -> bool {{\n        false\n    }}", f"pub fn {name}(&self) -> bool {{\n        true\n    }}")
    refresh_production_manifest(root)


def add_active_workflow(root: Path) -> None:
    path = root / ".github/workflows/unreviewed.yml"
    path.write_text("name: unreviewed\non:\n  push:\njobs: {}\n", encoding="utf-8")


def main() -> None:
    workflow = Path(".github/workflows/ci.yml")
    historical = Path(".github/workflows/stage5f-base-authority.yml")
    gate = Path("scripts/current_tree_ci_gate.sh")
    cases: tuple[tuple[str, Callable[[Path], None]], ...] = (
        ("workflow-gate-echo-noop", lambda r: replace(r / workflow, "run: bash scripts/current_tree_ci_gate.sh", "run: echo 'bash scripts/current_tree_ci_gate.sh'")),
        ("workflow-debug-test-echo-noop", lambda r: replace(r / workflow, "run: cargo test --workspace --all-targets -- --test-threads=1", "run: echo 'cargo test --workspace --all-targets -- --test-threads=1'")),
        ("workflow-release-test-echo-noop", lambda r: replace(r / workflow, "run: cargo test --workspace --release --all-targets -- --test-threads=1", "run: echo 'cargo test --workspace --release --all-targets -- --test-threads=1'")),
        ("workflow-doctest-echo-noop", lambda r: replace(r / workflow, "run: cargo test --workspace --doc", "run: echo 'cargo test --workspace --doc'")),
        ("workflow-clippy-echo-noop", lambda r: replace(r / workflow, "run: cargo clippy --workspace --all-targets --all-features -- -D warnings", "run: echo 'cargo clippy --workspace --all-targets --all-features -- -D warnings'")),
        ("workflow-no-redis-smoke-noop", lambda r: replace(r / workflow, "run: bash scripts/test_m4_3x_evidence_no_redis.sh", "run: true # bash scripts/test_m4_3x_evidence_no_redis.sh")),
        ("workflow-redis-smoke-noop", lambda r: replace(r / workflow, "run: scripts/redis_shadow_smoke.sh", "run: echo scripts/redis_shadow_smoke.sh")),
        ("gate-checker-commented", lambda r: replace(r / gate, "python3 scripts/current_tree_authority_check.py", "# python3 scripts/current_tree_authority_check.py")),
        ("gate-negative-commented", lambda r: replace(r / gate, "python3 scripts/current_tree_authority_negative_harness.py", "# python3 scripts/current_tree_authority_negative_harness.py")),
        ("gate-replay-echo-noop", lambda r: replace(r / gate, '  bash "$replay_root/repo/scripts/stage8a5_gate.sh"', '  echo \'bash "$replay_root/repo/scripts/stage8a5_gate.sh"\'')),
        ("gate-shell-wrapper-substitution", lambda r: replace(r / gate, "python3 scripts/current_tree_authority_check.py", "runner=python3\n$runner scripts/current_tree_authority_check.py")),
        ("gate-stage8a5-ref-drift", lambda r: replace(r / gate, "bf58b47fdef8af774a4107455dfcc6204e594283", "0000000000000000000000000000000000000000")),
        ("historical-scanner-current-gate", lambda r: replace(r / gate, "python3 scripts/current_tree_authority_check.py", "bash scripts/forbidden_surface_scan.sh")),
        ("unapproved-active-workflow", add_active_workflow),
        ("historical-pull-request-target-reactivated", lambda r: replace(r / historical, "  workflow_dispatch:", "  pull_request_target:\n    branches: [main]\n  workflow_dispatch:")),
        ("checker-control-plane-drift", lambda r: append(r / "scripts/current_tree_authority_check.py", "\n# drift\n")),
        ("negative-harness-control-plane-drift", lambda r: append(r / "scripts/current_tree_authority_negative_harness.py", "\n# drift\n")),
        ("handoff-maker-control-plane-drift", lambda r: append(r / "scripts/make_gov_ci_1_handoff.py", "\n# drift\n")),
        ("production-file-outside-baseline", lambda r: (r / "crates/finam-gateway/src/gov_ci_backdoor.rs").write_text("pub fn added_surface() {}\n", encoding="utf-8")),
        ("semantic-finam-default-feature", semantic_feature_open),
        ("semantic-redis-command-consumer", lambda r: semantic_accessor_open(r, "redis_command_consumer_attached")),
        ("semantic-runtime-live", lambda r: semantic_accessor_open(r, "runtime_live_attached")),
        ("semantic-broker-dispatch", lambda r: semantic_accessor_open(r, "broker_network_dispatch_attached")),
        ("semantic-real-orders", lambda r: semantic_accessor_open(r, "real_orders_enabled")),
        ("stage8b-s-opened", lambda r: mutate_closed_flag(r, "stage8b_s_authorized")),
        ("accepted-stage8a5-replay-ref-drift", mutate_replay_ref),
        ("historical-stage5d-current-gate", lambda r: replace(r / gate, "python3 scripts/current_tree_authority_check.py", "python3 scripts/stage5d_additive_freeze_negative_harness.py")),
    )
    passed = 0
    for name, mutation in cases:
        with tempfile.TemporaryDirectory(prefix="gov-ci-1b-negative-") as temp:
            work = Path(temp) / "repo"
            copy_tree(work)
            mutation(work)
            result = subprocess.run(
                CHECKER + ["--root", str(work)],
                cwd=work,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
            )
            if result.returncode == 0:
                raise SystemExit(f"current-tree-authority-negative: FAIL survived={name}")
            passed += 1
            print(f"PASS {name}")
    print(f"current-tree-authority-negative: PASS cases={passed}/{len(cases)}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"current-tree-authority-negative: FAIL {error}") from error
