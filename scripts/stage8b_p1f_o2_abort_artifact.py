#!/usr/bin/env python3
"""Accepted terminal-abort source -> offline full ELF artifact; no deployment."""
import argparse
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import uuid
import zipfile

import stage8b_p1f_o2_build_linux as builder
import stage8b_p1f_o2_recovery_review as handoff
import stage8b_p1f_o2_v4_timestamp_artifact as inherited

ROOT = handoff.ROOT
ACCEPTED = "3bfd98ab6623c8b339eb56f4d9e28834640e1044"
TREE = "47954e14fdf6cbf574117e095188c716ad6746ba"
SOURCE_ZIP_SHA = "a3b50a83b6763b0f06b379aed4ae91b0049b8e0418ded9a7dad9cd78707be366"
REVIEW = "docs/stage-8/reviews/REVIEW_3bfd98a_O2_TERMINAL_ABORT_20261007_RU.txt"
REVIEW_SHA = "b169bf8927e23fe5766d26af459558ffb50a296615ea58b67857c66c83aeef13"
CUSTODY = "scripts/stage8b_p1f_o2_abort_linux_custody.sh"
AUTHORITY = "docs/stage-8/gov-ci-1-authority.json"
OLD_INSTALLATION = "395f9e3e987ce6f5b2c52b6083d1ebc79c71287df72096c6d313ee3e56da32f5"
ALLOWED = {AUTHORITY, REVIEW, CUSTODY, "docs/current-status.md", "docs/roadmap.md",
           "docs/stage-8/stage8b-p1f-o2-abort-artifact.md",
           "scripts/stage8b_p1f_o2_abort_artifact.py",
           "scripts/test_stage8b_p1f_o2_abort_artifact.py"}
HANDOFF_PREFIX = "handoff-evidence/"
require, sha = handoff.require, handoff.digest
handoff.BASE = ACCEPTED


def clean_ref():
    ref = handoff.clean_ref()
    require(handoff.git("rev-parse", ACCEPTED + "^{tree}").decode().strip() == TREE, "accepted tree")
    require(handoff.git("rev-parse", ref + "^").decode().strip() == ACCEPTED, "preparation parent")
    changed = set(handoff.git("diff", "--name-only", ACCEPTED, ref).decode().splitlines())
    require(changed == ALLOWED, "unexpected preparation scope")
    require(sha((ROOT / REVIEW).read_bytes()) == REVIEW_SHA, "source acceptance review")
    return ref


def configure(ref):
    builder.SOURCE_REF = ref
    builder.SOURCE_TREE = handoff.git("rev-parse", ref + "^{tree}").decode().strip()


def build(output, registry, resume=False):
    ref = clean_ref()
    configure(ref)
    builder.build(output, registry, resume)
    require(clean_ref() == ref, "tree changed during build")


def check_export(output, ref):
    files = {}
    for row in handoff.git("ls-tree", "-rz", ref).split(b"\0"):
        if row:
            meta, name = row.split(b"\t", 1)
            files[name.decode()] = meta.split()[2].decode()
    source = output / "source"
    require({p.relative_to(source).as_posix() for p in source.rglob("*") if not p.is_dir()} == set(files),
            "source export inventory")
    for name, oid in files.items():
        p = source / name
        require(not p.is_symlink() and p.read_bytes() == handoff.git("cat-file", "blob", oid), "export drift: " + name)


