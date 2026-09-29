#!/usr/bin/env python3
"""Bounded local package gate. No SSH, credentials, network or production writes."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import zipfile

import stage8b_p1f_o2_install as installer

ROOT = Path(__file__).resolve().parents[1]
IMAGE = "sha256:c27e53c36bd26412143ff8e81d524f12a203f54709f3bce034c918e8311c2552"
PROTECTED_DIRS = ("crates", "deploy", ".github", "config")
PROTECTED_FILES = ("Cargo.toml", "Cargo.lock", "docs/stage-8/stage8b-p1e-runtime-profile-v1.json", "scripts/stage8b_p1e_i1_fixed_install.py")


def is_protected(name):
    return name in PROTECTED_FILES or any(name.startswith(directory + "/") for directory in PROTECTED_DIRS)


def check_accepted_bytes(artifact):
    # Reproduce directly from an extracted handoff; Git history is not required.
    # Also reject additional untracked production/deploy files in a checkout.
    with zipfile.ZipFile(artifact) as archive:
        expected = {name for name in archive.namelist() if is_protected(name)}
        present = set(PROTECTED_FILES)
        for directory in PROTECTED_DIRS:
            for path in (ROOT / directory).rglob("*"):
                installer.require(not path.is_symlink(), "protected symlink")
                if path.is_file():
                    present.add(path.relative_to(ROOT).as_posix())
        installer.require(present == expected, "accepted protected path inventory changed")
        for name in expected:
            path = ROOT / name
            installer.require(not path.is_symlink() and path.read_bytes() == archive.read(name), "accepted protected bytes changed: " + name)
    print("PASS accepted-source-deploy-profile-workflow bytes_and_inventory_unchanged", flush=True)


def run(artifact, old_o1, output):
    output.mkdir(parents=True, exist_ok=True)
    installer.require(installer.sha(artifact.read_bytes()) == installer.ARTIFACT_SHA, "artifact pin drift")
    installer.require(installer.sha(old_o1.read_bytes()) == "d8f9695bdb7a29b220dfe1396e31856fa7e71fb8a932dea126cd13e79c78e985", "O1 fixture archive pin drift")
    spec = json.loads((ROOT / "docs/stage-8/stage8b-p1f-o2-installation-package.json").read_text())
    _, _, inventory = installer.load_package(artifact)
    installer.require(spec["installation_identity_sha256"] == installer.sha(installer.canonical(inventory)) and spec["inventory"] == inventory, "installation identity drift")
    installer.require(spec["execution_authorized"] is False and spec["target_mutation_performed"] is False, "scope opened")
    check_accepted_bytes(artifact)
    for command in (
        [sys.executable, "-m", "py_compile", "scripts/stage8b_p1f_o2_install.py", "scripts/test_stage8b_p1f_o2_install.py", "scripts/stage8b_p1f_o2_install_gate.py", "scripts/make_stage8b_p1f_o2_install_handoff.py"],
        [sys.executable, "scripts/stage8b_p1f_o2_install_r1_evidence_check.py"],
        [sys.executable, "scripts/current_tree_authority_check.py"],
        [sys.executable, "scripts/current_tree_authority_negative_harness.py"],
    ):
        print("COMMAND", " ".join(command), flush=True)
        subprocess.run(command, cwd=ROOT, check=True)
    command = ["docker", "run", "--rm", "--network", "none", "--platform", "linux/amd64",
               "--mount", f"type=bind,src={ROOT / 'scripts'},dst=/work/scripts,readonly",
               "--mount", f"type=bind,src={artifact},dst=/inputs/{installer.ARTIFACT_NAME},readonly",
               "--mount", f"type=bind,src={old_o1},dst=/inputs/old-o1.zip,readonly",
               IMAGE, "python3", "/work/scripts/test_stage8b_p1f_o2_install.py"]
    print("COMMAND", " ".join(command), flush=True)
    process = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    (output / "linux-filesystem-tests.log").write_bytes(process.stdout)
    print(process.stdout.decode(), flush=True)
    installer.require(process.returncode == 0 and b"Ran 17 tests" in process.stdout and b"\nOK\n" in process.stdout, "Linux suite failed or inventory changed")
    installer.require(b"PASS replacement durable-file-frontier 12/12" in process.stdout, "frontier inventory incomplete")
    print("PASS stage8b-p1f-o2-install-gate linux_filesystem_tests=17 durable_file_frontiers=12 network=none systemd_manager_tested=false target_mutation=false", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact", type=Path, required=True)
    parser.add_argument("--old-o1", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    run(args.artifact.resolve(), args.old_o1.resolve(), args.output.resolve())
