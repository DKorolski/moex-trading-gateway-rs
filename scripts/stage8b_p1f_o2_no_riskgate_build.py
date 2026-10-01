#!/usr/bin/env python3
"""Offline three-ELF O2 build from the accepted PR #11 merge; no installation."""
import argparse
from pathlib import Path

import stage8b_p1f_o2_build_linux as builder

SOURCE_REF = "ca1e5da7ea41eec219bce1cfe2bdf4b8d63d9029"
SOURCE_TREE = "0dd557b97b1cc208ffa8ae6228cef03c1bbabc19"
IMAGE = builder.IMAGE


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--registry-cache", type=Path, required=True)
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    builder.SOURCE_REF, builder.SOURCE_TREE = SOURCE_REF, SOURCE_TREE
    builder.build(args.output.resolve(), args.registry_cache.resolve(), args.resume)
