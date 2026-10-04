#!/usr/bin/env python3
"""Commit-bound sparse-M10 source review package; no deployment or pin refresh.

Build after a clean local commit and the full local gate. Verify independently
with --verify ARCHIVE; no Git database is needed for archive verification.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import zipfile

import make_stage8b_design_handoff as packing
import stage8b_p1e_i1a_handoff_safety_check as safety
from stage8b_p1f_alor_finam_source_correction_handoff_safety_check import validate_member_name

ROOT = Path(__file__).resolve().parents[1]
PREFIX = "handoff-evidence/sparse-m10/"
MANIFEST = PREFIX + "source-tree-manifest.json"
COMMIT = PREFIX + "source-commit.txt"
INDEX = PREFIX + "evidence-index.json"
GATE = PREFIX + "progress-evidence.json"
QUALIFICATION = PREFIX + "packaging-qualification.json"
PACKAGER = "scripts/make_stage8b_sparse_m10_handoff.py"
CLIPPY = ["cargo", "clippy", "--offline", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"]
MARKER = "handoff-commit.txt"
BRANCH = "stage8b-sparse-m10-correction"
BASE = "eb1a974d884196f9620f3db2cced3a4ad0991921"


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def qualification_delta(gate, files):
    tested = gate["source_inventory"]
    current = {name: sha(raw) for name, raw in files.items()}
    require(set(tested) == set(current), "gate/source inventory differs")
    delta = {name: {"tested_sha256": tested[name], "packaged_sha256": digest}
             for name, digest in current.items() if tested[name] != digest}
    # The exporter is never executed by a Rust test or by the source gate.
    # Permit only its packaging repair, not production, harness or doc drift.
    require(set(delta) <= {PACKAGER}, "tested source changed outside the packager")
    return delta


def verify(path):
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
        marker_lines = [line.split("=", 1) for line in files[MARKER].decode().splitlines()]
        marker = dict(marker_lines)
        require(len(marker_lines) == len(marker) and set(marker) == {
            "source_commit", "source_ref", "source_tree", "source_parent", "branch", "archive_name",
        }, "marker shape")
        require(marker["archive_name"] == Path(path).name, "archive name")
        require(marker["branch"] == BRANCH and marker["source_parent"] == BASE, "lineage")
        require(marker["source_commit"] == marker["source_ref"][:7], "short SHA")
        require(safety.git_object_id("commit", files[COMMIT]) == marker["source_ref"], "commit SHA")
        headers = files[COMMIT].split(b"\n\n", 1)[0].decode().splitlines()
        require(headers[0] == "tree " + marker["source_tree"], "commit tree")
        require([h for h in headers if h.startswith("parent ")] == ["parent " + BASE], "commit parent")
        manifest = json.loads(files[MANIFEST])
        require(manifest["source_ref"] == marker["source_ref"], "manifest source")
        entries = manifest["entries"]
        require(manifest["entry_count"] == len(entries), "manifest count")
        modes = {i.filename: f"{i.external_attr >> 16:06o}" for i in infos}
        tracked = {}
        for entry in entries:
            name = entry["path"]
            require(name not in tracked, "duplicate manifest path")
            raw = files[name]
            require(sha(raw) == entry["sha256"] and len(raw) == entry["size"], "source bytes: " + name)
            require(modes[name] == entry["mode"], "source mode: " + name)
            tracked[name] = raw
        require(safety.build_tree_oid(entries, tracked) == marker["source_tree"], "reconstructed Git tree")
        index = json.loads(files[INDEX])
        require(set(files) - set(tracked) == set(index) | {INDEX}, "generated inventory")
        require(all(sha(files[name]) == digest for name, digest in index.items()), "evidence digest")
        gate = json.loads(files[GATE])
        require(gate["scope"] == "full" and gate["source_review_gate_passed"] is True
                and gate["source_unchanged_during_check"] is True, "full gate not passed")
        qualification = json.loads(files[QUALIFICATION])
        require(qualification["source_ref"] == marker["source_ref"], "qualification commit")
        require(qualification["packaging_only_delta"] == qualification_delta(gate, tracked), "qualification delta")
        require(qualification["canonical_clippy_command"] == CLIPPY
                and qualification["canonical_clippy_exit_code"] == 0, "canonical clippy")
        require(qualification["canonical_clippy_sha256"] == sha(files[PREFIX + "canonical-clippy.txt"]), "clippy log hash")
        require(gate["operational_activation"] is False and gate["o2_verdict"] == "HOLD"
                and gate["independent_acceptance"] == "NOT_CLAIMED", "review scope")
        for record in gate["commands"]:
            require(record["exit_code"] == 0, "failed source gate")
            require(sha(files[PREFIX + record["log"]]) == record["sha256"], "gate log")
        for key in ["replay", "linked"]:
            require(sha(files[PREFIX + gate[key]["path"]]) == gate[key]["sha256"], "replay/linked hash")
        require(sha(files[PREFIX + "current-tree-authority.txt"]) == gate["authority_log_sha256"], "authority log")
        return {"result": "PASS", "source_ref": marker["source_ref"], "source_tree": marker["source_tree"],
                "archive_members": len(files), "tracked_members_verified": len(tracked),
                "duplicates": 0, "symlinks": 0, "unsafe_paths": 0,
                "source_gate_passed": True, "governance_rebind": "PENDING_SOURCE_ACCEPTANCE",
                "packaging_only_delta": qualification["packaging_only_delta"],
                "independent_acceptance": "NOT_CLAIMED", "operational_activation": False,
                "archive_sha256": sha(Path(path).read_bytes())}


def build():
    require(not git("status", "--porcelain", "--untracked-files=all").strip(), "dirty worktree")
    branch = git("branch", "--show-current").decode().strip()
    require(branch == BRANCH, "wrong branch")
    ref = git("rev-parse", "HEAD").decode().strip()
    tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    parent = git("rev-parse", "HEAD^").decode().strip()
    require(parent == BASE, "unexpected base")
    out = ROOT / "reports/handoff"
    out.mkdir(parents=True, exist_ok=True)
    path = out / f"moex-trading-project-{ref[:7]}-sparse-m10-source-review.zip"
    require(not path.exists(), "archive already exists; do not overwrite evidence")
    manifest, entries = packing.source_manifest(ref)
    files = {e["path"]: git("show", f"{ref}:{e['path']}") for e in entries}
    generated = {
        MANIFEST: manifest, COMMIT: git("cat-file", "commit", ref),
        MARKER: (f"source_commit={ref[:7]}\nsource_ref={ref}\nsource_tree={tree}\n"
                 f"source_parent={parent}\nbranch={branch}\narchive_name={path.name}\n").encode(),
    }
    logs = ROOT / "reports/stage8b-sparse-m10-local"
    gate_raw = (logs / "progress-evidence.json").read_bytes()
    gate = json.loads(gate_raw)
    delta = qualification_delta(gate, files)
    clippy = subprocess.run(CLIPPY, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    require(clippy.returncode == 0, "canonical clippy failed")
    require(not git("status", "--porcelain", "--untracked-files=all").strip()
            and git("rev-parse", "HEAD").decode().strip() == ref, "tree changed while packaging")
    generated[PREFIX + "canonical-clippy.txt"] = clippy.stdout
    generated[QUALIFICATION] = encoded({
        "source_ref": ref, "packaging_only_delta": delta,
        "note": "Rust, Cargo, tests, gate, docs and fixtures exactly match the full test inventory. Only this exporter may differ; no test result is relabelled as testing the exporter.",
        "canonical_clippy_command": CLIPPY, "canonical_clippy_exit_code": clippy.returncode,
        "canonical_clippy_sha256": sha(clippy.stdout),
    })
    selected = {"progress-evidence.json", "model-replay.json", "linked-source-lifecycle.json", "current-tree-authority.txt"}
    selected.update(r["log"] for r in gate["commands"])
    for name in sorted(selected):
        require(Path(name).name == name, "gate path")
        generated[PREFIX + name] = (logs / name).read_bytes()
    generated[INDEX] = encoded({name: sha(raw) for name, raw in generated.items()})
    require(not set(files) & set(generated), "generated/source collision")
    modes = {e["path"]: e["mode"] for e in entries}
    with zipfile.ZipFile(path, "x", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, raw in sorted((files | generated).items()):
            validate_member_name(name)
            archive.writestr(packing.zip_info(name, modes.get(name, "100644")), raw)
    result = verify(path)
    Path(str(path) + ".sha256").write_text(f"{result['archive_sha256']}  {path.name}\n")
    Path(str(path) + ".safety.json").write_bytes(encoded(result))
    print(path)
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify", type=Path)
    args = parser.parse_args()
    if args.verify:
        print(json.dumps(verify(args.verify), sort_keys=True))
    else:
        build()
