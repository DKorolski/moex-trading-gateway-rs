#!/usr/bin/env python3
"""O3 source-review packaging / offline verification / local template rendering.

No network, signing, service actions, Redis, installation or operational permit.
The local witnesses are imported byte-for-byte; never rerun by packaging.
"""
import argparse
import csv
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys
import zipfile
from stage8b_p1f_o3_shutdown_witness import CASES as O301_CASES, PREFIX as O301_PREFIX

ROOT = Path(__file__).resolve().parents[1]
PREFIX = "o3-source/"
BASE = "bc9813434b35238e9e6e861476cab2aaf3fc72a2"
CORRECTION_BASE = "6f0a6bf8f49a57272ce6a20024f57743b6b11e1d"
TEMPLATES = "deploy/stage8b-p1f-o3/"
UNITS = ("moex-finam-p1f-o3-signer", "moex-finam-p1f-o3-supervisor")
REVIEW = "docs/stage-8/reviews/REVIEW_bc98134_O3_PREPARATION_AND_MINIMAL_SCOPE_20261008_RU.txt"
# Only reviewed packaging/documentation additions after the recorded witnesses.
POST_PROOF = {
    "docs/current-status.md", "docs/roadmap.md",
    "docs/stage-8/stage8b-p1f-o3-source-progress.md",
    "docs/stage-8/stage8b-p1f-o3-source-handoff.md", REVIEW,
    "docs/stage-8/stage8b-p1f-o3-shutdown-correction.md",
    "docs/stage-8/reviews/REVIEW_6f0a6bf_O3_SOURCE_HOLD_20261009_RU.txt",
    "scripts/stage8b_p1f_o3_source_handoff.py",
    "scripts/test_stage8b_p1f_o3_source_handoff.py",
    *(TEMPLATES + name + ".service.in" for name in UNITS),
}
RESULT_PINS = {
    "components": "e9819ec36f630008aa7d6a480b768c88bde4fe55adc3f6a8e3147a944065d04e",
    "production": "9730d8fac191c65e49559bf1dff2f59398728522ea2dadbd03b44f0a29bc6b84",
}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encode(value):
    return (json.dumps(value, sort_keys=True, indent=2) + "\n").encode()


def safe_name(name):
    p = PurePosixPath(name)
    require(name and not p.is_absolute() and "\\" not in name
            and ".." not in p.parts and "." not in p.parts
            and str(p) == name and not any(ord(c) < 32 for c in name), "unsafe path")
    require(not set(p.parts) & {".git", "target", "__pycache__", "issuer-private-keys"},
            "excluded directory")
    require(p.name not in {".env", "key.ed25519", "schedule.ed25519",
                          "package-authorization.ed25519", "helper-acceptance.ed25519",
                          "account-binding-generation-1.hex", "stage8b-p1-lifecycle.key"},
            "private payload filename")


def scan(name, data):
    safe_name(name)
    require(not re.search(rb"-----BEGIN (?:RSA |EC |OPENSSH |ENCRYPTED )?PRIVATE KEY-----", data),
            "private-key PEM")
    if name.startswith("evidence/"):
        require(b"r2b-trust-rebind-generation-2-20260830" not in data,
                "private ceremony path in evidence")


def tree_hash(files, modes):
    """Recompute Git's SHA-1 tree from every source blob AND executable mode."""
    tree = {}
    for name, data in files.items():
        node = tree
        parts = name.split("/")
        for part in parts[:-1]:
            node = node.setdefault(part, {})
        blob = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).digest()
        node[parts[-1]] = (modes[name], blob)

    def recurse(node):
        data = b""
        for name, value in sorted(node.items(), key=lambda x: (x[0] + ("/" if isinstance(x[1], dict) else "")).encode()):
            mode, oid = ("40000", recurse(value)) if isinstance(value, dict) else value
            data += mode.encode() + b" " + name.encode() + b"\0" + oid
        return hashlib.sha1(b"tree " + str(len(data)).encode() + b"\0" + data).digest()
    return recurse(tree).hex()


