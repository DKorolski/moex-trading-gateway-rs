#!/usr/bin/env python3
"""Targeted mutation matrix for Stage 8B-P R2B Implementation Package R0."""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECKER = Path("scripts/stage8b_p_r2b_implementation_r0_check.py")
AUTHORITY = Path("docs/stage-8/stage8b-p-r2b-implementation-r0-authority.json")
EVIDENCE = Path("docs/stage-8/stage8b-p-r2b-implementation-r0-evidence.json")
SUPPORT = (
    CHECKER,
    AUTHORITY,
    EVIDENCE,
    Path("docs/stage-8/STAGE8B_P_R2B_IMPLEMENTATION_R0_ACCEPTANCE_MATRIX_2026-08-29.csv"),
    Path("docs/stage-8/STAGE8B_P_R2B_IMPLEMENTATION_PACKAGE_R0_2026-08-29.md"),
)


def run(root: Path) -> int:
    return subprocess.run(
        ["python3", str(root / CHECKER)], cwd=root,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    ).returncode


def write_json(path: Path, document: object) -> None:
    path.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


def set_path(document: dict, keys: tuple[str, ...], value: object) -> None:
    current = document
    for key in keys[:-1]:
        current = current[key]
    current[keys[-1]] = value


def rebind_artifact(root: Path, relative: str) -> None:
    authority_path = root / AUTHORITY
    authority = json.loads(authority_path.read_text(encoding="utf-8"))
    authority["implementation_artifacts"][relative] = hashlib.sha256((root / relative).read_bytes()).hexdigest()
    write_json(authority_path, authority)


