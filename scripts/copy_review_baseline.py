#!/usr/bin/env python3
"""Copy a complete review/checker baseline with one shared exclusion policy."""

from __future__ import annotations

import argparse
import shutil
from pathlib import Path


EXCLUDED_DIRECTORIES = {".git", "target", "tmp", "reports", "__pycache__", "__MACOSX"}


def excluded_name(name: str, *, directory: bool) -> bool:
    if directory and name in EXCLUDED_DIRECTORIES:
        return True
    return (
        name == ".DS_Store"
        or name == ".env"
        or name.startswith(".env.")
        or name.endswith(".log")
        or ".local." in name
        or name.endswith((".pyc", ".pyo"))
    )


def assert_no_included_symlinks(source: Path) -> None:
    for path in source.rglob("*"):
        relative = path.relative_to(source)
        if any(part in EXCLUDED_DIRECTORIES for part in relative.parts):
            continue
        if excluded_name(path.name, directory=path.is_dir()):
            continue
        if path.is_symlink():
            raise SystemExit(f"review baseline refuses included symlink: {relative}")


def copy_review_baseline(source: Path, destination: Path) -> None:
    source = source.resolve()
    destination = destination.resolve()
    if destination.exists():
        raise SystemExit(f"review baseline destination already exists: {destination}")
    if source == destination or source in destination.parents:
        raise SystemExit("review baseline destination must be outside the source tree")
    assert_no_included_symlinks(source)

    def ignore(_directory: str, names: list[str]) -> set[str]:
        ignored = set()
        for name in names:
            candidate = Path(_directory) / name
            if excluded_name(name, directory=candidate.is_dir()):
                ignored.add(name)
        return ignored

    shutil.copytree(source, destination, ignore=ignore)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    copy_review_baseline(args.source, args.destination)
    print(f"review-baseline-copy: ok destination={args.destination}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
