#!/usr/bin/env python3
"""Mutation controls for Stage 8B-P1-f O1 operational evidence."""

from __future__ import annotations

import json
import hashlib
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


def mutate_raw(root: Path, raw_path: str, old: str, new: str, digest_path: list[str]) -> None:
    target = root / raw_path
    raw = target.read_text()
    if raw.count(old) != 1:
        raise RuntimeError(f"raw mutation anchor drift: {old!r}")
    target.write_text(raw.replace(old, new))
    digest = hashlib.sha256(target.read_bytes()).hexdigest()
    mutate_json(root, digest_path, digest)


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
        ("raw-binary-hash", lambda root: mutate_raw(
            root, check.POST_RAW,
            "managed_5_sha256=cee324a4e4f251227a25d4a7b23dda332a94522b45407671982f4fc896614406",
            "managed_5_sha256=" + "0" * 64,
            ["post_install", "raw_probe_sha256"],
        )),
        ("raw-credentials-mode", lambda root: mutate_raw(
            root, check.POST_RAW, "directory_2_mode=700", "directory_2_mode=777",
            ["post_install", "raw_probe_sha256"],
        )),
        ("raw-p0-fingerprint", lambda root: mutate_raw(
            root, check.POST_RAW,
            "moex_finam_paper_runtime_fragment_sha256=8f8f2854191887a75317869c8e6ff3c8edd4197c1ae594fa93c0b56fc35585fc",
            "moex_finam_paper_runtime_fragment_sha256=" + "0" * 64,
            ["post_install", "raw_probe_sha256"],
        )),
        ("raw-pre-p1-present", lambda root: mutate_raw(
            root, check.PRE_RAW, "p1_path_0_present=false", "p1_path_0_present=true",
            ["pre_o0", "raw_probe_sha256"],
        )),
        ("raw-systemd-query-failed", lambda root: mutate_raw(
            root, check.POST_RAW, "p1_systemd_query_ok=true", "p1_systemd_query_ok=false",
            ["post_install", "raw_probe_sha256"],
        )),
        ("raw-enabled-runtime", lambda root: mutate_raw(
            root, check.POST_RAW, "p1_unit_0_unit_file_state=static", "p1_unit_0_unit_file_state=enabled-runtime",
            ["post_install", "raw_probe_sha256"],
        )),
        ("raw-empty-load-state", lambda root: mutate_raw(
            root, check.POST_RAW, "p1_unit_0_load_state=loaded", "p1_unit_0_load_state=",
            ["post_install", "raw_probe_sha256"],
        )),
        ("probe-swallows-error", lambda root: (root / check.PROBE).write_text(
            (root / check.PROBE).read_text().replace(
                'if ! load_state="$(unit_value "$unit" LoadState)"; then',
                'load_state="$(unit_value "$unit" LoadState 2>/dev/null || true)"; if false; then',
            )
        )),
        ("probe-removes-query-proof", lambda root: (root / check.PROBE).write_text(
            (root / check.PROBE).read_text().replace("kv p1_systemd_query_ok true", "kv p1_systemd_query_ok false")
        )),
    ]
    source_paths = check.ALLOWED_CHANGES | check.RETAINED_REQUIRED
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
