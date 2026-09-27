#!/usr/bin/env python3
"""Mutation tests for the Stage 8B-P1-f O0 read-only preflight."""

from __future__ import annotations

import shutil
import tempfile
from pathlib import Path

import stage8b_p1f_o0_check as check


def copy_contract(root: Path) -> None:
    for relative in check.ALLOWED_CHANGES:
        source = check.ROOT / relative
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def replace(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError(f"mutation anchor count for {relative}: {old!r}")
    path.write_text(text.replace(old, new, 1))


CASES = (
    ("target-ip", check.RAW, "ipv4=45.150.11.252", "ipv4=45.150.11.253"),
    ("host-key", check.RAW, "SHA256:8fOAPkfvZ61LYmldUBs6A5ZC+BFr6O3yCvUrBAWZsAo", "SHA256:wrong"),
    ("ntp", check.RAW, "ntp_synchronized=yes", "ntp_synchronized=no"),
    ("redis-server-version", check.RAW, "redis_server_version=7.0.15", "redis_server_version=6.2.13"),
    ("redis-server-version-missing", check.RAW, "redis_server_version=7.0.15\n", ""),
    ("redis-listener", check.RAW, "redis_listeners=127.0.0.1:6379,[::1]:6379", "redis_listeners=0.0.0.0:6379"),
    ("db15-size", check.RAW, "redis_db15_size=0", "redis_db15_size=1"),
    ("db15-digest", check.RAW, f"redis_db15_keyspace_sha256={check.collect.EMPTY_SHA256}", "redis_db15_keyspace_sha256=" + "1" * 64),
    ("p0-inactive", check.RAW, "moex_finam_paper_runtime_active_state=active", "moex_finam_paper_runtime_active_state=inactive"),
    ("p0-hash", check.RAW, check.EXPECTED_SERVICES["moex-finam-paper-runtime.service"]["fragment_sha256"], "2" * 64),
    ("p1-user", check.RAW, "p1_service_user_present=false", "p1_service_user_present=true"),
    ("p1-group", check.RAW, "p1_service_group_present=false", "p1_service_group_present=true"),
    ("p1-path", check.RAW, "p1_path_0_present=false", "p1_path_0_present=true"),
    ("p1-recovery-artifact", check.RAW, "p1_path_4_present=false", "p1_path_4_present=true"),
    ("p1-unit-loaded", check.RAW, "p1_unit_0_load_state=not-found", "p1_unit_0_load_state=loaded"),
    ("p1-recovery-instance", check.RAW, "p1_recovery_instances_count=0", "p1_recovery_instances_count=1"),
    ("p1-systemd-query-error", check.RAW, "p1_systemd_query_ok=true", "p1_systemd_query_ok=false"),
    ("mutation-marker", check.RAW, "remote_mutation_performed=false", "remote_mutation_performed=true"),
    ("duplicate-raw-key", check.RAW, "schema_version=1\n", "schema_version=1\nschema_version=1\n"),
    ("normalized-evidence", check.EVIDENCE, '"db0_size": 6', '"db0_size": 7'),
    ("probe-target", check.PROBE, 'target="root@45.150.11.252"', 'target="root@127.0.0.1"'),
    ("probe-mutation", check.PROBE, "set -euo pipefail\n\nkv()", "set -euo pipefail\nmkdir /tmp/forbidden\n\nkv()"),
    ("matrix-row", check.MATRIX, "P1FO0-020,next,Only independent O0 acceptance may authorize a separate O1 package,REQUIRED\n", ""),
    ("document-self-accept", check.DOCUMENT, "REVIEW_CANDIDATE_R2_SYSTEMD_QUERY_FAIL_CLOSED_NO_MUTATION", "ACCEPTED"),
    ("document-opens-o1", check.DOCUMENT, "O0 does not authorize O1", "O0 authorizes O1"),
    ("status-opens-o1", check.STATUS, "independent O0 correction acceptance", "immediate O1 activation"),
    ("roadmap-opens-o1", check.ROADMAP, "cannot authorize O1 without independent", "authorizes O1 without independent"),
)


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="stage8b-p1f-o0-positive-") as raw:
        root = Path(raw)
        copy_contract(root)
        check.validate(root, check_lineage=False)
        print("PASS positive-control")
        path = root / check.DOCUMENT
        path.write_text(path.read_text() + "\n")
        check.validate(root, check_lineage=False)
        print("PASS nonsemantic-control")
    for name, relative, old, new in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-p1f-o0-{name}-") as raw:
            root = Path(raw)
            copy_contract(root)
            replace(root, relative, old, new)
            try:
                check.validate(root, check_lineage=False)
            except (check.CheckFailure, OSError, UnicodeDecodeError, ValueError, KeyError, TypeError):
                print(f"PASS {name}")
            else:
                raise SystemExit(f"FAIL mutation survived: {name}")
    print(f"PASS stage8b-p1f-o0-negative-harness {len(CASES)}/{len(CASES)}")


if __name__ == "__main__":
    main()
