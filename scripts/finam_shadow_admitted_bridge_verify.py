#!/usr/bin/env python3
"""Offline source gates for the opt-in typed IMOEXF bridge. No VPS or broker."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--target-dir", type=Path, required=True)
    args = p.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    target = args.target_dir.resolve()
    baseline = json.loads((ROOT/"docs/stage-8/shadow-models-admitted-bridge-baseline-20261113.json").read_bytes())
    for name, expected in baseline["unchanged_git_blobs"].items():
        raw = (ROOT/name).read_bytes()
        actual = hashlib.sha1(b"blob "+str(len(raw)).encode()+b"\0"+raw).hexdigest()
        if actual != expected:
            raise AssertionError("accepted source changed: "+name)
    mapping_sha = hashlib.sha256((ROOT/"docs/stage-8/shadow-models-finam-mapping-20261113.json").read_bytes()).hexdigest()
    assert mapping_sha == "9e06d4ff4c66a9f88534bd4f46404b12588a1c716f1c64373a97174401376b0c"
    rust_version = subprocess.check_output(["rustc","--version"], text=True).strip()

    def run(label, argv):
        start = time.monotonic()
        r = subprocess.run(argv, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                           text=True, timeout=1800)
        (out/(label+".txt")).write_text(r.stdout)
        result = dict(label=label, command=argv, exit_code=r.returncode,
                      elapsed_seconds=round(time.monotonic()-start,3),
                      test_summaries=re.findall(r"test result:.*|Ran [0-9]+ tests",r.stdout))
        (out/(label+".json")).write_text(json.dumps(result, indent=2)+"\n")
        print(label+": "+("PASS" if r.returncode == 0 else "FAIL"), flush=True)
        if r.returncode:
            print(r.stdout[-6000:],flush=True)
        return result

    def models():
        cargo=["cargo"]
        common=["--offline","--locked","--manifest-path","crates/finam-shadow-models/Cargo.toml",
                "--target-dir",str(target/"shadow-models-imoexf")]
        return [
            run("models-debug",cargo+["test"]+common),
            run("models-release",cargo+["test"]+common+["--release"]),
            run("models-clippy",cargo+["clippy"]+common+["--all-targets","--","-D","warnings"]),
            run("models-fmt",["cargo","fmt","--manifest-path","crates/finam-shadow-models/Cargo.toml","--","--check"]),
        ]

    def native():
        common=["--offline","--locked","-p","finam-signal-shadow","--target-dir",str(target)]
        return [
            run("native-default",["cargo","test"]+common+["--lib"]),
            run("native-bridge",["cargo","test"]+common+["--features","models-bridge","--lib"]),
            run("native-doctests",["cargo","test"]+common+["--features","models-bridge","--doc"]),
            run("native-clippy",["cargo","clippy"]+common+["--features","models-bridge","--lib","--tests","--","-D","warnings"]),
            run("native-fmt",["rustfmt","--edition","2021","--check",
                "crates/finam-signal-shadow/src/primary/models_bridge.rs",
                "crates/finam-signal-shadow/src/primary/tests/models_bridge.rs",
                "crates/finam-signal-shadow/src/primary/cache.rs"]),
        ]

    with ThreadPoolExecutor(max_workers=2) as executor:
        model_run = executor.submit(models)
        native_run = executor.submit(native)
        gates=[run("catalog-unit",[sys.executable,"-B","-m","unittest","discover","-s","tests",
                                  "-p","test_finam_shadow_catalog.py","-v"])]
        gates += model_run.result()+native_run.result()
    result=dict(schema="finam.shadow.admitted-imoexf-bridge.source-gates.v1",
        result="PASS" if all(g["exit_code"] == 0 for g in gates) else "FAIL",
        rustc=rust_version, accepted_baseline=baseline["accepted_ref"],
        immutable_baseline_files=len(baseline["unchanged_git_blobs"]),
        immutable_mapping_sha256=mapping_sha, gates=gates,
        active_native_instruments=["IMOEXF"], numerical_policy_arms=2,
        all_native_producers_complete=False, source_evidence="synthetic_native_admission_tests",
        model_evidence="declared_clock_replay", observed_clean_sessions_added=0,
        linux_release=False, operational_activation=False, orders=False, redis_db0=False)
    (out/"summary.json").write_text(json.dumps(result,indent=2)+"\n")
    (out/"SHA256SUMS").write_text("".join(
        hashlib.sha256(f.read_bytes()).hexdigest()+"  "+f.name+"\n"
        for f in sorted(out.iterdir()) if f.is_file() and f.name!="SHA256SUMS"))
    print(json.dumps(result))
    return 0 if result["result"]=="PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
