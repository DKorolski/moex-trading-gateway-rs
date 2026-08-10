#!/usr/bin/env python3
"""One-commit source/archive seal for Stage 6B-R1."""
from __future__ import annotations
import argparse,io,subprocess,tarfile
from pathlib import Path,PurePosixPath
import stage6b_check as checker

EXPECTED=sorted([
    "crates/strategy-runtime-core/src/stage6_journal_backend.rs",
    "docs/stage-6/stage6b-isolated-journal-backend.md",
    "docs/stage-6/stage6b-storage-descriptor.json",
    "scripts/stage6b_check.py","scripts/stage6b_negative_harness.py",
    "scripts/stage6b_preseal_check.py","scripts/stage6b_r1_gate.sh",
    "scripts/make_stage6b_handoff_archive.py",
])
def output(*args,root): return subprocess.check_output(args,cwd=root,text=True).strip()
def fail(message): raise SystemExit(f"stage6b-preseal: FAIL: {message}")
def safe(name):
    path=PurePosixPath(name)
    return not path.is_absolute() and ".." not in path.parts and not any(part in {".git","target","tmp","reports","__MACOSX"} for part in path.parts) and path.name != ".env" and path.suffix != ".log"
def main():
    parser=argparse.ArgumentParser();parser.add_argument("--require-origin",action="store_true");args=parser.parse_args()
    root=Path.cwd().resolve()
    if output("git","rev-parse","HEAD^",root=root)!=checker.R1_BASE:fail("HEAD is not direct Stage 6B successor")
    if output("git","branch","--show-current",root=root)!=checker.BRANCH:fail("wrong branch")
    changed=sorted(output("git","diff","--name-only",f"{checker.R1_BASE}..HEAD",root=root).splitlines())
    if changed!=EXPECTED:fail("changed-path allowlist drift")
    if output("git","status","--porcelain",root=root):fail("worktree not clean")
    if args.require_origin and output("git","rev-parse",f"origin/{checker.BRANCH}",root=root)!=output("git","rev-parse","HEAD",root=root):fail("origin branch mismatch")
    tracked={line.split("\t",1)[1] for line in output("git","ls-files","-s",root=root).splitlines()}
    archive=subprocess.check_output(["git","archive","--format=tar","HEAD"],cwd=root);archived=set()
    with tarfile.open(fileobj=io.BytesIO(archive),mode="r:") as tar:
        for member in tar.getmembers():
            if member.isdir():continue
            if not member.isfile() or not safe(member.name) or member.name in archived:fail(f"unsafe archive member: {member.name}")
            archived.add(member.name)
    if tracked!=archived:fail("index/archive mismatch")
    checker.check(root)
    print(f"stage6b-r1-preseal: PASS delta={len(EXPECTED)} archive={len(archived)}")
if __name__ == "__main__":main()
