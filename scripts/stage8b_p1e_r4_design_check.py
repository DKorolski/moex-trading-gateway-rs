#!/usr/bin/env python3
"""Fail-closed semantic checker for Stage 8B-P1-e R4 design correction."""

from __future__ import annotations

import csv
import hashlib
import hmac
import io
import json
import pathlib
import subprocess
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "913424b73c5a83df2131a1f9a2901b78035bfc4c"
ACCEPTED = "c2a9e1246dfdd59f3a6297268de907dedcb19903"
R3_REVIEW_SHA256 = "e34d3992d40bdc0a488ef3c491d96c05e3793763464f676780f2594a5e66427b"

DOCS = "docs/stage-8/"
FILES = {
    "design": DOCS + "stage8b-p1e-deployable-supervisor-design-r4.md",
    "r1": DOCS + "stage8b-p1e-deployable-supervisor-r1-acceptance-matrix.csv",
    "r2": DOCS + "stage8b-p1e-deployable-supervisor-r2-acceptance-matrix.csv",
    "r3": DOCS + "stage8b-p1e-deployable-supervisor-r3-acceptance-matrix.csv",
    "r4": DOCS + "stage8b-p1e-deployable-supervisor-r4-acceptance-matrix.csv",
    "active": DOCS + "stage8b-p1e-active-acceptance-contract-v4.json",
    "semantic": DOCS + "stage8b-p1e-semantic-authority-registry-v4.json",
    "identity": DOCS + "stage8b-p1e-deployment-identity-v1.json",
    "transaction": DOCS + "stage8b-p1e-first-boot-transaction-v3.json",
    "receipt": DOCS + "stage8b-p1e-first-boot-receipt-v2.json",
    "digests": DOCS + "stage8b-p1e-derived-digests-v1.json",
    "golden": DOCS + "stage8b-p1e-derived-digests-v1-golden.json",
    "acquisition": DOCS + "stage8b-p1e-acquisition-model-v2.json",
    "precedence": DOCS + "stage8b-p1e-source-timer-precedence-v1.json",
    "operational": DOCS + "stage8b-p1e-operational-pretransition-matrix-v4.csv",
    "package": DOCS + "stage8b-p1e-authenticated-restart-package-v2.json",
    "outer": DOCS + "stage8b-p1e-restart-continuation-matrix-v2.csv",
    "evidence": DOCS + "stage8b-p1e-deployable-supervisor-r4-design-evidence.json",
    "status": "docs/current-status.md",
    "roadmap": "docs/roadmap.md",
}

EXPECTED_CHANGED = {
    FILES[name] for name in (
        "design", "r4", "active", "semantic", "identity", "transaction",
        "receipt", "digests", "golden", "acquisition", "precedence",
        "operational", "evidence", "status", "roadmap",
    )
} | {
    "scripts/stage8b_p1e_r4_design_check.py",
    "scripts/stage8b_p1e_r4_design_negative_harness.py",
    "scripts/stage8b_p1e_r4_design_gate.sh",
    "scripts/stage8b_p1e_r4_design_handoff_safety_check.py",
    "scripts/make_stage8b_p1e_r4_design_handoff.py",
}

SOURCE_EXPECTATIONS = {
    "r1": (88, "b937e5c2434c665914cc1e27db12aee5f0669cb96e8db9764ca3d1791a938c0c", {
        "P1ER1-002", "P1ER1-003", "P1ER1-004", "P1ER1-043", "P1ER1-074",
    }),
    "r2": (48, "6d43489edf8039ce7697f81395dd09cfde0d541eadf6ef5ae7df9e4b5774987b", {
        "P1ER2-002", "P1ER2-004", "P1ER2-005", "P1ER2-006", "P1ER2-012",
        "P1ER2-016", "P1ER2-017", "P1ER2-022", "P1ER2-026", "P1ER2-027",
        "P1ER2-028", "P1ER2-035", "P1ER2-036", "P1ER2-044",
    }),
    "r3": (43, "7da8c390d6474e32b0b680892cce335eb528d0d633fcc1e6f7ddedf0d1c7b342", {
        *{f"P1ER3-{number:03d}" for number in range(1, 23)},
        *{f"P1ER3-{number:03d}" for number in range(27, 34)},
        "P1ER3-036", "P1ER3-037", "P1ER3-041", "P1ER3-042", "P1ER3-043",
    }),
    "r4": (56, "af576620f7dc57b05e613b0f33f44c5da802248431e5b08b71bdf1de6b6ea70b", set()),
}

