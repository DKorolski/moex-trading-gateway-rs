#!/usr/bin/env python3
"""Atomically retain and verify Stage 8B-P1-e I0 acceptance evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import subprocess
import sys
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[1]
MANIFEST = "artifact-manifest.json"
MANIFEST_DIGEST = "artifact-manifest.sha256"
RESULT = "run-result.json"
REQUIRED_PASS_MARKERS = (
    "PASS stage8b-p1e-i0-scope-check",
    "PASS stage8b-p1e-i0-p1d4-regression-check",
    "PASS stage8b-p1d4-source-negative-harness",
    "PASS stage8b-p1d4-crash-evidence-check",
    "PASS stage8b-p1d4-crash-evidence-negative-harness",
    "PASS exact-regression-test index=6 selected=1 passed=1",
)


class EvidenceFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise EvidenceFailure(message)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def git(*args: str) -> str:
    return subprocess.check_output(("git", *args), cwd=ROOT, text=True).strip()


def write_json(path: pathlib.Path, value: Any) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def artifact_entries(directory: pathlib.Path) -> list[dict[str, Any]]:
    entries: list[dict[str, Any]] = []
    for path in sorted(item for item in directory.rglob("*") if item.is_file()):
        relative = path.relative_to(directory).as_posix()
        if relative in {MANIFEST, MANIFEST_DIGEST}:
            continue
        data = path.read_bytes()
        entries.append({"path": relative, "size": len(data), "sha256": sha256(data)})
    return entries


def source_entries(source_ref: str) -> list[dict[str, str]]:
    output = git("ls-tree", "-r", "--full-tree", source_ref)
    entries: list[dict[str, str]] = []
    for line in output.splitlines():
        metadata, path = line.split("\t", 1)
        mode, kind, object_id = metadata.split()
        require(kind == "blob", f"non-blob tracked source entry: {path}")
        entries.append({"path": path, "mode": mode, "object_id": object_id})
    return entries


def finalize(args: argparse.Namespace) -> None:
    temporary = pathlib.Path(args.temporary).resolve()
    output = pathlib.Path(args.output).resolve()
    require(temporary.is_dir(), "temporary evidence directory is absent")
    require(output.is_absolute(), "retained output must be absolute")
    require(not output.exists(), "retained output already exists")
    require(output.parent.is_dir(), "retained output parent is absent")
    require(ROOT.resolve() not in output.parents and output != ROOT.resolve(), "retained output must be outside repository")
    require(temporary.parent == output.parent, "temporary and retained output must share an atomic-rename parent")

    actual_ref = git("rev-parse", "HEAD")
    actual_tree = git("rev-parse", "HEAD^{tree}")
    clean_after = not git("status", "--porcelain", "--untracked-files=all")
    source = source_entries(args.source_ref)
    source_manifest = {
        "schema_version": 1,
        "source_ref": args.source_ref,
        "source_tree": args.source_tree,
        "entries": source,
    }
    write_json(temporary / "source-tree-manifest.json", source_manifest)
    result = {
        "schema_version": 1,
        "domain": "moex.stage8b.p1e.i0.retained-evidence-result.v1",
        "status": args.status,
        "exit_code": args.exit_code,
        "accepted_r10_ref": args.accepted_r10,
        "source_ref": args.source_ref,
        "source_tree": args.source_tree,
        "actual_ref_after": actual_ref,
        "actual_tree_after": actual_tree,
        "clean_before": args.clean_before,
        "clean_after": clean_after,
        "same_source_before_after": actual_ref == args.source_ref and actual_tree == args.source_tree,
    }
    write_json(temporary / RESULT, result)
    manifest = {
        "schema_version": 1,
        "domain": "moex.stage8b.p1e.i0.retained-evidence-manifest.v1",
        "source_ref": args.source_ref,
        "source_tree": args.source_tree,
        "status": args.status,
        "files": artifact_entries(temporary),
    }
    manifest_bytes = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()
    (temporary / MANIFEST).write_bytes(manifest_bytes)
    (temporary / MANIFEST_DIGEST).write_text(f"{sha256(manifest_bytes)}  {MANIFEST}\n", encoding="utf-8")
    os.rename(temporary, output)
    print(
        "PASS stage8b-p1e-i0-retained-evidence-finalize "
        f"status={args.status} output={output} manifest_sha256={sha256(manifest_bytes)}"
    )


def check(args: argparse.Namespace) -> None:
    directory = pathlib.Path(args.directory).resolve()
    require(directory.is_dir(), "retained evidence directory is absent")
    result = json.loads((directory / RESULT).read_text(encoding="utf-8"))
    manifest_bytes = (directory / MANIFEST).read_bytes()
    manifest = json.loads(manifest_bytes)
    expected_digest = (directory / MANIFEST_DIGEST).read_text(encoding="utf-8").split()[0]
    require(sha256(manifest_bytes) == expected_digest, "artifact manifest digest mismatch")
    for entry in manifest.get("files", []):
        path = directory / entry["path"]
        require(path.is_file(), f"retained artifact absent: {entry['path']}")
        data = path.read_bytes()
        require(len(data) == entry["size"] and sha256(data) == entry["sha256"], f"retained artifact mismatch: {entry['path']}")
    actual_files = {path.relative_to(directory).as_posix() for path in directory.rglob("*") if path.is_file()}
    expected_files = {entry["path"] for entry in manifest["files"]} | {MANIFEST, MANIFEST_DIGEST}
    require(actual_files == expected_files, "retained artifact inventory mismatch")
    require(result["source_ref"] == manifest["source_ref"] and result["source_tree"] == manifest["source_tree"], "result/manifest source mismatch")
    if args.require_pass:
        require(result["status"] == "PASS" and result["exit_code"] == 0, "retained run is not PASS")
        require(result["clean_before"] and result["clean_after"] and result["same_source_before_after"], "tested source was not clean and immutable")
        require(git("rev-parse", "HEAD") == result["source_ref"], "current HEAD differs from retained evidence")
        require(git("rev-parse", "HEAD^{tree}") == result["source_tree"], "current tree differs from retained evidence")
        log = (directory / "gate.log").read_text(encoding="utf-8")
        for marker in REQUIRED_PASS_MARKERS:
            require(marker in log, f"mandatory gate marker absent: {marker}")
        crash = directory / "crash-evidence"
        require(crash.is_dir() and any(path.is_file() for path in crash.rglob("*")), "retained crash evidence is empty")
    print(
        "PASS stage8b-p1e-i0-retained-evidence-check "
        f"status={result['status']} files={len(manifest['files'])} source_ref={result['source_ref']}"
    )


def parser() -> argparse.ArgumentParser:
    top = argparse.ArgumentParser()
    sub = top.add_subparsers(dest="command", required=True)
    finish = sub.add_parser("finalize")
    finish.add_argument("--temporary", required=True)
    finish.add_argument("--output", required=True)
    finish.add_argument("--status", choices=("PASS", "FAIL"), required=True)
    finish.add_argument("--exit-code", type=int, required=True)
    finish.add_argument("--accepted-r10", required=True)
    finish.add_argument("--source-ref", required=True)
    finish.add_argument("--source-tree", required=True)
    finish.add_argument("--clean-before", action="store_true")
    verify = sub.add_parser("check")
    verify.add_argument("directory")
    verify.add_argument("--require-pass", action="store_true")
    return top


def main() -> None:
    args = parser().parse_args()
    try:
        finalize(args) if args.command == "finalize" else check(args)
    except (EvidenceFailure, OSError, subprocess.CalledProcessError, KeyError, ValueError, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-i0-retained-evidence: FAIL {error}")
        raise SystemExit(1)


if __name__ == "__main__":
    main()
