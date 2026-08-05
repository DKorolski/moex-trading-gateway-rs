#!/usr/bin/env python3
"""Compilation-control mutation matrix for Stage 5G-e-d-a R6."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path

import stage5g_eda_r3_negative_harness as r3
import stage5g_eda_r4_negative_harness as r4
import stage5g_eda_r5_negative_harness as r5


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage5g_eda_r6_check.py"
FREEZE = "docs/stage-5/stage5g-e-d-a-r6-protected-tree-freeze.json"
ROOT_CARGO = "Cargo.toml"
CARGO_LOCK = "Cargo.lock"
RUNTIME_ROOT = "crates/strategy-runtime-core"
RUNTIME_CARGO = f"{RUNTIME_ROOT}/Cargo.toml"
RUNTIME_SRC = f"{RUNTIME_ROOT}/src"
PRESEAL = "scripts/stage5g_eda_r6_preseal_check.py"


def inventory() -> list[str]:
    freeze = json.loads((ROOT / FREEZE).read_text())
    protected = [row["path"] for row in freeze["rows"]]
    mutable = [row["path"] for row in freeze["mutable_allowlist"]]
    return sorted(set(protected + mutable))


FILES = inventory()


def replace_once(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError(
            f"mutation anchor must occur once in {relative}: {old!r}; got {text.count(old)}"
        )
    path.write_text(text.replace(old, new, 1))


def append_text(root: Path, relative: str, value: str) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text((path.read_text() if path.exists() else "") + value)


def redirect_runtime_lib(root: Path) -> None:
    source = root / RUNTIME_SRC
    alternate = root / RUNTIME_ROOT / "alt_src"
    shutil.copytree(source, alternate)
    append_text(
        root,
        f"{RUNTIME_ROOT}/alt_src/stage5g_fresh_broker_truth.rs",
        "\npub(crate) fn alternate_root_reducer(\n"
        "    _package: Stage5gValidatedFreshBrokerTruthPackage,\n"
        ") -> Stage5gRestartReconciliationDisposition {\n"
        "    Stage5gRestartReconciliationDisposition::ExactReplay\n"
        "}\n",
    )
    append_text(root, RUNTIME_CARGO, '\n[lib]\npath = "alt_src/lib.rs"\n')


def redirect_workspace_member(root: Path) -> None:
    alternate = root / "crates/strategy-runtime-core-alt"
    alternate.mkdir(parents=True)
    (alternate / "Cargo.toml").write_text(
        '[package]\nname = "strategy-runtime-core"\nversion = "0.1.0"\nedition = "2021"\n'
    )
    (alternate / "src").mkdir()
    (alternate / "src/lib.rs").write_text("pub fn alternate_runtime() {}\n")
    replace_once(
        root, ROOT_CARGO,
        '    "crates/strategy-runtime-core",',
        '    "crates/strategy-runtime-core-alt",',
    )


def add_duplicate_runtime_member(root: Path) -> None:
    duplicate = root / "crates/strategy-runtime-core-duplicate"
    duplicate.mkdir(parents=True)
    (duplicate / "Cargo.toml").write_text(
        '[package]\nname = "strategy-runtime-core"\nversion = "0.1.0"\nedition = "2021"\n'
    )
    (duplicate / "src").mkdir()
    (duplicate / "src/lib.rs").write_text("pub fn duplicate_runtime() {}\n")
    replace_once(
        root, ROOT_CARGO,
        '    "crates/strategy-runtime-core",',
        '    "crates/strategy-runtime-core",\n    "crates/strategy-runtime-core-duplicate",',
    )


def add_cargo_wrapper(root: Path, config_name: str) -> None:
    cargo = root / ".cargo"
    cargo.mkdir()
    (cargo / config_name).write_text('[build]\nrustc-wrapper = "scripts/r6-rustc-wrapper.sh"\n')
    (root / "scripts/r6-rustc-wrapper.sh").write_text("#!/usr/bin/env bash\nexec rustc \"$@\"\n")


def add_unreviewed_workspace_crate(root: Path) -> None:
    crate = root / "crates/unreviewed-runtime-helper"
    crate.mkdir(parents=True)
    (crate / "Cargo.toml").write_text(
        '[package]\nname = "unreviewed-runtime-helper"\nversion = "0.1.0"\nedition = "2021"\n'
    )
    (crate / "src").mkdir()
    (crate / "src/lib.rs").write_text("pub fn helper() {}\n")
    replace_once(
        root, ROOT_CARGO,
        '    "crates/strategy-runtime-core",',
        '    "crates/strategy-runtime-core",\n    "crates/unreviewed-runtime-helper",',
    )


def run_case(name: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix=f"stage5g-eda-r6-{name}-") as directory:
        root = Path(directory)
        for relative in FILES:
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        mutation(root)
        result = subprocess.run(
            ["python3", str(CHECKER), "--root", str(root)],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
        if result.returncode == 0:
            raise SystemExit(f"stage5g-eda-r6-negative: FAIL: mutation survived: {name}")
        print(f"PASS {name}")


def r6_mutation_cases() -> list[tuple[str, object]]:
    return [
        ("redirect-runtime-lib-to-alternate-source-tree", lambda root: redirect_runtime_lib(root)),
        ("redirect-workspace-member-to-alternate-package", lambda root: redirect_workspace_member(root)),
        ("remove-runtime-member-from-workspace", lambda root: replace_once(
            root, ROOT_CARGO, '    "crates/strategy-runtime-core",\n', "")),
        ("add-duplicate-runtime-package-member", lambda root: add_duplicate_runtime_member(root)),
        ("add-default-runtime-build-rs", lambda root: append_text(
            root, f"{RUNTIME_ROOT}/build.rs", "fn main() {}\n")),
        ("set-runtime-package-build-script", lambda root: replace_once(
            root, RUNTIME_CARGO, '[package]\n', '[package]\nbuild = "build.rs"\n')),
        ("add-repository-cargo-config-rustc-wrapper", lambda root: add_cargo_wrapper(
            root, "config.toml")),
        ("add-extensionless-repository-cargo-config", lambda root: add_cargo_wrapper(
            root, "config")),
        ("modify-root-cargo-toml", lambda root: append_text(
            root, ROOT_CARGO, "\n# unauthorized root manifest drift\n")),
        ("modify-cargo-lock", lambda root: append_text(
            root, CARGO_LOCK, "\n# unauthorized lock drift\n")),
        ("modify-runtime-cargo-toml", lambda root: append_text(
            root, RUNTIME_CARGO, "\n# unauthorized runtime manifest drift\n")),
        ("add-runtime-rust-file-outside-src", lambda root: append_text(
            root, f"{RUNTIME_ROOT}/alternate.rs", "pub fn alternate() {}\n")),
        ("add-runtime-integration-target", lambda root: append_text(
            root, f"{RUNTIME_ROOT}/tests/r6_unreviewed.rs", "#[test]\nfn unreviewed() {}\n")),
        ("add-runtime-bench-target", lambda root: append_text(
            root, f"{RUNTIME_ROOT}/benches/r6_unreviewed.rs", "fn main() {}\n")),
        ("add-runtime-example-target", lambda root: append_text(
            root, f"{RUNTIME_ROOT}/examples/r6_unreviewed.rs", "fn main() {}\n")),
        ("modify-inherited-r5-checker-dependency", lambda root: append_text(
            root, "scripts/stage5g_eda_r5_check.py", "\n# unauthorized inherited checker drift\n")),
        ("modify-broker-core-source-outside-r6-allowlist", lambda root: append_text(
            root, "crates/broker-core/src/lib.rs", "\n// unauthorized broker-core drift\n")),
        ("add-unreviewed-workspace-crate", lambda root: add_unreviewed_workspace_crate(root)),
        ("change-protected-tree-manifest-commitment", lambda root: replace_once(
            root, FREEZE,
            "ab1b8a16b582fd39d1ef1c97fa21dd29c8769c63735871f0a0cfd107bf11d3b8",
            "0000000000000000000000000000000000000000000000000000000000000000")),
        ("remove-protected-tree-delta-check", lambda root: replace_once(
            root, PRESEAL, "    if delta != EXPECTED_DELTA:\n",
            "    if False and delta != EXPECTED_DELTA:\n")),
        ("builder-remove-protected-tree-delta-check", lambda root: replace_once(
            root, "scripts/make_stage5g_ed_handoff_archive.py",
            "    if delta != EXPECTED_DELTA:\n",
            "    if False and delta != EXPECTED_DELTA:\n")),
    ]


def main() -> None:
    inherited_r3 = r3.mutation_cases()
    inherited_r4 = r4.r4_mutation_cases()
    inherited_r5 = r5.r5_mutation_cases()
    current = r6_mutation_cases()
    cases = inherited_r3 + inherited_r4 + inherited_r5 + current
    expected = (56, 23, 19, 21, 119)
    actual = (len(inherited_r3), len(inherited_r4), len(inherited_r5), len(current), len(cases))
    if actual != expected:
        raise SystemExit(f"stage5g-eda-r6-negative: FAIL: matrix counts {actual} != {expected}")
    names = [name for name, _ in cases]
    if len(names) != len(set(names)):
        raise SystemExit("stage5g-eda-r6-negative: FAIL: duplicate mutation names")
    for name, mutation in cases:
        run_case(name, mutation)
    print("stage5g-eda-r6-negative: PASS (119/119)")


if __name__ == "__main__":
    main()