def validate_build(info, ref, tree, raw_commit, log, binaries):
    require(info["implementation_ref"] == ref and info["source_tree"] == tree, "build source")
    require(handoff.git_objects.git_object_id("commit", raw_commit) == ref
            and raw_commit.splitlines()[0] == f"tree {tree}".encode(), "build commit")
    require(info["rust_image"] == builder.IMAGE and info["cargo_args"] == builder.CARGO_ARGS
            and info["platform"] == "linux/amd64" and info["network"] == "none"
            and info["cargo_offline"] is True, "build recipe")
    require(info["execution_authorized"] is False and info["target_mutation_performed"] is False,
            "operational build claim")
    require(info["build_log_sha256"] == sha(log) and info["source_commit_raw_sha256"] == sha(raw_commit), "build evidence")
    require(b"Finished `release`" in log and b"COMMAND cargo build --locked --release" in log, "build completion")
    require(len(info["binaries"]) == 3 and {b["name"] for b in info["binaries"]} == set(builder.BINS), "ELF inventory")
    for b in info["binaries"]:
        raw = binaries[b["name"]]
        require(sha(raw) == b["sha256"] and len(raw) == b["size"] and raw[:6] == b"\x7fELF\x02\x01"
                and int.from_bytes(raw[18:20], "little") == 62 and b["elf_machine"] == "x86-64", "ELF identity")


def require_tests(raw, passed, ignored=0):
    counts = re.findall(rb"test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;", raw)
    require(bool(counts) and tuple(map(int, counts[-1])) == (passed, ignored), "Linux test selection")


def validate_abort_log(raw):
    require_tests(raw, 3)
    require(b"QUALIFICATION uid=0 gid=987 umask=0077" in raw, "actual root/service context")
    negatives = re.findall(rb"PASS abort nonmutating negative (\d+)", raw)
    frontiers = re.findall(rb"PASS abort reopen frontier (\d+): ACTIVE/1/11 -> EXPIRED/1/12", raw)
    require(list(map(int, negatives)) == list(range(28)), "negative inventory")
    require(list(map(int, frontiers)) == [0, 2, 40, 41, 42, 43, 44, 45, 46, 5, 47, 52, 48, 51, 49], "reopen inventory")
    require(b"PASS Linux root:987 abort archive/consume/reopen under umask0077" in raw, "abort completion")


def mount(src, dst, writable=False):
    return ["--mount", f"type=bind,src={src},dst={dst}" + ("" if writable else ",readonly")]


def run(output, label, args, expected=0, timeout=1200):
    name = None
    if args[:2] == ["docker", "run"]:
        name = "o2-abort-qualification-" + uuid.uuid4().hex[:12]
        args = args[:2] + ["--name", name] + args[2:]
    print("RUN " + label, flush=True)
    log = output / (label + ".log")
    expired = False
    with log.open("xb") as stream:
        try:
            result = subprocess.run(args, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT,
                                    env=dict(os.environ, PYTHONDONTWRITEBYTECODE="1"), timeout=timeout)
            code = result.returncode
        except subprocess.TimeoutExpired:
            code, expired = 124, True
            if name:
                subprocess.run(["docker", "rm", "-f", name], capture_output=True, timeout=30, check=True)
    record = dict(label=label, command=args, exit_code=code, expected_exit_code=expected,
                  timeout=expired, log=log.name, sha256=sha(log.read_bytes()))
    with (output / (label + ".json")).open("x") as f:
        json.dump(record, f, indent=2)
    require(code == expected and not expired, "FAIL " + label + "; raw evidence retained")
    print("PASS " + label, flush=True)
    return record