EXPECTED_SEMANTIC_VALUES = {
    "systemd.User": "moex-p1-paper",
    "systemd.Group": "moex-p1-paper",
    "systemd.BinaryPath": "/usr/local/libexec/moex/stage8b-p1-paper-supervisor",
    "systemd.ConfigPath": "/etc/moex-finam-p1-paper/supervisor.json",
    "systemd.CredentialPath": "/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key",
    "filesystem.DurableParentOwner": "root:moex-p1-paper:0750-parent-plus-moex-p1-paper:moex-p1-paper:0700-state",
    "filesystem.MarkerOwner": "moex-p1-paper:moex-p1-paper:0600",
    "filesystem.ReceiptOwner": "moex-p1-paper:moex-p1-paper:0600",
    "firstboot.MarkerUpdateProtocol": "four-phase-aware-authenticated-temp-pending-completions-no-effect-replay",
    "redis.NonReadyAcquisitionOwner": "S06-observation-only-existing-resume-wrapper-sole-reclaim",
    "redis.ReadyLinearDeliveryOwner": "Stage8bP1eClaimedM10DeliveryV2-with-owned-canonical-payload",
    "shutdown.PostDeliveryLatch": "all-Ready-and-non-Ready-sole-acquisition-paths-before-parse-callback-provider-schedule-XACK",
    "ordering.SourceBeforeTimer": "SOURCE_FIRST_TIMER_DEFERRED-reclassify-against-returned-owner",
    "classifier.UnlistedTupleDisposition": "exit-67-before-transition-no-Ready-no-fresh-poll",
}


class CheckFailure(RuntimeError):
    pass


def require(value: bool, message: str) -> None:
    if not value:
        raise CheckFailure(message)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_json_sha256(value: Any) -> str:
    data = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
    return sha256(data)


def csv_rows(data: bytes) -> list[dict[str, str]]:
    return list(csv.DictReader(io.StringIO(data.decode("utf-8"))))


def read_all(overrides: dict[str, bytes] | None = None) -> dict[str, bytes]:
    overrides = overrides or {}
    return {name: overrides.get(path, (ROOT / path).read_bytes()) for name, path in FILES.items()}


def load_json(blobs: dict[str, bytes], name: str) -> dict[str, Any]:
    value = json.loads(blobs[name])
    require(isinstance(value, dict), f"{name} must be object")
    return value


def changed_files() -> set[str]:
    tracked = subprocess.run(
        ["git", "diff", "--name-only", BASE], cwd=ROOT, check=True,
        text=True, capture_output=True,
    ).stdout.splitlines()
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT,
        check=True, text=True, capture_output=True,
    ).stdout.splitlines()
    return set(tracked) | set(untracked)


def encode_value(kind: str, value: Any) -> bytes:
    if kind == "digest32-raw":
        require(isinstance(value, str) and len(value) == 64 and value == value.lower(), "digest input format")
        try:
            result = bytes.fromhex(value)
        except ValueError as error:
            raise CheckFailure("digest input hex") from error
        require(len(result) == 32, "digest input length")
        return result
    if kind == "u64be":
        require(type(value) is int and 0 <= value <= (1 << 64) - 1, "u64 value")
        return value.to_bytes(8, "big")
    if kind == "u16be":
        require(type(value) is int and 0 <= value <= 65535, "u16 value")
        return value.to_bytes(2, "big")
    if kind == "bool-byte":
        require(type(value) is bool, "bool value")
        return b"\x01" if value else b"\x00"
    if kind.startswith("ascii-") or kind.startswith("ascii["):
        require(isinstance(value, str), "ascii value")
        try:
            return value.encode("ascii")
        except UnicodeEncodeError as error:
            raise CheckFailure("non-ascii value") from error
    raise CheckFailure(f"unknown field type: {kind}")


