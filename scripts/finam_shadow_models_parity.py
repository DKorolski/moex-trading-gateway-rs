"""Offline reaction-port gate against the immutable ALOR handoff. No broker IO."""
import argparse
import copy
import hashlib
import json
from pathlib import Path, PurePosixPath
import subprocess
import sys

sys.dont_write_bytecode = True
MANIFEST_SHA256 = "42b6a7a625a8789082e65fd287f6d869f0f88520e3bb7fd9032f1eb81faa01b2"
SUMS_SHA256 = "7710bfafa2082f96c335a9109aa80d9af945a6be7a5576276fa3f9074d3febcd"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_handoff(root):
    if sha(root / "FILE_MANIFEST.json") != MANIFEST_SHA256 or sha(root / "SHA256SUMS") != SUMS_SHA256:
        raise ValueError("not the pinned 13 November ALOR handoff")
    expected = json.loads((root / "FILE_MANIFEST.json").read_text())["files"]
    for name, want in expected.items():
        if PurePosixPath(name).is_absolute() or ".." in PurePosixPath(name).parts:
            raise ValueError("unsafe manifest path")
        if (root / name).is_symlink() or sha(root / name) != want:
            raise ValueError("changed handoff member: " + name)
    actual = set()
    for p in root.rglob("*"):
        if p.is_symlink():
            raise ValueError("handoff symlink")
        if p.is_file():
            actual.add(p.relative_to(root).as_posix())
    if actual != set(expected) | {"FILE_MANIFEST.json", "SHA256SUMS"}:
        raise ValueError("unexpected handoff inventory")


