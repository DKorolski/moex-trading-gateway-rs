#!/usr/bin/env python3
"""Create the immutable Stage 8A-2 reviewer-transition handoff."""

from __future__ import annotations

import hashlib
import io
import json
import stat
import subprocess
import tarfile
import zipfile
from pathlib import Path, PurePosixPath

import stage8a1_check

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "reports/handoff"
BRANCH = "stage8a2-builder-composition"
ACCEPTED_PREDECESSOR = "1ff04154ba4b7a5ee060a73b853ce89bd7442f44"
ACCEPTED_ARCHIVE_SHA256 = (
    "02a332af7ccf1815a79de0f9a025be9d31094160f7977cd3e34de270497138a0"
)
ACCEPTED_REVIEW_SHA256 = (
    "ac11ffae08cc11c6e11f031deec9ca4e6d55b76eae6bcc5887f85b0fb913c74c"
)
ALLOWED_TRANSITION_PATHS = {
    "README.md",
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8-slice-plan.md",
    "docs/stage-8/stage8a1-r3-independent-acceptance.json",
    "docs/stage-8/stage8a2-entry-contract.md",
    "docs/stage-8/stage8a2-reviewer-transition-handoff.md",
    "scripts/make_stage8a2_transition_handoff.py",
}


def run(args: list[str]) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"stage8a2-transition-handoff: FAIL {message}")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def safe(name: str) -> bool:
    path = PurePosixPath(name)
    return (
        not path.is_absolute()
        and ".." not in path.parts
        and not any(
            part in {".git", "target", "tmp", "reports", "__MACOSX"}
            for part in path.parts
        )
        and path.name != ".env"
        and path.suffix != ".log"
    )


def verify_archive(path: Path, expected_marker: bytes) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [info.filename for info in infos]
        duplicates = len(names) - len(set(names))
        unsafe = [name for name in names if not safe(name)]
        symlinks = [
            info.filename
            for info in infos
            if stat.S_ISLNK((info.external_attr >> 16) & 0xFFFF)
        ]
        special = [
            info.filename
            for info in infos
            if not (
                stat.S_ISREG((info.external_attr >> 16) & 0xFFFF)
                or (info.external_attr >> 16) == 0
            )
        ]
        if duplicates or unsafe or symlinks or special:
            fail("archive member safety check failed")
        if archive.read("handoff-commit.txt") != expected_marker:
            fail("commit marker mismatch")
        manifest = json.loads(archive.read("source-tree-manifest.json"))
        for member in manifest["members"]:
            if sha256(archive.read(member["path"])) != member["sha256"]:
                fail(f"source member hash mismatch: {member['path']}")
    return {
        "result": "PASS",
        "members": len(names),
        "unique_members": len(set(names)),
        "duplicates": duplicates,
        "unsafe_paths": len(unsafe),
        "symlinks": len(symlinks),
        "special_files": len(special),
    }


