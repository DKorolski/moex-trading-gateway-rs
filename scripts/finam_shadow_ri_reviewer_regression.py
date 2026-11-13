"""Reproduce the accepted reviewer's extra RI dataset as a local regression.

Does not change the numerical library, source admission, runtime or any service.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
from finam_shadow_models_parity import verify_handoff, ri_mark_checks

ROOT = Path(__file__).resolve().parents[1]
REVIEW = ROOT / "scripts/reviewer"
RUNNER = REVIEW / "ri_targeted_differential_v1.py"
PROVENANCE = REVIEW / "ri_targeted_differential_v1.provenance.json"
RUNNER_SHA256 = "8bd386acb408afa7c6efee59f8247e5eb0ad4233e27b0f20e9044514d4ad12cc"
INPUT_SHA256 = "548a2e6484e1354e193b5bba97c2e1476945a8cb11b1258d4b90f22c513a4e21"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, allow_nan=False, indent=2)
        stream.write("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--handoff", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary, handoff, out = args.binary.resolve(), args.handoff.resolve(), args.output.resolve()
    protected = (handoff, ROOT/"scripts", ROOT/"crates", ROOT/"docs")
    if out == ROOT or any(out == p or p in out.parents for p in protected):
        raise ValueError("output must not alter immutable inputs or source")
    if sha(RUNNER) != RUNNER_SHA256:
        raise ValueError("reviewer generator changed")
    provenance = json.loads(PROVENANCE.read_text())
    if provenance["runner_sha256"] != RUNNER_SHA256 or provenance["generated_input_sha256"] != INPUT_SHA256:
        raise ValueError("regression provenance changed")
    verify_handoff(handoff)
    binary_sha = sha(binary)
    provenance_sha = sha(PROVENANCE)
    out.mkdir(parents=True)
    run_dir = out / "run"
    command = [sys.executable, "-B", str(RUNNER), "--binary", str(binary),
               "--handoff", str(handoff), "--output", str(run_dir)]
    result = subprocess.run(command, capture_output=True, text=True, timeout=240)
    (out / "runner.stdout").write_text(result.stdout)
    (out / "runner.stderr").write_text(result.stderr)
    if result.returncode:
        raise RuntimeError("reviewer regression failed; see runner.stderr")
    if sha(run_dir / "inputs.json") != INPUT_SHA256:
        raise ValueError("reviewer input bytes are not the accepted dataset")
    verification = json.loads((run_dir / "verification.json").read_text())
    fixture = json.loads((run_dir / "inputs.json").read_text())
    actual = json.loads((run_dir / "actual.json").read_text())
    if (verification["result"] != "PASS_TARGETED_DIFFERENTIAL"
        or verification["cases"] != 12 or verification["policies"] != 2
        or verification["state_rows"] != 2304 or len(verification["details"]) != 24
        or verification["binary_sha256"] != binary_sha
        or actual["input_sha256"] != INPUT_SHA256):
        raise ValueError("unexpected reviewer regression coverage/build")
    if actual["evidence_kind"] != "synthetic_model_clock":
        raise ValueError("synthetic data relabelled")
    mark_checks = ri_mark_checks(actual, fixture)
    if sha(binary) != binary_sha or sha(RUNNER) != RUNNER_SHA256 or sha(PROVENANCE) != provenance_sha:
        raise ValueError("inputs changed during regression")
    verify_handoff(handoff)
    summary = dict(result="PASS_ADOPTED_REVIEWER_REGRESSION", input_cases=12,
        case_policy_replays=24, state_rows=2304, ri_model_point_exit_checks=mark_checks,
        input_sha256=INPUT_SHA256, binary_sha256=binary_sha, runner_sha256=RUNNER_SHA256,
        accepted_reference_source=provenance["accepted_numerical_source"],
        runner_source_candidate_is_reference_only=True,
        evidence_kind="synthetic_model_clock", calls_RiSession_complete=False,
        observed_clean_sessions_added=0, operational_eod_acceptance=False,
        release_acceptance=False, execution_authority=False, deployment=False,
        command=command, branch_counts=verification["branch_counts"],details=verification["details"])
    write(out / "summary.json", summary)
    write(out / "manifest.json", {p.relative_to(out).as_posix():sha(p)
         for p in sorted(out.rglob("*")) if p.is_file()})
    print(json.dumps({k:v for k,v in summary.items() if k not in ("command", "branch_counts", "details")}))


if __name__ == "__main__":
    main()
