#!/usr/bin/env python3
"""Static, authority and golden checks for Stage 6B."""
from __future__ import annotations
import hashlib, json, subprocess
from pathlib import Path

BASE = "c399e2bc2c7e62cc2116a6eac970058bb47c4a49"
R1_BASE = "6dbc4e021f61860069c599ccd526a83e4bca01a6"
MAIN = "14359aadb3178c83692441b748b060d06ce12903"
BRANCH = "stage6-durable-chain"
MODULE = Path("crates/strategy-runtime-core/src/stage6_journal_backend.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
DESCRIPTOR = Path("docs/stage-6/stage6b-storage-descriptor.json")
AUTHORITY = Path("docs/stage-6/stage6b-stage6a-authority-inventory.json")
GOLDEN = Path("docs/stage-6/stage6b-golden-manifest.json")

REQUIRED_SOURCE = (
    "STAGE6_JOURNAL_STORAGE_SCHEMA_VERSION: u16 = 1",
    "STAGE6_JOURNAL_MAX_RECORD_BYTES: usize = 1024 * 1024",
    'JOURNAL_MAGIC: &[u8; 8] = b"S6JNLV1\\0"',
    'FRAME_MAGIC: &[u8; 4] = b"S6F1"',
    'FRAME_HASH_DOMAIN: &[u8] = b"stage6-journal-frame-v1"',
    'FRAME_GENESIS_DOMAIN: &[u8] = b"stage6-journal-frame-genesis-v1"',
    'CHECKPOINT_HASH_DOMAIN: &[u8] = b"stage6-journal-checkpoint-v1"',
    "pub trait Stage6JournalBackend", "pub struct Stage6MemoryJournalBackend",
    "pub struct Stage6FileJournalBackend", "pub struct Stage6JournalFrontierV1",
    "pub struct Stage6JournalCheckpointV1", "pub struct Stage6JournalAppendReceipt",
    "pub enum Stage6JournalStorageError", "UnsupportedStorageSchema",
    "InvalidJournalHeader", "InvalidFrameHeader", "InvalidFrameLength", "TornFrame",
    "FrameHashMismatch", "FrameChainMismatch", "NonCanonicalRecord", "RecordDecodeFailed",
    "TrailingGarbage", "DurabilityUncertain", "CheckpointInvalid",
    "ExternalMutationDetected", "record.encode_canonical()",
    "Stage6JournalRecordV1::decode_canonical(bytes)", "self.file.sync_data().is_err()",
    "validate_record_length(u64::from(declared))?", "stored_previous != previous",
    "stored_hash != computed", "scan_reader(&mut file, length)?",
    "if length == 0 ||", "length > STAGE6_JOURNAL_MAX_RECORD_BYTES as u64 ||",
    "validate_checkpoint_against_scan", "value.encode_canonical() != bytes",
    "stage6b_memory_and_file_framed_bytes_are_identical",
    "stage6b_outer_rehash_does_not_admit_noncanonical_json",
    "stage6b_sync_failure_returns_durability_uncertain_without_receipt",
    "stage6b_corrupt_journal_is_never_auto_repaired",
    "stage6b_one_frame_bytes_match_exact_golden_hex",
    ".create_new(true)", "error.kind() == ErrorKind::NotFound",
    "error.kind() == ErrorKind::AlreadyExists => continue",
    "fn from_validated_file(",
    "stage6b_r1_absent_path_creates_exact_empty_journal",
    "stage6b_r1_existing_header_only_opens_without_mutation",
    "stage6b_r1_existing_zero_length_fails_closed",
    "stage6b_r1_existing_zero_length_remains_unchanged",
    "stage6b_r1_existing_one_byte_remains_unchanged",
    "stage6b_r1_existing_nine_byte_header_remains_unchanged",
    "stage6b_r1_existing_bad_magic_remains_unchanged",
    "stage6b_r1_existing_corrupt_nonempty_frame_remains_unchanged",
    "stage6b_r1_valid_nonempty_reopen_does_not_rewrite",
    "stage6b_r1_repeated_valid_empty_open_remains_exact",
)

FORBIDDEN_PRODUCTION = (
    "redis::", "reqwest", "broker_finam", "finam_gateway", "Method::POST",
    "Method::DELETE", ".post(", ".delete(", "XREADGROUP", "XAUTOCLAIM",
    "std::thread::spawn", "tokio::spawn", "runtime_callback", "dispatch_command",
    "Stage6Replay", "Stage6Conflict", "ReplayDecision", "ConflictResolution",
    "NativeStopOrder", "ProtectiveOrderPayload", "ReplaceOrder", ".set_len(",
    "auto_repair", "skip_corrupt_frame", "checkpoint_source_of_truth",
    "serde_json::from_slice::<Stage6JournalRecordV1>",
)

class CheckFailure(ValueError): pass
def require(value: bool, message: str) -> None:
    if not value: raise CheckFailure(message)
def sha(path: Path) -> str: return hashlib.sha256(path.read_bytes()).hexdigest()

def extract_block(source: str, start: int, needle: str) -> str:
    position=source.index(needle,start); opening=source.index("{",position); depth=0
    for index in range(opening,len(source)):
        if source[index] == "{": depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0: return source[opening:index+1]
    raise CheckFailure(f"unterminated block: {needle}")

def validate_descriptor(value: dict) -> None:
    require(value.get("schema_version") == 1 and value.get("stage") == "6B", "descriptor header drift")
    require(value.get("status") == "implementation_candidate", "status drift")
    require(value.get("accepted_stage6a_ref") == BASE, "Stage 6A ref drift")
    require(value.get("required_branch") == BRANCH, "branch drift")
    require(value.get("storage_schema_version") == 1, "storage schema drift")
    require(value.get("byte_order") == "big_endian", "byte order drift")
    require(value.get("journal_magic_hex") == "53364a4e4c563100", "journal magic drift")
    require(value.get("journal_header_bytes") == 10, "journal header drift")
    require(value.get("frame_magic_hex") == "53364631", "frame magic drift")
    require(value.get("frame_version") == 1 and value.get("frame_prefix_bytes") == 42, "frame header drift")
    require(value.get("frame_hash_bytes") == 32, "frame hash width drift")
    require(value.get("max_record_bytes") == 1048576, "record bound drift")
    require(value.get("frame_hash_domain") == "stage6-journal-frame-v1", "frame domain drift")
    require(value.get("genesis_hash_domain") == "stage6-journal-frame-genesis-v1", "genesis drift")
    require(value.get("checkpoint_hash_domain") == "stage6-journal-checkpoint-v1", "checkpoint domain drift")
    require(value.get("persisted_record_decode_authority") == "Stage6JournalRecordV1::decode_canonical", "decode authority drift")
    require(value.get("filesystem_sync_policy") == "sync_data_before_receipt", "sync policy drift")
    require(value.get("checkpoint_sidecar_persisted") is False, "checkpoint sidecar opened")
    require(value.get("automatic_repair") is False, "automatic repair opened")
    require(value.get("single_logical_writer") is True, "writer model drift")
    require(value.get("positive_test_count") == 60, "positive count drift")
    require(value.get("negative_case_minimum") == 145, "negative minimum drift")
    require(value.get("open_create_policy") == "validate_existing_or_create_new", "open/create policy drift")
    require(value.get("existing_zero_length_policy") == "fail_closed_unchanged", "zero-length policy drift")
    require(value.get("creation_race_policy") == "already_exists_retry_existing_validation", "creation race policy drift")
    require(value.get("framing_golden_raw_sha256") == "5c555efb677d1313de8aa2ece47657d28063969b8bdaf3cdceefae53f610da30", "raw golden drift")
    require(value.get("stage6b_status") == "open_pending_independent_acceptance", "Stage 6B status drift")
    require(value.get("stage6c_plus_open") is False, "Stage 6C+ opened")
    require(value.get("closed_surfaces") and not any(value["closed_surfaces"].values()), "closed surface opened")

def validate_source(source: str) -> None:
    for token in REQUIRED_SOURCE: require(token in source, f"required source token absent: {token}")
    production=source.split("#[cfg(test)]\nmod tests",1)[0]
    for token in FORBIDDEN_PRODUCTION: require(token not in production, f"forbidden production token: {token}")
    require(source.count("fn stage6b_") == 60, "Stage 6B test count drift")
    require("derive(Debug)]\npub struct Stage6FileJournalBackend" in source, "filesystem writer shape drift")
    require("derive(Debug, Clone" not in source[source.index("pub struct Stage6FileJournalBackend")-30:source.index("pub struct Stage6FileJournalBackend")], "filesystem writer became Clone")
    append_impl=source.index("impl Stage6JournalBackend for Stage6FileJournalBackend")
    append=extract_block(source,append_impl,"fn append(")
    require(append.index("self.file.sync_data().is_err()") < append.index("append_receipt("), "receipt may precede sync")
    require("self.durability_uncertain = true" in append, "sync uncertainty is not sticky")
    record_validator=extract_block(source,source.index("fn validate_record_for_storage"),"fn validate_record_for_storage")
    require("record.encode_canonical()" in record_validator and "decode_persisted_record(&bytes)?" in record_validator, "typed append bypasses canonical authority")
    scanner=extract_block(source,source.index("fn scan_reader"),"fn scan_reader")
    require("decode_persisted_record(&record_bytes)?" in scanner, "persisted scan bypasses Stage 6A decode")
    require(scanner.index("validate_record_length(u64::from(declared))?") < scanner.index("vec![0_u8; record_length]"), "allocation occurs before length validation")
    require("stored_previous != previous" in scanner and "stored_hash != computed" in scanner, "physical chain validation drift")
    frame_hash=extract_block(source,source.index("fn frame_digest"),"fn frame_digest")
    require("hasher.update(previous)" in frame_hash and "hasher.update(record_bytes)" in frame_hash, "frame digest lost chain/payload")
    require("pub" not in source[source.index("enum TestIoFailpoint")-20:source.index("enum TestIoFailpoint")], "test failpoint exported")
    file_impl=source.index("impl Stage6FileJournalBackend")
    open_block=extract_block(source,file_impl,"pub fn open(")
    require(".create(true)" not in open_block, "existing open can create or overwrite")
    require(".truncate(" not in open_block and ".set_len(" not in open_block, "open/create can truncate")
    require(".create_new(true)" in open_block, "new journal creation is not exclusive")
    require("error.kind() == ErrorKind::NotFound" in open_block, "creation is not gated by NotFound")
    require("error.kind() == ErrorKind::AlreadyExists => continue" in open_block, "creation race does not retry existing validation")
    require(open_block.index(".open(&path)") < open_block.index("ErrorKind::NotFound") < open_block.index(".create_new(true)"), "existing-open must precede exclusive creation")
    require(open_block.index(".create_new(true)") < open_block.index("file.write_all(&journal_header())?") < open_block.index("file.sync_data()?") < open_block.rindex("Self::from_validated_file"), "new header is not written/synced before validation")
    validated=extract_block(source,file_impl,"fn from_validated_file(")
    require("scan_reader(&mut file, length)?" in validated, "existing journal bypasses complete scan")
    require("if length == 0" not in validated, "existing zero-length journal is treated as new")
    require("write_all" not in validated and "set_len" not in validated and "sync_data" not in validated, "existing validation mutates journal")
    require(validated.index("file.metadata()?.len()") < validated.index("scan_reader(&mut file, length)?"), "existing length is not scanned")

def validate_authority(root: Path, value: dict) -> None:
    require(value.get("schema_version") == 1 and value.get("accepted_stage6a_ref") == BASE, "authority header drift")
    authorities=value.get("authorities",[]); require(len(authorities)==5, "authority count drift")
    for item in authorities: require(sha(root/item["path"]) == item["sha256"], f"Stage 6A authority drift: {item['path']}")

def validate_golden(root: Path, value: dict) -> None:
    require(value.get("schema_version") == 1, "golden schema drift")
    require(value.get("framing_golden_raw_sha256") == "5c555efb677d1313de8aa2ece47657d28063969b8bdaf3cdceefae53f610da30", "raw frame SHA drift")
    fixtures=value.get("fixtures",[]); require(len(fixtures)==3, "golden count drift")
    for item in fixtures: require(sha(root/item["path"]) == item["sha256"], f"golden SHA drift: {item['path']}")
    raw=bytes.fromhex((root/fixtures[0]["path"]).read_text().strip())
    require(hashlib.sha256(raw).hexdigest() == value["framing_golden_raw_sha256"], "hex/raw frame mismatch")
    for item in fixtures[1:]: json.loads((root/item["path"]).read_text())

def check(root: Path) -> None:
    require(subprocess.check_output(["git","branch","--show-current"],cwd=root,text=True).strip()==BRANCH,"wrong branch")
    validate_descriptor(json.loads((root/DESCRIPTOR).read_text()))
    validate_source((root/MODULE).read_text())
    validate_authority(root,json.loads((root/AUTHORITY).read_text()))
    validate_golden(root,json.loads((root/GOLDEN).read_text()))
    lib=(root/LIB).read_text()
    require("mod stage6_journal_backend;" in lib and "pub use stage6_journal_backend::{" in lib, "Stage 6B linkage absent")
    print("stage6b-check: PASS positive=60 authorities=5 golden=3 open_create=fail_closed")

def main() -> None:
    try: check(Path.cwd().resolve())
    except CheckFailure as error: raise SystemExit(f"stage6b-check: FAIL: {error}") from error
if __name__ == "__main__": main()
