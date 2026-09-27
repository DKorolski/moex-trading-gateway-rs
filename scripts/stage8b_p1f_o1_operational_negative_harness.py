#!/usr/bin/env python3
"""Mutation controls for Stage 8B-P1-f O1 operational evidence."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage8b_p1f_o1_operational_check as check


ROOT = Path(__file__).resolve().parents[1]


def mutate_json(root: Path, path: list[str], value: object) -> None:
    target = root / check.EVIDENCE
    data = json.loads(target.read_text())
    cursor = data
    for component in path[:-1]:
        cursor = cursor[int(component)] if isinstance(cursor, list) else cursor[component]
    if isinstance(cursor, list):
        cursor[int(path[-1])] = value
    else:
        cursor[path[-1]] = value
    target.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")


def main() -> None:
    cases: list[tuple[str, callable]] = [
        ("package-ref", lambda root: mutate_json(root, ["accepted_package", "source_ref"], "0" * 40)),
        ("review-digest", lambda root: mutate_json(root, ["accepted_package", "acceptance_review_sha256"], "0" * 64)),
        ("installer-exit", lambda root: mutate_json(root, ["installation_execution", "installer_exit_code"], 1)),
        ("installer-result", lambda root: mutate_json(root, ["installation_execution", "installer_result"], "UNKNOWN")),
        ("status", lambda root: mutate_json(root, ["post_install", "status", "result"], "ABSENT")),
        ("activation", lambda root: mutate_json(root, ["post_install", "installation_manifest_content", "activation_performed"], True)),
        ("daemon-reload", lambda root: mutate_json(root, ["post_install", "installation_manifest_content", "daemon_reload_performed"], True)),
        ("redis-contact", lambda root: mutate_json(root, ["post_install", "installation_manifest_content", "redis_contact_performed"], True)),
        ("operator-material", lambda root: mutate_json(root, ["post_install", "operator_files", "0", "present"], True)),
        ("state-created", lambda root: mutate_json(root, ["post_install", "state_extra_entry_count"], 1)),
        ("process-running", lambda root: mutate_json(root, ["post_install", "p1_process_count"], 1)),
        ("unit-active", lambda root: mutate_json(root, ["post_install", "p1_units", "0", "active"], "active")),
        ("p0-stopped", lambda root: mutate_json(root, ["post_install", "p0_services", "moex-finam-paper-runtime.service", "active_state"], "inactive")),
        ("db15-nonempty", lambda root: mutate_json(root, ["post_install", "redis", "db15_size"], 1)),
        ("check-false", lambda root: mutate_json(root, ["checks", "status_exact_installed"], False)),
        ("surface-open", lambda root: mutate_json(root, ["closed_surfaces", "o2_bootstrap"], True)),
        ("pre-raw", lambda root: (root / check.PRE_RAW).write_text("drift\n")),
        ("post-raw", lambda root: (root / check.POST_RAW).write_text("drift\n")),
        ("matrix-row", lambda root: (root / check.MATRIX).write_text("id,requirement,evidence,expected\n")),
        ("documentation-opens-o2", lambda root: (root / check.DOCUMENT).write_text("O2 enabled\n")),
    ]
    source_paths = check.ALLOWED_CHANGES
    passed = 0
    for name, mutation in cases:
        with tempfile.TemporaryDirectory(prefix="stage8b-p1f-o1-operational-negative-") as temporary:
            root = Path(temporary)
            for path in source_paths:
                source = ROOT / path
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
            mutation(root)
            try:
                check.validate(root)
            except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError, check.CheckError):
                passed += 1
            else:
                raise SystemExit(f"stage8b-p1f-o1-operational-negative-harness: FAIL {name}")
    print(f"PASS stage8b-p1f-o1-operational-negative-harness {passed}/{len(cases)}")


if __name__ == "__main__":
    main()