def encode_record(spec: dict[str, Any], inputs: dict[str, Any]) -> bytes:
    magic = b"M8BP1E01"
    schema = spec["schema_version"]
    domain = spec["domain_ascii"].encode("ascii")
    fields = spec["fields_in_order"]
    require(set(inputs) == {field["name"] for field in fields}, "golden input inventory drifted")
    result = bytearray(magic)
    result.extend(schema.to_bytes(2, "big"))
    result.extend(len(domain).to_bytes(2, "big"))
    result.extend(domain)
    result.extend(len(fields).to_bytes(2, "big"))
    for field in fields:
        name = field["name"].encode("ascii")
        value = encode_value(field["type"], inputs[field["name"]])
        result.extend(len(name).to_bytes(2, "big"))
        result.extend(name)
        result.extend(len(value).to_bytes(4, "big"))
        result.extend(value)
    return bytes(result)


def validate_active(blobs: dict[str, bytes], active: dict[str, Any]) -> None:
    require(active.get("schema_version") == 4, "active schema")
    require(active.get("domain") == "moex.stage8b.p1e.active-acceptance-contract.v4", "active domain")
    require(active.get("accepted_predecessor") == ACCEPTED and active.get("direct_parent") == BASE, "active lineage")
    sources = active.get("sources")
    require(isinstance(sources, list) and [item.get("name") for item in sources] == ["r1", "r2", "r3", "r4"], "source order")
    active_ids: set[str] = set()
    excluded: set[str] = set()
    all_ids: set[str] = set()
    active_counts: dict[str, int] = {}
    for source in sources:
        name = source["name"]
        count, digest, exact_excluded = SOURCE_EXPECTATIONS[name]
        data = blobs[name]
        rows = csv_rows(data)
        ids = {row["id"] for row in rows}
        require(len(rows) == count and len(ids) == count, f"{name} rows")
        require(all(row["status"] == "REQUIRED" for row in rows), f"{name} required")
        require(sha256(data) == digest == source.get("file_sha256"), f"{name} digest")
        actual_excluded = set(source.get("superseded_rows", []))
        require(actual_excluded == exact_excluded and actual_excluded <= ids, f"{name} supersession")
        require(not all_ids.intersection(ids), f"{name} duplicate ids")
        all_ids |= ids
        active_ids |= ids - actual_excluded
        excluded |= actual_excluded
        active_counts[name] = len(ids - actual_excluded)
    mapped: list[str] = []
    for item in active.get("supersession_map", []):
        old, new = item.get("superseded", []), item.get("replacement", [])
        require(old and new, "empty supersession map entry")
        require(all(value in active_ids for value in new), "inactive replacement")
        mapped.extend(old)
    require(len(mapped) == len(set(mapped)) and set(mapped) == excluded, "supersession map not exact")
    require(active_counts == {"r1": 83, "r2": 34, "r3": 9, "r4": 56}, "active counts")
    require(len(active_ids) == 182, "active total")
    require(active.get("active_contract_expectation") == {
        "r1_active_rows": 83, "r2_active_rows": 34, "r3_active_rows": 9,
        "r4_active_rows": 56, "total_active_rows": 182, "all_status_required": True,
    }, "active expectation")


def validate_semantic(registry: dict[str, Any]) -> None:
    require(registry.get("schema_version") == 4, "semantic schema")
    require(registry.get("domain") == "moex.stage8b.p1e.semantic-authority-registry.v4", "semantic domain")
    require(registry.get("conflict_rule") == "for-each-key-all-active-authorities-must-have-one-byte-identical-value", "semantic conflict rule")
    require(registry.get("required_keys") == list(EXPECTED_SEMANTIC_VALUES), "semantic required key order")
    require(registry.get("expected_active_values") == EXPECTED_SEMANTIC_VALUES, "semantic expected values")
    grouped: dict[str, set[str]] = {}
    for item in registry.get("authorities", []):
        require(item.get("key") in EXPECTED_SEMANTIC_VALUES, "unknown semantic key")
        if item.get("active") is True:
            grouped.setdefault(item["key"], set()).add(item["value"])
        else:
            require(item.get("active") is False and item.get("superseded_by"), "inactive authority unbound")
    require(set(grouped) == set(EXPECTED_SEMANTIC_VALUES), "semantic key coverage")
    for key, expected in EXPECTED_SEMANTIC_VALUES.items():
        require(grouped[key] == {expected}, f"semantic conflict: {key}")


