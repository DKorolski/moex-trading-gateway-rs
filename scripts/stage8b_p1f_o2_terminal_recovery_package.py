#!/usr/bin/env python3
"""Prepare one offline recovery ELF and immutable review ZIP. No deployment API."""
from __future__ import annotations

import argparse
import io
import json
import os
from pathlib import Path
import subprocess
import zipfile

import stage8b_p1f_o2_build_linux as builder
import stage8b_p1f_o2_recovery_review as handoff

ROOT = handoff.ROOT
ACCEPTED = "590304af44830197704503c8ebc67329693ac75b"
ACCEPTED_TREE = "133a375e30491a0897dbc85d2d5a50ca9061fee5"
SOURCE_ZIP_SHA = "ca0fe37980d5ea69d3cd95fe2951664b1441f2e8344cd005b325183afd04c485"
REVIEW_SHA = "4ff21077154d17b2f911fa2919dba9f9a2adfc8b6ba48f6979eda0ed51da26fe"
OPERATOR = "stage8b-p1f-o2-operator"
CARGO_ARGS = ["build", "--locked", "--release", "-p", "runtime-durable-service", "--bin", OPERATOR]
OLD_MANIFEST = "b8ce96287e82a1268a3617c3e29bf8ec0167014bd216c544011c07b709e3b3b8"
OLD_INSTALLATION = "316c2376cf4d02a7f0ee3837e96d93bbf2cb1b2b8e3aabe10205f783f088aad8"
handoff.BASE = ACCEPTED
require = handoff.require
sha = handoff.digest


def protected(path: str) -> bool:
    return path in {"Cargo.toml", "Cargo.lock"} or path.startswith(("crates/", "deploy/", ".github/", "config/"))


def clean_preparation_ref() -> str:
    ref = handoff.clean_ref()
    require(handoff.git("rev-parse", ACCEPTED + "^{tree}").decode().strip() == ACCEPTED_TREE,
            "accepted source tree mismatch")
    subprocess.run(["git", "merge-base", "--is-ancestor", ACCEPTED, ref], cwd=ROOT, check=True)
    changed = handoff.git("diff", "--name-only", ACCEPTED, ref).decode().splitlines()
    require(changed and all(not protected(p) for p in changed), "production/deploy/CI delta or missing successor")
    return ref


def configure_builder(ref: str) -> None:
    # The historical builder and artifact pins stay unchanged. Its Git export,
    # locked/offline build and raw commit/log evidence are reused verbatim.
    builder.SOURCE_REF = ref
    builder.SOURCE_TREE = handoff.git("rev-parse", ref + "^{tree}").decode().strip()
    builder.BINS = (OPERATOR,)
    builder.CARGO_ARGS = CARGO_ARGS


def validate_build(build: dict, ref: str, tree: str, binary: bytes, log: bytes, raw_commit: bytes) -> None:
    require(build["implementation_ref"] == ref and build["source_tree"] == tree, "build source mismatch")
    require(handoff.git_objects.git_object_id("commit", raw_commit) == ref
            and raw_commit.splitlines()[0] == f"tree {tree}".encode(), "raw build commit mismatch")
    require(build["rust_image"] == builder.IMAGE and build["platform"] == "linux/amd64", "build image/platform mismatch")
    require(build["cargo_args"] == CARGO_ARGS, "build features/command mismatch")
    require(build["network"] == "none" and build["cargo_offline"] is True, "network scope mismatch")
    require(build["execution_authorized"] is False and build["target_mutation_performed"] is False,
            "build operational claim")
    require(build["build_log_sha256"] == sha(log) and build["source_commit_raw_sha256"] == sha(raw_commit),
            "build evidence hash mismatch")
    require(len(build["binaries"]) == 1, "recovery binary inventory mismatch")
    entry = build["binaries"][0]
    require(entry["name"] == OPERATOR and entry["sha256"] == sha(binary) and entry["size"] == len(binary),
            "recovery binary binding mismatch")
    require(binary[:6] == b"\x7fELF\x02\x01" and int.from_bytes(binary[18:20], "little") == 62
            and entry["elf_machine"] == "x86-64", "not x86-64 ELF")
    require(b"COMMAND cargo build --locked --release -p runtime-durable-service --bin stage8b-p1f-o2-operator\n" in log
            and b"Finished `release`" in log, "release build marker missing")


def build(output: Path, registry: Path) -> None:
    ref = clean_preparation_ref()
    configure_builder(ref)
    builder.build(output, registry)
    require(clean_preparation_ref() == ref, "source changed during build")


