#!/usr/bin/env python3
"""Commit-bound installation candidate, retaining the exact accepted artifact."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import zipfile

import make_stage8b_design_handoff as common
import stage8b_p1f_o2_artifact_handoff_safety_check as artifact_safety
import stage8b_p1f_o2_install as install
from stage8b_p1f_o2_install_gate import is_protected

ROOT = Path(__file__).resolve().parents[1]
REVIEW_NAME = "FINAM_7196aaa_O2_ARTIFACT_ACCEPT_REVIEW_2026-09-28.md"
REVIEW_SHA = "84971a4cee1e0e38168c9fc1fe9005481131ae3d82f652fa703c9d0fe5c97187"
OLD_SHA = "d8f9695bdb7a29b220dfe1396e31856fa7e71fb8a932dea126cd13e79c78e985"
SPEC = "docs/stage-8/stage8b-p1f-o2-installation-package.json"
ARTIFACT = "accepted-artifact/" + install.ARTIFACT_NAME
OLD_O1 = "accepted-artifact/old-o1.zip"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
COMMIT = "handoff-evidence/source-commit.raw"
GATE = "handoff-evidence/o2-installation-gate.log"
TESTS = "handoff-evidence/linux-filesystem-tests.log"
REVIEW = "handoff-evidence/reviews/" + REVIEW_NAME
EVIDENCE = "handoff-evidence/o2-installation-evidence.json"
GENERATED = {"handoff-commit.txt", MANIFEST, COMMIT, GATE, TESTS, REVIEW, EVIDENCE, ARTIFACT, OLD_O1}


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def strict_json(raw):
    return json.loads(raw, object_pairs_hook=install.old.reject_duplicate_keys)


def verify(path):
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        install.require(len(infos) == len({i.filename for i in infos}), "duplicate archive member")
        files = {}
        modes = {}
        for info in infos:
            artifact_safety.validate_name(info.filename)
            mode = info.external_attr >> 16
            install.require(mode in {0o100644, 0o100755}, "non-regular ZIP member")
            files[info.filename] = archive.read(info.filename)
            modes[info.filename] = f"{mode:06o}"
    rows = [row.split("=", 1) for row in files["handoff-commit.txt"].decode().splitlines()]
    install.require(len(rows) == 4 and all(len(row) == 2 for row in rows), "marker shape drift")
    marker = dict(rows)
    install.require(set(marker) == {"archive_name", "branch", "source_ref", "source_tree"}, "marker keys drift")
    install.require(marker["archive_name"] == path.name and marker["branch"] == "stage8b-o2-nonactivating-install", "marker mismatch")
    raw = files[COMMIT]
    install.require(artifact_safety.common.git_object_id("commit", raw) == marker["source_ref"], "commit object mismatch")
    install.require(raw.splitlines()[0] == ("tree " + marker["source_tree"]).encode(), "commit tree mismatch")
    manifest = strict_json(files[MANIFEST])
    entries = manifest["entries"]
    install.require(manifest["source_ref"] == marker["source_ref"] and manifest["entry_count"] == len(entries), "manifest identity drift")
    names = set()
    for entry in entries:
        name = entry["path"]
        install.require(name not in names and modes[name] == entry["mode"] and len(files[name]) == entry["size"] and install.sha(files[name]) == entry["sha256"], "tracked blob mismatch")
        names.add(name)
    install.require(set(files) - names == GENERATED, "unexpected generated entries")
    install.require(artifact_safety.common.build_tree_oid(entries, files) == marker["source_tree"], "tree reconstruction failed")
    install.require(install.sha(files[ARTIFACT]) == install.ARTIFACT_SHA and install.sha(files[OLD_O1]) == OLD_SHA, "accepted input drift")
    install.require(install.sha(files[REVIEW]) == REVIEW_SHA, "review digest drift")
    with tempfile.TemporaryDirectory(prefix="o2-install-verify-") as directory:
        accepted = Path(directory) / install.ARTIFACT_NAME
        accepted.write_bytes(files[ARTIFACT])
        artifact_safety.check(str(accepted))
        with zipfile.ZipFile(accepted) as archive:
            expected = {name for name in archive.namelist() if is_protected(name)}
            install.require({name for name in files if is_protected(name)} == expected, "accepted source/deploy inventory changed")
            for name in expected:
                install.require(files[name] == archive.read(name), "accepted source/deploy changed")
    spec = strict_json(files[SPEC])
    inventory = spec["inventory"]
    install.require(install.sha(install.canonical(inventory)) == spec["installation_identity_sha256"], "new installation identity mismatch")
    install.require(inventory["installer_sha256"] == install.sha(files["scripts/stage8b_p1f_o2_install.py"]), "installer not bound")
    install.require(inventory["custody_helper_sha256"] == install.sha(files["scripts/stage8b_p1e_i1_fixed_install.py"]), "custody helper not bound")
    evidence = strict_json(files[EVIDENCE])
    install.require(evidence["source_ref"] == marker["source_ref"] and evidence["source_tree"] == marker["source_tree"], "evidence source mismatch")
    for field, name in (("gate_sha256", GATE), ("tests_sha256", TESTS), ("manifest_sha256", MANIFEST)):
        install.require(evidence[field] == install.sha(files[name]), "evidence digest mismatch")
    for value in (spec, evidence):
        install.require(value["execution_authorized"] is False and value["target_mutation_performed"] is False and value["systemd_manager_tested"] is False, "operational claim opened")
    install.require(b"PASS stage8b-p1f-o2-install-gate linux_filesystem_tests=17 durable_file_frontiers=12 network=none systemd_manager_tested=false target_mutation=false" in files[GATE], "gate marker absent")
    install.require(b"PASS o2-installation-r1-evidence stopped_before_install=true staging_retained=true p0_unchanged=true db15_empty=true" in files[GATE], "native stop evidence gate absent")
    install.require(b"Ran 17 tests" in files[TESTS] and b"\nOK\n" in files[TESTS] and b"PASS replacement durable-file-frontier 12/12" in files[TESTS], "test inventory incomplete")
    return {"result": "PASS", "source_ref": marker["source_ref"], "source_tree": marker["source_tree"], "members": len(files), "tracked": len(entries), "duplicates": 0, "unsafe_paths": 0, "symlinks": 0, "accepted_artifact_sha256": install.ARTIFACT_SHA, "installation_identity_sha256": spec["installation_identity_sha256"], "execution_authorized": False, "systemd_manager_tested": False}


def make(args):
    install.require(not git("status", "--porcelain", "--untracked-files=all").strip(), "dirty worktree")
    branch = git("branch", "--show-current").decode().strip()
    install.require(branch == "stage8b-o2-nonactivating-install", "branch drift")
    ref = git("rev-parse", "HEAD").decode().strip()
    tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    subprocess.run(["git", "merge-base", "--is-ancestor", install.ARTIFACT_REF, ref], cwd=ROOT, check=True)
    report = ROOT / "tmp/o2-install-evidence"
    command = ["python3", "scripts/stage8b_p1f_o2_install_gate.py", "--artifact", str(args.artifact.resolve()), "--old-o1", str(args.old_o1.resolve()), "--output", str(report)]
    gate = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    (report / "package-gate.log").write_bytes(gate.stdout)
    install.require(gate.returncode == 0, "gate failed; see tmp/o2-install-evidence/package-gate.log")
    install.require(git("rev-parse", "HEAD").decode().strip() == ref and not git("status", "--porcelain", "--untracked-files=all").strip(), "tree changed during gate")
    manifest, entries = common.source_manifest(ref)
    name = f"moex-trading-project-{ref[:7]}-stage8b-p1f-o2-installation-package.zip"
    review = args.review.read_bytes()
    install.require(install.sha(review) == REVIEW_SHA, "review digest mismatch")
    tests = (report / "linux-filesystem-tests.log").read_bytes()
    evidence = {"source_ref": ref, "source_tree": tree, "execution_authorized": False, "target_mutation_performed": False, "systemd_manager_tested": False,
                "gate_sha256": install.sha(gate.stdout), "tests_sha256": install.sha(tests), "manifest_sha256": install.sha(manifest)}
    additions = {"handoff-commit.txt": f"source_ref={ref}\nsource_tree={tree}\nbranch={branch}\narchive_name={name}\n".encode(), MANIFEST: manifest, COMMIT: git("cat-file", "commit", ref), GATE: gate.stdout, TESTS: tests, REVIEW: review, EVIDENCE: install.canonical(evidence), ARTIFACT: args.artifact.read_bytes(), OLD_O1: args.old_o1.read_bytes()}
    output = ROOT / "reports/handoff"; output.mkdir(parents=True, exist_ok=True)
    final = output / name
    install.require(not final.exists(), "immutable archive already exists")
    with tempfile.TemporaryDirectory(prefix=".o2-install-preseal-", dir=output) as directory:
        temporary = Path(directory) / name
        with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for entry in entries:
                archive.writestr(common.zip_info(entry["path"], entry["mode"]), git("show", f"{ref}:{entry['path']}"))
            for key, value in additions.items(): archive.writestr(common.zip_info(key), value)
        safety = verify(temporary)
        os.link(temporary, final)
    digest = install.sha(final.read_bytes())
    final.with_suffix(".zip.sha256").write_text(f"{digest}  {name}\n")
    final.with_suffix(".zip.safety.json").write_text(json.dumps(safety, indent=2) + "\n")
    print(f"archive={final}\nsha256={digest}\nsource_ref={ref}\nPASS o2-installation-handoff")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--verify", type=Path)
    parser.add_argument("--artifact", type=Path)
    parser.add_argument("--old-o1", type=Path)
    parser.add_argument("--review", type=Path)
    args = parser.parse_args()
    if args.verify:
        print(json.dumps(verify(args.verify), sort_keys=True))
    else:
        install.require(all((args.artifact, args.old_o1, args.review)), "artifact, old-o1 and review are required")
        make(args)
