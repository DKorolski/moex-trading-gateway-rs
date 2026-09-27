#!/usr/bin/env python3
"""Prove the corrected O1 binary argument matches accepted installer admission."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import tempfile
from pathlib import Path

import stage8b_p1f_o1_check as check


def load_installer():
    path = check.ROOT / check.INSTALLER
    spec = importlib.util.spec_from_file_location("stage8b_p1e_i1_fixed_install", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("installer import failed")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main() -> None:
    document = json.loads((check.ROOT / check.SPEC).read_text())
    execution = document["execution"]
    if execution["bundle_directory_command"] != 'bundle_dir="$(pwd -P)"':
        raise SystemExit("FAIL bundle directory command drift")
    if '"$bundle_dir/payload/stage8b-p1-paper-supervisor"' not in execution["install_command"]:
        raise SystemExit("FAIL absolute binary argument missing")
    installer = load_installer()
    with tempfile.TemporaryDirectory(prefix="stage8b-p1f-o1-command-") as temporary:
        bundle = Path(temporary).resolve()
        binary = bundle / "payload/stage8b-p1-paper-supervisor"
        binary.parent.mkdir()
        binary.write_bytes(b"stage8b-p1f-o1-binary-admission-control\n")
        os.chmod(binary, 0o755)
        descriptor, digest = installer.open_validated_binary(binary)
        os.close(descriptor)
        expected = hashlib.sha256(binary.read_bytes()).hexdigest()
        if digest != expected:
            raise SystemExit("FAIL absolute binary digest")
        try:
            installer.open_validated_binary(Path("payload/stage8b-p1-paper-supervisor"))
        except installer.InstallError:
            pass
        else:
            raise SystemExit("FAIL relative binary admitted")
    print("PASS stage8b-p1f-o1-command-behavioral-test controls=2 absolute=admitted relative=rejected")


if __name__ == "__main__":
    main()