def qualify(build_dir, output, accepted_zip):
    ref = clean_ref()
    configure(ref)
    require(sha(accepted_zip.read_bytes()) == SOURCE_ZIP_SHA, "accepted source ZIP")
    info = json.loads((build_dir / "build.json").read_bytes())
    binaries = {n: (build_dir / "target/release" / n).read_bytes() for n in builder.BINS}
    validate_build(info, ref, builder.SOURCE_TREE, (build_dir / "source-commit.raw").read_bytes(),
                   (build_dir / "build.log").read_bytes(), binaries)
    check_export(build_dir, ref)
    output.mkdir(parents=True, exist_ok=False)
    records = []
    for label, command in (
        ("authority", [sys.executable, "-B", "scripts/current_tree_authority_check.py"]),
        ("authority-negative", [sys.executable, "-B", "scripts/current_tree_authority_negative_harness.py"]),
        ("package-tests", [sys.executable, "-B", "-m", "unittest", "discover", "-s", "scripts",
                           "-p", "test_stage8b_p1f_o2_abort_artifact.py"]),
        ("diff", ["git", "diff", "--check", ACCEPTED]),
    ):
        records.append(run(output, label, command))
    # Reuse unchanged V4 fractional, sparse/no-riskgate release-library and
    # exact-three-ELF smoke. No historical packager pins/checks are modified.
    inherited.source.SOURCE_REF, inherited.source.SOURCE_TREE = ref, builder.SOURCE_TREE
    inherited.qualify(build_dir, output / "inherited")
    docker = ["docker", "run", "--rm", "--network", "none", "--platform", "linux/amd64",
              "--user", "0:987", "--ulimit", "core=0",
              "--tmpfs", "/src/crates/runtime-durable-service/target:rw,nosuid,nodev,mode=0755",
              *mount(build_dir / "source", "/src"), *mount(build_dir / "target", "/target", True),
              *mount(build_dir / "cargo-home", "/cargo-home", True), "-w", "/src",
              "-e", "CARGO_HOME=/cargo-home", "-e", "CARGO_TARGET_DIR=/target",
              "-e", "CARGO_NET_OFFLINE=true", "-e", "CARGO_INCREMENTAL=0",
              "-e", "CARGO_TERM_COLOR=never", "-e", "RUST_MIN_STACK=33554432"]
    for label in ("abort", "prevention", "custody"):
        records.append(run(output, label, docker + [builder.IMAGE, "bash", "/src/" + CUSTODY, label], timeout=2400))
    validate_abort_log((output / "abort.log").read_bytes())
    require_tests((output / "prevention.log").read_bytes(), 1)
    require(b"PASS exact materialization/temp/receipt replay under umask 0077" in
            (output / "prevention.log").read_bytes(), "prevention witness")
    custody = (output / "custody.log").read_bytes()
    require_tests(custody, 1)
    require(custody.count(b"PASS service custody ") == 6 and b"PASS Linux root:987 service-65534:987 custody" in custody,
            "service custody witness")
    cli = ["docker", "run", "--rm", "--network", "none", "--platform", "linux/amd64",
           "--user", "65534:987", "--cap-drop=ALL", "--read-only", "--security-opt=no-new-privileges",
           *mount(build_dir / "target/release" / builder.BINS[1], "/operator"),
           builder.IMAGE, "/operator", "terminal-abort-oct7-fixed"]
    records.append(run(output, "cli-root-required", cli, expected=70, timeout=60))
    require(b"O2 runner requires the fixed root identity" in (output / "cli-root-required.log").read_bytes(), "nonroot CLI boundary")
    require({n: sha((build_dir / "target/release" / n).read_bytes()) for n in builder.BINS}
            == {n: sha(raw) for n, raw in binaries.items()}, "qualification changed ELF")
    check_export(build_dir, ref)
    require(clean_ref() == ref, "tree changed during qualification")
    summary = dict(stage="O2 terminal-abort authority/full artifact qualification", source_ref=ref,
        source_tree=builder.SOURCE_TREE, baseline_ref=ACCEPTED, source_gate_passed=True,
        status="ARTIFACT_AND_AUTHORITY_REVIEW_PENDING", production_delta_from_accepted=[],
        records=records, old_installation_sha256=OLD_INSTALLATION, intended_cli="terminal-abort-oct7-fixed",
        authority_local_passed=True, merge_ready=False, execution_authorized=False,
        vps_contacted=False, installed_files_changed=False, o2_status="HOLD",
        limitations=["Linux/amd64 containers on arm64 Docker Desktop: emulated CPU, real Linux kernel/custody",
                     "fixture phase/pins, controlled reopen; not production fixed-command execution or SIGKILL",
                     "fresh GitHub CI not run; no push/merge", "no operational systemd/FINAM/Redis qualification"])
    # All extras are hashed and retained. Do not embed build caches or secrets.
    extras = {"build.json": (build_dir / "build.json").read_bytes(),
              "build.log": (build_dir / "build.log").read_bytes(),
              "accepted-source.zip": accepted_zip.read_bytes()}
    extras.update({"payload/" + n: raw for n, raw in binaries.items()})
    for name, raw in extras.items():
        p = output / name
        p.parent.mkdir(parents=True, exist_ok=True)
        with p.open("xb") as f:
            f.write(raw)
    summary["logs_sha256"] = {p.relative_to(output).as_posix(): sha(p.read_bytes())
        for p in sorted(output.rglob("*")) if p.is_file()}
    with (output / "summary.json").open("x") as f:
        json.dump(summary, f, indent=2)
    print("PASS artifact qualification; review pending; O2 HOLD", flush=True)


