#!/usr/bin/env python3
"""Named adversarial mutation matrix for Stage 6A."""
from __future__ import annotations
import copy, json
from pathlib import Path
import stage6a_check as checker

ROOT = Path(__file__).resolve().parents[1]

def rejected(name, action):
    try: action()
    except (checker.CheckFailure, KeyError, ValueError, json.JSONDecodeError): print(f"PASS {name}")
    else: raise SystemExit(f"stage6a-negative: FAIL accepted mutation: {name}")

def main():
    count = 0
    descriptor = json.loads((ROOT / checker.DESCRIPTOR).read_text())
    mutations = [
        ("schema_version", 2), ("stage", "6B"), ("status", "accepted"),
        ("accepted_predecessor", "0"*40), ("required_branch", "main"),
        ("durable_record_schema_version", 2), ("positive_test_count", 23),
        ("negative_case_minimum", 79), ("logical_record_id_includes_payload_digest", True),
        ("cancel_request_identity_separate_from_target_client_identity", False),
        ("stage6a_status", "closed"), ("stage6b_plus_open", True),
    ]
    for field, value in mutations:
        candidate=copy.deepcopy(descriptor); candidate[field]=value
        rejected(f"descriptor-{field}", lambda c=candidate: checker.validate_descriptor(c)); count += 1
    for surface in descriptor["closed_surfaces"]:
        candidate=copy.deepcopy(descriptor); candidate["closed_surfaces"][surface]=True
        rejected(f"open-{surface}", lambda c=candidate: checker.validate_descriptor(c)); count += 1
    source=(ROOT/checker.MODULE).read_text()
    for index, token in enumerate(checker.REQUIRED_SOURCE):
        candidate=source.replace(token, f"REMOVED_REQUIRED_{index}")
        rejected(f"missing-required-{index:02d}", lambda c=candidate: checker.validate_source(c)); count += 1
    for index, token in enumerate(checker.FORBIDDEN_SOURCE):
        rejected(f"forbidden-{index:02d}", lambda t=token: checker.validate_source(source+"\n"+t)); count += 1
    inventory=json.loads((ROOT/checker.INVENTORY).read_text())
    for index in range(3):
        candidate=copy.deepcopy(inventory); candidate["direct_schema_authorities"][index]["sha256"]="0"*64
        rejected(f"direct-authority-{index}", lambda c=candidate: checker.validate_inventory(ROOT,c)); count += 1
    for field, value in (("sha256","0"*64),("authority_count",16),("path","missing.json")):
        candidate=copy.deepcopy(inventory); candidate["accepted_transition_inventory"][field]=value
        rejected(f"transition-inventory-{field}", lambda c=candidate: checker.validate_inventory(ROOT,c)); count += 1
    golden=json.loads((ROOT/checker.GOLDEN).read_text())
    for index in range(2):
        candidate=copy.deepcopy(golden); candidate["fixtures"][index]["sha256"]="0"*64
        rejected(f"golden-sha-{index}", lambda c=candidate: checker.validate_golden(ROOT,c)); count += 1
    if count < 80: raise SystemExit(f"stage6a-negative: FAIL only {count} cases")
    print(f"stage6a-negative: PASS {count}/{count}")

if __name__ == "__main__": main()