def validate_identity(identity: dict[str, Any]) -> None:
    require(identity.get("domain") == "moex.stage8b.p1e.deployment-identity.v1", "identity domain")
    service = identity["service_identity"]
    require(service["user"] == service["group"] == "moex-p1-paper", "service identity")
    require(service["supplementary_deployment_groups_allowed"] is False, "supplementary group")
    paths = identity["paths"]
    expected_paths = {
        "main_unit": "moex-finam-p1-paper.service",
        "bootstrap_unit": "moex-finam-p1-paper-bootstrap.service",
        "bootstrap_recovery_template_unit": "moex-finam-p1-paper-bootstrap-recover@.service",
        "binary": "/usr/local/libexec/moex/stage8b-p1-paper-supervisor",
        "config": "/etc/moex-finam-p1-paper/supervisor.json",
        "credential_source": "/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key",
        "durable_parent": "/var/lib/moex-finam-p1-paper",
        "mutable_state_directory": "/var/lib/moex-finam-p1-paper/state",
    }
    require(all(paths.get(key) == value for key, value in expected_paths.items()), "canonical paths")
    shared = identity["shared_unit_contract"]
    require(shared["User"] == shared["Group"] == "moex-p1-paper", "shared unit identity")
    require(shared["NoNewPrivileges"] is True and shared["PrivateNetwork"] is True, "unit hardening")
    units = identity["units"]
    require({value["unit"] for value in units.values()} == {
        expected_paths["main_unit"], expected_paths["bootstrap_unit"], expected_paths["bootstrap_recovery_template_unit"],
    }, "unit inventory")
    for unit in units.values():
        require(unit["exec_start"].startswith(expected_paths["binary"] + " "), "unit binary drift")
        require(expected_paths["config"] in unit["exec_start"], "unit config drift")
    ownership = {row["object"]: row for row in identity["ownership_table"]}
    require(len(ownership) == 11, "ownership inventory")
    require(ownership["durable_parent"]["owner"] == "root" and ownership["durable_parent"]["group"] == "moex-p1-paper" and ownership["durable_parent"]["mode"] == "0750", "parent custody")
    for name in ("transaction_marker", "transaction_marker_temp", "accepted_receipt", "accepted_receipt_temp", "journal", "seal"):
        row = ownership[name]
        require(row["owner"] == row["group"] == "moex-p1-paper" and row["mode"] == "0600", f"{name} custody")
    for name in ("mutable_state_directory", "canonical_root", "quarantine_directory", "quarantined_transaction_root"):
        row = ownership[name]
        require(row["owner"] == row["group"] == "moex-p1-paper" and row["mode"] == "0700", f"{name} custody")
    checks = set(identity["post_create_validation"])
    require({"fstat-new-fd", "st_nlink-equals-1", "group-and-world-write-bits-absent"} <= checks, "post-create checks")
    require("O_NOFOLLOW" in " ".join(checks), "nofollow check")
    require(identity["alternate_identity_policy"].startswith("any-second-deployment"), "alternate identity")