def run_logged(output: Path, name: str, command: list[str], expected: int = 0) -> bytes:
    print("RUN", name, flush=True)
    with (output / name).open("xb") as stream:
        result = subprocess.run(command, cwd=ROOT,
                                env=dict(os.environ, CARGO_TERM_COLOR="never", RUST_MIN_STACK="33554432"),
                                stdout=stream, stderr=subprocess.STDOUT)
    require(result.returncode == expected, f"FAIL {name}, exit {result.returncode}; log retained")
    return (output / name).read_bytes()


def gate(output: Path, review: Path, accepted_zip: Path) -> None:
    ref = clean_preparation_ref()
    tree = handoff.git("rev-parse", ref + "^{tree}").decode().strip()
    require(sha(review.read_bytes()) == REVIEW_SHA, "source acceptance review mismatch")
    require(sha(accepted_zip.read_bytes()) == SOURCE_ZIP_SHA, "accepted source ZIP mismatch")
    build_info = json.loads((output / "build.json").read_text())
    binary = (output / "target/release" / OPERATOR).read_bytes()
    validate_build(build_info, ref, tree, binary, (output / "build.log").read_bytes(),
                   (output / "source-commit.raw").read_bytes())
    # Tests consume the same exact Git-exported inputs, not the mutable checkout.
    expected = {}
    for row in handoff.git("ls-tree", "-rz", ref).split(b"\0"):
        if row:
            meta, name = row.split(b"\t")
            expected[name.decode()] = meta.split()[2].decode()
    source = output / "source"
    require({p.relative_to(source).as_posix() for p in source.rglob("*") if not p.is_dir()} == set(expected),
            "exported source inventory mismatch")
    for name, oid in expected.items():
        p = source / name
        require(not p.is_symlink() and p.read_bytes() == handoff.git("cat-file", "blob", oid), "exported source drift")
    for name, command in (
        ("authority.txt", ["python3", "scripts/current_tree_authority_check.py"]),
        ("authority-negative.txt", ["python3", "scripts/current_tree_authority_negative_harness.py"]),
        ("package-tests.txt", ["python3", "-m", "unittest", "discover", "-s", "scripts", "-p", "test_stage8b_p1f_o2_terminal_recovery_package.py"]),
        ("diff.txt", ["git", "diff", "--check", ACCEPTED]),
    ):
        run_logged(output, name, command)
    docker = ["docker", "run", "--rm", "--network", "none", "--platform", "linux/amd64",
              "--mount", f"type=bind,src={source},dst=/src,readonly",
              "--mount", f"type=bind,src={output / 'target'},dst=/target",
              "--mount", f"type=bind,src={output / 'cargo-home'},dst=/cargo-home",
              "-w", "/src", "-e", "CARGO_HOME=/cargo-home", "-e", "CARGO_TARGET_DIR=/target",
              "-e", "CARGO_TERM_COLOR=never", "-e", "CARGO_INCREMENTAL=0", "-e", "CARGO_NET_OFFLINE=true",
              "-e", "RUST_MIN_STACK=33554432", builder.IMAGE]
    tests = run_logged(output, "linux-release-o2.txt", [*docker, "cargo", "test", "--locked", "--release",
                       "-p", "runtime-durable-service", "--lib", "o2_", "--", "--test-threads=1"])
    require(b"16 passed; 0 failed" in tests and b"pending_terminal_durable_frontiers" in tests,
            "Linux cleanup regression selection mismatch")
    cli = run_logged(output, "linux-elf-cli.txt", ["docker", "run", "--rm", "--network", "none",
                     "--platform", "linux/amd64", "--user", "65534:65534", "--cap-drop=ALL",
                     "--read-only", "--security-opt=no-new-privileges",
                     "--mount", f"type=bind,src={output / 'target/release' / OPERATOR},dst=/operator,readonly",
                     builder.IMAGE, "/operator"], expected=70)
    require(b"usage: stage8b-p1f-o2-operator" in cli, "ELF CLI did not execute")
    for name, data in ((OPERATOR, binary), ("source-acceptance.md", review.read_bytes()),
                       ("accepted-source.zip", accepted_zip.read_bytes())):
        with (output / name).open("xb") as stream:
            stream.write(data)
    names = ["build.json", "build.log", "source-commit.raw", OPERATOR, "source-acceptance.md", "accepted-source.zip",
             "authority.txt", "authority-negative.txt", "package-tests.txt", "diff.txt", "linux-release-o2.txt", "linux-elf-cli.txt"]
    summary = {
        "stage": "Stage 8B-P1-f O2 old-phase recovery preparation",
        "status": "AUTHORITY_AND_ARTIFACT_REVIEW_PENDING",
        "source_ref": ref, "source_tree": tree, "baseline_ref": ACCEPTED,
        "source_gate_passed": True, "source_gate_scope": "preparation gates; inherited Rust source acceptance, not full CI",
        "production_delta_from_accepted": [], "authority_local_passed": True,
        "github_ci_status": "REQUIRED_SEPARATE_EXACT_HEAD_OBSERVATION",
        "logs_sha256": {name: sha((output / name).read_bytes()) for name in names},
        "old_installation_sha256": OLD_INSTALLATION, "old_manifest_sha256": OLD_MANIFEST,
        "intended_cli": "cleanup-fixed", "delivery_mode": "separate-root-only-staging-no-install-overwrite",
        "merge_ready": False, "execution_authorized": False, "vps_contacted": False,
        "finam_contacted": False, "operational_redis_activated": False,
        "installed_manifest_changed": False, "systemd_manager_tested": False,
        "limitations": ["native Linux real-store tests substitute systemd observations and clock",
                        "ELF CLI test is not target installation or cleanup execution",
                        "CI and independent artifact acceptance precede operational permission",
                        "materializer lock-mount qualification deferred to next full O2 artifact",
                        "known FINAM all-features baseline failure unchanged; default-feature recovery build"],
    }
    require(clean_preparation_ref() == ref, "source changed during gate")
    with (output / "summary.json").open("x") as stream:
        stream.write(json.dumps(summary, indent=2) + "\n")
    print("PASS preparation gates; execution=false merge_ready=false", flush=True)


