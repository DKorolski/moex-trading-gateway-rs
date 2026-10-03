#!/usr/bin/env python3
"""Offline sparse O2 build; accepted Rust, authority-only successor, no install."""
import argparse
from pathlib import Path

import stage8b_p1f_o2_build_linux as builder

SOURCE_REF = "4668b424a58a0bb3e0083380ceb33c0d8312b76d"
SOURCE_TREE = "d2ee7e951c32ad8db1317b46d1a4cff787369506"
ACCEPTED_SOURCE = "d63c378a51899a4d407dddc732896e89ccc51b43"
IMAGE = builder.IMAGE

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--registry-cache", type=Path, required=True)
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    if builder.git("diff", "--name-only", ACCEPTED_SOURCE, SOURCE_REF,
                   "--", "crates", "Cargo.toml", "Cargo.lock", ".github"):
        raise SystemExit("Accepted production/workflow bytes differ")
    builder.SOURCE_REF, builder.SOURCE_TREE = SOURCE_REF, SOURCE_TREE
    builder.build(args.output.resolve(), args.registry_cache.resolve(), args.resume)
