#!/usr/bin/env python3
"""Full O2 rebuild after accepted terminal recovery; old build pins stay intact."""
import argparse
from pathlib import Path

import stage8b_p1f_o2_build_linux as builder

SOURCE_REF = "589b80144adaa4c615aaa94781035d5a6af64c71"
SOURCE_TREE = "9809a954866d60a3dbc0892906f9458e7ad7a6ca"

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--registry-cache", type=Path, required=True)
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    builder.SOURCE_REF, builder.SOURCE_TREE = SOURCE_REF, SOURCE_TREE
    builder.build(args.output.resolve(), args.registry_cache.resolve(), args.resume)
