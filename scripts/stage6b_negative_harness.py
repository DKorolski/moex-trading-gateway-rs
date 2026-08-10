#!/usr/bin/env python3
"""Named adversarial mutation matrix for Stage 6B."""
from __future__ import annotations
import copy, json
from pathlib import Path
import stage6b_check as checker

ROOT=Path(__file__).resolve().parents[1]
def rejected(name,action):
    try: action()
    except (checker.CheckFailure,KeyError,ValueError,json.JSONDecodeError): print(f"PASS {name}")
    else: raise SystemExit(f"stage6b-negative: FAIL accepted mutation: {name}")

def main():
    count=0
    descriptor=json.loads((ROOT/checker.DESCRIPTOR).read_text())
    mutations=[
        ("schema_version",2),("stage","6C"),("status","accepted"),
        ("accepted_stage6a_ref","0"*40),("required_branch","main"),
        ("storage_schema_version",2),("byte_order","little_endian"),
        ("journal_magic_hex","00"*8),("journal_header_bytes",9),
        ("frame_magic_hex","00"*4),("frame_version",2),("frame_prefix_bytes",41),
        ("frame_hash_bytes",31),("max_record_bytes",0),
        ("frame_hash_domain","changed"),("genesis_hash_domain","changed"),
        ("checkpoint_hash_domain","changed"),("persisted_record_decode_authority","serde_json"),
        ("filesystem_sync_policy","receipt_before_sync"),("checkpoint_sidecar_persisted",True),
        ("automatic_repair",True),("single_logical_writer",False),
        ("positive_test_count",49),("negative_case_minimum",127),
        ("framing_golden_raw_sha256","0"*64),("stage6b_status","closed"),
        ("stage6c_plus_open",True),
    ]
    for field,value in mutations:
        candidate=copy.deepcopy(descriptor); candidate[field]=value
        rejected(f"descriptor-{field}",lambda c=candidate:checker.validate_descriptor(c)); count+=1
    for surface in descriptor["closed_surfaces"]:
        candidate=copy.deepcopy(descriptor); candidate["closed_surfaces"][surface]=True
        rejected(f"open-{surface}",lambda c=candidate:checker.validate_descriptor(c)); count+=1

    source=(ROOT/checker.MODULE).read_text()
    for index,token in enumerate(checker.REQUIRED_SOURCE):
        candidate=source.replace(token,f"REMOVED_STAGE6B_REQUIRED_{index}")
        rejected(f"missing-required-{index:02d}",lambda c=candidate:checker.validate_source(c)); count+=1
    marker="#[cfg(test)]\nmod tests"
    for index,token in enumerate(checker.FORBIDDEN_PRODUCTION):
        candidate=source.replace(marker,token+"\n"+marker,1)
        rejected(f"forbidden-production-{index:02d}",lambda c=candidate:checker.validate_source(c)); count+=1

    semantic=[
        ("remove-previous-frame-from-hash",source.replace("hasher.update(previous);","",1)),
        ("remove-record-bytes-from-hash",source.replace("hasher.update(record_bytes);","",1)),
        ("remove-frame-hash-validation",source.replace("if stored_hash != computed","if false",1)),
        ("remove-frame-chain-validation",source.replace("if stored_previous != previous","if false",1)),
        ("accept-zero-length",source.replace("if length == 0 ||","if false ||",1)),
        ("remove-max-record-length",source.replace("length > STAGE6_JOURNAL_MAX_RECORD_BYTES as u64 ||","false ||",1)),
        ("skip-stage6a-canonical-decode",source.replace("decode_persisted_record(&record_bytes)?","record_bytes; place_record()",1)),
        ("generic-serde-persisted-admission",source.replace(marker,"serde_json::from_slice::<Stage6JournalRecordV1>(b\"{}\");\n"+marker,1)),
        ("auto-truncate-corrupt-tail",source.replace(marker,"file.set_len(0);\n"+marker,1)),
        ("skip-corrupt-frame",source.replace(marker,"skip_corrupt_frame\n"+marker,1)),
        ("return-receipt-before-sync",source.replace("self.file.sync_data().is_err()","false",1)),
        ("ignore-sync-failure",source.replace("self.durability_uncertain = true;\n            return Err(Stage6JournalStorageError::DurabilityUncertain);","return Ok(append_receipt(record, start, &frame.digest, &self.scan.frontier));",1)),
        ("rewrite-journal-on-sync-failure",source.replace(marker,"journal.set_len(0);\n"+marker,1)),
        ("checkpoint-becomes-source-of-truth",source.replace(marker,"checkpoint_source_of_truth\n"+marker,1)),
        ("allocation-bound-bypass",source.replace("validate_record_length(u64::from(declared))?","declared",1)),
        ("filesystem-writer-clone",source.replace("#[derive(Debug)]\npub struct Stage6FileJournalBackend","#[derive(Debug, Clone)]\npub struct Stage6FileJournalBackend",1)),
        ("public-failpoint",source.replace("enum TestIoFailpoint","pub enum TestIoFailpoint",1)),
    ]
    for name,candidate in semantic:
        rejected(name,lambda c=candidate:checker.validate_source(c)); count+=1

    authority=json.loads((ROOT/checker.AUTHORITY).read_text())
    candidate=copy.deepcopy(authority);candidate["accepted_stage6a_ref"]="0"*40
    rejected("authority-wrong-stage6a-ref",lambda c=candidate:checker.validate_authority(ROOT,c));count+=1
    for index in range(5):
        candidate=copy.deepcopy(authority);candidate["authorities"][index]["sha256"]="0"*64
        rejected(f"authority-sha-{index}",lambda c=candidate:checker.validate_authority(ROOT,c));count+=1

    golden=json.loads((ROOT/checker.GOLDEN).read_text())
    candidate=copy.deepcopy(golden);candidate["framing_golden_raw_sha256"]="0"*64
    rejected("golden-raw-sha",lambda c=candidate:checker.validate_golden(ROOT,c));count+=1
    for index in range(3):
        candidate=copy.deepcopy(golden);candidate["fixtures"][index]["sha256"]="0"*64
        rejected(f"golden-fixture-{index}",lambda c=candidate:checker.validate_golden(ROOT,c));count+=1
    if count < 128: raise SystemExit(f"stage6b-negative: FAIL only {count} cases")
    print(f"stage6b-negative: PASS {count}/{count}")
if __name__ == "__main__": main()
