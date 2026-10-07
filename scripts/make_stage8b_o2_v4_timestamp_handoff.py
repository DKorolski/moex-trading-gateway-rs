#!/usr/bin/env python3
"""Narrow V4 source-review ZIP, not an operational artifact or acceptance.

Run build from a clean candidate-authority checkout. Reuses existing Git-tree
reconstruction and member safety helpers; does not update authority or use VPS.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import zipfile

import make_stage8b_design_handoff as packing
import stage8b_p1e_i1a_handoff_safety_check as tree
from stage8b_p1f_alor_finam_source_correction_handoff_safety_check import validate_member_name

ROOT = Path(__file__).resolve().parents[1]
BASE = "b8732cc7ddff1322fd569add0d3438844cc090a1"
AUTHORITY = "docs/stage-8/gov-ci-1-authority.json"
PREFIX = "handoff-evidence/o2-v4-timestamp/"
MARKER = "handoff-commit.txt"
MANIFEST = PREFIX + "source-tree-manifest.json"
INDEX = PREFIX + "evidence-index.json"
RESULT = PREFIX + "result.json"
SOURCE_DELTA = {
    "crates/finam-gateway/src/stage8b_p1f_o2_materializer/observed/source.rs",
    "crates/finam-gateway/src/stage8b_p1f_o2_materializer/observed/tests.rs",
    "crates/finam-gateway/src/bin/stage8b-p1f-o2-materializer/observed_tests.rs",
    "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs",
    "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source/observed.rs",
    "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source/observed/tests.rs",
    "crates/runtime-durable-service/src/stage8b_p1f_guardian.rs",
    "docs/current-status.md", "docs/roadmap.md",
    "docs/stage-8/stage8b-p1f-o2-v4-timestamp-correction-2026-10-05.md",
    "scripts/make_stage8b_o2_v4_timestamp_handoff.py",
}
COMMANDS = [
    ("authority", ["python3", "scripts/current_tree_authority_check.py"]),
    ("authority-negative", ["python3", "scripts/current_tree_authority_negative_harness.py"]),
    ("fixtures", ["python3", "scripts/stage8b_sparse_m10_fixture_import.py", "--verify-only"]),
    ("gateway", ["cargo", "test", "--offline", "-p", "finam-gateway", "--lib", "stage8b_p1f_o2_materializer"]),
    ("materializer-cli", ["cargo", "test", "--offline", "-p", "finam-gateway", "--bin", "stage8b-p1f-o2-materializer"]),
    ("first-boot", ["cargo", "test", "--offline", "-p", "runtime-durable-service", "--lib", "stage8b_p1e_first_boot"]),
    ("guardian-staged", ["cargo", "test", "--offline", "-p", "runtime-durable-service", "--lib", "stage8b_p1f_", "--", "--test-threads=1"]),
    ("fmt", ["cargo", "fmt", "--all", "--", "--check"]),
    ("clippy", ["cargo", "clippy", "--offline", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"]),
    ("diff", ["git", "diff", "--check"]),
]
TEST_COUNTS = {"gateway": (27, 0), "materializer-cli": (14, 0),
               "first-boot": (47, 0), "guardian-staged": (64, 1)}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def commit_headers(raw):
    return raw.split(b"\n\n", 1)[0].decode().splitlines()


def authority_delta(before, after):
    fields = {"production_code_manifest", "governance_control_plane_manifest"}
    require({k: v for k, v in before.items() if k not in fields}
            == {k: v for k, v in after.items() if k not in fields}, "non-inventory authority delta")
    require(after["status"] == "independent_review_required", "acceptance not granted")
    for field in fields:
        require(set(before[field]["entries"]) == set(after[field]["entries"]), "authority path-set drift")


def verify(path):
    path = Path(path)
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [i.filename for i in infos]
        require(len(names) == len(set(names)), "duplicate members")
        require(archive.testzip() is None, "CRC failure")
        for info in infos:
            validate_member_name(info.filename)
            require(info.filename == str(Path(info.filename).as_posix()), "noncanonical path")
            require(info.external_attr >> 16 in {0o100644, 0o100755}, "non-regular mode")
        files = {i.filename: archive.read(i.filename) for i in infos}
    rows = [line.split("=", 1) for line in files[MARKER].decode().splitlines()]
    marker = dict(rows)
    require(len(rows) == len(marker) and set(marker) == {
        "source_ref", "source_short_ref", "source_tree", "source_parent", "source_implementation_ref",
        "baseline_ref", "branch", "archive_name",
    }, "marker shape")
    require(marker["archive_name"] == path.name and marker["baseline_ref"] == BASE, "archive name/base")
    require(marker["source_short_ref"] == marker["source_ref"][:7], "short ref")
    require(marker["source_parent"] == marker["source_implementation_ref"], "source lineage")
    manifest = json.loads(files[MANIFEST])
    entries = manifest["entries"]
    require(manifest["source_ref"] == marker["source_ref"] and manifest["entry_count"] == len(entries), "manifest identity/count")
    modes = {i.filename: f"{i.external_attr >> 16:06o}" for i in infos}
    tracked = {}
    for entry in entries:
        name = entry["path"]
        require(name not in tracked, "duplicate manifest path")
        raw = files[name]
        require(sha(raw) == entry["sha256"] and len(raw) == entry["size"], "source bytes: " + name)
        require(modes[name] == entry["mode"], "source mode: " + name)
        tracked[name] = raw
    require(tree.build_tree_oid(entries, tracked) == marker["source_tree"], "reconstructed Git tree")
    closure_raw = files[PREFIX + "authority-commit.raw"]
    require(tree.git_object_id("commit", closure_raw) == marker["source_ref"], "closure SHA")
    headers = commit_headers(closure_raw)
    require(headers[0] == "tree " + marker["source_tree"], "closure tree")
    require([h for h in headers if h.startswith("parent ")] == ["parent " + marker["source_parent"]], "closure parent")
    source_raw = files[PREFIX + "implementation-commit.raw"]
    require(tree.git_object_id("commit", source_raw) == marker["source_parent"], "implementation SHA")
    headers = commit_headers(source_raw)
    require([h for h in headers if h.startswith("parent ")] == ["parent " + BASE], "implementation parent")
    before = files[PREFIX + "implementation-authority.json"]
    parent_files = tracked | {AUTHORITY: before}
    require(headers[0] == "tree " + tree.build_tree_oid(entries, parent_files), "authority-only parent tree")
    authority_delta(json.loads(before), json.loads(files[AUTHORITY]))
    index = json.loads(files[INDEX])
    require(set(files) - set(tracked) == set(index) | {INDEX}, "generated inventory")
    require(all(sha(files[name]) == digest for name, digest in index.items()), "evidence digest")
    result = json.loads(files[RESULT])
    require(result["source_ref"] == marker["source_ref"] and result["source_tree"] == marker["source_tree"], "gate tree binding")
    require(result["source_inventory"] == {name: sha(raw) for name, raw in tracked.items()}, "gate source inventory")
    require(result["source_unchanged"] is True and result["independent_acceptance"] == "NOT_CLAIMED"
            and result["o2_status"] == "HOLD" and result["operational_activation"] is False
            and result["full_workspace_tests_claimed"] is False and result["github_ci_claimed"] is False, "scope claims")
    require([(r["id"], r["command"]) for r in result["commands"]] == COMMANDS, "gate commands")
    for record in result["commands"]:
        require(record["exit_code"] == 0, "failed gate")
        log = files[PREFIX + record["log"]]
        require(sha(log) == record["sha256"], "gate log digest")
        if record["id"] in TEST_COUNTS:
            counts = re.findall(rb"test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;", log)
            require(counts and tuple(map(int, counts[-1])) == TEST_COUNTS[record["id"]], "test count")
        if record["id"] == "authority-negative":
            require(b"current-tree-authority-negative: PASS cases=45/45" in log, "negative inventory")
    return dict(result="PASS", source_ref=marker["source_ref"], source_tree=marker["source_tree"],
                source_implementation_ref=marker["source_parent"], archive_members=len(files),
                tracked_members_verified=len(tracked), authority_only_diff=True, reconstructed_git_trees=2,
                duplicates=0, symlinks=0, unsafe_paths=0, independent_acceptance="NOT_CLAIMED",
                o2_status="HOLD", operational_activation=False, archive_sha256=sha(path.read_bytes()))


def build(output):
    require(not git("status", "--porcelain", "--untracked-files=all").strip(), "dirty worktree")
    ref, parent, oid = (git("rev-parse", s).decode().strip() for s in ["HEAD", "HEAD^", "HEAD^{tree}"])
    require(git("rev-parse", "HEAD^^").decode().strip() == BASE, "unexpected baseline")
    require(set(git("diff", "--name-only", parent, ref).decode().splitlines()) == {AUTHORITY}, "authority commit scope")
    require(set(git("diff", "--name-only", BASE, parent).decode().splitlines()) == SOURCE_DELTA, "source commit scope")
    manifest, entries = packing.source_manifest(ref)
    files = {e["path"]: git("show", f"{ref}:{e['path']}") for e in entries}
    inventory = {name: sha(raw) for name, raw in files.items()}
    require(all(sha((ROOT / n).read_bytes()) == h for n, h in inventory.items()), "checkout bytes")
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    name = f"moex-trading-project-{ref[:7]}-o2-v4-timestamp-review.zip"
    path = output / name
    require(not path.exists(), "archive exists; do not overwrite")
    log_dir = output / (name + ".logs")
    log_dir.mkdir(exist_ok=True)
    records, generated = [], {}
    env = {k: v for k, v in os.environ.items() if "REDIS" not in k}
    env.update(RUST_MIN_STACK="33554432", CARGO_NET_OFFLINE="true")
    for label, command in COMMANDS:
        print("RUN " + label, flush=True)
        process = subprocess.run(command, cwd=ROOT, env=env, stdout=subprocess.PIPE,
                                 stderr=subprocess.STDOUT, timeout=1200, check=False)
        raw = process.stdout
        (log_dir / (label + ".log")).write_bytes(raw)
        require(process.returncode == 0, f"gate {label} failed: {raw[-4000:].decode(errors='replace')}")
        print("PASS " + label, flush=True)
        log_name = "logs/" + label + ".log"
        generated[PREFIX + log_name] = raw
        records.append(dict(id=label, command=command, exit_code=process.returncode, log=log_name, sha256=sha(raw)))
    require(git("rev-parse", "HEAD").decode().strip() == ref
            and not git("status", "--porcelain", "--untracked-files=all").strip()
            and all(sha((ROOT / n).read_bytes()) == h for n, h in inventory.items()), "source changed during gates")
    generated[RESULT] = encoded(dict(source_ref=ref, source_tree=oid, source_inventory=inventory,
        source_unchanged=True, commands=records, independent_acceptance="NOT_CLAIMED", o2_status="HOLD",
        operational_activation=False, full_workspace_tests_claimed=False, github_ci_claimed=False))
    generated[MANIFEST] = manifest
    generated[PREFIX + "authority-commit.raw"] = git("cat-file", "commit", ref)
    generated[PREFIX + "implementation-commit.raw"] = git("cat-file", "commit", parent)
    generated[PREFIX + "implementation-authority.json"] = git("show", parent + ":" + AUTHORITY)
    marker = dict(source_ref=ref, source_short_ref=ref[:7], source_tree=oid, source_parent=parent,
                  source_implementation_ref=parent, baseline_ref=BASE,
                  branch="stage8b-o2-v4-timestamp-correction", archive_name=name)
    generated[MARKER] = "".join(f"{k}={v}\n" for k, v in marker.items()).encode()
    generated[INDEX] = encoded({n: sha(raw) for n, raw in generated.items()})
    require(not set(files) & set(generated), "generated/source collision")
    modes = {e["path"]: e["mode"] for e in entries}
    with zipfile.ZipFile(path, "x", zipfile.ZIP_DEFLATED) as archive:
        for n, raw in sorted((files | generated).items()):
            validate_member_name(n)
            archive.writestr(packing.zip_info(n, modes.get(n, "100644")), raw)
    safety = verify(path)
    Path(str(path) + ".safety.json").write_bytes(encoded(safety))
    Path(str(path) + ".sha256").write_text(safety["archive_sha256"] + "  " + name + "\n")
    print(path)
    print(json.dumps(safety, sort_keys=True))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["build", "verify"])
    parser.add_argument("path", type=Path)
    args = parser.parse_args()
    if args.action == "verify":
        print(json.dumps(verify(args.path), sort_keys=True))
    else:
        build(args.path)