def main() -> None:
    authority = json.loads((ROOT / AUTHORITY).read_text(encoding="utf-8"))
    files = set(SUPPORT)
    files.update(Path(path) for path in authority["implementation_artifacts"])

    authority_cases = (
        ("predecessor-drift", ("accepted_predecessor", "source_ref"), "0" * 40),
        ("status-issued", ("status",), "ACCEPTED_ISSUED"),
        ("scope-expanded", ("scope",), "LIVE_ACTIVATION"),
        ("builder-caller-arguments", ("builder", "caller_arguments_allowed"), True),
        ("builder-network", ("builder", "network_allowed"), True),
        ("builder-signing-key", ("builder", "signing_key_access"), True),
        ("builder-finam-credential", ("builder", "finam_credential_access"), True),
        ("builder-receipt-count", ("builder", "required_receipt_count"), 10),
        ("builder-output-path", ("builder", "fixed_output"), "/tmp/package.json"),
        ("builder-overwrite", ("builder", "output_no_replace"), False),
        ("builder-no-file-fsync", ("builder", "file_fsync"), False),
        ("signer-output-path", ("signer", "fixed_output"), "/tmp/signed.json"),
        ("signer-overwrite", ("signer", "output_no_replace"), False),
        ("signer-no-revalidation", ("signer", "revalidates_all_builder_inputs"), False),
        ("signer-repairs-draft", ("signer", "may_repair_or_synthesize_draft"), True),
        ("signer-before-builder", ("signer", "requires_builder_success"), False),
        ("phase-count", ("transaction", "phase_count"), 5),
        ("service-count", ("transaction", "service_invocation_count"), 30),
        ("target-replay", ("transaction", "prior_active_state_may_satisfy_new_run"), True),
        ("installed", ("repository_state", "installed"), True),
        ("enabled", ("repository_state", "enabled"), True),
        ("started", ("repository_state", "started"), True),
        ("operator-selected", ("repository_state", "operator_selected"), True),
        ("run-nonce-present", ("repository_state", "run_nonce_issued"), True),
        ("credential-present", ("repository_state", "credentials_materialized"), True),
        ("signed-package-present", ("repository_state", "signed_run_package_present"), True),
        ("authorization-issued", ("authorization", "r2b"), "ISSUED"),
        ("activation-authority", ("authorization", "activation_authority_present"), True),
        ("finam-request", ("closed_surfaces", "finam_requests"), 1),
        ("post-open", ("closed_surfaces", "order_post_sent"), True),
        ("runtime-live-open", ("closed_surfaces", "runtime_live"), True),
    )
    evidence_cases = (
        ("evidence-enabled", ("activation_closure", "enabled"), True),
        ("evidence-preset", ("activation_closure", "preset_reference"), True),
        ("evidence-credential", ("activation_closure", "credential_present"), True),
        ("evidence-signed-package", ("activation_closure", "signed_package_present"), True),
        ("evidence-issued", ("authorization",), "ISSUED"),
        ("evidence-finam", ("finam_requests",), 1),
    )
    text_cases = (
        ("phase-service-missing", "deploy/stage8b-r2b/moex-stage8b-r2b-phase3-authority-producers.target", " stage8b-r2a5-producer@m8p8111.service", ""),
        ("phase-edge-removed", "deploy/stage8b-r2b/moex-stage8b-r2b-phase4-authority-issuers.target", "Requires=moex-stage8b-r2b-phase3-authority-producers.target ", "Requires="),
        ("phase-order-reversed", "deploy/stage8b-r2b/moex-stage8b-r2b-phase5-run-package.target", "After=moex-stage8b-r2b-phase4-authority-issuers.target", "After=moex-stage8b-r2b-phase6-readonly-preflight.target"),
        ("phase-target-retained", "deploy/stage8b-r2b/moex-stage8b-r2b-phase2-manifest-source.target", "StopWhenUnneeded=yes", "StopWhenUnneeded=no"),
        ("phase-install-section", "deploy/stage8b-r2b/moex-stage8b-r2b-phase1-current-source.target", "Before=", "[Install]\nWantedBy=multi-user.target\nBefore="),
        ("producer-remain-after-exit", "deploy/stage8b-r2a5/stage8b-r2a5-producer@.service", "Type=oneshot", "Type=oneshot\nRemainAfterExit=yes"),
        ("issuer-condition-skip", "deploy/stage8b-r2a5/stage8b-r2a5-issuer@.service", "RefuseManualStart=yes", "RefuseManualStart=yes\nConditionPathExists=/tmp/receipt"),
        ("adapter-condition-skip", "deploy/stage8b-r2a5/stage8b-r2a7-source-adapter.service", "RefuseManualStart=yes", "RefuseManualStart=yes\nConditionPathExists=/tmp/source"),
        ("builder-network-enabled", "deploy/stage8b-r2b/moex-stage8b-r2b-run-package-draft-builder.service", "RestrictAddressFamilies=AF_UNIX", "RestrictAddressFamilies=AF_UNIX AF_INET"),
        ("builder-credential-exposed", "deploy/stage8b-r2b/moex-stage8b-r2b-run-package-draft-builder.service", "ReadOnlyPaths=/etc", "ReadOnlyPaths=/run/credentials/moex-trading/stage8b/r2a5/package-authorization.ed25519 /etc"),
        ("builder-argument-added", "deploy/stage8b-r2b/moex-stage8b-r2b-run-package-draft-builder.service", "stage8b-r2b-run-package-draft-builder", "stage8b-r2b-run-package-draft-builder --input /tmp/draft"),
        ("builder-phase-barrier-removed", "deploy/stage8b-r2b/moex-stage8b-r2b-run-package-draft-builder.service", "Requires=moex-stage8b-r2b-phase4-authority-issuers.target", "Requires=local-fs.target"),
        ("signer-requires-removed", "deploy/stage8b-r2b/moex-stage8b-r2b-package-issuer.service", "Requires=moex-stage8b-r2b-run-package-draft-builder.service", "Requires=local-fs.target"),
        ("signer-after-removed", "deploy/stage8b-r2b/moex-stage8b-r2b-package-issuer.service", "After=moex-stage8b-r2b-run-package-draft-builder.service", "After=local-fs.target"),
        ("supervisor-barrier-removed", "deploy/stage8b-r2b/moex-stage8b-r2b-readonly-supervisor.service", "Requires=moex-stage8b-r2b-phase5-run-package.target", "Requires=local-fs.target"),
        ("aggregate-install", "deploy/stage8b-r2b/moex-stage8b-r2b-issuance.target", "After=", "[Install]\nWantedBy=multi-user.target\nAfter="),
        ("aggregate-manual-start", "deploy/stage8b-r2b/moex-stage8b-r2b-issuance.target", "RefuseManualStart=yes", "RefuseManualStart=no"),
        ("aggregate-stale-active", "deploy/stage8b-r2b/moex-stage8b-r2b-issuance.target", "StopWhenUnneeded=yes", "StopWhenUnneeded=no"),
        ("builder-cli-args", "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-run-package-draft-builder.rs", "fn main()", "fn main() /* std::env::args */"),
        ("fixed-builder-entry-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", "pub fn build_run_package_draft_from_fixed_inputs()", "pub fn build_run_package_draft_from_paths()"),
        ("signer-revalidation-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", "fn validate_unsigned_draft_inputs(", "fn accept_unsigned_draft_inputs("),
        ("no-replace-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", "std::fs::hard_link(&temporary, path)", "std::fs::rename(&temporary, path)"),
        ("directory-fsync-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", "std::fs::remove_file(&temporary)?;\n    File::open(parent)?.sync_all()?", "std::fs::remove_file(&temporary)?;\n    // directory fsync removed"),
        ("unsigned-path-drift", "tools/stage8b-readonly-preflight/src/r2a5.rs", 'state_root.join("r2b-run-package.unsigned.json")', 'state_root.join("caller-package.json")'),
        ("signed-path-drift", "tools/stage8b-readonly-preflight/src/r2a5.rs", 'etc_root.join("r2b-run-package.json")', 'etc_root.join("caller-signed.json")'),
        ("ttl-drift", "tools/stage8b-readonly-preflight/src/r2a5.rs", "chrono::Duration::seconds(30)", "chrono::Duration::seconds(300)"),
        ("receipt-validation-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", "r2a3::validate_signed_authorities", "r2a3::accept_signed_authorities"),
        ("generation-binding-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", "source_generation_commitment(&envelope.receipts)?", '"0".repeat(64)'),
        ("helper-binding-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", "load_accepted_helper_authority", "load_unchecked_helper_authority"),
        ("operator-binding-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", 'etc_root.join("operator-decision.json")', 'etc_root.join("unbound-decision.json")'),
        ("account-binding-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", 'etc_root.join("account-key-manifest.json")', 'etc_root.join("unbound-account.json")'),
        ("no-replace-test-removed", "tools/stage8b-readonly-preflight/src/r2a5.rs", "run_package_atomic_create_is_no_replace_and_symlink_safe", "run_package_create_test_removed"),
    )

    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2b-impl-negative-") as temporary:
        base = Path(temporary) / "base"
        for relative in files:
            destination = base / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        if run(base) != 0:
            raise SystemExit("stage8b-p-r2b-implementation-negative: FAIL baseline")

        for name, keys, value in authority_cases:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            document = json.loads((case / AUTHORITY).read_text(encoding="utf-8"))
            set_path(document, keys, value)
            write_json(case / AUTHORITY, document)
            if run(case) == 0:
                raise SystemExit(f"stage8b-p-r2b-implementation-negative: FAIL accepted {name}")
            passed += 1

        for name, keys, value in evidence_cases:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            document = json.loads((case / EVIDENCE).read_text(encoding="utf-8"))
            set_path(document, keys, value)
            write_json(case / EVIDENCE, document)
            if run(case) == 0:
                raise SystemExit(f"stage8b-p-r2b-implementation-negative: FAIL accepted {name}")
            passed += 1

        for name, relative, old, new in text_cases:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            target = case / relative
            source = target.read_text(encoding="utf-8")
            if source.count(old) < 1:
                raise SystemExit(f"stage8b-p-r2b-implementation-negative: FAIL setup {name}")
            target.write_text(source.replace(old, new, 1), encoding="utf-8")
            rebind_artifact(case, relative)
            if run(case) == 0:
                raise SystemExit(f"stage8b-p-r2b-implementation-negative: FAIL accepted {name}")
            passed += 1

        # A separately created activation source must also fail.
        case = Path(temporary) / "timer-activates-aggregate"
        shutil.copytree(base, case)
        timer = case / "deploy/stage8b-r2b/forbidden.timer"
        timer.write_text("[Timer]\nUnit=moex-stage8b-r2b-issuance.target\n", encoding="utf-8")
        if run(case) == 0:
            raise SystemExit("stage8b-p-r2b-implementation-negative: FAIL accepted timer activation")
        passed += 1

    expected = len(authority_cases) + len(evidence_cases) + len(text_cases) + 1
    if passed != expected:
        raise SystemExit(f"stage8b-p-r2b-implementation-negative: FAIL {passed}/{expected}")
    print(f"stage8b-p-r2b-implementation-negative: PASS {passed}/{expected}")


if __name__ == "__main__":
    main()
