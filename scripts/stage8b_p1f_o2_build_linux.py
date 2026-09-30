#!/usr/bin/env python3
"""Build O2 from the pinned merge, without mounting the workstation checkout."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SOURCE_REF = "9d9bd1192467532d0ee48350d3c531d9e156dee3"
SOURCE_TREE = "0e1ee00d73e23460dc0c4af5a13296d6e2ad12e3"
IMAGE = "rust@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922"
BINS = ("stage8b-p1f-o2-materializer", "stage8b-p1f-o2-operator", "stage8b-p1-paper-supervisor")
CARGO_ARGS = ["build", "--locked", "--release", "-p", "finam-gateway", "--bin", BINS[0],
              "-p", "runtime-durable-service", "--bin", BINS[1], "--bin", BINS[2]]


def git(*args: str) -> bytes:
    return subprocess.check_output(["git", *args], cwd=ROOT)


def sha(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def build(output: Path, registry_cache: Path, resume: bool = False) -> None:
    if git("rev-parse", SOURCE_REF + "^{tree}").decode().strip() != SOURCE_TREE:
        raise SystemExit("build source tree mismatch")
    # Export only Git blobs: no checkout .env, private inputs, Git credentials,
    # Docker socket, SSH agent or operational host mounts enter the container.
    for entry in git("ls-tree", "-rz", SOURCE_REF).split(b"\0"):
        if entry:
            metadata, name = entry.split(b"\t", 1)
            if metadata.split()[:2] not in ([b"100644", b"blob"], [b"100755", b"blob"]):
                raise SystemExit(f"non-regular build input: {name!r}")
    source = output / "source"
    if resume:
        if (output / "source-commit.raw").read_bytes() != git("cat-file", "commit", SOURCE_REF):
            raise SystemExit("resumed source ref mismatch")
        expected = set()
        for entry in git("ls-tree", "-rz", SOURCE_REF).split(b"\0"):
            if not entry:
                continue
            metadata, name = entry.split(b"\t", 1)
            path = source / name.decode()
            expected.add(path)
            if path.is_symlink() or path.read_bytes() != git("cat-file", "blob", metadata.split()[2].decode()):
                raise SystemExit("resumed source content mismatch")
        if {p for p in source.rglob("*") if not p.is_dir()} != expected:
            raise SystemExit("resumed source inventory mismatch")
    else:
        output.mkdir(parents=True, exist_ok=False)
        source.mkdir()
        subprocess.run(["tar", "-xf", "-", "-C", str(source)],
                       input=git("archive", "--format=tar", SOURCE_REF), check=True)
        (output / "source-commit.raw").write_bytes(git("cat-file", "commit", SOURCE_REF))
        for directory in ("target", "cargo-home"):
            (output / directory).mkdir()
        # Reuse only public Cargo registry data, never Cargo config/credentials.
        shutil.copytree(registry_cache, output / "cargo-home/registry")
    docker = ["docker", "run", "--rm", "--network", "none", "--platform", "linux/amd64",
              "--mount", f"type=bind,src={source},dst=/src,readonly",
              "--mount", f"type=bind,src={output / 'target'},dst=/target",
              "--mount", f"type=bind,src={output / 'cargo-home'},dst=/cargo-home",
              "-w", "/src", "-e", "CARGO_HOME=/cargo-home", "-e", "CARGO_TARGET_DIR=/target",
              "-e", "CARGO_TERM_COLOR=never", "-e", "CARGO_INCREMENTAL=0", "-e", "CARGO_NET_OFFLINE=true"]
    with (output / "build.log").open("ab" if resume else "wb") as log:
        for args in (["rustc", "--version", "--verbose"], ["cargo", "--version"], ["cargo", *CARGO_ARGS]):
            log.write(("COMMAND " + " ".join(args) + "\n").encode()); log.flush()
            result = subprocess.run([*docker, IMAGE, *args], stdout=log, stderr=subprocess.STDOUT)
            if result.returncode:
                raise SystemExit(f"build failed ({result.returncode}); see {output / 'build.log'}")
    binaries = []
    for name in BINS:
        path = output / "target/release" / name
        data = path.read_bytes()
        if data[:6] != b"\x7fELF\x02\x01" or int.from_bytes(data[18:20], "little") != 62:
            raise SystemExit(f"not x86-64 ELF: {name}")
        identity = subprocess.check_output(["file", "-b", str(path)], text=True).strip()
        binaries.append({"name": name, "sha256": sha(data), "size": len(data),
                         "elf_machine": "x86-64", "file_identity": identity})
    evidence = {"schema_version": 1, "platform": "linux/amd64",
                "implementation_ref": SOURCE_REF, "source_tree": SOURCE_TREE,
                "rust_image": IMAGE, "cargo_args": CARGO_ARGS,
                "build_log_sha256": sha((output / "build.log").read_bytes()),
                "source_commit_raw_sha256": sha((output / "source-commit.raw").read_bytes()),
                "binaries": binaries, "network": "none", "cargo_offline": True,
                "execution_authorized": False,
                "target_mutation_performed": False}
    (output / "build.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print(json.dumps(evidence, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("--registry-cache", type=Path, required=True)
    parser.add_argument("--resume", action="store_true", help="verify exported source before reusing build cache")
    args = parser.parse_args()
    build(args.output.resolve(), args.registry_cache.resolve(), args.resume)