def validate_templates(files):
    for name in UNITS:
        data = files[TEMPLATES + name + ".service.in"].decode()
        lines = [x.strip() for x in data.splitlines() if x.strip() and not x.startswith("#")]
        require(data.count("@PHASE_SHA256@") == 1 and "Restart=no" in lines,
                "template hash/restart contract")
        require("[Install]" not in lines and not any(
            x.startswith(("ExecStartPre=", "ExecStartPost=", "ExecStopPost=", "SuccessExitStatus="))
            for x in lines), "template activates or masks failures")
        executable = "stage8b-p1f-o3-signer" if name == UNITS[0] else "stage8b-p1f-local-supervisor"
        require([x for x in lines if x.startswith("ExecStart=")] ==
                [f"ExecStart=/usr/local/libexec/moex/{executable} run @PHASE_SHA256@"],
                "template executable drift")
    parent = files[TEMPLATES + UNITS[1] + ".service.in"]
    require(b"CapabilityBoundingSet=CAP_SETUID CAP_SETGID CAP_KILL" in parent
            and b"KillMode=mixed" in parent and b"TimeoutStopSec=40s" in parent
            and b"RuntimeMaxSec=1800s" in parent, "guardian service policy")
    require(b"LoadCredential=" not in parent and
            b"Environment=CREDENTIALS_DIRECTORY=/run/moex-finam-p1-o3-runtime-credentials" in parent,
            "child lifecycle credential custody")


def bind_source(files, read):
    current = {n: sha(b) for n, b in files.items()}
    result = {}
    for label in RESULT_PINS:
        old = json.loads(read(f"evidence/{label}/checked-source.json"))
        changes = {n: {"checked": old.get(n), "packaged": current.get(n)}
                   for n in sorted(old.keys() | current.keys()) if old.get(n) != current.get(n)}
        require(set(changes) <= POST_PROOF, f"{label}: qualified source drift: {set(changes) - POST_PROOF}")
        result[label] = {"production_and_witness_sources_exact": True,
                         "post_proof_packaging_only_changes": changes}
    return result


def o301_check(value, read):
    proof = value.get("o301_early_shutdown", {})
    require(proof.get("all_passed") is True
            and proof.get("deterministic_datagram_barrier") is True
            and proof.get("test_elf_not_production") is True, "O301 proof boundary")
    cases = proof.get("cases", [])
    require(len(cases) == len(O301_CASES), "O301 missing case")
    for recorded, (label, name, mutation) in zip(cases, O301_CASES):
        selector = O301_PREFIX + name
        require(recorded["selector"] == selector and recorded["harness_exit"] == 0
                and recorded["negative_control"] is mutation
                and recorded["child_exit"] == (101 if mutation else 0), "O301 selector/exit")
        prefix = "evidence/production/o301/" + label + "/"
        raw = read(prefix + "result.json")
        require(sha(raw) == recorded["evidence_sha256"], "O301 result hash")
        case = json.loads(raw)
        require(case["disconnected_latch_mutation"] is mutation
                and case["child_exit_code"] == recorded["child_exit"]
                and case["signal"] == recorded["signal"] == label.split("-")[0]
                and case["durable_root_and_bootstrap_unchanged"] is True
                and case["producer_checkpoint_retained"] is True
                and case["redis_before"] == case["redis_after"]
                and case["fresh_runtime_admission"] is False
                and case["callback_effects"] == 0 and case["force_kill"] is False,
                "O301 zero-effect invariant")
        log = read("evidence/production/o301-process-" + label + ".log")
        require(f"test {selector} ...".encode() in log
                and b"test result: ok. 1 passed; 0 failed; 0 ignored;" in log,
                "O301 exact parent log")
        if mutation:
            require(b"P1-O301: stop accepted without retained runtime intent" in
                    read(prefix + "child.stderr"), "O301 negative diagnostic")
        else:
            require(case["stop_accepted_before_release"] is True
                    and case["accepted"]["cause"] == "ExternalSignal"
                    and case["accepted"]["first_sequence"] == 1
                    and b"O301_ZERO_ADMISSION_STOP runtime_exit_class=66" in
                    read(prefix + "child.stdout"), "O301 retained stop")
            require(log.index(b"STOP_ACCEPTED") < log.index(b"RUNTIME_RELEASED"),
                    "O301 event ordering")


