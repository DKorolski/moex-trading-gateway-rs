#!/usr/bin/env python3
"""Stage HEAD + an explicit shadow allowlist. No tests, ZIP or commit creation.

The destination must be new and outside the active repository. All tracked
baseline files are retained, including fixtures/reports that may be compile-time
inputs. No dirty O4 source is copied. The parent must test this isolated snapshot
before packaging; manifest.json explicitly records incomplete source acceptance.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat
import subprocess
import tarfile
import tempfile


CRATE = "crates/finam-signal-shadow"
PARENT = "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs"
SEAM = "crates/strategy-runtime-core/src/hybrid_intraday_runtime/signal_shadow.rs"
DOCS = (
    "docs/stage-8/reviews/REVIEW_SIGNAL_SHADOW_SCOPE_TZ_20261023_RU.txt",
    "docs/stage-8/stage8b-signal-shadow-source-review-r0.md",
    "docs/stage-8/stage8b-signal-shadow-source-review-r1.md",
    "docs/stage-8/stage8b-signal-shadow-source-review-r2.md",
    "docs/stage-8/reviews/REVIEW_SIGNAL_SHADOW_e1d5e8b3_R1_RU.txt",
    "docs/stage-8/reviews/REVIEW_SIGNAL_SHADOW_3b200dd_R0C_RU.txt",
    "docs/stage-8/stage8b-signal-shadow-priority-decision.md",
    "docs/stage-8/stage8b-signal-shadow-review-request-ru.md",
    "docs/stage-8/stage8b-signal-shadow-review-readiness.md",
    "docs/stage-8/stage8b-signal-shadow-source-progress.md",
)
SCRIPTS = (
    "scripts/signal_shadow_review_package.py",
    "scripts/signal_shadow_reference_inventory.py",
    "scripts/test_signal_shadow_reference_inventory.py",
    "scripts/signal_shadow_state_reference.py",
    "scripts/signal_shadow_seal_review.py",
)
OVERLAY = (
    SEAM, "crates/broker-finam/src/ws.rs",
    "crates/strategy-runtime-core/src/hybrid_intraday/intraday_breakout.rs",
    "crates/strategy-runtime-core/src/hybrid_intraday/orchestrator.rs",
    *DOCS, *SCRIPTS,
)


def run(args, cwd, *, stdout=subprocess.PIPE):
    env = os.environ.copy()
    # Do not let an inherited alternate index/worktree redirect Git reads.
    for key in list(env):
        if key.startswith("GIT_"):
            del env[key]
    env["CARGO_NET_OFFLINE"] = "true"
    return subprocess.run(args, cwd=cwd, env=env, stdout=stdout,
                          stderr=subprocess.PIPE, check=True).stdout


def safe_name(name):
    p = PurePosixPath(name)
    if (not name or p.is_absolute() or "\\" in name
            or any(x in ("", ".", "..") for x in name.split("/"))
            or any(ord(c) < 32 for c in name) or ":" in name):
        raise ValueError(f"unsafe member path: {name!r}")
    for part in p.parts:
        n = part.lower()
        if (n in {".git", "id_rsa", "id_ed25519", "credentials.json",
                  "credentials.toml", "secrets.json", "secrets.toml"}
                or (n.startswith(".env") and n != ".env.example")
                or n.endswith((".pem", ".key", ".p12", ".pfx", ".env"))):
            raise ValueError(f"secret/unsafe filename refused: {name}")
    return p


def regular_files(root):
    seen = set()
    for directory, dirs, files in os.walk(root, followlinks=False):
        for name in sorted(dirs + files):
            path = Path(directory) / name
            relative = path.relative_to(root).as_posix()
            safe_name(relative)
            mode = path.lstat().st_mode
            if not (stat.S_ISREG(mode) or stat.S_ISDIR(mode)):
                raise ValueError(f"symlink/special file refused: {relative}")
            if relative.casefold() in seen:
                raise ValueError(f"duplicate/case collision: {relative}")
            seen.add(relative.casefold())
            if stat.S_ISREG(mode):
                yield relative, path


def read_regular(root, relative):
    safe_name(relative)
    path = root
    for part in PurePosixPath(relative).parts:
        path = path / part
        if path.is_symlink():
            raise ValueError(f"symlink refused: {relative}")
    if not stat.S_ISREG(path.stat().st_mode):
        raise ValueError(f"not a regular file: {relative}")
    return path.read_bytes(), 0o755 if path.stat().st_mode & 0o111 else 0o644


def write(root, relative, data, mode=0o644):
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    path.chmod(mode)


def inventory(root):
    return {name: {"sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                   "bytes": path.stat().st_size,
                   "mode": "100755" if path.stat().st_mode & 0o111 else "100644"}
            for name, path in sorted(regular_files(root))}


def stage(repo, destination):
    repo = repo.resolve(strict=True)
    destination = destination.absolute()
    if destination.exists() or destination.is_symlink():
        raise ValueError("destination must not exist")
    destination = destination.parent.resolve(strict=True) / destination.name
    if destination == repo or repo in destination.parents:
        raise ValueError("destination must be outside the active repository")
    top = Path(run(["git", "rev-parse", "--show-toplevel"], repo).decode().strip())
    if top.resolve() != repo:
        raise ValueError("--repo must be the repository root")
    baseline = run(["git", "rev-parse", "HEAD^{commit}"], repo).decode().strip()

    # Capture only selected working files; never copy whole dirty shared crates.
    overlay = {name: read_regular(repo, name) for name in OVERLAY}
    fixture_root = "fixtures/signal-shadow-state-parity"
    for name, _ in regular_files(repo / fixture_root):
        overlay[f"{fixture_root}/{name}"] = read_regular(repo, f"{fixture_root}/{name}")
    for name, _ in regular_files(repo / CRATE):
        if name != "Cargo.toml" and not name.startswith(("src/", "tests/")):
            raise ValueError(f"unexpected crate artifact: {name}")
        overlay[f"{CRATE}/{name}"] = read_regular(repo, f"{CRATE}/{name}")
    if f"{CRATE}/Cargo.toml" not in overlay:
        raise ValueError("missing shadow manifest")

    destination.mkdir(mode=0o700)
    source = destination / "source"
    source.mkdir()
    # Extract regular files ourselves: never tar.extractall(), links or traversal.
    with tempfile.TemporaryFile() as archive:
        run(["git", "archive", "--format=tar", baseline], repo, stdout=archive)
        archive.seek(0)
        seen = set()
        with tarfile.open(fileobj=archive, mode="r:") as tar:
            for member in tar:
                name = member.name.rstrip("/") if member.isdir() else member.name
                safe_name(name)
                if name.casefold() in seen:
                    raise ValueError(f"duplicate archive member: {name}")
                seen.add(name.casefold())
                if member.isdir():
                    (source / name).mkdir(parents=True, exist_ok=True)
                elif member.isfile():
                    stream = tar.extractfile(member)
                    if stream is None:
                        raise ValueError(f"unreadable member: {name}")
                    write(source, name, stream.read(),
                          0o755 if member.mode & 0o111 else 0o644)
                else:
                    raise ValueError(f"archive link/special member refused: {name}")
    before = inventory(source)
    # Refuse Git export-ignore/export-subst surprises: retain exact tracked blobs.
    tracked = run(["git", "ls-tree", "-rz", baseline], repo).split(b"\0")
    tracked_names = set()
    algorithm = run(["git", "rev-parse", "--show-object-format"], repo).decode().strip()
    for entry in filter(None, tracked):
        metadata, raw_name = entry.split(b"\t", 1)
        mode, kind, oid = metadata.decode().split()
        name = raw_name.decode()
        safe_name(name)
        if kind != "blob" or mode not in ("100644", "100755"):
            raise ValueError(f"unsupported tracked entry: {name}")
        data = (source / name).read_bytes()
        digest = hashlib.new(algorithm, b"blob " + str(len(data)).encode() + b"\0" + data)
        if digest.hexdigest() != oid or before[name]["mode"] != mode:
            raise ValueError(f"archive changed tracked blob: {name}")
        tracked_names.add(name)
    if tracked_names != set(before):
        raise ValueError("archive inventory differs from HEAD")

    parent = (source / PARENT).read_bytes()
    anchor = b"mod session_catchup;\n"
    if parent.count(anchor) != 1 or b"mod signal_shadow;" in parent:
        raise ValueError("unexpected baseline core module layout")
    write(source, PARENT, parent.replace(anchor, anchor + b"mod signal_shadow;\n", 1))
    cargo = (source / "Cargo.toml").read_bytes()
    anchor = b'    "crates/runtime-durable-service",\n'
    if cargo.count(anchor) != 1 or CRATE.encode() in cargo:
        raise ValueError("unexpected baseline workspace layout")
    write(source, "Cargo.toml", cargo.replace(
        anchor, anchor + b'    "crates/finam-signal-shadow",\n', 1))
    for name, (data, mode) in overlay.items():
        write(source, name, data, mode)
    # Refuse concurrent changes to captured files; parent can retry a new stage.
    for name, captured in overlay.items():
        if read_regular(repo, name) != captured:
            raise ValueError(f"overlay changed during capture: {name}")

    # Resolve the added workspace member while preserving existing locked versions.
    # generate-lockfile would unnecessarily re-resolve every transitive dependency.
    host = next(line.split(": ", 1)[1] for line in
                run(["rustc", "-vV"], source).decode().splitlines() if line.startswith("host: "))
    run(["cargo", "metadata", "--offline", "--format-version", "1", "--filter-platform", host], source)
    after = inventory(source)
    changed = sorted(name for name in set(before) | set(after)
                     if before.get(name) != after.get(name))
    allowed = set(overlay) | {PARENT, "Cargo.toml", "Cargo.lock"}
    if set(changed) - allowed:
        raise ValueError("unexpected changes outside package allowlist")
    manifest = {
        "schema_version": 1,
        "status": "SOURCE_REVIEW_R2_PENDING_REVIEW",
        "baseline_commit": baseline,
        "baseline_policy": "all tracked HEAD files retained; no dirty O4 overlays",
        "known_gaps": [
            "S5/S6/S9/S10 actual FINAM subscription, timing, two observed sessions and VPS isolation not executed",
            "Restart promotion requires separately trusted complete prior-output/quote inventory; local payload hashes alone do not prove completeness",
            "Historical fixed-profile conditional parity is not universal strategy or current ALOR live execution parity",
        ],
        "tests_run": False, "source_acceptance": False,
        "operational_acceptance": False, "commit_created": False,
        "lock_policy": "cargo metadata --offline preserving baseline locked versions; review delta before sealing",
        "intentional_changed_paths": changed,
        "overlay_paths": sorted(overlay),
        "files": after,
    }
    (destination / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    (destination / "baseline.txt").write_text(baseline + "\n")
    (destination / "README.txt").write_text(
        "SOURCE REVIEW R2 — PENDING INDEPENDENT SOURCE ACCEPTANCE\n"
        "Local contract evidence is separate from operational acceptance; see the R2 cover.\n"
        "No tests, ZIP, commit, VPS or operational run performed by this utility.\n"
        "Parent: copy source/ to a separate test checkout outside the eventual ZIP;\n"
        "run offline locked shadow tests there and retain logs separately.\n"
        "Verify source files against manifest.json before packaging.\n"
        "All baseline reports are historical inherited inputs, NOT new acceptance evidence.\n"
        "Only filename safety was screened; perform content/secret review before release.\n")
    print(json.dumps({"stage": str(destination), "source": str(source),
                      "baseline": baseline, "changed_paths": changed,
                      "tests_run": False, "commit_created": False}, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--destination", type=Path, required=True)
    args = parser.parse_args()
    try:
        stage(args.repo, args.destination)
    except (OSError, ValueError, subprocess.CalledProcessError, tarfile.TarError) as error:
        # Keep failed staging for diagnosis, but never emit a success manifest.
        parser.exit(1, f"staging refused: {error}\n")


if __name__ == "__main__":
    main()
