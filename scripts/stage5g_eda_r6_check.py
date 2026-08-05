#!/usr/bin/env python3
"""Compilation-control and protected-tree seal for Stage 5G-e-d-a R6."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
from pathlib import Path

import stage5g_eda_r3_check as r3
import stage5g_eda_r4_check as r4
import stage5g_eda_r5_check as r5


BASE_REF = "c84ee07c2700f04b5c070eab713598777d5195b6"
FREEZE = Path("docs/stage-5/stage5g-e-d-a-r6-protected-tree-freeze.json")
GATE = Path("scripts/stage5g_eda_r6_gate.sh")
NEGATIVE = Path("scripts/stage5g_eda_r6_negative_harness.py")
PRESEAL = Path("scripts/stage5g_eda_r6_preseal_check.py")
BUILDER = Path("scripts/make_stage5g_ed_handoff_archive.py")
CONTRACT = r5.CONTRACT
DESIGN = r5.DESIGN
STATUS = r5.STATUS
ONBOARDING = r5.ONBOARDING
PROTECTED_FILE_COUNT = 881
PROTECTED_MANIFEST_SHA256 = "ab1b8a16b582fd39d1ef1c97fa21dd29c8769c63735871f0a0cfd107bf11d3b8"
FREEZE_SHA256 = "87f9c272ccf3211b494a62688e0e34ce34a19f554b253dd6df7c9a9fb2a3ba4f"
RUNTIME_RUST_TARGET_COUNT = 23
RUNTIME_RUST_TARGET_MANIFEST_SHA256 = "408fed03bb16da80641dd79cb7b4bb8a28dcb55f2304e43633274a13b52fb5e2"
ROOT_CARGO_SHA256 = "a1ce5ddaf5477579ae283b4f64ebc779fa261367333fdf048a15253a1bac86a8"
CARGO_LOCK_SHA256 = "8e9464979eadf00a1e5dcd7cd89e40e70067eb4d332e557fef9b7f4451d09244"
RUNTIME_CARGO_SHA256 = "384f3c5a2b388a7084ad188ead216e68de341c346a44947bb79be752900ec736"
GATE_SHA256 = "076c4d705e9d5ef25912d6d743b2f689e030c668a3501d11ab936a34d5841a1f"
PRESEAL_SHA256 = "2e0aa849d3384804e385da485267e5cd48fc4bfe11a73133d86d3b0f147290c4"
BUILDER_SHA256 = "b20a3deeba9e05fefb0941836ee2a655eaeaab39766cd39e755807dd8b2df217"

EXPECTED_WORKSPACE_MEMBERS = [
    "crates/broker-core",
    "crates/broker-finam",
    "crates/finam-gateway",
    "crates/broker-cli",
    "crates/strategy-runtime-core",
]
EXPECTED_DELTA = [
    {"path": "docs/current-status.md", "status": "M"},
    {"path": "docs/reviewer-onboarding-and-roadmap.md", "status": "M"},
    {"path": "docs/stage-5/stage5g-e-d-a-r6-protected-tree-freeze.json", "status": "A"},
    {"path": "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.json", "status": "M"},
    {"path": "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.md", "status": "M"},
    {"path": "scripts/make_stage5g_ed_handoff_archive.py", "status": "M"},
    {"path": "scripts/stage5g_eda_r6_check.py", "status": "A"},
    {"path": "scripts/stage5g_eda_r6_gate.sh", "status": "A"},
    {"path": "scripts/stage5g_eda_r6_negative_harness.py", "status": "A"},
    {"path": "scripts/stage5g_eda_r6_preseal_check.py", "status": "A"},
]
EXPECTED_GATE_COMMANDS = [
    "python3 scripts/stage5g_eda_r6_check.py",
    "python3 scripts/stage5g_eda_r6_negative_harness.py",
    "python3 scripts/stage5g_eda_r6_preseal_check.py",
    "cargo fmt --all -- --check",
    "cargo test -p strategy-runtime-core --lib stage5g_fresh_broker_truth",
    "cargo test --release -p strategy-runtime-core --lib stage5g_fresh_broker_truth",
    "cargo test -p strategy-runtime-core --lib",
    "cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings",
]
R6_MUTATIONS = {
    "redirect-runtime-lib-to-alternate-source-tree",
    "redirect-workspace-member-to-alternate-package",
    "remove-runtime-member-from-workspace",
    "add-duplicate-runtime-package-member",
    "add-default-runtime-build-rs",
    "set-runtime-package-build-script",
    "add-repository-cargo-config-rustc-wrapper",
    "add-extensionless-repository-cargo-config",
    "modify-root-cargo-toml",
    "modify-cargo-lock",
    "modify-runtime-cargo-toml",
    "add-runtime-rust-file-outside-src",
    "add-runtime-integration-target",
    "add-runtime-bench-target",
    "add-runtime-example-target",
    "modify-inherited-r5-checker-dependency",
    "modify-broker-core-source-outside-r6-allowlist",
    "add-unreviewed-workspace-crate",
    "change-protected-tree-manifest-commitment",
    "remove-protected-tree-delta-check",
    "builder-remove-protected-tree-delta-check",
}
ZIP_GENERATED = {
    "handoff-commit.txt",
    "handoff-source-tree-manifest.json",
    "stage5g-e-d-a-r6-gate-result.json",
    "stage5g-e-d-a-r6-gate-output.txt",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage5g-eda-r6-check: FAIL: {message}")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def load_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage5g-eda-r6-check: FAIL: cannot load {path}: {error}") from error
    require(isinstance(value, dict), f"{path} must contain an object")
    return value


def canonical_sha256(rows: list[dict[str, object]]) -> str:
    return sha256_bytes(json.dumps(rows, sort_keys=True, separators=(",", ":")).encode())


def git_inventory(root: Path) -> set[str] | None:
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], cwd=root,
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, check=False,
    )
    if result.returncode != 0 or Path(result.stdout.strip()).resolve() != root:
        return None
    tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=root)
    untracked = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard", "-z"], cwd=root
    )
    return {
        value.decode() for value in (tracked + untracked).split(b"\0") if value
    }


def filesystem_inventory(root: Path) -> set[str]:
    excluded_dirs = {".git", "target", "tmp", "reports", "__MACOSX", "__pycache__"}
    values: set[str] = set()
    for current, directories, files in os.walk(root):
        directories[:] = [name for name in directories if name not in excluded_dirs]
        current_path = Path(current)
        for name in files:
            path = current_path / name
            relative = path.relative_to(root).as_posix()
            if name == ".env" or name == ".DS_Store" or name.endswith(".log"):
                continue
            require(path.is_file() and not path.is_symlink(), f"non-regular project member: {relative}")
            values.add(relative)
    return values


def project_inventory(root: Path) -> set[str]:
    return git_inventory(root) or filesystem_inventory(root)


def assert_protected_tree(root: Path) -> dict:
    freeze_path = root / FREEZE
    require(sha256_bytes(freeze_path.read_bytes()) == FREEZE_SHA256,
            "reviewed R6 freeze manifest hash drifted")
    freeze = load_json(freeze_path)
    require(freeze.get("schema_version") == 1, "R6 freeze schema drifted")
    require(freeze.get("stage") == "5G-e-d-a-r6", "R6 freeze stage drifted")
    require(freeze.get("base_commit") == BASE_REF, "R6 freeze base drifted")
    require(freeze.get("accepted_base_tracked_file_count") == 886,
            "accepted R5 file count drifted")
    require(freeze.get("protected_file_count") == PROTECTED_FILE_COUNT,
            "protected file count drifted")
    require(freeze.get("protected_manifest_sha256") == PROTECTED_MANIFEST_SHA256,
            "protected manifest commitment field drifted")
    require(freeze.get("mutable_allowlist") == EXPECTED_DELTA,
            "exact R6 mutable allowlist drifted")
    require(freeze.get("implemented_restart_case_ids") == [], "R6 claims GRST execution")
    require(freeze.get("stage5g_e_d_b_open") is False, "R6 opens e-d-b")
    rows = freeze.get("rows")
    require(isinstance(rows, list) and len(rows) == PROTECTED_FILE_COUNT,
            "protected rows/count drifted")
    require(canonical_sha256(rows) == PROTECTED_MANIFEST_SHA256,
            "protected manifest canonical commitment mismatch")
    protected_paths: set[str] = set()
    for row in rows:
        require(set(row) == {"path", "size", "sha256"}, "protected row shape drifted")
        path = row["path"]
        require(isinstance(path, str) and path not in protected_paths,
                "duplicate/invalid protected path")
        protected_paths.add(path)
        payload = (root / path).read_bytes()
        require(len(payload) == row["size"], f"protected size drifted: {path}")
        require(sha256_bytes(payload) == row["sha256"], f"protected SHA-256 drifted: {path}")

    mutable_paths = {row["path"] for row in EXPECTED_DELTA}
    for path in mutable_paths:
        require((root / path).is_file() and not (root / path).is_symlink(),
                f"R6 mutable path missing/non-regular: {path}")
    actual = project_inventory(root)
    extras = actual - protected_paths - mutable_paths
    if extras:
        require(extras == ZIP_GENERATED, f"unreviewed project paths: {sorted(extras)[:5]}")
    require(protected_paths | mutable_paths | extras == actual,
            "protected-tree inventory mismatch")
    return freeze


def assert_cargo_topology(root: Path, freeze: dict) -> None:
    topology = freeze.get("cargo_topology")
    require(isinstance(topology, dict), "Cargo topology missing")
    require(sha256_bytes((root / "Cargo.toml").read_bytes()) == ROOT_CARGO_SHA256,
            "root Cargo.toml drifted")
    require(sha256_bytes((root / "Cargo.lock").read_bytes()) == CARGO_LOCK_SHA256,
            "Cargo.lock drifted")
    runtime_manifest_path = root / "crates/strategy-runtime-core/Cargo.toml"
    require(sha256_bytes(runtime_manifest_path.read_bytes()) == RUNTIME_CARGO_SHA256,
            "runtime Cargo.toml drifted")
    root_manifest = (root / "Cargo.toml").read_text()
    workspace = re.search(r"(?ms)^\[workspace\]\s*(.*?)(?=^\[|\Z)", root_manifest)
    require(workspace is not None, "root [workspace] table missing")
    member_block = re.search(r"(?s)\bmembers\s*=\s*\[(.*?)\]", workspace.group(1))
    require(member_block is not None, "workspace members missing")
    members = re.findall(r'"([^"]+)"', member_block.group(1))
    require(members == EXPECTED_WORKSPACE_MEMBERS, "workspace members/order drifted")
    runtime_manifest = runtime_manifest_path.read_text()
    runtime_package = re.search(r"(?ms)^\[package\]\s*(.*?)(?=^\[|\Z)", runtime_manifest)
    require(runtime_package is not None, "runtime [package] table missing")
    runtime_name = re.search(r'(?m)^name\s*=\s*"([^"]+)"\s*$', runtime_package.group(1))
    require(runtime_name is not None and runtime_name.group(1) == "strategy-runtime-core",
            "runtime package name drifted")
    require(not re.search(r"(?m)^build\s*=", runtime_package.group(1)),
            "runtime package.build opened")
    require(not re.search(r"(?m)^\[lib\]\s*$", runtime_manifest),
            "runtime [lib] target redirect opened")
    runtime_root = root / "crates/strategy-runtime-core"
    require(not (runtime_root / "build.rs").exists(), "runtime build.rs opened")
    nested_manifests = sorted(
        path.relative_to(root).as_posix() for path in runtime_root.rglob("Cargo.toml")
    )
    require(nested_manifests == ["crates/strategy-runtime-core/Cargo.toml"],
            "nested/alternate runtime package manifest opened")
    package_names: list[str | None] = []
    for member in members:
        manifest = root / member / "Cargo.toml"
        require(manifest.is_file(), f"workspace member manifest missing: {member}")
        package = re.search(r"(?ms)^\[package\]\s*(.*?)(?=^\[|\Z)", manifest.read_text())
        require(package is not None, f"workspace member package table missing: {member}")
        name = re.search(r'(?m)^name\s*=\s*"([^"]+)"\s*$', package.group(1))
        package_names.append(name.group(1) if name is not None else None)
    require(package_names.count("strategy-runtime-core") == 1,
            "workspace must contain exactly one strategy-runtime-core package")

    require(not (root / ".cargo").exists(), "repository-local .cargo configuration opened")
    rust_rows = freeze.get("runtime_rust_targets")
    require(isinstance(rust_rows, list) and len(rust_rows) == RUNTIME_RUST_TARGET_COUNT,
            "runtime Rust target count drifted")
    require(canonical_sha256(rust_rows) == RUNTIME_RUST_TARGET_MANIFEST_SHA256,
            "runtime Rust target commitment drifted")
    actual_rows = []
    for path in sorted(runtime_root.rglob("*.rs")):
        payload = path.read_bytes()
        actual_rows.append({
            "path": path.relative_to(root).as_posix(),
            "size": len(payload),
            "sha256": sha256_bytes(payload),
        })
    require(actual_rows == rust_rows, "runtime Rust target set/hash/size drifted")


def mutation_names(path: Path) -> set[str]:
    return set(re.findall(r'\("([^"]+)",\s*lambda root:', path.read_text()))


def assert_contract_and_mutations(root: Path) -> None:
    r5.assert_source_freeze(root)
    contract = load_json(root / CONTRACT)
    require(contract.get("status") == "r6_final_compilation_control_review_candidate",
            "R6 contract status drifted")
    require(contract.get("rejected_r5_commit") == BASE_REF, "R5 base binding drifted")
    require(contract.get("protected_tree_freeze_manifest") == FREEZE.name,
            "R6 protected-tree pointer drifted")
    require(contract.get("contract") == r4.EXPECTED_CONTRACT_SHAPE, "contract map drifted")
    require(contract.get("closed_surfaces") == r4.EXPECTED_CLOSED_SURFACES,
            "closed-surface map drifted")
    require(contract.get("implemented_restart_case_ids") == [], "e-d-b implementation claimed")
    require(contract.get("dispositions") == r3.EXPECTED_DISPOSITIONS,
            "dispositions drifted")
    require(contract.get("operational_identity_fields") == r3.EXPECTED_OPERATIONAL_FIELDS,
            "operational identity fields drifted")
    scenarios = contract.get("restart_scenarios")
    require([row.get("id") for row in scenarios] == r3.EXPECTED_GRST_IDS,
            "GRST IDs/order drifted")

    names = (
        mutation_names(root / "scripts/stage5g_eda_r3_negative_harness.py")
        | mutation_names(root / "scripts/stage5g_eda_r4_negative_harness.py")
        | mutation_names(root / "scripts/stage5g_eda_r5_negative_harness.py")
        | mutation_names(root / NEGATIVE)
    )
    require(R6_MUTATIONS <= names, "mandatory R6 mutation missing")
    require(len(names) >= 118, "R6 negative matrix has fewer than 118 cases")


def assert_execution_seal(root: Path) -> None:
    gate_payload = (root / GATE).read_bytes()
    preseal_payload = (root / PRESEAL).read_bytes()
    builder_payload = (root / BUILDER).read_bytes()
    require(sha256_bytes(gate_payload) == GATE_SHA256, "reviewed R6 gate hash drifted")
    require(sha256_bytes(preseal_payload) == PRESEAL_SHA256, "reviewed R6 preseal hash drifted")
    require(sha256_bytes(builder_payload) == BUILDER_SHA256, "reviewed R6 builder hash drifted")
    lines = [line.strip() for line in gate_payload.decode().splitlines()]
    positions = []
    for command in EXPECTED_GATE_COMMANDS:
        require(lines.count(command) == 1, f"gate command must occur once: {command}")
        positions.append(lines.index(command))
    require(positions == sorted(positions), "R6 gate command order drifted")
    require(lines.count("bash scripts/stage5g_eda_r5_gate.sh") == 1,
            "detached R5 gate command must occur once")
    require(f'r5_ref="{BASE_REF}"' in lines, "detached R5 reference drifted")
    require('git worktree add --detach "$snapshot_root" "$r5_ref" >/dev/null' in lines,
            "R5 gate must run detached")
    preseal = preseal_payload.decode()
    require('git", "diff", "--name-status", f"{BASE_REF}..HEAD"' in preseal,
            "preseal exact changed-path check missing")
    require("if delta != EXPECTED_DELTA:" in preseal,
            "preseal changed-path comparison missing")
    builder = builder_payload.decode()
    required = (
        'BRANCH = "stage5g-lifecycle"',
        'STAGE = "5G-e-d-a-r6"',
        f'REQUIRED_PARENT = "{BASE_REF}"',
        '["git", "status", "--porcelain", "--untracked-files=all"]',
        'if branch != BRANCH:',
        'if parent_ref != REQUIRED_PARENT:',
        'if origin_ref != source_ref:',
        'if delta != EXPECTED_DELTA:',
        'if gate.returncode != 0:',
        '["git", "archive", "--format=tar", source_ref]',
    )
    for anchor in required:
        require(anchor in builder, f"R6 builder guard missing: {anchor}")


def assert_docs(root: Path) -> None:
    design = (root / DESIGN).read_text()
    status = (root / STATUS).read_text()
    onboarding = (root / ONBOARDING).read_text()
    for value in (
        "R6 is final e-d-a compilation-control acceptance closure.",
        "No Rust source, test, Cargo topology or dependency semantic changed.",
        "implemented_restart_case_ids remains empty",
        "Stage 5G-e-d-b remains closed pending independent R6 acceptance",
        "Primary current-HEAD gate: `bash scripts/stage5g_eda_r6_gate.sh`",
    ):
        require(value in design, f"R6 design statement missing: {value}")
    require("Stage 5G-e-d-a R6" in status, "status R6 target missing")
    require("pending independent R6 acceptance" in status, "status R6 boundary missing")
    require("Stage 5G-e-d-a R6" in onboarding, "onboarding R6 target missing")
    require("pending independent R6 acceptance" in onboarding,
            "onboarding R6 boundary missing")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    root = parser.parse_args().root.resolve()
    required = (FREEZE, GATE, NEGATIVE, PRESEAL, BUILDER, CONTRACT, DESIGN, STATUS, ONBOARDING)
    for path in required:
        require((root / path).is_file(), f"missing {path}")
    freeze = assert_protected_tree(root)
    assert_cargo_topology(root, freeze)
    assert_contract_and_mutations(root)
    assert_execution_seal(root)
    assert_docs(root)
    print("stage5g-eda-r6-check: PASS")


if __name__ == "__main__":
    main()
