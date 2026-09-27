#!/usr/bin/env python3
"""Validate the Stage 8B-P1-f O2 execution-contract package."""

from __future__ import annotations

import csv
import hashlib
import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONTRACT = "docs/stage-8/stage8b-p1f-o2-execution-package.json"
MATRIX = "docs/stage-8/stage8b-p1f-o2-acceptance-matrix.csv"
PREDECESSOR = "e11744e31f11d567633716f21211c484bb9045eb"
EXPECTED_HASHES = {
    "docs/stage-8/stage8b-p1f-o1-governance-closure.json": "6d663cce47a38c859e5e0649774eb453a6d1d433abfe0fcf36e40be8fa7034fa",
    "docs/stage-8/stage8b-p1f-isolated-operational-acceptance-design.json": "0077a7dbfdca4cbf2323726bffa2208125427b7c7cafc20cc0a9e848cdf6b0cd",
    "docs/stage-8/stage8b-p1e-first-boot-source-plan-v2.json": "2a507577075b8b5315a462ffeee221dd0a7f8a8f61d42516fbdb9346cc3464ca",
    "docs/stage-8/stage8b-p1e-deployment-identity-v2.json": "428415fdedd3fd24ac128ee2ca703a6572e57644cad0cb40b30a9c96ea62a038",
    "deploy/stage8b-p1e/moex-finam-p1-paper-bootstrap.service": "f5f6db6e08d45f39fa3976701f78526a55e1485f3398f4140d13eaba1b2bc62b",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def strict_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate(document: dict[str, object], root: Path = ROOT) -> None:
    require(document.get("schema_version") == 1, "schema drift")
    require(
        document.get("status") == "R0_EXECUTION_CONTRACT_REVIEW_CANDIDATE_DO_NOT_EXECUTE",
        "status opened",
    )
    predecessor = document["accepted_predecessor"]
    require(predecessor["o1_governance_closure_commit"] == PREDECESSOR, "O1 closure drift")
    require(
        predecessor["o1_operational_commit"]
        == "997e8a1d201048fcdec0e948660f32a0bee3cceb",
        "O1 operational drift",
    )
    require(
        predecessor["installed_binary_sha256"]
        == "cee324a4e4f251227a25d4a7b23dda332a94522b45407671982f4fc896614406",
        "installed binary drift",
    )
    target = document["target"]
    require(target == {
        "target_id": "stage8b-p1f-isolated-vps-1",
        "hostname": "nektodk1.ispvds.com",
        "ipv4": "45.150.11.252",
        "ssh_ed25519_fingerprint": "SHA256:8fOAPkfvZ61LYmldUBs6A5ZC+BFr6O3yCvUrBAWZsAo",
    }, "target drift")
    subphases = document["subphases"]
    require([item["id"] for item in subphases] == ["P1F-O2-M", "P1F-O2-B"], "subphase drift")
    require("no Redis" in subphases[0]["network"] and "no order endpoint" in subphases[0]["network"], "O2-M network widened")
    require(subphases[1]["network"] == "PrivateNetwork=yes and AF_UNIX only", "O2-B network widened")
    paths = document["fixed_paths"]
    require(paths == {
        "control_root": "/var/lib/moex-finam-p1-paper-control",
        "config": "/etc/moex-finam-p1-paper/supervisor.json",
        "source": "/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json",
        "lifecycle_credential": "/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key",
        "durable_state": "/var/lib/moex-finam-p1-paper/state",
        "bootstrap_unit": "moex-finam-p1-paper-bootstrap.service",
    }, "fixed path drift")
    require(document["freshness"]["broker_truth_max_age_seconds_at_bootstrap"] == 300, "freshness widened")
    require(document["source_contract"]["wire_schema_version"] == 2, "wire version drift")
    require(document["bootstrap"] == {
        "daemon_reload_allowed_once_after_full_reread": True,
        "enable_allowed": False,
        "command": "systemctl start --wait moex-finam-p1-paper-bootstrap.service",
        "ordinary_service_start_allowed": False,
        "redis_contact_allowed": False,
        "finam_contact_allowed": False,
    }, "bootstrap boundary drift")
    require(all(value is False for value in document["package_boundary"].values()), "package performed an effect")
    require(all(value is False for value in document["closed_surfaces"].values()), "closed surface opened")
    require(len(document["facades_required_in_later_execution_artifact"]) == 5, "facade inventory drift")
    for path, digest in EXPECTED_HASHES.items():
        require(sha256(root / path) == digest, f"accepted input drift: {path}")
    guardian = (root / "crates/runtime-durable-service/src/stage8b_p1f_guardian.rs").read_text()
    for marker in (
        "pub fn initialize_authority(",
        "pub fn activate_authority(",
        "pub fn claim_phase(",
        "pub fn materialize_o2(",
        "pub fn admit_active_phase(",
        "pub fn finish_phase(",
        'state: "ReadyForBootstrap".to_string()',
        "if !(0..=300).contains(&age)",
    ):
        require(marker in guardian, f"guardian seam missing: {marker}")
    unit = (root / "deploy/stage8b-p1e/moex-finam-p1-paper-bootstrap.service").read_text()
    for marker in ("Type=oneshot", "PrivateNetwork=yes", "RestrictAddressFamilies=AF_UNIX"):
        require(marker in unit, f"bootstrap unit drift: {marker}")


def main() -> None:
    document = json.loads((ROOT / CONTRACT).read_text(), object_pairs_hook=strict_object)
    validate(document)
    with (ROOT / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 30, "acceptance row count drift")
    require([row["id"] for row in rows] == [f"O2R0-{index:02d}" for index in range(1, 31)], "acceptance id drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "non-required acceptance row")
    print("PASS stage8b-p1f-o2-check rows=30 execution=false")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"stage8b-p1f-o2-check: FAIL {error}")
        raise SystemExit(1)

