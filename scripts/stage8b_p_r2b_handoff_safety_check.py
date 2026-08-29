#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P R2B Proposal R4 handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

EVIDENCE = "handoff-evidence/stage8b-p-r2b-r4-evidence.json"
GATE = "handoff-evidence/stage8b-p-r2b-r4-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
NEGATIVE_MUTATIONS = 269
BINARIES = {
    "upstream_publisher": "handoff-evidence/linux-amd64/production/stage8b-r2a8-upstream-current-authority-publisher",
    "authoritative_creator": "handoff-evidence/linux-amd64/production/stage8b-r2a8-authoritative-intake-creator",
    "production_stager": "handoff-evidence/linux-amd64/production/stage8b-r2a8-production-intake-stager",
    "production_writer": "handoff-evidence/linux-amd64/production/stage8b-r2a8-production-current-source-writer",
    "production_manifest_issuer": "handoff-evidence/linux-amd64/production/stage8b-r2a8-current-manifest-issuer",
    "production_adapter": "handoff-evidence/linux-amd64/production/stage8b-r2a7-source-adapter",
    "authority_producer": "handoff-evidence/linux-amd64/tools/stage8b-r2a5-authority-producer",
    "authority_issuer": "handoff-evidence/linux-amd64/tools/stage8b-r2a5-authority-issuer",
    "package_issuer": "handoff-evidence/linux-amd64/tools/stage8b-r2a5-package-issuer",
    "production_launcher": "handoff-evidence/linux-amd64/tools/stage8b-r2b-launcher",
    "accepted_helper": "handoff-evidence/linux-amd64/tools/stage8b-readonly-preflight",
    "controlled_adapter": "handoff-evidence/linux-amd64/controlled/stage8b-r2a7-source-adapter",
    "controlled_manifest_issuer": "handoff-evidence/linux-amd64/controlled/stage8b-r2a8-current-manifest-issuer",
    "controlled_seeder": "handoff-evidence/linux-amd64/controlled/stage8b-r2a7-controlled-seeder",
    "creator_chain_seeder": "handoff-evidence/linux-amd64/controlled/stage8b-r2b-creator-chain-seeder",
    "controlled_tls_server": "handoff-evidence/linux-amd64/controlled/stage8b-r2a5-controlled-server",
    "controlled_layout": "handoff-evidence/linux-amd64/controlled/stage8b-r2a5-controlled-layout",
    "controlled_launcher": "handoff-evidence/linux-amd64/controlled/stage8b-r2b-launcher",
}
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST, *BINARIES.values()}
REQUIRED = GENERATED | {
    "docs/stage-8/STAGE8B_P_R2B_PROPOSAL_2026-08-27.md",
    "docs/stage-8/STAGE8B_P_R2B_PROPOSAL_ACCEPTANCE_MATRIX_2026-08-27.csv",
    "docs/stage-8/stage8b-p-r2b-proposal-authority.json",
    "docs/stage-8/stage8b-p-r2b-runtime-composition-contract.json",
    "docs/stage-8/stage8b-p-r2b-r4-build-evidence.json",
    "scripts/stage8b_p_r2b_proposal_gate.sh",
    "scripts/stage8b_p_r2b_proposal_check.py",
    "scripts/stage8b_p_r2b_proposal_negative_harness.py",
    "scripts/stage8b_p_r2b_r3_linux_custody_rehearsal.sh",
}


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        if len(names) != len(set(names)):
            raise ValueError("duplicate members")
        if missing := REQUIRED - set(names):
            raise ValueError(f"missing members: {sorted(missing)}")
        for item in infos:
            member = PurePosixPath(item.filename)
            mode = (item.external_attr >> 16) & 0o177777
            if member.is_absolute() or ".." in member.parts or "" in member.parts:
                raise ValueError(f"unsafe path: {item.filename}")
            if mode & 0o170000 == 0o120000:
                raise ValueError(f"symlink: {item.filename}")
            if mode & 0o170000 not in {0, 0o100000, 0o040000}:
                raise ValueError(f"special file: {item.filename}")
            if member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden root: {item.filename}")
            if ".env" in member.parts or item.filename.endswith((".log", ".pem", ".key")):
                raise ValueError(f"secret/runtime artifact: {item.filename}")

        marker = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        manifest = json.loads(archive.read(MANIFEST))
        build = json.loads(archive.read("docs/stage-8/stage8b-p-r2b-r4-build-evidence.json"))
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        expected_closed = {
            "authorization_status": "NOT_ISSUED",
            "finam_network_accessed": False,
            "order_post_delete_sent": False,
            "redis_live_accessed": False,
            "broker_dispatch_entered": False,
            "runtime_live_entered": False,
            "real_orders_sent": False,
        }
        for name, expected in expected_closed.items():
            if evidence.get(name) != expected:
                raise ValueError(f"closure field drift: {name}")
        expected_hardening = {
            "root_authenticated_admission": True,
            "immutable_root_terminal_evidence": True,
            "authoritative_intake_creator": True,
            "same_uid_isolation": True,
            "typed_terminal_protocol": True,
            "absolute_supervisor_deadline": True,
            "creator_to_stager_rehearsed": True,
            "post_chmod_metadata_fsync": True,
            "full_admission_to_terminal_supervisor": True,
        }
        for name, expected in expected_hardening.items():
            if evidence.get(name) != expected:
                raise ValueError(f"hardening field drift: {name}")
        if evidence.get("negative_mutations") != NEGATIVE_MUTATIONS:
            raise ValueError("negative coverage mismatch")
        gate = archive.read(GATE)
        if b"stage8b-p-r2b-proposal-gate: PASS revision=R4" not in gate or sha(gate) != evidence.get("gate_sha256"):
            raise ValueError("gate evidence mismatch")

        production = build["production_binaries"]
        controlled = build["controlled_qualification_binaries"]
        expected_hashes = {
            "upstream_publisher": production["stage8b-r2a8-upstream-current-authority-publisher"]["build_a_sha256"],
            "authoritative_creator": production["stage8b-r2a8-authoritative-intake-creator"]["build_a_sha256"],
            "production_stager": production["stage8b-r2a8-production-intake-stager"]["build_a_sha256"],
            "production_writer": production["stage8b-r2a8-production-current-source-writer"]["build_a_sha256"],
            "production_manifest_issuer": production["stage8b-r2a8-current-manifest-issuer"]["build_a_sha256"],
            "production_adapter": production["stage8b-r2a7-source-adapter"]["build_a_sha256"],
            "authority_producer": production["stage8b-r2a5-authority-producer"]["build_a_sha256"],
            "authority_issuer": production["stage8b-r2a5-authority-issuer"]["build_a_sha256"],
            "package_issuer": production["stage8b-r2a5-package-issuer"]["build_a_sha256"],
            "production_launcher": production["stage8b-r2b-launcher"]["build_a_sha256"],
            "accepted_helper": production["accepted-stage8b-readonly-preflight"]["build_a_sha256"],
            "controlled_adapter": controlled["stage8b-r2a7-source-adapter"]["build_a_sha256"],
            "controlled_manifest_issuer": controlled["stage8b-r2a8-current-manifest-issuer"]["build_a_sha256"],
            "controlled_seeder": controlled["stage8b-r2a7-controlled-seeder"]["build_a_sha256"],
            "creator_chain_seeder": controlled["stage8b-r2b-creator-chain-seeder"]["build_a_sha256"],
            "controlled_tls_server": controlled["stage8b-r2a5-controlled-server"]["build_a_sha256"],
            "controlled_layout": controlled["stage8b-r2a5-controlled-layout"]["build_a_sha256"],
            "controlled_launcher": controlled["stage8b-r2b-controlled-custody-launcher"]["build_a_sha256"],
        }
        for name, expected in expected_hashes.items():
            if sha(archive.read(BINARIES[name])) != expected:
                raise ValueError(f"binary mismatch: {name}")
        if sha(archive.read(MANIFEST)) != evidence.get("manifest_sha256"):
            raise ValueError("manifest digest mismatch")
        by_name = {item.filename: item for item in infos}
        tracked: set[str] = set()
        for entry in manifest["entries"]:
            name = entry["path"]
            data = archive.read(name)
            mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
            if name in tracked or len(data) != entry["size"] or sha(data) != entry["sha256"] or mode != entry["mode"]:
                raise ValueError(f"manifest mismatch: {name}")
            tracked.add(name)
        if manifest["entry_count"] != len(tracked) or set(names) - tracked != GENERATED:
            raise ValueError("member inventory mismatch")
        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "linux_binaries_verified": len(BINARIES),
            "source_ref": source_ref,
            **expected_closed,
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p_r2b_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2b-handoff-safety: FAIL {error}") from error
    print("stage8b-p-r2b-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
