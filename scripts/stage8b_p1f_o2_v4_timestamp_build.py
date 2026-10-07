#!/usr/bin/env python3
"""Full, offline O2 successor from accepted V4 timestamp source/authority."""
import argparse
from pathlib import Path

import stage8b_p1f_o2_build_linux as builder

SOURCE_REF = "f3b349949802abd5eff80cad6b9e9fc37bc327e1"
SOURCE_TREE = "e9a744c59d43064283620903ea9b79175073efab"
ACCEPTED_SOURCE = "c5545da055ec91035a9ee695dbe6c7c6ad2c67a9"
IMAGE = builder.IMAGE


def build(output, registry_cache, resume=False):
    if builder.git("diff", "--name-only", ACCEPTED_SOURCE, SOURCE_REF,
                   "--", "crates", "Cargo.toml", "Cargo.lock", ".github"):
        raise ValueError("accepted source/authority production mismatch")
    builder.SOURCE_REF, builder.SOURCE_TREE = SOURCE_REF, SOURCE_TREE
    builder.build(output, registry_cache, resume)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--registry-cache", type=Path, required=True)
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    build(args.output.resolve(), args.registry_cache.resolve(), args.resume)
