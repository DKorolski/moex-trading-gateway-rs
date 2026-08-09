#!/usr/bin/env python3
"""One-commit source/archive seal for Stage 6A."""
from __future__ import annotations
import argparse, io, subprocess, tarfile
from pathlib import Path, PurePosixPath
import stage6a_check as checker

EXPECTED = sorted([
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage6_durable_identity.rs",
    "fixtures/stage6a/cancel-request-accepted-v1.json",
    "fixtures/stage6a/place-request-accepted-v1.json",
    "docs/stage-6/stage6a-durable-identity-schema.md",
    "docs/stage-6/stage6a-schema-descriptor.json",
    "docs/stage-6/stage6a-direct-authority-inventory.json",
    "docs/stage-6/stage6a-golden-manifest.json",
    "scripts/stage6a_check.py", "scripts/stage6a_closed_surface_check.py",
    "scripts/stage6a_negative_harness.py", "scripts/stage6a_preseal_check.py",
    "scripts/stage6a_gate.sh", "scripts/make_stage6a_handoff_archive.py",
])

def output(*args, root): return subprocess.check_output(args,cwd=root,text=True).strip()
def fail(msg): raise SystemExit(f"stage6a-preseal: FAIL: {msg}")
def safe(name):
    p=PurePosixPath(name)
    return not p.is_absolute() and ".." not in p.parts and not any(x in {".git","target","tmp","reports","__MACOSX"} for x in p.parts) and p.name != ".env" and p.suffix != ".log"

def main():
    parser=argparse.ArgumentParser(); parser.add_argument("--require-origin",action="store_true"); args=parser.parse_args()
    root=Path.cwd().resolve()
    if output("git","rev-parse","HEAD^",root=root) != checker.BASE: fail("HEAD is not direct successor")
    if output("git","branch","--show-current",root=root) != checker.BRANCH: fail("wrong branch")
    if sorted(output("git","diff","--name-only",f"{checker.BASE}..HEAD",root=root).splitlines()) != EXPECTED: fail("changed-path allowlist drift")
    if output("git","status","--porcelain",root=root): fail("worktree not clean")
    if args.require_origin and output("git","rev-parse",f"origin/{checker.BRANCH}",root=root) != output("git","rev-parse","HEAD",root=root): fail("origin branch mismatch")
    tracked={line.split("\t",1)[1] for line in output("git","ls-files","-s",root=root).splitlines()}
    archive=subprocess.check_output(["git","archive","--format=tar","HEAD"],cwd=root)
    archived=set()
    with tarfile.open(fileobj=io.BytesIO(archive),mode="r:") as tar:
        for member in tar.getmembers():
            if member.isdir(): continue
            if not member.isfile() or not safe(member.name) or member.name in archived: fail(f"unsafe member: {member.name}")
            archived.add(member.name)
    if tracked != archived: fail("index/archive mismatch")
    checker.check(root)
    print(f"stage6a-preseal: PASS delta={len(EXPECTED)} archive={len(archived)}")
if __name__ == "__main__": main()