def check(path):
    result = handoff.check_archive(path)
    with zipfile.ZipFile(path) as z:
        read = lambda n: z.read(HANDOFF_PREFIX + n)
        summary = json.loads(read("summary.json"))
        require(summary["baseline_ref"] == ACCEPTED and summary["old_installation_sha256"] == OLD_INSTALLATION
                and summary["production_delta_from_accepted"] == [], "accepted source/old installation binding")
        require(summary["status"] == "ARTIFACT_AND_AUTHORITY_REVIEW_PENDING" and summary["o2_status"] == "HOLD"
                and summary["intended_cli"] == "terminal-abort-oct7-fixed", "scope")
        for name in ("merge_ready", "execution_authorized", "vps_contacted", "installed_files_changed"):
            require(summary[name] is False, "operational claim")
        require(sha(z.read(REVIEW)) == REVIEW_SHA and sha(read("accepted-source.zip")) == SOURCE_ZIP_SHA,
                "acceptance evidence")
        manifest = json.loads(read("source-tree-manifest.json"))
        with zipfile.ZipFile(io.BytesIO(read("accepted-source.zip"))) as before:
            old = json.loads(before.read(HANDOFF_PREFIX + "source-tree-manifest.json"))
            names = {e["path"]: e for e in manifest["entries"]}
            old_names = {e["path"]: e for e in old["entries"]}
            delta = {n for n in names.keys() | old_names.keys() if names.get(n) != old_names.get(n)}
            require(delta == ALLOWED, "protected source drift")
            old_authority = json.loads(before.read(AUTHORITY))
            new_authority = json.loads(z.read(AUTHORITY))
            fields = {"production_code_manifest", "governance_control_plane_manifest"}
            require({k: v for k, v in old_authority.items() if k not in fields}
                    == {k: v for k, v in new_authority.items() if k not in fields}, "authority contract drift")
        validate_build(json.loads(read("build.json")), summary["source_ref"], summary["source_tree"],
                       read("source-commit.raw"), read("build.log"), {n: read("payload/" + n) for n in builder.BINS})
        require(f"parent {ACCEPTED}".encode() in read("source-commit.raw").splitlines(), "preparation parent")
        validate_abort_log(read("abort.log"))
        require_tests(read("prevention.log"), 1)
        require_tests(read("custody.log"), 1)
        require(read("custody.log").count(b"PASS service custody ") == 6, "custody negative count")
        require(b"O2 runner requires the fixed root identity" in read("cli-root-required.log"), "ELF root boundary")
        require(b"current-tree-authority-check: PASS" in read("authority.log")
                and b"PASS" in read("authority-negative.log"), "authority gate")
        require({r["label"] for r in summary["records"]} ==
                {"authority", "authority-negative", "package-tests", "diff", "abort", "prevention", "custody", "cli-root-required"},
                "qualification inventory")
        for record in summary["records"]:
            require(record["exit_code"] == (70 if record["label"] == "cli-root-required" else 0)
                    and record["timeout"] is False and sha(read(record["log"])) == record["sha256"], "command evidence")
            command = record["command"]
            if record["label"] in {"abort", "prevention", "custody", "cli-root-required"}:
                require("--network" in command and command[command.index("--network") + 1] == "none"
                        and "--privileged" not in command and not any("docker.sock" in a for a in command)
                        and builder.IMAGE in command, "Linux isolation")
        inherited_result = json.loads(read("inherited/timestamp-tests.json"))
        require(inherited_result["result"] == "PASS" and inherited_result["compiled_ref"] == summary["source_ref"]
                and inherited_result["finam_contact"] is False and inherited_result["redis_contact"] is False, "inherited result")
        hashes = {b["name"]: b["sha256"] for b in json.loads(read("build.json"))["binaries"]}
        require(inherited_result["binaries_before"] == inherited_result["binaries_after"] == hashes, "inherited ELF continuity")
        require(len(inherited_result["records"]) == len(inherited.TESTS), "inherited tests inventory")
        for r, (label, args, count, tests) in zip(inherited_result["records"], inherited.TESTS):
            require(r["id"] == label and r["exit_code"] == 0 and r["timeout"] is False
                    and sha(read("inherited/" + r["log"])) == r["sha256"], "inherited test evidence")
            inherited.validate_log(read("inherited/" + r["log"]), count, tests)
        require(all(m.encode() in read("inherited/smoke.log") for m in inherited.base.SMOKE_MARKERS), "three ELF smoke")
        for name, digest in inherited.base.FIXTURE_SHA.items():
            require(sha(read("inherited/" + name)) == digest, "fixed release probe: " + name)
    return result | dict(full_three_elf_artifact=True, accepted_production_unchanged=True, operational_execution=False)