def validate_package(path: Path) -> dict:
    result = handoff.check_archive(path)
    with zipfile.ZipFile(path) as archive:
        read = lambda name: archive.read("handoff-evidence/" + name)
        summary = json.loads(read("summary.json"))
        require(summary["baseline_ref"] == ACCEPTED and summary["production_delta_from_accepted"] == [], "source lineage mismatch")
        require(summary["old_manifest_sha256"] == OLD_MANIFEST and summary["old_installation_sha256"] == OLD_INSTALLATION,
                "old phase/installation binding mismatch")
        require(summary["intended_cli"] == "cleanup-fixed" and summary["delivery_mode"] == "separate-root-only-staging-no-install-overwrite",
                "delivery scope mismatch")
        for key in ("execution_authorized", "vps_contacted", "finam_contacted", "operational_redis_activated",
                    "installed_manifest_changed", "systemd_manager_tested", "merge_ready"):
            require(summary[key] is False, "operational claim: " + key)
        require(sha(read("source-acceptance.md")) == REVIEW_SHA and sha(read("accepted-source.zip")) == SOURCE_ZIP_SHA,
                "acceptance evidence mismatch")
        with zipfile.ZipFile(io.BytesIO(read("accepted-source.zip"))) as accepted:
            names = {p for p in archive.namelist() if protected(p)}
            require(names == {p for p in accepted.namelist() if protected(p)}, "protected inventory delta")
            require(all(archive.read(p) == accepted.read(p) for p in names), "protected bytes delta")
        validate_build(json.loads(read("build.json")), summary["source_ref"], summary["source_tree"],
                       read(OPERATOR), read("build.log"), read("source-commit.raw"))
        require(b"16 passed; 0 failed" in read("linux-release-o2.txt") and b"usage: stage8b-p1f-o2-operator" in read("linux-elf-cli.txt"),
                "Linux witness markers missing")
    return result | {"recovery_artifact_binding": True, "protected_bytes_match_accepted_source": True,
                     "execution_authorized": False, "systemd_manager_tested": False}


def package(output: Path) -> None:
    # Reuse raw Git commit/tree reconstruction and strict exact-member checks.
    original = handoff.check_archive
    def checked(path: Path) -> dict:
        handoff.check_archive = original
        try:
            return validate_package(path)
        finally:
            handoff.check_archive = checked
    handoff.check_archive = checked
    try:
        handoff.package(output)
    finally:
        handoff.check_archive = original


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["build", "gate", "package", "check"])
    parser.add_argument("path", type=Path)
    parser.add_argument("--registry-cache", type=Path)
    parser.add_argument("--review", type=Path)
    parser.add_argument("--accepted-source-zip", type=Path)
    args = parser.parse_args()
    if args.action == "build":
        require(args.registry_cache is not None, "--registry-cache required")
        build(args.path.resolve(), args.registry_cache.resolve())
    elif args.action == "gate":
        require(args.review is not None and args.accepted_source_zip is not None, "review and accepted source ZIP required")
        gate(args.path.resolve(), args.review.resolve(), args.accepted_source_zip.resolve())
    elif args.action == "package":
        package(args.path.resolve())
    else:
        print(json.dumps(validate_package(args.path), indent=2))
