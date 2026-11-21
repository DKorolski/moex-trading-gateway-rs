#!/usr/bin/env python3
"""One-off review seal. Execute only after fresh checks in the explicit stage.

Creates Git objects, never refs/index/branch changes. No source edits, network,
tests or VPS runs. Local evidence is not final source or operational acceptance.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import tempfile
import tarfile
import zipfile

STAGE = Path(__file__).resolve().parent
DEFAULT_REPO = STAGE.parent
LOGS = ("shadow-tests.log", "reference-tests.log", "clippy.log",
        "ws-regression.log", "model-regression.log", "shadow-format.log", "core-format.log",
        "reference-state-check.log", "restart-handoff-check.log")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def safe(name, data=None):
    p = PurePosixPath(name)
    assert name and not p.is_absolute() and "\\" not in name and ":" not in name, name
    assert all(x not in ("", ".", "..") for x in name.split("/")), name
    assert all(ord(c) >= 32 for c in name), name
    for part in p.parts:
        n = part.lower()
        assert n not in {".git", "id_rsa", "id_ed25519", "credentials.json",
                         "credentials.toml", "secrets.json", "secrets.toml"}, name
        assert not (n.startswith(".env") and n != ".env.example"), name
        assert not n.endswith((".key", ".pem", ".p12", ".pfx", ".env")), name
    if data is not None:
        # Match actual PEM payload, not source code mentioning a PEM header.
        private = rb"-----BEGIN (?:RSA |EC |OPENSSH |DSA |ENCRYPTED )?PRIVATE KEY-----\s+[A-Za-z0-9+/=\r\n]{40,}"
        github = rb"\b(?:gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{60,})\b"
        assert not re.search(private, data) and not re.search(github, data), "secret content: " + name


def read(path):
    assert not path.is_symlink() and stat.S_ISREG(path.lstat().st_mode), str(path)
    return path.read_bytes()


def source_inventory(source):
    result = {}
    folded = set()
    for directory, dirs, files in os.walk(source, followlinks=False):
        for name in dirs + files:
            p = Path(directory) / name
            rel = p.relative_to(source).as_posix()
            safe(rel)
            assert not p.is_symlink(), rel
            assert rel.casefold() not in folded, "case collision: " + rel
            folded.add(rel.casefold())
            assert stat.S_ISDIR(p.lstat().st_mode) or stat.S_ISREG(p.lstat().st_mode), rel
            if p.is_file():
                data = read(p)
                safe(rel, data)
                result[rel] = {"sha256": sha(data), "bytes": len(data),
                               "mode": "100755" if p.stat().st_mode & 0o111 else "100644"}
    return result


def main():
    global STAGE
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", type=Path, required=True,
                        help="completed staging directory; no evidence is read from script location")
    parser.add_argument("--repo", type=Path, default=DEFAULT_REPO)
    parser.add_argument("--fmt-exit-code", type=int, choices=[0], required=True,
                        help="parent-observed successful fmt exit code; preserves silent/empty fmt.log")
    parser.add_argument("--execute-seal", action="store_true", required=True,
                        help="explicit parent authorization to create local objects and ZIP")
    args = parser.parse_args()
    STAGE = args.stage.resolve(strict=True)
    assert STAGE.is_dir(), "stage must be a completed directory"
    source = STAGE / "source"
    manifest_bytes = read(STAGE / "manifest.json")
    manifest = json.loads(manifest_bytes)
    expected = manifest["files"]
    assert source_inventory(source) == expected, "source does not exactly match manifest"
    baseline = manifest["baseline_commit"]
    assert read(STAGE / "baseline.txt").decode().strip() == baseline
    changed = manifest["intentional_changed_paths"]
    assert changed and len(changed) == len(set(changed)), "empty or duplicate intentional paths"
    payload = {"manifest.json": manifest_bytes, "README.txt": read(STAGE / "README.txt"),
               "seal_review.py": read(Path(__file__))}
    counts = {}
    captured_logs = {}
    for name in LOGS:
        raw = read(STAGE / name)
        text = raw.decode()
        assert not re.search(r"\b(?:FAIL(?:ED)?|ERROR)\b|^error(?:\[|:)|panicked at", text, re.M), name
        if name in ("shadow-tests.log", "ws-regression.log", "model-regression.log"):
            results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed;", text)
            assert results and all(int(failed) == 0 for _, failed in results), "unfinished: " + name
            assert re.search(r"test result: ok\..*finished in[^\n]*\s*\Z", text), "unfinished: " + name
            counts[name] = sum(int(passed) for passed, _ in results)
            assert counts[name] > 0, name
        elif name == "reference-tests.log":
            match = re.search(r"Ran (\d+) tests? in .*\n\s*OK\s*\Z", text)
            assert match, "unfinished Python tests"
            counts[name] = int(match[1])
        elif name == "clippy.log":
            assert re.search(r"Finished `dev` profile[^\n]*\s*\Z", text), "unfinished Clippy"
        elif name == "reference-state-check.log":
            match = re.fullmatch(r"Independent rerun matches: (\d+) steps, (\d+) rounds\s*", text)
            assert match and int(match[1]) > 0 and int(match[2]) > 0, "unfinished independent state rerun"
            counts[name] = {"steps": int(match[1]), "rounds": int(match[2])}
        elif name == "restart-handoff-check.log":
            assert "CURRENT_SESSION_NONFLAT_HANDOFF_ASSERTIONS_COMPLETED" in text, "positive real-clock branch did not execute"
            assert "test result: ok. 1 passed; 0 failed;" in text, "handoff check incomplete"
        else:
            # fmt may be entirely silent. Preserve the actual log; the explicit
            # CLI value is the parent's exit-status attestation, not inferred proof.
            assert args.fmt_exit_code == 0
            if text.strip():
                assert "cargo fmt" in text and re.search(r"^exit_code=0\s*$", text, re.M), "unexpected fmt output"
        payload["logs/" + name] = raw
        captured_logs[name] = raw
    assert counts["shadow-tests.log"] >= 87 and counts["reference-tests.log"] >= 4, counts
    assert counts["ws-regression.log"] >= 6 and counts["model-regression.log"] >= 2, counts
    cover = "docs/stage-8/stage8b-signal-shadow-source-review-r2.md"
    payload["source-review-r2.md"] = read(source / cover)
    for name in expected:
        payload["source/" + name] = read(source / name)
    reference = Path("/Users/denisq/Documents/from_mac/projects/strategies_list/analiz_alpha_si/imoexf_live_shadow_finam_freeze_2026_09_27")
    for name in ("freeze_metadata.json", "FINAM_PARITY_GUIDE_RU.md"):
        payload["historical-reference/" + name] = read(reference / name)
    with tarfile.open(reference / "source/alor_reference_HEAD.tar.gz", "r:gz") as archive:
        for member in archive:
            prefix = "alor-rs-main/strategy-runtime/src/strategies/"
            if (member.name == prefix + "hybrid_intraday_runtime.rs"
                    or member.name.startswith(prefix + "hybrid_intraday/") and member.name.endswith(".rs")):
                assert member.isfile() and member.size < 2_000_000
                raw = archive.extractfile(member).read()
                payload["historical-reference/" + member.name] = raw
    for name, data in payload.items():
        safe(name, data)

    output = STAGE.parent / (STAGE.name + ".zip")
    assert not output.exists() and not output.is_symlink(), "ZIP already exists"
    assert not Path(str(output) + ".sha256").exists(), "SHA sidecar already exists"
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)
    repo = args.repo.resolve(strict=True)
    def git(*words, data=None):
        return subprocess.run(["git", "-C", str(repo), *words], input=data,
                              env=env, check=True, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE).stdout
    before_head = git("rev-parse", "HEAD")
    before_refs = git("show-ref")
    real_index = Path(git("rev-parse", "--path-format=absolute", "--git-path", "index").decode().strip())
    index_before = read(real_index) if real_index.exists() else None
    assert git("rev-parse", baseline + "^{commit}").decode().strip() == baseline
    algorithm = git("rev-parse", "--show-object-format").decode().strip()
    def blob_oid(data):
        return hashlib.new(algorithm, b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
    with tempfile.TemporaryDirectory(prefix="shadow-seal-index-") as tmp:
        env["GIT_INDEX_FILE"] = str(Path(tmp) / "index")
        git("read-tree", baseline)
        updates = bytearray()
        for name in changed:
            assert name in expected, "deletions are outside this one-off seal"
            oid = git("hash-object", "-w", "--stdin", data=payload["source/" + name]).decode().strip()
            updates.extend(f'{expected[name]["mode"]} {oid}\t{name}\0'.encode())
        git("update-index", "-z", "--index-info", data=bytes(updates))
        tree = git("write-tree").decode().strip()
        entries = {}
        for entry in git("ls-tree", "-rz", tree).split(b"\0"):
            if not entry:
                continue
            meta, name = entry.split(b"\t", 1)
            mode, kind, oid = meta.decode().split()
            assert kind == "blob" and mode in ("100644", "100755")
            entries[name.decode()] = (mode, oid)
        assert entries == {name: (item["mode"], blob_oid(payload["source/" + name]))
                           for name, item in expected.items()}, "Git tree differs from full manifest"
        actual_changed = set(filter(None, git("diff-tree", "--no-commit-id", "--name-only", "-r", "-z", baseline, tree).decode().split("\0")))
        assert actual_changed == set(changed), "unexpected baseline diff"
        assert source_inventory(source) == expected, "source changed during seal"
        assert all(read(STAGE / name) == raw for name, raw in captured_logs.items()), "logs still changing"
        env.update(GIT_AUTHOR_NAME="Signal shadow source review", GIT_AUTHOR_EMAIL="review@localhost",
                   GIT_COMMITTER_NAME="Signal shadow source review", GIT_COMMITTER_EMAIL="review@localhost")
        commit = git("-c", "commit.gpgsign=false", "commit-tree", tree, "-p", baseline,
                     data=b"Signal-shadow source-review r2; local evidence; acceptance and operations not implied\n").decode().strip()
        payload["handoff-commit.txt"] = f"commit {commit}\ntree {tree}\nbaseline {baseline}\n".encode()
        payload["intentional.diff"] = git("diff", "--no-ext-diff", "--no-textconv", "--binary", baseline, commit, "--")
        predecessor = "e1d5e8b3da38c61eb1d6cbe7cdb1b4a5718fba40"
        assert git("rev-parse", predecessor + "^{commit}").decode().strip() == predecessor
        payload["review-predecessor.diff"] = git(
            "diff", "--no-ext-diff", "--no-textconv", "--binary", predecessor, commit, "--")
        payload["review-predecessor.txt"] = (predecessor + "\n").encode()
    assert git("rev-parse", "HEAD") == before_head and git("show-ref") == before_refs, "refs moved externally"
    assert (read(real_index) if real_index.exists() else None) == index_before, "real index changed externally"
    evidence = {"status": "SOURCE_REVIEW_R2_PENDING_REVIEW", "commit": commit,
                "tree": tree, "baseline": baseline, "test_counts_from_logs": counts,
                "clippy": "completed log", "fmt": "parent --fmt-exit-code 0 attestation; original log preserved",
                "test_environment": {"rustc": "1.95", "platform": "local macOS",
                                     "provenance": "parent-reported test environment",
                                     "msrv_tested": False, "native_linux_tested": False},
                "known_gaps": manifest["known_gaps"], "final_source_acceptance": False,
                "vps_runs": False, "refs_updated": False,
                "note": "Staging manifest is retained unchanged; this evidence supersedes its tests_run/commit_created fields."}
    payload["evidence.json"] = (json.dumps(evidence, indent=2) + "\n").encode()
    package_manifest = {name: {"bytes": len(data), "sha256": sha(data)} for name, data in sorted(payload.items())}
    payload["package-manifest.json"] = (json.dumps(package_manifest, indent=2, sort_keys=True) + "\n").encode()
    assert len({n.casefold() for n in payload}) == len(payload)
    for name, data in payload.items():
        safe(name, data)
    with zipfile.ZipFile(output, "x", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in sorted(payload.items()):
            info = zipfile.ZipInfo(name, (2026, 10, 23, 0, 0, 0))
            info.create_system = 3
            mode = int(expected[name[7:]]["mode"], 8) if name.startswith("source/") else 0o100644
            info.external_attr = mode << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data)
    with zipfile.ZipFile(output) as archive:
        names = archive.namelist()
        assert len(names) == len(set(names)) == len(payload) and set(names) == set(payload)
        assert archive.testzip() is None
        for info in archive.infolist():
            assert stat.S_ISREG(info.external_attr >> 16), info.filename
            data = archive.read(info)
            safe(info.filename, data)
            assert data == payload[info.filename], "ZIP readback mismatch"
    digest = sha(read(output))
    with Path(str(output) + ".sha256").open("x") as out:
        out.write(f"{digest}  {output.name}\n")
    safety = {"archive": output.name, "sha256": digest, "commit": commit, "tree": tree,
              "members": len(payload), "source_files": len(expected),
              "duplicates": 0, "unsafe_paths": 0, "symlinks": 0, "special_files": 0,
              "zip_crc": "PASS", "byte_exact_readback": "PASS", "git_tree_manifest": "PASS",
              "secret_screen": "filename policy and high-confidence private-key/GitHub-token patterns only",
              "source_acceptance": False, "operational_authorization": False}
    with Path(str(output) + ".safety.json").open("x") as out:
        json.dump(safety, out, indent=2)
        out.write("\n")
    with Path(str(output) + ".review.md").open("xb") as out:
        out.write(payload["source-review-r2.md"])
    print(json.dumps({"zip": str(output), "sha256": digest, "commit": commit,
                      "tree": tree, "test_counts": counts, "final_acceptance": False}, indent=2))


if __name__ == "__main__":
    if not __debug__:
        raise SystemExit("Do not run this safety checker with python -O")
    main()

