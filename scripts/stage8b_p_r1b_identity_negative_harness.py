#!/usr/bin/env python3
"""Reject the 36 Stage 8B-P R1B endpoint/run identity mutations."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage8b_p_r1b_identity_check.py"
AUTHORITY = "docs/stage-8/stage8b-p-r1b-authorization-authority.json"
NETWORK = "docs/stage-8/stage8b-p-r1b-network-endpoint-authority.json"
RUN = "docs/stage-8/stage8b-p-r1b-run-identity-authority.json"


def mutate(root: Path, path: str, keys: tuple[str, ...], value: Any) -> None:
    target_path = root / path
    document = json.loads(target_path.read_text())
    target = document
    for key in keys[:-1]:
        target = target[key]
    target[keys[-1]] = value
    target_path.write_text(json.dumps(document, indent=2) + "\n")


def edit_list(root: Path, path: str, keys: tuple[str, ...], edit: Callable[[list[Any]], None]) -> None:
    target_path = root / path
    document = json.loads(target_path.read_text())
    target = document
    for key in keys:
        target = target[key]
    edit(target)
    target_path.write_text(json.dumps(document, indent=2) + "\n")


def delete_key(root: Path, path: str, keys: tuple[str, ...]) -> None:
    target_path = root / path
    document = json.loads(target_path.read_text())
    target = document
    for key in keys[:-1]:
        target = target[key]
    del target[keys[-1]]
    target_path.write_text(json.dumps(document, indent=2) + "\n")


def case(path: str, keys: tuple[str, ...], value: Any) -> Callable[[Path], None]:
    return lambda root: mutate(root, path, keys, value)


def list_case(path: str, keys: tuple[str, ...], edit: Callable[[list[Any]], None]) -> Callable[[Path], None]:
    return lambda root: edit_list(root, path, keys, edit)


def cases() -> list[tuple[str, Callable[[Path], None]]]:
    a, n, r = AUTHORITY, NETWORK, RUN
    return [
        ("r1a-authority-digest", case(a, ("lineage", "r1a_authority_sha256"), "0" * 64)),
        ("network-authority-digest", case(a, ("identity_authorities", "network_endpoint_sha256"), "0" * 64)),
        ("run-authority-digest", case(a, ("identity_authorities", "accepted_run_sha256"), "0" * 64)),
        ("endpoint-domain", case(n, ("endpoint_identity", "domain_utf8"), "moex-stage8b-endpoint-identity-v1")),
        ("endpoint-encoding", case(n, ("endpoint_identity", "encoding"), "nul_delimited")),
        ("endpoint-operation-component", case(n, ("endpoint_identity", "operation_as_extra_component_allowed"), True)),
        ("endpoint-nul-encoding", case(n, ("endpoint_identity", "nul_delimited_encoding_allowed"), True)),
        ("endpoint-component-reorder", list_case(n, ("endpoint_identity", "parts_in_exact_order"), lambda x: x.reverse())),
        ("endpoint-length-width", case(n, ("endpoint_identity", "length_prefix_width_bits"), 32)),
        ("endpoint-byte-order", case(n, ("endpoint_identity", "length_prefix_byte_order"), "little_endian")),
        ("place-method", case(n, ("operations", "PLACE", "method"), "DELETE")),
        ("cancel-route", case(n, ("operations", "CANCEL", "route_template_id"), "PlaceOrderV1")),
        ("golden-account", case(n, ("golden_vectors", "keyed_account_binding_sha256"), "0" * 64)),
        ("golden-renderer", case(n, ("golden_vectors", "endpoint_renderer_sha256"), "0" * 64)),
        ("golden-place-endpoint", case(n, ("golden_vectors", "place_endpoint_identity_sha256"), "0" * 64)),
        ("golden-cancel-endpoint", case(n, ("golden_vectors", "cancel_endpoint_identity_sha256"), "0" * 64)),
        ("run-domain", case(r, ("run_identity", "domain_utf8"), "stage8b-p-run-v0")),
        ("run-encoding", case(r, ("run_identity", "encoding"), "canonical_json")),
        ("run-common-order", list_case(r, ("run_identity", "common_fields_in_exact_order_excluding_run_identity"), lambda x: x.reverse())),
        ("run-place-order", list_case(r, ("run_identity", "place_fields_in_exact_order"), lambda x: x.reverse())),
        ("run-cancel-order", list_case(r, ("run_identity", "cancel_fields_in_exact_order"), lambda x: x.reverse())),
        ("run-operation-omitted", case(r, ("run_identity", "operation_discriminator_included"), False)),
        ("run-self-included", case(r, ("run_identity", "run_identity_field_included_in_own_preimage"), True)),
        ("run-not-computed", case(r, ("run_identity", "computed_and_verified_not_caller_asserted"), False)),
        ("caller-run-digest", case(r, ("validation", "caller_asserted_unverified_digest_allowed"), True)),
        ("place-run-golden", case(r, ("golden_vectors", "PLACE", "run_identity_sha256"), "0" * 64)),
        ("cancel-run-golden", case(r, ("golden_vectors", "CANCEL", "run_identity_sha256"), "0" * 64)),
        ("place-endpoint-old-run", case(r, ("golden_vectors", "PLACE", "manifest_without_run_identity_sha256", "endpoint_identity_sha256"), "0" * 64)),
        ("place-body-old-run", case(r, ("golden_vectors", "PLACE", "manifest_without_run_identity_sha256", "place_request_body_sha256"), "0" * 64)),
        ("place-freshness-old-run", case(r, ("golden_vectors", "PLACE", "manifest_without_run_identity_sha256", "freshness_budget_authority_sha256"), "0" * 64)),
        ("place-build-old-run", case(r, ("golden_vectors", "PLACE", "manifest_without_run_identity_sha256", "execution_build_identity_sha256"), "0" * 64)),
        ("cancel-operation-missing", lambda root: delete_key(root, r, ("golden_vectors", "CANCEL", "manifest_without_run_identity_sha256", "operation"))),
        ("position-noncanonical", case(r, ("golden_vectors", "PLACE", "manifest_without_run_identity_sha256", "approved_pre_run_position"), "+0")),
        ("generation-noncanonical", case(r, ("golden_vectors", "PLACE", "manifest_without_run_identity_sha256", "kill_switch_generation"), "017")),
        ("expiry-noncanonical", case(r, ("golden_vectors", "PLACE", "manifest_without_run_identity_sha256", "run_expires_at_utc"), "2026-08-25T12:00:00Z")),
        ("authorization-issued", case(a, ("authorization", "status"), "ISSUED")),
    ]


def main() -> None:
    mutations = cases()
    if len(mutations) != 36:
        raise SystemExit(f"stage8b-p-r1b-identity-negative: FAIL inventory={len(mutations)}")
    with tempfile.TemporaryDirectory(prefix="stage8b-p-r1b-negative-") as temp:
        root = Path(temp) / "root"
        shutil.copytree(ROOT, root, ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports"))
        originals = {path: (root / path).read_bytes() for path in (AUTHORITY, NETWORK, RUN)}
        for index, (name, mutation) in enumerate(mutations, 1):
            for path, content in originals.items():
                (root / path).write_bytes(content)
            mutation(root)
            result = subprocess.run(
                ["python3", CHECKER, "--no-git"], cwd=root, text=True,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r1b-identity-negative: FAIL mutation passed: {name}")
            print(f"PASS {index:02d}/36 {name}")
    print("stage8b-p-r1b-identity-negative: PASS 36/36 inherited=98 total=134")


if __name__ == "__main__":
    main()