def package(output):
    ref = clean_ref()
    summary = json.loads((output / "summary.json").read_text())
    require(summary["source_ref"] == ref, "evidence/HEAD mismatch")
    manifest, entries = handoff.packaging.source_manifest(ref)
    path = ROOT / "reports/handoff" / f"moex-trading-project-{ref[:7]}-o2-abort-artifact-review.zip"
    path.parent.mkdir(parents=True, exist_ok=True)
    extra = {"handoff-commit.txt": handoff.marker(ref, summary["source_tree"], path.name),
             HANDOFF_PREFIX + "source-tree-manifest.json": manifest,
             HANDOFF_PREFIX + "source-commit.raw": handoff.git("cat-file", "commit", ref),
             HANDOFF_PREFIX + "summary.json": (output / "summary.json").read_bytes()}
    for name, digest in summary["logs_sha256"].items():
        raw = (output / name).read_bytes()
        require(sha(raw) == digest, "retained evidence drift")
        extra[HANDOFF_PREFIX + name] = raw
    with zipfile.ZipFile(path, "x", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for entry in entries:
            require(entry["path"] not in extra, "source/evidence collision")
            z.writestr(handoff.packaging.zip_info(entry["path"], entry["mode"]), handoff.git("show", ref + ":" + entry["path"]))
        for name, raw in extra.items():
            mode = "100755" if name.startswith(HANDOFF_PREFIX + "payload/") else "100644"
            z.writestr(handoff.packaging.zip_info(name, mode), raw)
    result = check(path)
    require(clean_ref() == ref, "tree changed during packaging")
    Path(str(path) + ".sha256").write_text(result["archive_sha256"] + "  " + path.name + "\n")
    Path(str(path) + ".safety.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(dict(result, archive=str(path)), indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("build", "qualify", "package", "check"))
    parser.add_argument("path", type=Path)
    parser.add_argument("--registry-cache", type=Path)
    parser.add_argument("--build", type=Path)
    parser.add_argument("--accepted-source-zip", type=Path)
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    if args.action == "build":
        require(args.registry_cache is not None, "registry cache required")
        build(args.path.resolve(), args.registry_cache.resolve(), args.resume)
    elif args.action == "qualify":
        require(args.build is not None and args.accepted_source_zip is not None, "build and accepted source ZIP required")
        qualify(args.build.resolve(), args.path.resolve(), args.accepted_source_zip.resolve())
    elif args.action == "package":
        package(args.path.resolve())
    else:
        print(json.dumps(check(args.path.resolve()), indent=2))
