#!/usr/bin/env python3
"""Exact mutation matrix for the Trust Rebind R0 fail-closed checker."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = Path("scripts/stage8b_p_r2b_trust_rebind_r0_check.py")
AUTHORITY = Path("docs/stage-8/stage8b-p-r2b-trust-rebind-r0-authority.json")
SUPERSESSION = Path("docs/stage-8/stage8b-p-r2b-trust-rebind-r0-supersession.json")
TRUST = Path("docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json")
ACCOUNT = Path("docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-account-key-manifest.json")
MATRIX = Path("docs/stage-8/STAGE8B_P_R2B_TRUST_REBIND_R0_ACCEPTANCE_MATRIX_2026-08-30.csv")
RUST = Path("tools/stage8b-readonly-preflight/src/r2a5.rs")

FILES = (
    CHECKER,
    AUTHORITY,
    SUPERSESSION,
    TRUST,
    ACCOUNT,
    MATRIX,
    Path("docs/stage-8/STAGE8B_P_R2B_TRUST_REBIND_R0_2026-08-30.md"),
    Path("tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-trust-rebind-key-ceremony.rs"),
    Path("tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-trust-rebind-key-ceremony-verify.rs"),
    RUST,
    Path("docs/stage-8/stage8b-p-r2a5-authority.json"),
    Path("docs/stage-8/stage8b-p-r2a5-production-trust-manifest.json"),
    Path("docs/stage-8/stage8b-p-r2a5-production-account-key-manifest.json"),
    Path("docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-canary-ceremony.json"),
    Path("docs/stage-8/stage8b-p-r2b-implementation-r0-r1-authority.json"),
    Path("docs/stage-8/stage8b-p-r2b-preproduction-supersession.json"),
    Path("docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-preflight-authority.json"),
)


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def mutate_json(root: Path, relative: Path, keys: tuple[str | int, ...], replacement: object) -> None:
    path = root / relative
    value = json.loads(path.read_text(encoding="utf-8"))
    cursor: object = value
    for key in keys[:-1]:
        cursor = cursor[key]  # type: ignore[index]
    cursor[keys[-1]] = replacement  # type: ignore[index]
    write_json(path, value)


def replace_text(root: Path, relative: Path, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text(encoding="utf-8")
    if old not in text:
        raise RuntimeError(f"mutation source missing: {old}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def add_secret(root: Path) -> None:
    path = root / "handoff-evidence/package-authorization.ed25519"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("redacted-test-value\n", encoding="utf-8")


CASES: list[tuple[str, Callable[[Path], None]]] = [
    ("loss-classification-hidden", lambda r: mutate_json(r, AUTHORITY, ("incident", "classification"), "ROTATION")),
    ("lost-generation-rewritten", lambda r: mutate_json(r, AUTHORITY, ("incident", "affected_generation"), 2)),
    ("public-projection-claimed-recoverable", lambda r: mutate_json(r, AUTHORITY, ("incident", "private_material_recoverable_from_public_projection"), True)),
    ("old-authorization-claimed-issued", lambda r: mutate_json(r, AUTHORITY, ("incident", "authorization_issued_before_loss"), True)),
    ("old-installation-claimed", lambda r: mutate_json(r, AUTHORITY, ("incident", "installation_performed_before_loss"), True)),
    ("old-finam-request-claimed", lambda r: mutate_json(r, AUTHORITY, ("incident", "finam_requests_before_loss"), 1)),
    ("candidate-generation-downgraded", lambda r: mutate_json(r, AUTHORITY, ("candidate_generation_2", "generation"), 1)),
    ("candidate-activated", lambda r: mutate_json(r, AUTHORITY, ("candidate_generation_2", "active"), True)),
    ("candidate-trust-hash-drift", lambda r: mutate_json(r, AUTHORITY, ("candidate_generation_2", "trust_manifest_sha256"), "0" * 64)),
    ("private-material-in-repository", lambda r: mutate_json(r, AUTHORITY, ("custody", "private_material_in_repository"), True)),
    ("private-material-in-handoff", lambda r: mutate_json(r, AUTHORITY, ("custody", "private_material_in_handoff"), True)),
    ("unattested-backup-claimed-verified", lambda r: mutate_json(r, AUTHORITY, ("custody", "encrypted_offline_backup_status"), "VERIFIED")),
    ("backup-attestation-forged", lambda r: mutate_json(r, AUTHORITY, ("custody", "backup_attestation_present"), True)),
    ("activation-without-backup", lambda r: mutate_json(r, AUTHORITY, ("custody", "activation_without_verified_backup_allowed"), True)),
    ("signing-seed-inventory-reduced", lambda r: mutate_json(r, AUTHORITY, ("custody", "private_signing_seed_count"), 12)),
    ("private-public-binding-count-reduced", lambda r: mutate_json(r, AUTHORITY, ("verification", "private_to_public_bindings_verified"), 12)),
    ("public-authority-selection-opened", lambda r: mutate_json(r, AUTHORITY, ("activation", "public_authority_selection_changed"), True)),
    ("production-binaries-claimed-rebuilt", lambda r: mutate_json(r, AUTHORITY, ("activation", "production_binaries_rebuilt"), True)),
    ("helper-acceptance-claimed-reissued", lambda r: mutate_json(r, AUTHORITY, ("activation", "helper_acceptance_reissued"), True)),
    ("production-credentials-claimed-installed", lambda r: mutate_json(r, AUTHORITY, ("activation", "production_credentials_installed"), True)),
    ("package-authorization-opened", lambda r: mutate_json(r, AUTHORITY, ("activation", "package_authorization_issued"), True)),
    ("controlled-installation-opened", lambda r: mutate_json(r, AUTHORITY, ("activation", "controlled_installation_allowed"), True)),
    ("authority-issued", lambda r: mutate_json(r, AUTHORITY, ("authorization",), "ISSUED")),
    ("finam-network-opened", lambda r: mutate_json(r, AUTHORITY, ("closed_surfaces", "finam_network"), True)),
    ("authorization-key-generation-one", lambda r: mutate_json(r, TRUST, ("authorization_key", "generation"), 1)),
    ("source-key-generation-one", lambda r: mutate_json(r, TRUST, ("source_keys", "schedule", "generation"), 1)),
    ("source-key-domain-renamed-v2", lambda r: mutate_json(r, TRUST, ("source_keys", "schedule", "key_id"), "schedule-ed25519-v2")),
    ("account-generation-one", lambda r: mutate_json(r, ACCOUNT, ("entries", 0, "generation_id"), "1")),
    ("lost-generation-may-authorize", lambda r: mutate_json(r, SUPERSESSION, ("superseded_candidate", "may_authorize_future_execution"), True)),
    ("transition-execution-opened", lambda r: mutate_json(r, SUPERSESSION, ("transition_state", "execution_allowed"), True)),
    ("secret-file-enters-tree", add_secret),
    ("ephemeral-path-rejection-removed", lambda r: replace_text(r, RUST, 'if output.starts_with(root) {', 'if false && output.starts_with(root) {')),
    ("nofollow-rejection-removed", lambda r: replace_text(r, RUST, ".custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);\n    let file = options.open(path)?;\n    let mut bytes = Vec::new();", ".custom_flags(libc::O_CLOEXEC);\n    let file = options.open(path)?;\n    let mut bytes = Vec::new();")),
    ("binding-verifier-renamed-away", lambda r: replace_text(r, RUST, "fn verify_seed_binding(", "fn unchecked_seed_binding(")),
    ("acceptance-matrix-failure", lambda r: replace_text(r, MATRIX, ",PASS\n", ",FAIL\n")),
    ("historical-authority-rewritten", lambda r: mutate_json(r, Path("docs/stage-8/stage8b-p-r2a5-authority.json"), ("authorization_status",), "ISSUED")),
]


def main() -> None:
    passed = 0
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-trust-rebind-r0-{name}-") as temporary:
            root = Path(temporary)
            for relative in FILES:
                destination = root / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / relative, destination)
            mutation(root)
            result = subprocess.run(
                [sys.executable, str(root / CHECKER), "--root", str(root)],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r2b-trust-rebind-r0-negative: FAIL accepted {name}")
        passed += 1
        print(f"PASS {name}")
    print(f"stage8b-p-r2b-trust-rebind-r0-negative: PASS {passed}/{len(CASES)}")


if __name__ == "__main__":
    main()