def validate_first_boot(transaction: dict[str, Any], receipt: dict[str, Any]) -> None:
    require(transaction.get("schema_version") == 3 and transaction.get("domain") == "moex.stage8b.p1e.first-boot-transaction.v3", "transaction identity")
    require(len(transaction["base_classifications"]) == 10, "base class count")
    temps = transaction["marker_update_temp_classifications"]
    expected = {
        "PreparedToRootPublishedMarkerTempPending": ("prepared", "root_published"),
        "RootPublishedToJournalDurableMarkerTempPending": ("root_published", "journal_durable"),
        "JournalDurableToSealCommittedMarkerTempPending": ("journal_durable", "seal_committed"),
        "SealCommittedToAdoptedMarkerTempPending": ("seal_committed", "adopted"),
    }
    require(len(temps) == 4 and {row["classification"] for row in temps} == set(expected), "marker temp classes")
    for row in temps:
        require((row["expected_old_phase"], row["expected_temp_phase"]) == expected[row["classification"]], "phase pair")
        require(row["expected_generation_delta"] == 1 and row["required_phase_effect"] and row["forbidden_repeated_effect"], "marker temp evidence")
        require(row["final_phase"] == row["expected_temp_phase"], "marker final phase")
    protocol = transaction["marker_temp_completion_protocol"]
    require(len(protocol) == 10 and any("without-reexecuting" in value for value in protocol), "marker completion")
    require(len(transaction["marker_temp_conflicts"]) == 7, "marker conflict count")
    hooks = transaction["required_sigkill_hooks"]
    require(len(hooks) == 9 and sum("marker-temp-sync-before-rename" in value for value in hooks) == 4, "SIGKILL hooks")
    admin = transaction["administrative_boundary"]
    require(admin["user"] == admin["group"] == "moex-p1-paper", "recovery identity")
    require(admin["binary"] == EXPECTED_SEMANTIC_VALUES["systemd.BinaryPath"], "recovery binary")
    require(admin["config"] == EXPECTED_SEMANTIC_VALUES["systemd.ConfigPath"], "recovery config")
    require(admin["credential"] == EXPECTED_SEMANTIC_VALUES["systemd.CredentialPath"], "recovery credential")
    require(admin["action_grammar"] == "remove-marker-temp|resume-prepared|quarantine-root|finalize-quarantine|remove-receipt-temp-and-adopt|adopt-committed-root|complete-prepared-to-root-published|complete-root-published-to-journal-durable|complete-journal-durable-to-seal-committed|complete-seal-committed-to-adopted", "recovery action grammar")
    require(admin["direct_manual_execution"] == "not-an-accepted-production-boundary", "manual recovery")
    require(receipt.get("schema_version") == 2 and receipt.get("domain") == "moex.stage8b.p1e.first-boot-receipt.v2", "receipt identity")
    fields = receipt["fields_in_storage_order"]
    for required in ("transaction_id_sha256", "canonical_root_identity_sha256", "restart_package_canonical_sha256", "first_boot_provenance_canonical_sha256", "adoption_ready_owner_sha256", "receipt_hmac_sha256"):
        require(required in fields, f"receipt missing {required}")
    persistence = receipt["persistence"]
    require(persistence["owner"] == persistence["group"] == "moex-p1-paper" and persistence["mode"] == "0600", "receipt custody")
    require(persistence["commit_point"] == "final-rename-plus-parent-fsync", "receipt commit")
    require(len(receipt["replay_rejection"]) == 9, "receipt replay rejection")


