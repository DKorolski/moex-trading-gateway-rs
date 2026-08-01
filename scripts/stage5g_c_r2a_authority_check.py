#!/usr/bin/env python3
"""Fail-closed authority gate for Stage 5G-c R2-a."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

INVENTORY = "docs/stage-5/stage5g-lifecycle-entry-inventory.json"
DESCRIPTOR = "docs/stage-5/stage5g-c-source-projection-extension.json"
STAGE5C = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
STAGE5F = "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs"

INVENTORY_SHA256 = "d9a2e3e9cb7a0aae5f26a68be784ef7b0f1ce7bfbc017eaee29b991d2ad91539"
STAGE5C_BASELINE_SHA256 = "93c0b48e1b564ef1763354579885bea3cd5b448133afccbc611584184bb13f2d"
STAGE5F_SHA256 = "cf8fe7900a2f1f84d3928c0d911db69415f19ee640c26dea47227759e375c508"
REGIONS = {
    "source-projection-types": "b01a3731afa1da385628425f5d58c9529277f3605c1f996ebe4a184eb322135f",
    "settled-test-read-only-accessor": "e1e1241691e4460bdd01e455b82959e04f245a745fc14d3f6439c3f110b5081f",
    "resolved-read-only-accessor": "a8d24dc238f3d396ba48b114d413472fb0245930415ff17080a29f75ad525c19",
    "source-projection-function": "c127ac7928a5f0f6f87a438061ab5840f10b826d241d77f4f7ff261133ef7126",
}


def digest_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def digest_file(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file():
        raise ValueError(f"required file missing: {relative}")
    return digest_bytes(path.read_bytes())


def extract_regions(source: str) -> tuple[dict[str, str], str]:
    bodies: dict[str, str] = {}
    stripped = source
    for name in REGIONS:
        begin = f"// STAGE5G-C-SOURCE-PROJECTION-BEGIN: {name}"
        end = f"// STAGE5G-C-SOURCE-PROJECTION-END: {name}"
        if source.count(begin) != 1 or source.count(end) != 1:
            raise ValueError(f"marker cardinality drift: {name}")
        pattern = rf"(?m)^\s*{re.escape(begin)}\n(.*?)^\s*{re.escape(end)}\n"
        match = re.search(pattern, source, re.S)
        if match is None:
            raise ValueError(f"malformed or reordered region: {name}")
        bodies[name] = match.group(1)
        stripped, count = re.subn(pattern, "", stripped, count=1, flags=re.S)
        if count != 1:
            raise ValueError(f"region cannot be stripped exactly once: {name}")
    return bodies, stripped


def check(root: Path) -> None:
    if digest_file(root, INVENTORY) != INVENTORY_SHA256:
        raise ValueError("accepted Stage 5G-a inventory changed or self-authorized")
    inventory = json.loads((root / INVENTORY).read_text())
    authorities = {row["id"]: row for row in inventory["reuse_authorities"]}
    if authorities["STAGE5C_PAPER_HOST"]["sha256"] != STAGE5C_BASELINE_SHA256:
        raise ValueError("accepted Stage 5C inventory authority rewritten")
    if authorities["STAGE5F_SEMANTIC_ROUTE"]["sha256"] != STAGE5F_SHA256:
        raise ValueError("accepted Stage 5F inventory authority rewritten")
    if digest_file(root, STAGE5F) != STAGE5F_SHA256:
        raise ValueError("frozen Stage 5F semantic route changed")

    descriptor = json.loads((root / DESCRIPTOR).read_text())
    if descriptor.get("stage") != "5G-c-R2-a-authority-recovery":
        raise ValueError("descriptor stage drift")
    if descriptor.get("rejected_r1_base") != "16591e819c571aa2ccb8e4b0d087d28c84090415":
        raise ValueError("R2-a base drift")
    if descriptor.get("accepted_functional_predecessor") != "92f57c7831d8a15fb2e37668d3b07f1ccea03af7":
        raise ValueError("accepted functional predecessor drift")
    if any(descriptor.get("closed_surfaces", {}).values()):
        raise ValueError("closed surface opened")

    source = (root / STAGE5C).read_text()
    bodies, stripped = extract_regions(source)
    if digest_bytes(stripped.encode()) != STAGE5C_BASELINE_SHA256:
        raise ValueError("Stage 5C code outside approved projection regions changed")
    declared = descriptor["stage5c_extension"]
    if declared.get("accepted_baseline_sha256") != STAGE5C_BASELINE_SHA256:
        raise ValueError("descriptor baseline rewritten")
    if declared.get("stripped_sha256") != STAGE5C_BASELINE_SHA256:
        raise ValueError("descriptor stripped authority rewritten")
    if declared.get("current_sha256") != digest_file(root, STAGE5C):
        raise ValueError("descriptor current Stage 5C hash mismatch")
    declared_regions = {row["name"]: row["sha256"] for row in declared.get("regions", [])}
    if declared_regions != REGIONS:
        raise ValueError("descriptor region authority drift")

    joined = "\n".join(bodies.values())
    for name, expected in REGIONS.items():
        if digest_bytes(bodies[name].encode()) != expected:
            raise ValueError(f"approved projection region digest drift: {name}")
    for token in (
        "redis", "finam", "reqwest", ".post(", ".delete(", "tokio::spawn",
        "apply_stage5c", "resolve_stage5c", "Strategy::set_state", "&mut ",
        "on_broker_bar", "on_bar(", "dispatch", "send(",
    ):
        if token.lower() in joined.lower():
            raise ValueError(f"forbidden mutation/callback/I/O token in projection: {token}")
    if joined.count("pub(crate) fn stage5g_source_intent_projections") != 2:
        raise ValueError("projection accessor cardinality drift")
    if "#[cfg(test)]" not in bodies["settled-test-read-only-accessor"]:
        raise ValueError("settled projection accessor escaped test-only scope")
    if "#[cfg(test)]" in bodies["resolved-read-only-accessor"]:
        raise ValueError("production projection accessor was disabled")
    if joined.count("fn stage5g_source_intent_projections(") != 3:
        raise ValueError("projection writer/accessor cardinality drift")
    if re.search(r"(?m)^pub (?:struct|enum|fn)\s", joined) or "pub(super)" in joined or "pub(in " in joined:
        raise ValueError("projection escaped crate-private boundary")
    if descriptor["stage5c_extension"].get("production_accessor_count") != 1:
        raise ValueError("production accessor declaration drift")
    if descriptor["stage5c_extension"].get("new_callback_callsite_count") != 0:
        raise ValueError("callback declaration drift")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"stage5g-c-r2a-authority-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-r2a-authority-check: PASS")
    print("stage5f_frozen: exact")
    print("stage5c_projection_regions: 4/4")
    print("production_accessors: 1; new_callback_callsites: 0")
    print("closed_surfaces: preserved")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
