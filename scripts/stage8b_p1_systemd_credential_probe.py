#!/usr/bin/env python3
"""Run the actual Rust loader in native systemd, using only public test bytes.

Root-only qualification, not a bootstrap/guardian invocation. Does not read or
change any real key, unit, history, Redis state or installed executable.
The supplied binary is cargo's stage8b_systemd_credential integration test.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

from stage8b_p1f_o2_boot_id_check import bootstrap_properties

NAME = "stage8b-p1-lifecycle.key"
ROOT = Path(__file__).resolve().parents[1]


def run(binary, unit):
    if os.geteuid() != 0 or not Path("/run/systemd/system").is_dir():
        raise RuntimeError("native root systemd host required")
    original_unit = unit.read_bytes()
    props = bootstrap_properties(original_unit.decode())
    props = {k: v for k, v in props.items() if k not in ("ExecStart", "LoadCredential")}
    # No durable write access is required by this test executable.
    props.update(ReadWritePaths="", TimeoutStartSec="30s", TimeoutStopSec="5s")
    results = []

    def invoke(label, extra, expected):
        policy = props | extra
        name = f"moex-credential-probe-{os.getpid()}-{label}"
        args = ["systemd-run", "--quiet", "--wait", "--pipe", "--collect", "--unit=" + name]
        args += ["--property=" + k + "=" + v for k, v in policy.items()]
        args += ["--setenv=MOEX_CREDENTIAL_PROBE_EXPECT=" + expected]
        args += [str(binary), "native_systemd_credential_loader", "--exact", "--ignored", "--nocapture"]
        completed = subprocess.run(args, capture_output=True, text=True, timeout=45)
        marker = "CREDENTIAL_PROBE_PASS expected_accept=" + str(expected == "accept").lower()
        if completed.returncode or marker not in completed.stdout or "1 passed" not in completed.stdout:
            raise RuntimeError(label + " failed: " + completed.stdout + completed.stderr)
        results.append({"case": label, "status": "PASS", "expected": expected,
                        "service_user": policy["User"], "stdout": completed.stdout,
                        "stderr": completed.stderr, "exit_code": completed.returncode})

    # All input bytes are public fixtures. They never come from an installed
    # credential path. Only LoadCredential's temporary copy is read by Rust.
    with tempfile.TemporaryDirectory(prefix="moex-public-credential-", dir="/run") as temp:
        directory = Path(temp)
        for size in (32, 31, 33):
            source = directory / f"public-{size}"
            source.write_bytes(b"x" * size)
            source.chmod(0o400)
            invoke(f"loadcredential-{size}", {"LoadCredential": NAME + ":" + str(source)},
                   "accept" if size == 32 else "reject")
        # A read-only, root-owned ordinary 0440 file does not get the exception.
        source = directory / NAME
        source.write_bytes(b"x" * 32)
        source.chmod(0o440)
        directory.chmod(0o550)
        invoke("ordinary-readonly-0440", {"User": "root", "Group": "root",
               "Environment": "CREDENTIALS_DIRECTORY=" + str(directory)}, "reject")
        directory.chmod(0o700)

    # A matching pathname is insufficient: the open FD must be on a RO mount.
    with tempfile.TemporaryDirectory(prefix="moex-public-fake-", dir="/run/credentials") as temp:
        directory = Path(temp)
        source = directory / NAME
        source.write_bytes(b"x" * 32)
        source.chmod(0o440)
        directory.chmod(0o550)
        try:
            invoke("writable-fake-systemd", {"User": "root", "Group": "root",
                   "ReadWritePaths": str(directory),
                   "Environment": "CREDENTIALS_DIRECTORY=" + str(directory)}, "reject")
        finally:
            directory.chmod(0o700)
    if unit.read_bytes() != original_unit:
        raise RuntimeError("bootstrap unit changed during qualification")
    return {"status": "PASS", "cases": results, "production_loader_called": True,
            "probe_binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "unit_sha256": hashlib.sha256(original_unit).hexdigest(),
            "real_keys_used": False, "operational_o2_invoked": False,
            "installed_units_changed": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--test-binary", type=Path, required=True)
    parser.add_argument("--unit", type=Path,
                        default=ROOT / "deploy/stage8b-p1e/moex-finam-p1-paper-bootstrap.service")
    args = parser.parse_args()
    print(json.dumps(run(args.test_binary.resolve(strict=True), args.unit), indent=2))
