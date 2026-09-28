#!/usr/bin/env python3
"""Run/package the narrow O2 recovery source review; never install or activate."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import zipfile

import current_tree_authority_check as authority
import make_stage8b_design_handoff as packaging
import stage8b_p1e_i1a_handoff_safety_check as git_objects

ROOT = Path(__file__).resolve().parents[1]
BASE = "38c7b825863d8a54c018eb5581ed8e243e017cfe"
PRODUCTION = {
    "crates/finam-gateway/src/bin/stage8b-p1f-o2-materializer.rs",
    "crates/runtime-durable-service/src/bin/stage8b-p1f-o2-operator.rs",
    "crates/runtime-durable-service/src/stage8b_p1f_guardian.rs",
}
ALLOWED = PRODUCTION | {
    "docs/current-status.md", "docs/roadmap.md",
    "docs/stage-8/stage8b-p1f-o2-recovery-source-correction.md",
    "scripts/stage8b_p1f_o2_recovery_review.py",
}
COMMANDS = [
    ("fmt", ["cargo", "fmt", "--all", "--", "--check"]),
    ("finam-debug", ["cargo", "test", "--locked", "-p", "finam-gateway", "--all-targets", "--all-features", "--", "--test-threads=1"]),
    ("durable-debug", ["cargo", "test", "--locked", "-p", "runtime-durable-service", "--all-targets", "--all-features", "--", "--test-threads=1"]),
    ("materializer-release", ["cargo", "test", "--locked", "--release", "-p", "finam-gateway", "--all-features", "--bin", "stage8b-p1f-o2-materializer", "--", "--test-threads=1"]),
    ("guardian-release", ["cargo", "test", "--locked", "--release", "-p", "runtime-durable-service", "--all-features", "--lib", "stage8b_p1f_guardian::tests::", "--", "--test-threads=1"]),
    ("alor-regression", ["bash", "scripts/stage8b_p1f_alor_finam_source_correction_gate.sh"]),
    ("doctests", ["cargo", "test", "--locked", "-p", "finam-gateway", "-p", "runtime-durable-service", "--all-features", "--doc"]),
    ("clippy", ["cargo", "clippy", "--locked", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"]),
    ("diff", ["git", "diff", "--check", BASE]),
]


def git(*args: str) -> bytes:
    return subprocess.check_output(["git", *args], cwd=ROOT)


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def require(value: bool, message: str) -> None:
    if not value:
        raise SystemExit(message)


def clean_ref() -> str:
    require(not git("status", "--porcelain", "--untracked-files=all").strip(), "dirty worktree")
    return git("rev-parse", "HEAD").decode().strip()


def run_gate(output: Path) -> None:
    ref = clean_ref()
    changed = set(git("diff", "--name-only", BASE, ref).decode().splitlines())
    require(PRODUCTION <= changed <= ALLOWED, "source correction scope drift")
    pinned = json.loads((ROOT / authority.AUTHORITY).read_text())["production_code_manifest"]["entries"]
    actual = authority.file_inventory(ROOT, authority.production_files(ROOT))
    drift = {name for name in set(pinned) | set(actual) if pinned.get(name) != actual.get(name)}
    require(drift == PRODUCTION, "unexpected production authority drift")
    output.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, RUST_MIN_STACK="33554432", CARGO_TERM_COLOR="never")
    logs = {}
    for name, command in COMMANDS:
        print(f"RUN {name}: {' '.join(command)}", flush=True)
        path = output / f"{name}.txt"
        with path.open("wb") as stream:
            result = subprocess.run(command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT)
        require(result.returncode == 0, f"FAIL {name}; see {path}")
        raw = path.read_bytes()
        if name == "materializer-release":
            require(b"4 passed; 0 failed" in raw, "materializer tests did not execute")
        if name == "guardian-release":
            require(raw.count(b"::o2_selector_") == 6,
                    "selector regression tests did not execute")
        logs[path.name] = digest(raw)
        print(f"PASS {name}", flush=True)
    # This is intentionally not reported as a passing authority gate. The
    # accepted binding stays immutable until this source delta is reviewed.
    result = subprocess.run(["python3", "scripts/current_tree_authority_check.py"], cwd=ROOT,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    require(result.returncode != 0 and b"production entry drift" in result.stdout,
            "unexpected authority result (source acceptance/rebind is separate)")
    (output / "authority-pending.txt").write_bytes(result.stdout)
    logs["authority-pending.txt"] = digest(result.stdout)
    require(clean_ref() == ref, "source changed during gate")
    summary = {
        "stage": "Stage 8B-P1-f O2 recovery source correction",
        "status": "SOURCE_REVIEW_PENDING", "source_gate_passed": True,
        "source_ref": ref, "source_tree": git("rev-parse", "HEAD^{tree}").decode().strip(),
        "baseline_ref": BASE, "commands": COMMANDS, "logs_sha256": logs,
        "new_regression_tests": 10, "authority_status": "ACCEPTED_BASELINE_REBIND_PENDING",
        "authority_drift_paths": sorted(drift), "merge_ready": False,
        "execution_authorized": False, "vps_contacted": False,
        "finam_contacted": False, "operational_redis_activated": False,
        "limitations": ["selector restart tests reopen real files at simulated durable frontiers; not SIGKILL",
                         "local macOS source tests; no new Linux deployment/binary artifact",
                         "historical replay/CI already passed at baseline; not rerun for source review"],
    }
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"PASS source gate; authority/review pending; evidence={output}", flush=True)


def check_archive(path: Path) -> dict:
    with zipfile.ZipFile(path) as archive:
        require(archive.testzip() is None, "ZIP CRC failure")
        infos = archive.infolist()
        require(len(infos) == len({item.filename for item in infos}), "duplicate members")
        for item in infos:
            p = PurePosixPath(item.filename)
            require(not p.is_absolute() and ".." not in p.parts and "\\" not in item.filename,
                    "unsafe member path")
            require((item.external_attr >> 16) in (0o100644, 0o100755), "symlink/special/mode")
            require(not ({".git", "target", "tmp", ".env"} & set(p.parts)), "excluded member")
        files = {i.filename: archive.read(i) for i in infos}
    prefix = "handoff-evidence/"
    summary = json.loads(files[prefix + "summary.json"])
    manifest = json.loads(files[prefix + "source-tree-manifest.json"])
    entries = manifest["entries"]
    source = {}
    modes = {i.filename: f"{i.external_attr >> 16:06o}" for i in infos}
    for entry in entries:
        name = entry["path"]
        require(name not in source, "duplicate manifest entry")
        raw = files[name]
        require(digest(raw) == entry["sha256"] and len(raw) == entry["size"] and modes[name] == entry["mode"], "manifest mismatch")
        source[name] = raw
    require(manifest["entry_count"] == len(source), "manifest cardinality")
    raw_commit = files[prefix + "source-commit.raw"]
    ref = git_objects.git_object_id("commit", raw_commit)
    tree = git_objects.build_tree_oid(entries, source)
    require(ref == summary["source_ref"] == manifest["source_ref"], "commit mismatch")
    require(raw_commit.splitlines()[0] == f"tree {tree}".encode() and tree == summary["source_tree"], "tree mismatch")
    require(summary["source_gate_passed"] is True and summary["merge_ready"] is False, "status mismatch")
    for name, expected in summary["logs_sha256"].items():
        require(digest(files[prefix + name]) == expected, "log mismatch")
    require(files["handoff-commit.txt"] == marker(ref, tree, path.name), "marker mismatch")
    generated = {"handoff-commit.txt", prefix + "source-commit.raw", prefix + "source-tree-manifest.json", prefix + "summary.json"}
    generated |= {prefix + name for name in summary["logs_sha256"]}
    require(set(files) == set(source) | generated and not set(source) & generated, "member inventory mismatch")
    return {"safe": True, "source_ref": ref, "source_tree": tree, "archive_sha256": digest(path.read_bytes()),
            "members": len(files), "tracked": len(source), "duplicates": 0, "unsafe_paths": 0,
            "symlinks": 0, "special_files": 0, "commit_tree_binding": True}


def marker(ref: str, tree: str, name: str) -> bytes:
    return f"source_short_ref={ref[:7]}\nsource_ref={ref}\nsource_tree={tree}\nbaseline_ref={BASE}\narchive_name={name}\n".encode()


def package(output: Path) -> None:
    ref = clean_ref()
    summary = json.loads((output / "summary.json").read_text())
    require(summary["source_ref"] == ref, "evidence is not for HEAD")
    manifest, entries = packaging.source_manifest(ref)
    path = ROOT / "reports/handoff" / f"moex-trading-project-{ref[:7]}-o2-recovery-review.zip"
    require(not path.exists(), "immutable archive already exists")
    path.parent.mkdir(parents=True, exist_ok=True)
    additions = {"handoff-commit.txt": marker(ref, summary["source_tree"], path.name),
                 "handoff-evidence/source-commit.raw": git("cat-file", "commit", ref),
                 "handoff-evidence/source-tree-manifest.json": manifest,
                 "handoff-evidence/summary.json": (output / "summary.json").read_bytes()}
    for name in summary["logs_sha256"]:
        additions["handoff-evidence/" + name] = (output / name).read_bytes()
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            require(entry["path"] not in additions, "generated/tracked collision")
            archive.writestr(packaging.zip_info(entry["path"], entry["mode"]), git("show", f"{ref}:{entry['path']}"))
        for name, raw in additions.items():
            archive.writestr(packaging.zip_info(name), raw)
    result = check_archive(path)
    require(clean_ref() == ref, "source changed during packaging")
    Path(str(path) + ".sha256").write_text(f"{result['archive_sha256']}  {path.name}\n")
    Path(str(path) + ".safety.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result | {"archive": str(path)}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["gate", "package", "check"])
    parser.add_argument("path", type=Path)
    args = parser.parse_args()
    if args.action == "gate":
        run_gate(args.path.resolve())
    elif args.action == "package":
        package(args.path.resolve())
    else:
        print(json.dumps(check_archive(args.path), indent=2))