def evidence_check(read):
    for label, pin in RESULT_PINS.items():
        raw = read(f"evidence/{label}/result.json")
        require(sha(raw) == pin, label + " immutable result")
        value = json.loads(raw)
        require(value["source_unchanged_during_checks"] is True
                and value["operational_o3_ready"] is False and value["vps_mutated"] is False,
                label + " evidence boundary")
        if label == "components":
            records = value["records"]
            require(value["all_component_checks_passed"] is True and len(records) == 32,
                    "component count/result")
            require(sum(r["selector"] is not None for r in records) == 28, "exact selectors")
            for r in records:
                require(r["passed"] is True and r["exit_code"] == 0, "component failure")
                require(sha(read("evidence/components/" + r["log"])) == r["log_sha256"],
                        "component log mismatch")
        else:
            require(value["verdict"] == "PASS_LOCAL_PRODUCTION_PAPER_LIFECYCLE", "linked verdict")
            o301_check(value, read)
            for name, digest in value["evidence_files_sha256"].items():
                require(sha(read("evidence/production/" + name)) == digest, "production evidence drift")
            require(value["parent_exit_code"] == 0 and value["child_restart"] is True
                    and value["paper_outcome"] is True and value["bounded_stop"] is True
                    and value["signer_exit_code"] == 70, "record actual exits and lifecycle")
            for name in ("stage6.journal", "stage7b-recovery.seal",
                         "parent-state/.stage8b-p1-first-boot-receipt-v2.json",
                         "parent-state/.stage8b-p1-first-boot-transaction-v4.json"):
                require(read("evidence/production/root-before-child-kill/" + name) ==
                        read("evidence/production/root-after-child-restart/" + name), "restart drift")


def verify(path):
    with zipfile.ZipFile(path) as z:
        names = z.namelist()
        require(len(names) == len(set(names)), "duplicate member")
        require(z.testzip() is None, "CRC failure")
        modes = {}
        for item in z.infolist():
            scan(item.filename, z.read(item))
            mode = item.external_attr >> 16
            require(stat.S_ISREG(mode) and stat.S_IMODE(mode) in (0o644, 0o755),
                    "nonregular member or unsafe mode")
            modes[item.filename] = "100755" if mode & 0o111 else "100644"
        index = json.loads(z.read(PREFIX + "files-sha256.json"))
        require(set(index) == set(names) - {PREFIX + "files-sha256.json"}, "inventory mismatch")
        require(all(sha(z.read(n)) == h for n, h in index.items()), "file hash mismatch")
        source = json.loads(z.read(PREFIX + "source-index.json"))
        files = {name: z.read(name) for name in source}
        require(all(sha(files[n]) == h for n, h in source.items()), "source index mismatch")
        require(set(names) - set(source) == {
            n for n in names if n.startswith(("evidence/", PREFIX)) or n == "handoff-commit.txt"
        }, "unindexed source")
        d = json.loads(z.read(PREFIX + "descriptor.json"))
        require(re.fullmatch("[0-9a-f]{40}", d["commit"])
                and re.fullmatch("[0-9a-f]{40}", d["parent"])
                and d["accepted_baseline"] == BASE
                and d["correction_review_baseline"] == CORRECTION_BASE, "lineage")
        require(tree_hash(files, modes) == d["git_tree"], "Git tree binding")
        require(d["operational_ready"] is False and d["installation_authorized"] is False,
                "false operational claim")
        expected_commit = (f"short_sha={d['commit'][:7]}\nfull_sha={d['commit']}\n"
                           f"archive_name={d['archive']}\nsource_ref={d['commit']}\n").encode()
        require(z.read("handoff-commit.txt") == expected_commit, "commit descriptor")
        validate_templates(files)
        require(bind_source(files, z.read) == json.loads(z.read(PREFIX + "source-binding.json")),
                "qualified source binding")
        evidence_check(z.read)
        packaging = json.loads(z.read("evidence/packaging/result.json"))
        require(packaging["systemd_execution"] == "NOT_RUN", "systemd evidence claim")
        for record in packaging["records"]:
            require(record["exit_code"] == 0 and sha(z.read(record["log"])) == record["sha256"],
                    "packaging regression evidence")
        rows = list(csv.DictReader(io.StringIO(files[
            "docs/stage-8/stage8b-p1f-o3-acceptance-matrix.csv"].decode())))
        require(len(rows) == 13 and all(r["status"] == "NOT_RUN" for r in rows), "operational matrix")
        return dict(status="PASS_SOURCE_HANDOFF_SAFETY", commit=d["commit"], git_tree=d["git_tree"],
                    archive=d["archive"], archive_sha256=sha(Path(path).read_bytes()),
                    archive_bytes=Path(path).stat().st_size, members=len(names),
                    source_files=len(source), duplicates=0, unsafe_paths=0, symlinks=0,
                    special_files=0, secret_filename_or_pem_hits=0,
                    source_evidence_binding="PASS", operational_o3="NOT_RUN",
                    installed=False, cryptographic_secret_absence_proof=False)


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def tracked_modes():
    result = {}
    for entry in git("ls-tree", "-r", "-z", "HEAD").split(b"\0"):
        if not entry:
            continue
        header, name = entry.split(b"\t", 1)
        mode, kind, _oid = header.decode().split()
        require(kind == "blob" and mode in ("100644", "100755"), "tracked special file")
        result[name.decode()] = mode
    return result


