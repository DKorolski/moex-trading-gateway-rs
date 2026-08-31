#!/usr/bin/env python3
"""Targeted negative matrix for the R2B issuance-package R0 design."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage8b_p_r2b_issuance_check.py"
AUTHORITY = "docs/stage-8/stage8b-p-r2b-issuance-package-r0-authority.json"
FILES = (
    AUTHORITY,
    "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_2026-08-29.md",
    "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_ACCEPTANCE_MATRIX_2026-08-29.csv",
    "docs/stage-8/stage8b-p-r2b-issuance-package-r0-evidence.json",
    CHECKER,
    "scripts/stage8b_p_r2b_issuance_systemd_check.py",
    "deploy/stage8b-r2b/moex-stage8b-r2a8-upstream-current-authority-publisher.service",
    "deploy/stage8b-r2b/moex-stage8b-r2a8-authoritative-intake-creator.service",
    "deploy/stage8b-r2b/moex-stage8b-r2a8-production-intake-stager.service",
    "deploy/stage8b-r2b/moex-stage8b-r2a8-production-current-source-writer.service",
    "deploy/stage8b-r2a5/stage8b-r2a8-current-manifest-issuer.service",
    "deploy/stage8b-r2a5/stage8b-r2a7-source-adapter.service",
    "deploy/stage8b-r2a5/stage8b-r2a5-producer@.service",
    "deploy/stage8b-r2a5/stage8b-r2a5-issuer@.service",
    "deploy/stage8b-r2b/moex-stage8b-r2b-readonly-supervisor.service",
)


def run(root: Path) -> int:
    return subprocess.run(
        ["python3", str(root / CHECKER)], cwd=root,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    ).returncode


def main() -> None:
    json_cases = (
        ("authorization-issued", ("authorization", "r2b"), "ISSUED"),
        ("target-implemented", ("future_activation_target", "implemented_by_r0"), True),
        ("manual-start-open", ("future_activation_target", "manual_start_allowed"), True),
        ("operator-selected", ("operator_local_inputs", "operation"), "PLACE"),
        ("signed-package-present", ("operator_local_inputs", "signed_run_package"), "present"),
        ("credential-used", ("closed_surfaces", "finam_credentials_accessed"), True),
        ("broker-get-sent", ("closed_surfaces", "broker_account_get_sent"), True),
        ("post-sent", ("closed_surfaces", "order_post_sent"), True),
        ("runtime-open", ("closed_surfaces", "runtime_live"), True),
        ("transaction-count-drift", ("transaction", "service_invocation_count"), 29),
        ("contract-refresh-drift", ("fresh_public_contract_refresh", "all_bytes_and_hashes_match"), False),
    )
    text_cases = (
        ("unsupported-issuer-condition", "deploy/stage8b-r2a5/stage8b-r2a8-current-manifest-issuer.service", "RefuseManualStart=yes", "RefuseManualStart=yes\nConditionPathIsRegular=/tmp/input"),
        ("adapter-refuse-removed", "deploy/stage8b-r2a5/stage8b-r2a7-source-adapter.service", "RefuseManualStart=yes", "# removed"),
        ("producer-refuse-wrong-section", "deploy/stage8b-r2a5/stage8b-r2a5-producer@.service", "RefuseManualStart=yes\nConditionPathExists", "ConditionPathExists"),
        ("issuer-unknown-key", "deploy/stage8b-r2a5/stage8b-r2a5-issuer@.service", "Description=MOEX Stage 8B R2A5 authority issuer UID %i", "Description=MOEX Stage 8B R2A5 authority issuer UID %i\nUnknownDirective=yes"),
        ("supervisor-refuse-removed", "deploy/stage8b-r2b/moex-stage8b-r2b-readonly-supervisor.service", "RefuseManualStart=yes", "# removed"),
    )
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8b-r2b-issuance-negative-") as temporary:
        base = Path(temporary) / "base"
        for relative in FILES:
            destination = base / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        if run(base) != 0:
            raise SystemExit("stage8b-p-r2b-issuance-negative: FAIL baseline")

        for name, path, value in json_cases:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            target = case / AUTHORITY
            document = json.loads(target.read_text(encoding="utf-8"))
            current = document
            for key in path[:-1]:
                current = current[key]
            current[path[-1]] = value
            target.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
            if run(case) == 0:
                raise SystemExit(f"stage8b-p-r2b-issuance-negative: FAIL accepted {name}")
            passed += 1

        for name, relative, old, new in text_cases:
            case = Path(temporary) / name
            shutil.copytree(base, case)
            target = case / relative
            source = target.read_text(encoding="utf-8")
            if source.count(old) < 1:
                raise SystemExit(f"stage8b-p-r2b-issuance-negative: FAIL setup {name}")
            target.write_text(source.replace(old, new, 1), encoding="utf-8")
            if run(case) == 0:
                raise SystemExit(f"stage8b-p-r2b-issuance-negative: FAIL accepted {name}")
            passed += 1

    expected = len(json_cases) + len(text_cases)
    if passed != expected:
        raise SystemExit(f"stage8b-p-r2b-issuance-negative: FAIL {passed}/{expected}")
    print(f"stage8b-p-r2b-issuance-negative: PASS {passed}/{expected}")


if __name__ == "__main__":
    main()