def validate_acquisition(acquisition: dict[str, Any], precedence: dict[str, Any], operational: bytes, outer: bytes) -> None:
    require(acquisition.get("schema_version") == 2 and acquisition.get("selected_model", "").startswith("B-observation-only-S06"), "acquisition model")
    forbidden = set(acquisition["s06_non_ready"]["forbidden_redis_operations"])
    require(forbidden == {"XAUTOCLAIM", "XREADGROUP", "XACK"}, "S06 non-ready operations")
    owners = acquisition["non_ready_source_owners"]
    require(len(owners) == len(set(owners)) == 19, "non-ready owner inventory")
    rule = acquisition["non_ready_continuation_rule"]
    require(rule["maximum_successful_reclaims"] == 1 and "sole-reclaim" in rule["sole_reclaim_owner"], "sole reclaim")
    require("immediately-after-reclaim" in rule["post_reclaim_latch_check"], "non-ready latch")
    ready = acquisition["ready_paths"]
    require(ready["linear_type"] == "Stage8bP1eClaimedM10DeliveryV2", "linear owner type")
    require(acquisition["pre_transition_forbidden"] == [
        "strategy-callback-preview", "callback-result-classification",
        "provider-or-schedule-preview", "second-source-acquisition",
    ], "pre-transition preview prohibition")
    fields = [item["name"] for item in acquisition["linear_delivery_v2"]["fields"]]
    require(fields == [
        "source_stream", "consumer_group", "redis_entry_id", "canonical_m10_payload_bytes",
        "payload_sha256", "semantic_m10_identity", "semantic_id_sha256",
        "operational_identity_sha256", "acquisition_disposition", "consumer_identity",
        "delivery_generation",
    ], "linear payload fields")
    for item in acquisition["ready_non_acquiring_consumers"].values():
        if "allowed_acquisition_operations" in item:
            require(item["allowed_acquisition_operations"] == [], "second acquisition allowed")
    require(len(acquisition["shutdown_latch"]["applies_to"]) == 3, "latch coverage")
    require(acquisition["instrumentation_bounds"] == {
        "successful_source_acquisition_total": 1, "callback_total": 1,
        "provider_total": 1, "source_xack_total": 1,
        "meaning": "upper-bounds-per-source-and-zero-is-valid-for-retained-blocked-or-already-acknowledged-paths",
    }, "instrumentation bounds")

    require(precedence.get("rule") == "SOURCE_FIRST_TIMER_DEFERRED", "source/timer rule")
    derived = precedence["source_bearing_owner_derivation"]["non_ready"]
    require(derived == owners, "source/timer owner coverage")
    require(precedence["unlisted_tuple"]["disposition"] == "exit-67-before-transition", "unlisted disposition")
    require(not precedence["unlisted_tuple"]["ready_fallback_allowed"] and not precedence["unlisted_tuple"]["fresh_poll_fallback_allowed"], "unlisted fallback")
    require(len(precedence["simultaneous_due_timer"]["premature_actions_forbidden"]) == 5, "premature timer actions")
    require(len(precedence["post_source_timer_reclassification"]["discard_as_stale_if"]) == 6, "stale timer cases")

    rows = csv_rows(operational)
    require(len(rows) == 52 and len({row["id"] for row in rows}) == 52, "operational matrix")
    require(all(row["unlisted_disposition"] == "exit-67-before-transition" for row in rows), "matrix unlisted disposition")
    require(all("Stage8bP1eClaimedM10DeliveryV1" not in str(row) for row in rows), "V1 linear owner retained")
    require(sum(row["s06_acquisition_model"] == "ready-s06-linear-v2" for row in rows) == 2, "Ready S06 rows")
    require(sum(row["s06_acquisition_model"] == "ready-s08-linear-v2" for row in rows) == 1, "Ready S08 rows")
    non_ready_rows = [row for row in rows if row["local_restart_variant"] in owners]
    require(non_ready_rows and all(row["s06_acquisition_model"].startswith("non-ready-s06-observation") for row in non_ready_rows), "non-ready matrix acquisition")
    require(all(row["timer_derivation"] == "source-first-timer-deferred-v1" for row in rows if row["id"] in {"OC02", "OC09", "OC10"}), "Ready timer derivation")
    require(len(csv_rows(outer)) == 22, "outer matrix count")


def validate_digests(specs: dict[str, Any], golden: dict[str, Any]) -> None:
    encoding = specs["record_encoding"]
    require(encoding == {
        "magic_ascii": "M8BP1E01",
        "layout": "magic-8-bytes || schema-u16be || domain-length-u16be || domain-ascii || field-count-u16be || repeated-field-frames",
        "field_frame": "name-length-u16be || name-ascii || value-length-u32be || encoded-value",
        "length_units": "bytes", "integer_endianness": "big-endian",
        "string_encoding": "validated-exact-ASCII-with-no-normalization",
        "digest_input_encoding": "decode-exact-64-lowercase-hex-to-raw-32-bytes",
        "digest_output_encoding": "64-lowercase-hex",
        "boolean_encoding": "single-byte-00-false-01-true",
        "uuid_encoding": "raw-16-RFC4122-network-order-bytes",
        "filesystem_device_encoding": "unsigned-u64be-after-checked-platform-st_dev-conversion",
        "filesystem_inode_encoding": "unsigned-u64be-after-checked-platform-st_ino-conversion",
        "overflow_or_invalid_encoding": "fail-before-filesystem-mutation-or-ordinary-run",
    }, "record encoding")
    require(golden.get("record_magic_hex") == b"M8BP1E01".hex(), "golden magic")
    mapping = {
        "transaction_id_sha256": ("transaction_id_sha256", "expected_sha256", False),
        "canonical_root_identity_sha256": ("canonical_root_identity_sha256", "expected_sha256", False),
        "adoption_ready_owner_sha256": ("adoption_ready_owner_sha256", "expected_sha256", False),
        "first_boot_receipt_hmac_sha256": ("first_boot_receipt_hmac_preimage", "expected_hmac_sha256", True),
    }
    samples = golden["samples"]
    require(set(samples) == set(mapping), "golden sample inventory")
    for sample_name, (spec_name, output_name, is_hmac) in mapping.items():
        sample, spec = samples[sample_name], specs[spec_name]
        preimage = encode_record(spec, sample["inputs"])
        require(preimage.hex() == sample["canonical_preimage_hex"], f"{sample_name} preimage")
        digest = hmac.new(bytes.fromhex(sample["credential_key_hex"]), preimage, hashlib.sha256).hexdigest() if is_hmac else sha256(preimage)
        require(digest == sample[output_name], f"{sample_name} digest")


