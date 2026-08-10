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

def replace_nth(value: str, old: str, new: str, occurrence: int) -> str:
    start = 0
    for _ in range(occurrence + 1):
        position = value.index(old, start)
        start = position + len(old)
    return value[:position] + new + value[position + len(old):]

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
        ("accepted_transition_gate", "0"*40),
        ("constructor_deserializer_equivalence", False),
        ("strict_canonical_byte_decode", False),
        ("reserved_marker_events_accepted", True),
        ("supported_place_order_types", ["market", "limit", "stop"]),
        ("market_requires_absent_limit_price", False),
        ("limit_requires_positive_limit_price", False),
        ("place_quantity_positive", False),
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
    semantic_mutations = [
        ("identity-place-validator-bypass", replace_nth(source, "value.validate_self()?;", "let _ = &value;", 0)),
        ("identity-cancel-validator-bypass", replace_nth(source, "value.validate_self()?;", "let _ = &value;", 1)),
        ("snapshot-place-identity-bypass", replace_nth(source, "identity.validate_self()?;", "let _ = identity;", 0)),
        ("snapshot-cancel-identity-bypass", replace_nth(source, "identity.validate_self()?;", "let _ = identity;", 1)),
        ("record-build-identity-bypass", replace_nth(source, "identity.validate_self()?;", "let _ = &identity;", 2)),
        ("snapshot-place-intrinsic-bypass", replace_nth(source, "value.validate_intrinsic()?;", "let _ = &value;", 0)),
        ("snapshot-cancel-intrinsic-bypass", replace_nth(source, "value.validate_intrinsic()?;", "let _ = &value;", 1)),
        ("record-final-validation-bypass", source.replace("value.validate()?;\n        Ok(value)", "Ok(value)", 1)),
        ("decode-semantic-validation-bypass", source.replace("record.validate()?;\n        if record.encode_canonical()", "if record.encode_canonical()", 1)),
        ("decode-byte-canonicality-bypass", source.replace("if record.encode_canonical() != bytes", "if false")),
        ("reserved-marker-bypass", source.replace("return Err(Stage6DurableIdentityError::UnsupportedEventPayload);", "return Ok(());", 1)),
        ("empty-account-bypass", source.replace("if account_id.as_str().is_empty()", "if false && account_id.as_str().is_empty()")),
        ("empty-instrument-bypass", source.replace("if instrument.symbol.is_empty()", "if false && instrument.symbol.is_empty()")),
        ("attribution-equivalence-bypass", source.replace("|| attribution.validate_source_equivalence().is_err()", "|| false")),
        ("allow-all-order-types", source.replace(
            "Err(Stage6DurableIdentityError::UnsupportedDurablePlaceOrderType)", "Ok(())", 1
        )),
        ("allow-Stop", source.replace("OrderType::Stop\n", "OrderType::Market\n", 1)),
        ("allow-StopLimit", source.replace("| OrderType::StopLimit\n", "| OrderType::Market\n", 1)),
        ("allow-TakeProfit", source.replace("| OrderType::TakeProfit\n", "| OrderType::Market\n", 1)),
        ("allow-TakeProfitLimit", source.replace("| OrderType::TakeProfitLimit =>", "| OrderType::Market =>", 1)),
        ("remove-market-limit-price-check", source.replace(
            "OrderType::Market if limit_price.is_none()", "OrderType::Market", 1
        )),
        ("remove-limit-price-required-check", source.replace(
            "_ => Err(Stage6DurableIdentityError::InvalidDurablePlacePriceShape)",
            "_ => Ok(())", 1
        )),
        ("remove-positive-limit-price-check", source.replace(
            "Some(price) if price > &Price::ZERO => Ok(())", "Some(_price) => Ok(())", 1
        )),
        ("remove-positive-quantity-check", source.replace(
            "if quantity <= &Quantity::ZERO", "if false && quantity <= &Quantity::ZERO", 1
        )),
        ("constructor-only-shape-validation", source.replace(
            "validate_durable_place_shape(*order_type, quantity, limit_price)?;", "", 1
        )),
        ("deserialize-shape-validation-bypass", source.replace(
            "value\n            .validate_intrinsic()\n            .map_err(serde::de::Error::custom)?;",
            "let _ = &value;", 1
        )),
    ]
    for name, candidate in semantic_mutations:
        rejected(name, lambda c=candidate: checker.validate_source(c)); count += 1
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
    if count < 145: raise SystemExit(f"stage6a-negative: FAIL only {count} cases")
    print(f"stage6a-negative: PASS {count}/{count}")

if __name__ == "__main__": main()
