#!/usr/bin/env python3
"""Fail-closed gate for the Stage 5G-c R2-c-a authority commit."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

STAGE5C = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
STAGE5F = "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs"
DESCRIPTOR = "docs/stage-5/stage5g-c-r2ca-market-terminal-authority.json"
R2A_DESCRIPTOR = "docs/stage-5/stage5g-c-source-projection-extension.json"
R2A_CHECKER = "scripts/stage5g_c_r2a_authority_check.py"
INVENTORY = "docs/stage-5/stage5g-lifecycle-entry-inventory.json"

PREDECESSOR_STAGE5C_SHA256 = "636dd27ac64b1d9dc448fc065497d7d59c09fc8735891953e5b0432879b60193"
R2A_CHECKER_SHA256 = "e0b94ef8efe98b430478210cc783673f6a0b10281619b00281dafbd5c161b0bb"
R2A_DESCRIPTOR_SHA256 = "33eaaf5b7de3cdb69698ef1ff434f22480fdb059d17861124179703a88b5a088"
REGIONS = {
    "market-terminal-no-callback-v1": (
        "STAGE5G-C-R2CA-AUTHORITY",
        "1d98411788ec1e0b331a7377fc8efdc6074afcaac107c99ea30c8aba4e351202",
    ),
    "market-terminal-no-callback-tests-v1": (
        "STAGE5G-C-R2CA-AUTHORITY-TESTS",
        "e7c7ad7cadc8e8f93c1e07acdab0cf2d4558a42da519190a9447afeba01a0606",
    ),
}


def digest(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def file_digest(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file():
        raise ValueError(f"required file missing: {relative}")
    return digest(path.read_bytes())


def extract(source: str) -> tuple[dict[str, str], str]:
    bodies: dict[str, str] = {}
    stripped = source
    for name, (prefix, _) in REGIONS.items():
        begin = f"// {prefix}-BEGIN: market-terminal-no-callback-v1"
        end = f"// {prefix}-END: market-terminal-no-callback-v1"
        if source.count(begin) != 1 or source.count(end) != 1:
            raise ValueError(f"marker cardinality drift: {name}")
        pattern = rf"(?m)^\s*{re.escape(begin)}\n(.*?)^\s*{re.escape(end)}\n"
        match = re.search(pattern, source, re.S)
        if match is None:
            raise ValueError(f"malformed region: {name}")
        bodies[name] = match.group(1)
        stripped, count = re.subn(pattern, "", stripped, count=1, flags=re.S)
        if count != 1:
            raise ValueError(f"cannot strip region once: {name}")
    return bodies, stripped


def run_detached_r2a(root: Path, stripped_stage5c: str) -> None:
    if file_digest(root, R2A_CHECKER) != R2A_CHECKER_SHA256:
        raise ValueError("accepted detached R2-a checker drift")
    if file_digest(root, R2A_DESCRIPTOR) != R2A_DESCRIPTOR_SHA256:
        raise ValueError("accepted detached R2-a descriptor drift")
    with tempfile.TemporaryDirectory(prefix="stage5g-r2ca-r2a-") as tmp:
        detached = Path(tmp)
        for relative in (R2A_CHECKER, R2A_DESCRIPTOR, INVENTORY, STAGE5F):
            destination = detached / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(root / relative, destination)
        destination = detached / STAGE5C
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(stripped_stage5c)
        completed = subprocess.run(
            [sys.executable, str(detached / R2A_CHECKER), "--root", str(detached)],
            check=False,
            capture_output=True,
            text=True,
        )
        if completed.returncode != 0:
            raise ValueError(
                "detached accepted R2-a checker rejected stripped tree: "
                + completed.stderr.strip()
            )


def check(root: Path) -> None:
    descriptor = json.loads((root / DESCRIPTOR).read_text())
    if descriptor.get("stage") != "5G-c-R2-c-a-market-terminal-authority":
        raise ValueError("descriptor stage drift")
    if descriptor.get("base_commit") != "e6e761519d43be2c2f08632c6559f7b4bb0ea533":
        raise ValueError("base authority drift")
    if descriptor.get("accepted_stage5g_b_r3") != "92f57c7831d8a15fb2e37668d3b07f1ccea03af7":
        raise ValueError("Stage 5G-b R3 authority drift")
    if any(descriptor.get("closed_surfaces", {}).values()):
        raise ValueError("closed surface opened")

    source_path = root / STAGE5C
    source = source_path.read_text()
    bodies, stripped = extract(source)
    if digest(stripped.encode()) != PREDECESSOR_STAGE5C_SHA256:
        raise ValueError("Stage 5C outside approved additive regions changed")
    if descriptor.get("stage5c_predecessor_sha256") != PREDECESSOR_STAGE5C_SHA256:
        raise ValueError("predecessor digest rewritten")
    if descriptor.get("stage5c_current_sha256") != file_digest(root, STAGE5C):
        raise ValueError("current Stage 5C digest mismatch")
    declared_regions = descriptor.get("regions", {})
    for name, (_, expected) in REGIONS.items():
        if digest(bodies[name].encode()) != expected:
            raise ValueError(f"approved region digest drift: {name}")
        if declared_regions.get(name) != expected:
            raise ValueError(f"descriptor region digest drift: {name}")

    production = bodies["market-terminal-no-callback-v1"]
    tests = bodies["market-terminal-no-callback-tests-v1"]
    if "pub(crate) fn resolve_stage5c_market_terminal_order_without_callback" not in production:
        raise ValueError("crate-private completion authority missing")
    if re.search(r"(?m)^pub (?:struct|enum|fn)\s", production):
        raise ValueError("normalized public API expanded")
    if "Serialize" in production or "Deserialize" in production:
        raise ValueError("authority became serializable")
    for token in ("redis", "finam", "reqwest", ".post(", ".delete(", "dispatch(", "send("):
        if token.lower() in production.lower():
            raise ValueError(f"forbidden I/O/transport token in authority: {token}")
    for token in ("on_broker_order", "on_broker_position", "on_broker_trade", "on_bar("):
        if token in production:
            raise ValueError(f"strategy callback opened: {token}")
    if "Strategy::set_state" in production or "&mut strategy" in production:
        raise ValueError("strategy mutation opened")
    if tests.count("#[test]") < 4:
        raise ValueError("authority acceptance coverage reduced")

    run_detached_r2a(root, stripped)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"stage5g-c-r2ca-authority-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-r2ca-authority-check: PASS")
    print("detached_stage5g_c_r2a: PASS")
    print("stage5c_additive_regions: 2/2")
    print("public_api/callback/io/live: closed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