def main() -> None:
    if run(["git", "status", "--porcelain"]):
        fail("worktree must be clean")
    if run(["git", "branch", "--show-current"]) != BRANCH:
        fail(f"branch must be {BRANCH}")

    head = run(["git", "rev-parse", "HEAD"])
    short = run(["git", "rev-parse", "--short=7", "HEAD"])
    subprocess.run(
        ["git", "merge-base", "--is-ancestor", ACCEPTED_PREDECESSOR, head],
        cwd=ROOT,
        check=True,
    )
    if run(["git", "rev-parse", f"origin/{BRANCH}"]) != head:
        fail("origin branch must equal HEAD")

    changed = set(
        run(["git", "diff", "--name-only", ACCEPTED_PREDECESSOR, head]).splitlines()
    )
    if changed != ALLOWED_TRANSITION_PATHS:
        fail(f"transition changed-path drift: {sorted(changed)}")

    closure = json.loads(
        (ROOT / "docs/stage-8/stage8a1-r3-independent-acceptance.json").read_text()
    )
    if closure["accepted_source_ref"] != ACCEPTED_PREDECESSOR:
        fail("accepted predecessor drift")
    if closure["accepted_source_archive_sha256"] != ACCEPTED_ARCHIVE_SHA256:
        fail("accepted archive hash drift")
    if closure["independent_review_sha256"] != ACCEPTED_REVIEW_SHA256:
        fail("accepted review hash drift")

    # Recheck the immutable Stage 8A-1 source/contract hashes without applying
    # its historical branch/scope rule to this docs-only successor commit.
    stage8a1_check.check(ROOT, git_scope=False, pin_hashes=True)

    raw = subprocess.check_output(
        ["git", "archive", "--format=tar", "HEAD"], cwd=ROOT
    )
    source_members: list[tuple[str, bytes, int]] = []
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        for member in archive.getmembers():
            if member.isdir():
                continue
            if not member.isfile() or not safe(member.name):
                fail(f"unsafe tracked source member: {member.name}")
            payload = archive.extractfile(member)
            if payload is None:
                fail(f"missing source payload: {member.name}")
            source_members.append((member.name, payload.read(), member.mode))

    source_manifest = json.dumps(
        {
            "schema_version": 1,
            "source_ref": head,
            "source_branch": BRANCH,
            "accepted_stage8a1_ref": ACCEPTED_PREDECESSOR,
            "members": [
                {"path": name, "sha256": sha256(data)}
                for name, data, _ in sorted(source_members)
            ],
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"

    archive_name = f"moex-trading-project-{short}.zip"
    marker = (
        f"source_ref={head}\n"
        f"source_short_ref={short}\n"
        f"source_branch={BRANCH}\n"
        f"archive_name={archive_name}\n"
        f"accepted_stage8a1_ref={ACCEPTED_PREDECESSOR}\n"
        f"accepted_stage8a1_archive_sha256={ACCEPTED_ARCHIVE_SHA256}\n"
        f"accepted_stage8a1_review_sha256={ACCEPTED_REVIEW_SHA256}\n"
        "handoff_kind=reviewer_transition\n"
        "current_stage=Stage 8A-2 entry\n"
        "next_authorized_scope=Stage 8A-2 only\n"
        "finam_post_delete_authorized=false\n"
        "broker_dispatch_authorized=false\n"
        "runtime_live_authorized=false\n"
        "real_orders_authorized=false\n"
    ).encode()
    evidence = json.dumps(
        {
            "schema_version": 1,
            "handoff_kind": "reviewer_transition",
            "source_ref": head,
            "source_branch": BRANCH,
            "accepted_stage8a1_ref": ACCEPTED_PREDECESSOR,
            "accepted_stage8a1_archive_sha256": ACCEPTED_ARCHIVE_SHA256,
            "accepted_stage8a1_review_sha256": ACCEPTED_REVIEW_SHA256,
            "inherited_stage8a1_hash_check": "PASS",
            "changed_paths": sorted(changed),
            "current_stage": "Stage 8A-2 entry",
            "next_authorized_scope": "Stage 8A-2 only",
            "stage8a3_authorized": False,
            "finam_post_delete_authorized": False,
            "broker_dispatch_authorized": False,
            "runtime_live_authorized": False,
            "real_orders_authorized": False,
            "source_tree_manifest_sha256": sha256(source_manifest),
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"

    members = list(source_members)
    members.extend(
        [
            ("handoff-commit.txt", marker, 0o644),
            ("source-tree-manifest.json", source_manifest, 0o644),
            ("handoff-evidence/stage8a2-reviewer-transition.json", evidence, 0o644),
        ]
    )
    if len({name for name, _, _ in members}) != len(members):
        fail("duplicate archive member")

    OUT.mkdir(parents=True, exist_ok=True)
    destination = OUT / archive_name
    with zipfile.ZipFile(
        destination, "w", zipfile.ZIP_DEFLATED, compresslevel=9
    ) as target:
        for name, data, mode in sorted(members):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            permissions = 0o755 if mode & 0o111 else 0o644
            info.external_attr = (stat.S_IFREG | permissions) << 16
            target.writestr(info, data, zipfile.ZIP_DEFLATED, compresslevel=9)

    digest = sha256(destination.read_bytes())
    sidecar = destination.with_suffix(".zip.sha256")
    sidecar.write_text(f"{digest}  {destination.name}\n")
    safety = verify_archive(destination, marker)
    safety_path = destination.with_suffix(".zip.safety.json")
    safety_path.write_text(json.dumps(safety, indent=2, sort_keys=True) + "\n")

    print(destination)
    print(sidecar)
    print(safety_path)
    print(digest)


if __name__ == "__main__":
    main()
