#!/usr/bin/env python3
"""Targeted negative mutations for Stage 8B-P R2B R0-R1 closure."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage8b_p_r2b_implementation_r0_r1_check as checker

ROOT = Path(__file__).resolve().parents[1]


def replace(root: Path, relative: Path, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text(encoding="utf-8")
    if old not in text:
        raise RuntimeError(f"mutation anchor missing: {relative}: {old}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def json_set(root: Path, relative: Path, keys: tuple[str, ...], value: object) -> None:
    path = root / relative
    payload = json.loads(path.read_text(encoding="utf-8"))
    target = payload
    for key in keys[:-1]:
        target = target[key]
    target[keys[-1]] = value
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def copy_base(destination: Path) -> None:
    authority = json.loads((ROOT / checker.AUTHORITY).read_text(encoding="utf-8"))
    files = set(authority["implementation_artifacts"])
    files.update((str(item) for item in (
        checker.AUTHORITY, checker.BUILD, checker.REHEARSAL, checker.MATRIX,
        checker.CORE, checker.BUILDER, checker.SIGNER, checker.SUPERVISOR,
    )))
    for relative in files:
        source = ROOT / relative
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    for build_name in ("build-a", "build-b"):
        for binary in ("stage8b-r2b-run-package-draft-builder", "stage8b-r2a5-package-issuer"):
            relative = checker.ARTIFACT_ROOT / build_name / binary
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)


def main() -> None:
    builder_inaccessible = "InaccessiblePaths=/run/credentials/moex-trading/stage8b/r2a5"
    signer_inaccessible = "InaccessiblePaths=/run/credentials/moex-trading/stage8b/r2a5"
    signer_projection = "BindReadOnlyPaths=/run/credentials/moex-trading/stage8b/r2a5/package-authorization.ed25519:/run/moex-stage8b-r2b-package-issuer/package-authorization.ed25519"
    cases = (
        ("builder-package-signing-key-readable", lambda root: replace(root, checker.BUILDER, builder_inaccessible, "InaccessiblePaths=/run/credentials/unrelated")),
        ("builder-finam-secret-readable", lambda root: replace(root, checker.BUILDER, builder_inaccessible, "InaccessiblePaths=/run/credentials/moex-trading/stage8b/r2a5/package-authorization.ed25519")),
        ("builder-account-key-readable", lambda root: replace(root, checker.BUILDER, builder_inaccessible, "InaccessiblePaths=/run/credentials/moex-trading/stage8b/r2a5/finam-readonly-secret")),
        ("builder-issuer-key-readable", lambda root: replace(root, checker.BUILDER, builder_inaccessible, "InaccessiblePaths=/run/credentials/moex-trading/stage8b/r2a5/account-binding-keys")),
        ("builder-credential-root-visible", lambda root: replace(root, checker.BUILDER, builder_inaccessible + "\n", "")),
        ("builder-capability-bounding-set-open", lambda root: replace(root, checker.BUILDER, "CapabilityBoundingSet=\n", "CapabilityBoundingSet=CAP_DAC_OVERRIDE\n")),
        ("signer-finam-secret-readable", lambda root: replace(root, checker.SIGNER, signer_projection, signer_projection + "\nBindReadOnlyPaths=/run/credentials/moex-trading/stage8b/r2a5/finam-readonly-secret:/run/moex-stage8b-r2b-package-issuer/finam-readonly-secret")),
        ("signer-issuer-key-readable", lambda root: replace(root, checker.SIGNER, signer_inaccessible, "InaccessiblePaths=/run/credentials/moex-trading/stage8b/r2a5/finam-readonly-secret")),
        ("signer-unrelated-credential-readable", lambda root: replace(root, checker.SIGNER, signer_inaccessible + "\n", "")),
        ("signer-not-sole-package-key-consumer", lambda root: replace(root, checker.BUILDER, builder_inaccessible, signer_projection)),
        ("supervisor-package-signing-key-readable", lambda root: replace(root, checker.SUPERVISOR, "InaccessiblePaths=/run/credentials/moex-trading/stage8b/r2a5", "InaccessiblePaths=/run/credentials/moex-trading/stage8b/r2a5/helper-acceptance.ed25519")),
        ("supervisor-issuer-private-key-readable", lambda root: replace(root, checker.SUPERVISOR, "BindReadOnlyPaths=/run/credentials/moex-trading/stage8b/r2a5/account-id:", "BindReadOnlyPaths=/run/credentials/moex-trading/stage8b/r2a5/issuer-private-keys:/run/moex-stage8b-r2b-supervisor/issuer-private-keys /run/credentials/moex-trading/stage8b/r2a5/account-id:")),
        ("builder-write-scope-includes-input-root", lambda root: replace(root, checker.BUILDER, "ReadWritePaths=/var/lib/moex-trading/stage8b/r2a5/draft-output", "ReadWritePaths=/var/lib/moex-trading/stage8b/r2a5")),
        ("signer-write-scope-includes-config-root", lambda root: replace(root, checker.SIGNER, "ReadWritePaths=/var/lib/moex-trading/stage8b/r2a5/signed-output", "ReadWritePaths=/etc/moex-trading/stage8b/r2a5")),
        ("linux-builder-hash-missing", lambda root: json_set(root, checker.BUILD, ("binaries", "stage8b-r2b-run-package-draft-builder", "build_a_sha256"), "")),
        ("linux-signer-hash-missing", lambda root: json_set(root, checker.BUILD, ("binaries", "stage8b-r2a5-package-issuer", "build_a_sha256"), "")),
        ("linux-build-a-b-mismatch", lambda root: json_set(root, checker.BUILD, ("binaries", "stage8b-r2a5-package-issuer", "build_b_sha256"), "0" * 64)),
        ("controlled-feature-in-production-build", lambda root: json_set(root, checker.BUILD, ("controlled_custody_feature",), True)),
        ("packaged-linux-hash-drift", lambda root: (root / checker.ARTIFACT_ROOT / "build-a" / "stage8b-r2a5-package-issuer").write_bytes(b"forged")),
        ("actual-read-attempt-proof-removed", lambda root: json_set(root, checker.REHEARSAL, ("actual_read_attempts",), False)),
    )
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2b-r0-r1-negative-") as temporary:
        parent = Path(temporary)
        for name, mutate in cases:
            case_root = parent / name
            copy_base(case_root)
            mutate(case_root)
            try:
                checker.check(case_root)
            except (OSError, RuntimeError, KeyError, ValueError):
                passed += 1
                print(f"PASS {name}")
            else:
                raise SystemExit(f"stage8b-p-r2b-r0-r1-negative-harness: FAIL mutation survived: {name}")
    print(f"stage8b-p-r2b-r0-r1-negative-harness: PASS {passed}/{len(cases)}")


if __name__ == "__main__":
    main()