def package(args):
    require(git("status", "--porcelain") == b"", "commit source first; dirty tree")
    commit = git("rev-parse", "HEAD").decode().strip()
    require(git("merge-base", "HEAD", BASE).decode().strip() == BASE, "source baseline changed")
    require(git("merge-base", "HEAD", CORRECTION_BASE).decode().strip() == CORRECTION_BASE,
            "correction review baseline changed")
    archive = f"moex-trading-project-{commit[:7]}-o3-source.zip"
    payload, modes = {}, tracked_modes()
    with zipfile.ZipFile(io.BytesIO(git("archive", "--format=zip", "HEAD"))) as source:
        for item in source.infolist():
            if item.is_dir():
                continue
            # git archive uses DOS attributes=0 for ordinary non-executable
            # blobs on this Git version. Git tree mode is authoritative; never
            # infer "regular" for an arbitrary imported ZIP's zero attributes.
            require(item.filename in modes, "archive member absent from Git tree")
            payload[item.filename] = source.read(item)
    require(set(payload) == set(modes), "Git archive omitted tracked source")
    source_index = {n: sha(b) for n, b in payload.items()}
    for label in RESULT_PINS:
        directory = getattr(args, label)
        for p in sorted(directory.rglob("*")):
            require(not p.is_symlink(), "evidence symlink")
            if p.is_dir():
                continue
            require(p.is_file(), "evidence special file")
            name = f"evidence/{label}/{p.relative_to(directory).as_posix()}"
            require(name not in payload, "evidence collision")
            payload[name] = p.read_bytes()
    binding = bind_source({n: payload[n] for n in source_index}, payload.__getitem__)
    records = []
    for label, command in (
        ("packaging-tests", [sys.executable, "-B", "scripts/test_stage8b_p1f_o3_source_handoff.py"]),
        ("clean-status", ["git", "status", "--porcelain"]),
        ("diff-check", ["git", "diff", "--check", BASE, "HEAD"]),
    ):
        result = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=120, check=False)
        require(result.returncode == 0, f"{label}: {result.stdout.decode(errors='replace')}")
        require(label != "clean-status" or result.stdout == b"", "dirty source during packaging")
        log = "evidence/packaging/" + label + ".log"
        payload[log] = result.stdout
        records.append(dict(command=command, exit_code=result.returncode, log=log, sha256=sha(result.stdout)))
    payload["evidence/packaging/result.json"] = encode(
        dict(records=records, systemd_execution="NOT_RUN", vps_mutated=False))
    d = dict(schema_version=1, kind="O3_SOURCE_REVIEW_NOT_INSTALLATION",
             repository="https://github.com/DKorolski/moex-trading-gateway-rs",
             branch=git("branch", "--show-current").decode().strip(), commit=commit,
             parent=git("rev-parse", "HEAD^").decode().strip(), accepted_baseline=BASE,
             correction_review_baseline=CORRECTION_BASE,
             git_tree=git("rev-parse", "HEAD^{tree}").decode().strip(), archive=archive,
             operational_ready=False, installation_authorized=False,
             main_changed=False, pushed=False, result_sha256=RESULT_PINS,
             qualification="Linux debug production features; disposable local root; real gen2 schedule trust",
             entry_document="docs/stage-8/stage8b-p1f-o3-source-handoff.md")
    payload[PREFIX + "descriptor.json"] = encode(d)
    payload[PREFIX + "source-index.json"] = encode(source_index)
    payload[PREFIX + "source-binding.json"] = encode(binding)
    payload["handoff-commit.txt"] = (f"short_sha={commit[:7]}\nfull_sha={commit}\n"
                                   f"archive_name={archive}\nsource_ref={commit}\n").encode()
    payload[PREFIX + "files-sha256.json"] = encode({n: sha(b) for n, b in payload.items()})
    args.output.mkdir(parents=True, exist_ok=True)
    target = args.output / archive
    for suffix in ("", ".sha256", ".safety.json", ".review.txt", ".partial"):
        require(not Path(str(target) + suffix).exists(), "output exists; never overwrite")
    temporary = Path(str(target) + ".partial")
    with zipfile.ZipFile(temporary, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for name, data in sorted(payload.items()):
            scan(name, data)
            item = zipfile.ZipInfo(name, (2026, 10, 9, 0, 0, 0))
            item.create_system = 3
            item.compress_type = zipfile.ZIP_DEFLATED
            item.external_attr = (0o100755 if modes.get(name) == "100755" else 0o100644) << 16
            z.writestr(item, data)
    safety = verify(temporary)
    temporary.rename(target)
    review = (
        "Stage 8B-P1-f O3 — SOURCE REVIEW CANDIDATE\n"
        f"Repository: {d['repository']}\nBranch: {d['branch']}\nCommit: {commit}\n"
        f"Archive: {archive}\nSHA-256: {safety['archive_sha256']}\n"
        f"Correction compare against SOURCE HOLD: {CORRECTION_BASE}\n"
        f"Accepted preparation ancestor: {BASE}\n"
        "Read first: docs/stage-8/stage8b-p1f-o3-source-handoff.md inside ZIP.\n"
        "Evidence: 28 exact tests + fmt/build/clippy/diff; SIGTERM/SIGINT early-stop "
        "process cases and two disconnected-latch negative controls; local production Linux paper "
        "Market, successor M10, SIGKILL/restart, graceful stop.\n"
        "8 offline packaging tests; exact Git tree, source/evidence hashes and ZIP safety verified.\n"
        "Linux debug production-feature proof, NOT a release/installation artifact.\n"
        "O2 accepted root/history untouched. O3 operational 13/13 NOT_RUN.\n"
        "No VPS/DB0/FINAM execution, signing of operational phase, push/merge or CI.\n"
        "After review: narrow artifact/authority closure, stopped installation, separately "
        "authorized O3; then O4 WS/freshness/EOD and ALOR paper parity.\n"
    ).encode()
    for suffix, data in ((".sha256", f"{safety['archive_sha256']}  {archive}\n".encode()),
                         (".safety.json", encode(safety)), (".review.txt", review)):
        with Path(str(target) + suffix).open("xb") as f:
            f.write(data)
    print(json.dumps(safety, indent=2))


def render(args):
    require(re.fullmatch("[0-9a-f]{64}", args.phase), "phase must be lowercase SHA-256")
    target = args.output.expanduser().resolve()
    require(not any(target == p or p in target.parents for p in
                    map(Path, ("/etc", "/usr", "/var", "/private/etc", "/private/var",
                               "/run", "/System"))), "local output only")
    files = {TEMPLATES + n + ".service.in": (ROOT / TEMPLATES / (n + ".service.in")).read_bytes()
             for n in UNITS}
    validate_templates(files)
    target.mkdir(parents=True, exist_ok=False)
    for name in UNITS:
        with (target / (name + ".service")).open("xb") as f:
            f.write(files[TEMPLATES + name + ".service.in"].replace(b"@PHASE_SHA256@", args.phase.encode()))
    print("RENDERED_LOCAL_TEMPLATES_NOT_INSTALLED")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="action", required=True)
    v = sub.add_parser("verify")
    v.add_argument("archive", type=Path)
    b = sub.add_parser("package")
    for field in ("components", "production", "output"):
        b.add_argument("--" + field, required=True, type=Path)
    r = sub.add_parser("render")
    r.add_argument("--phase", required=True)
    r.add_argument("--output", required=True, type=Path)
    args = p.parse_args()
    if args.action == "verify":
        print(json.dumps(verify(args.archive), indent=2))
    elif args.action == "render":
        render(args)
    else:
        package(args)


if __name__ == "__main__":
    main()
