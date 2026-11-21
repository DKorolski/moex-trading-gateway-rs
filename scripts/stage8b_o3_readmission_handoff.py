#!/usr/bin/env python3
"""Commit-bound O3 SOURCE review packaging; never release or run authority."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import stat
import subprocess
import zipfile

from make_stage8b_design_handoff import zip_info
from stage8b_p1f_o3_source_handoff import tree_hash, scan

ROOT = Path(__file__).resolve().parents[1]
BASE = "7cf760c84f9005cd9dec7e6e93c23100d5a0b978"
PREFIX = "o3-source-review/"


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encode(value):
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def require(ok, why):
    if not ok:
        raise ValueError(why)


def verify(path):
    with zipfile.ZipFile(path) as z:
        names = z.namelist()
        require(len(names) == len(set(names)), "duplicate members")
        require(z.testzip() is None, "CRC")
        data, modes = {}, {}
        for member in z.infolist():
            mode = member.external_attr >> 16
            require(stat.S_ISREG(mode), "nonregular member")
            raw = z.read(member)
            scan(member.filename, raw)
            data[member.filename], modes[member.filename] = raw, format(mode, "o")
    manifest = json.loads(data[PREFIX + "manifest.json"])
    expected = manifest["members"]
    require(set(data) == set(expected) | {PREFIX + "manifest.json"}, "exact inventory")
    for name, item in expected.items():
        require(item == {"sha256": sha(data[name]), "mode": modes[name]}, name)
    source = {n: data[n] for n in manifest["source_members"]}
    source_tree = tree_hash(source, modes)
    raw_commit = data[PREFIX + "commit.raw"]
    oid = hashlib.sha1(b"commit " + str(len(raw_commit)).encode() + b"\0" + raw_commit).hexdigest()
    require(oid == manifest["source_ref"], "commit identity")
    require(raw_commit.splitlines()[0] == ("tree " + source_tree).encode(), "exact Git tree")
    result = json.loads(data[PREFIX + "result.json"])
    require(result["status"] == "SOURCE_REVIEW_CANDIDATE_NOT_OPERATIONAL", "scope")
    require(result["authority_rebound"] is False and result["operational_o3_ready"] is False, "authority")
    return {"status": "PASS", "scope": "SOURCE_INTEGRITY_NOT_OPERATIONAL_ACCEPTANCE",
            "source_ref": oid, "source_tree": source_tree, "members": len(data),
            "source_members": len(source), "duplicates": 0, "unsafe_paths": 0,
            "symlinks": 0, "special_files": 0, "archive_sha256": sha(path.read_bytes())}


def package(evidence, output):
    require(not git("status", "--porcelain", "--untracked-files=all").strip(), "dirty source")
    ref = git("rev-parse", "HEAD").decode().strip()
    data, modes = {}, {}
    for record in git("ls-tree", "-rz", "--full-tree", ref).split(b"\0"):
        if not record:
            continue
        metadata, name = record.split(b"\t")
        mode, kind, _oid = metadata.decode().split()
        require(kind == "blob" and mode in {"100644", "100755"}, "source mode")
        name = name.decode()
        require(not name.startswith(PREFIX), "generated/source collision")
        data[name] = git("show", f"{ref}:{name}")
        modes[name] = mode
    source_members = sorted(data)
    result = json.loads((evidence / "result.json").read_bytes())
    require(result["status"] == "SOURCE_REVIEW_CANDIDATE_NOT_OPERATIONAL", "evidence scope")
    for check in result["checks"]:
        log = evidence / check["log"]
        require(log.is_file() and log.resolve().is_relative_to(evidence.resolve()), "log custody")
        require(check["exit_code"] == 0, "required check exit: " + check["log"])
        raw = log.read_bytes()
        if check.get("tests"):
            counts = re.findall(rb"test result: ok\. (\d+) passed; 0 failed;", raw)
            require(counts and sum(map(int, counts)) >= check["minimum_passed"], "nonzero test selection")
        data[PREFIX + check["log"]] = raw
        check["log_sha256"] = sha(raw)
    for name in result.get("retained_failed_attempts", []):
        require(Path(name).name == name, "attempt path")
        data[PREFIX + "earlier/" + name] = (evidence / name).read_bytes()
    # Historical evidence is retained as custody, never promoted to a current
    # PASS (especially precursor suites and the reviewed baseline failures).
    for item in result.get("inherited_evidence", []):
        name = item["path"]
        path = evidence / name
        require(path.is_file() and path.resolve().is_relative_to(evidence.resolve()), "inherited custody")
        require(PREFIX + name not in data, "inherited collision")
        raw = path.read_bytes()
        require(sha(raw) == item["sha256"], "inherited hash")
        data[PREFIX + name] = raw
    data[PREFIX + "result.json"] = encode(result)
    data[PREFIX + "commit.raw"] = git("cat-file", "commit", ref)
    data[PREFIX + "status.txt"] = git("status", "--short")
    data[PREFIX + "delta.patch"] = git("diff", "--binary", BASE, ref, "--")
    name = f"moex-trading-project-{ref[:7]}-o3-readmission-source.zip"
    data["handoff-commit.txt"] = (
        f"source_commit={ref[:7]}\nsource_ref={ref}\narchive_name={name}\n"
        f"baseline={BASE}\nbranch={git('branch','--show-current').decode().strip()}\n").encode()
    require("handoff-commit.txt" not in source_members, "marker collision")
    for n, raw in data.items():
        scan(n, raw)
        modes.setdefault(n, "100644")
    data[PREFIX + "manifest.json"] = encode({
        "source_ref": ref, "source_members": source_members,
        "members": {n: {"sha256": sha(raw), "mode": modes[n]} for n, raw in data.items()}})
    output.mkdir(parents=True, exist_ok=True)
    path = output / name
    require(not path.exists(), "refuse archive overwrite")
    with zipfile.ZipFile(path, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for n, raw in sorted(data.items()):
            z.writestr(zip_info(n, modes.get(n, "100644")), raw)
    safety = verify(path)
    path.with_suffix(".zip.sha256").write_text(f"{safety['archive_sha256']}  {name}\n")
    path.with_suffix(".zip.safety.json").write_bytes(encode(safety))
    print(path)
    print(json.dumps(safety, indent=2))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--verify", type=Path)
    p.add_argument("--evidence", type=Path)
    p.add_argument("--output", type=Path)
    a = p.parse_args()
    if a.verify:
        print(json.dumps(verify(a.verify), indent=2))
    else:
        require(a.evidence and a.output, "--evidence and --output required")
        package(a.evidence, a.output)
