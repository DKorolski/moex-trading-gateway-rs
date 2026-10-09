"""Focused P1-O301 process regression in the existing disposable Linux witness.

Test-only Unix datagram barrier, real signals, ordinary Published producer pair.
Negative controls disconnect only the latch write and must fail that invariant.
Subsequent positive lifecycle witness uses separately rebuilt PRODUCTION ELFs.
"""
import hashlib
import json
import re
import subprocess

PREFIX = "stage8b_p1f_o3_composition::shutdown_tests::process::"
CASES = (
    ("SIGTERM-retained", "o3_sigterm_before_runtime_start_is_retained", False),
    ("SIGINT-retained", "o3_sigint_before_runtime_start_is_retained", False),
    ("SIGTERM-disconnected", "o3_sigterm_disconnected_latch_mutation_is_detected", True),
    ("SIGINT-disconnected", "o3_sigint_disconnected_latch_mutation_is_detected", True),
)


def build_tests(run, build_prefix, output):
    command = list(build_prefix)
    command[command.index("build")] = "test"
    raw = run("build-o301-process-tests", command + ["--lib", "--no-run"])
    artifacts = []
    for line in raw.splitlines():
        try:
            item = json.loads(line)
        except ValueError:
            continue
        if item.get("reason") == "compiler-artifact":
            artifacts.append(item)
    selected = [a for a in artifacts if a["target"]["name"] == "finam_gateway"
                and a.get("executable") and a["profile"]["test"]]
    if len(selected) != 1:
        raise RuntimeError("missing exact finam unit-test ELF")
    (output / "o301-test-artifacts.json").write_text(json.dumps(artifacts, indent=2) + "\n")
    return selected[0]["executable"]


def early_shutdown(run, docker, container, phase, elf, output, result):
    cases = []
    for label, name, mutation in CASES:
        selector = PREFIX + name
        target = output / "o301" / label
        target.mkdir(parents=True)
        try:
            raw = run("o301-process-" + label, [
                docker, "exec", "-e", "STAGE8B_O301_PHASE_SHA256=" + phase,
                "-e", "CREDENTIALS_DIRECTORY=/run/o3-local-lifecycle",
                "-e", "RUST_MIN_STACK=33554432", container, elf,
                "--ignored", "--exact", selector, "--nocapture", "--test-threads=1",
            ])
        finally:
            # Preserve child diagnostics even if the parent assertion fails;
            # cleanup still removes the disposable container in the caller.
            for filename in ("child.stdout", "child.stderr"):
                probe = subprocess.run([docker, "exec", container, "cat",
                    "/run/o3-public/early-stop-evidence/" + label + "/" + filename],
                    capture_output=True, timeout=20, check=False)
                if probe.returncode == 0:
                    (target / filename).write_bytes(probe.stdout)
        if (f"test {selector} ...".encode() not in raw
                or not re.search(rb"test result: ok\. 1 passed; 0 failed; 0 ignored;", raw)):
            raise RuntimeError("missing exact O301 selected process test")
        for filename in ("child.stdout", "child.stderr", "result.json"):
            data = run("o301-copy-" + label + "-" + filename, [
                docker, "exec", container, "cat", "/run/o3-public/early-stop-evidence/"
                + label + "/" + filename,
            ])
            (target / filename).write_bytes(data)
        evidence = json.loads((target / "result.json").read_bytes())
        if (evidence["disconnected_latch_mutation"] != mutation
                or evidence["child_exit_code"] != (101 if mutation else 0)
                or not evidence["durable_root_and_bootstrap_unchanged"]
                or not evidence["producer_checkpoint_retained"]
                or evidence["redis_before"] != evidence["redis_after"]
                or evidence["force_kill"]
                or evidence["fresh_runtime_admission"]
                or (not mutation and not evidence["stop_accepted_before_release"])):
            raise RuntimeError("O301 process evidence mismatch")
        cases.append(dict(selector=selector, harness_exit=0, signal=evidence["signal"],
                          negative_control=mutation, child_exit=evidence["child_exit_code"],
                          evidence_sha256=hashlib.sha256((target / "result.json").read_bytes()).hexdigest()))
    result["o301_early_shutdown"] = dict(
        cases=cases, all_passed=True, deterministic_datagram_barrier=True,
        test_elf_not_production=True, positive_production_control="subsequent ordinary witness",
    )