def write(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, allow_nan=False, separators=(",", ":"))
        stream.write("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--handoff", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--model", choices=("imoexf", "usdrubf"), default="imoexf")
    args = parser.parse_args()
    root, binary, out = args.handoff.resolve(), args.binary.resolve(), args.output.resolve()
    if out == root or root in out.parents:
        raise ValueError("output must not alter handoff")
    verify_handoff(root)
    repo = Path(__file__).resolve().parents[1]
    crate = repo / "crates/finam-shadow-models"
    # Cargo's default target/ may contain generated .rs files. They are not
    # source identity: only this crate's explicit sources/lock and this gate are.
    paths = [crate / n for n in ("Cargo.toml", "Cargo.lock", "provenance.json")]
    paths += list((crate / "src").rglob("*.rs")) + [Path(__file__).resolve()]
    if any(p.is_symlink() or not p.is_file() for p in paths):
        raise ValueError("regular source files required")
    sources = {p.relative_to(repo).as_posix(): sha(p) for p in sorted(paths)}
    source_sha = hashlib.sha256(json.dumps(sources, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    implementation = "finam-shadow-models-source-sha256:" + source_sha
    binary_sha = sha(binary)
    out.mkdir(parents=True)
    model = args.model
    source = root / "fixtures" / model / "inputs.json"
    command = [str(binary), str(source), str(out / "actual.json"), implementation]
    result = subprocess.run(command, capture_output=True, text=True, timeout=120)
    (out / "candidate.stdout").write_text(result.stdout)
    (out / "candidate.stderr").write_text(result.stderr)
    if result.returncode:
        raise RuntimeError("candidate failed; inspect candidate.stderr")
    actual = json.loads((out / "actual.json").read_text())
    if actual["input_sha256"] != sha(source) or actual["implementation_id"] != implementation:
        raise ValueError("candidate identity mismatch")
    sys.path.insert(0, str(root / "tools"))
    from check_parity import oracle, read, mutation_checks, fee_checks
    from reaction_compare import compare, project_case
    trace = [project_case(c) for c in actual["cases"]]
    expected = oracle(model, read(source))
    compare(expected, read(root / "fixtures" / model / "expected_state_trace.json.gz"), "stored_expected")
    compare(expected, trace, "finam_" + model)
    negatives = mutation_checks(trace)
    fees = fee_checks(model, actual["cases"])
    candidate = dict(schema_version="finam_shadow_candidate_v1", model=model, input_sha256=sha(source),
                     implementation_id=implementation, evidence_kind="synthetic_fixture_not_production", trace=trace)
    write(out / "candidate.json", candidate)
    official = subprocess.run([sys.executable, "-B", str(root / "tools/compare_candidate.py"),
                               "--model", model, "--candidate", str(out / "candidate.json")],
                              capture_output=True, text=True, timeout=60)
    (out / "official-comparator.txt").write_text(official.stdout + official.stderr)
    if official.returncode:
        raise RuntimeError("official comparator failed")

    # Actual candidate failures, not expected-vs-expected comparator checks.
    base = read(source)
    base["cases"] = [base["cases"][0]]
    mutations = {}
    def mutate(name, function):
        value = copy.deepcopy(base)
        function(value)
        mutations[name] = value
    def first_bar(value):
        return next(op for op in value["cases"][0]["operations"] if op["kind"] == "bar")
    mutate("missing_terminal", lambda f: f["cases"][0]["operations"].pop())
    mutate("duplicate_first_bar", lambda f: f["cases"][0]["operations"].insert(1, copy.deepcopy(first_bar(f))))
    mutate("unclosed_bar", lambda f: first_bar(f).__setitem__("observed_at_utc", first_bar(f)["value"]["close_time_utc"]+599))
    mutate("wrong_contract", lambda f: first_bar(f)["value"].__setitem__("symbol", "RIZ6"))
    mutate("observed_not_synthetic", lambda f: f.__setitem__("evidence_kind", "observed_runtime"))
    if model == "imoexf":
        mutate("changed_cost", lambda f: f["cases"][0]["config"]["costs"].__setitem__("components", 1))
    else:
        mutate("changed_profile", lambda f: f["cases"][0]["config"]["settings"].__setitem__("bo_wait", 2))
    mutate("expected_injection", lambda f: f.__setitem__("expected", []))
    mutate("corrupt_ohlcv", lambda f: first_bar(f)["value"].__setitem__("h", 1.0))
    candidate_negatives = {}
    negative_dir = out / "negative"
    negative_dir.mkdir()
    for name, value in mutations.items():
        inp, dest = negative_dir / (name+".json"), negative_dir / (name+"-actual.json")
        write(inp, value)
        run = subprocess.run([str(binary), str(inp), str(dest), implementation], capture_output=True, text=True, timeout=60)
        if run.returncode == 0 or dest.exists():
            raise RuntimeError("invalid input was accepted or exported: " + name)
        candidate_negatives[name] = dict(result="REJECTED_NO_EXPORT", exit_code=run.returncode, stderr=run.stderr)
    verify_handoff(root)
    if sha(binary) != binary_sha or sources != {n:sha(repo/n) for n in sources}:
        raise ValueError("source/binary changed during qualification")
    report = dict(result="PASS_LOCAL_NUMERICAL_PORT", model=model, cases=len(trace),
                  state_rows=sum(len(c["rows"]) for c in trace), policies=sorted({c["identity"]["policy"] for c in trace}),
                  comparator_negative_mutations=negatives, candidate_negative_cases=candidate_negatives,
                  fee_arithmetic_trades=fees, input_sha256=sha(source), binary_sha256=binary_sha,
                  source_tree_sha256=source_sha, source_files=sources, command=command,
                  operational_readiness=False, finam_m10_equivalence_proven=False,
                  historical_cross_broker_parity=False, deployment=False, order_emission_allowed=False)
    write(out / "summary.json", report)
    write(out / "manifest.json", {p.relative_to(out).as_posix():sha(p) for p in sorted(out.rglob("*")) if p.is_file()})
    print(json.dumps({k:v for k,v in report.items() if k not in ("source_files","candidate_negative_cases","command")}))


if __name__ == "__main__":
    main()