def validate_text_and_evidence(blobs: dict[str, bytes], evidence: dict[str, Any]) -> None:
    design = blobs["design"].decode()
    status = blobs["status"].decode()
    roadmap = blobs["roadmap"].decode()
    for token in (
        "Status: design-only review candidate", "182 active REQUIRED rows",
        "SOURCE_FIRST_TIMER_DEFERRED", "Stage8bP1eClaimedM10DeliveryV2",
        "exits class 67 before transition", "0 < child_pid <= u32::MAX",
    ):
        require(token in design, f"design token: {token}")
    for text, name in ((status, "status"), (roadmap, "roadmap")):
        for token in ("P1-e R4", "913424b73c5a83df2131a1f9a2901b78035bfc4c", "implementation remains unauthorized"):
            require(token in text, f"{name} token: {token}")
    require(evidence.get("status") == "R4_DESIGN_REVIEW_CANDIDATE", "evidence status")
    require(evidence.get("parent") == BASE and evidence.get("accepted_predecessor") == ACCEPTED, "evidence lineage")
    require(evidence.get("r3_review_sha256") == R3_REVIEW_SHA256, "review binding")
    require(evidence.get("design_only") is True and evidence.get("source_implementation_authorized") is False, "evidence scope")
    require(evidence.get("active_rows") == 182 and evidence.get("operational_rows") == 52 and evidence.get("negative_cases") == 48, "evidence inventory")
    require(all(value is False for value in evidence.get("closed_surfaces", {}).values()), "closed surface opened")
    hashes = evidence.get("contract_sha256", {})
    expected_names = {name for name in FILES if name not in {"evidence", "status", "roadmap", "outer"}}
    require(set(hashes) == expected_names, "evidence hash inventory")
    for name in expected_names:
        require(hashes[name] == sha256(blobs[name]), f"evidence hash: {name}")


def validate(overrides: dict[str, bytes] | None = None, check_scope: bool = False) -> None:
    blobs = read_all(overrides)
    active = load_json(blobs, "active")
    semantic = load_json(blobs, "semantic")
    identity = load_json(blobs, "identity")
    transaction = load_json(blobs, "transaction")
    receipt = load_json(blobs, "receipt")
    specs = load_json(blobs, "digests")
    golden = load_json(blobs, "golden")
    acquisition = load_json(blobs, "acquisition")
    precedence = load_json(blobs, "precedence")
    evidence = load_json(blobs, "evidence")
    validate_active(blobs, active)
    validate_semantic(semantic)
    validate_identity(identity)
    validate_first_boot(transaction, receipt)
    validate_acquisition(acquisition, precedence, blobs["operational"], blobs["outer"])
    validate_digests(specs, golden)
    validate_text_and_evidence(blobs, evidence)
    if check_scope:
        actual = changed_files()
        require(actual == EXPECTED_CHANGED, f"scope mismatch missing={sorted(EXPECTED_CHANGED-actual)} extra={sorted(actual-EXPECTED_CHANGED)}")


def main() -> None:
    try:
        validate(check_scope=True)
    except (CheckFailure, OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-r4-design-check: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1e-r4-design-scope files=20 production_rust=0 cargo=0 workflow=0 active_unit=0")
    print("PASS stage8b-p1e-r4-active-contract rows=182 r1=83 r2=34 r3=9 r4=56")
    print("PASS stage8b-p1e-r4-semantic-authority keys=14 conflicts=0")
    print("PASS stage8b-p1e-r4-first-boot base=10 marker_temp=4 sigkill_hooks=9")
    print("PASS stage8b-p1e-r4-operational rows=52 non_ready_owners=19 acquisition_model=B")
    print("PASS stage8b-p1e-r4-golden-digests sha256=3 hmac_sha256=1")


if __name__ == "__main__":
    main()
