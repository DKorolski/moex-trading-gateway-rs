#!/usr/bin/env python3
"""Exact 44-case semantic mutation harness for Stage 8A-3 R2."""

from __future__ import annotations

import shutil
import tempfile
from pathlib import Path

import stage8a3_check as scanner


def insert(source: str, statement: str) -> str:
    marker = "#[cfg(test)]\nmod tests;"
    return source.replace(marker, f"const _: &str = {statement!r};\n\n{marker}", 1)


def insert_code(source: str, code: str) -> str:
    marker = "#[cfg(test)]\nmod tests;"
    if marker not in source:
        raise RuntimeError("test-module insertion anchor missing")
    return source.replace(marker, f"{code}\n\n{marker}", 1)


def replace(source: str, old: str, new: str) -> str:
    if old not in source:
        raise RuntimeError(f"mutation anchor missing: {old}")
    return source.replace(old, new, 1)


CASES = [
    ("contextless default PLACE", lambda s: replace(s, "pub fn for_place(", "pub fn default_to_place(")),
    ("historical contextless classifier", lambda s: insert(s, "classify_order_endpoint_local_http_response(")),
    ("historical context classifier", lambda s: insert(s, "classify_order_endpoint_local_http_response_for_context(")),
    (
        "historical classifier imported and invoked through alias",
        lambda s: insert_code(
            s,
            """use broker_finam::{
    classify_order_endpoint_local_http_response_for_context as legacy_stage8_classifier,
    FinamOrderEndpointClassifiedResponse, FinamOrderEndpointContext,
    FinamOrderEndpointLocalHttpResponse,
};

fn historical_classifier_alias_bypass(
    response: &FinamOrderEndpointLocalHttpResponse,
) -> FinamOrderEndpointClassifiedResponse {
    legacy_stage8_classifier(FinamOrderEndpointContext::Place, response)
}""",
        ),
    ),
    (
        "FINAM venue symbol falls back to broker-neutral symbol",
        lambda s: replace(
            s,
            """let venue_symbol = instrument
            .venue_symbol
            .filter(|value| !value.trim().is_empty())
            .ok_or(Stage8a3ContextError::EmptyInstrumentIdentity)?;""",
            """let venue_symbol = instrument
            .venue_symbol
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(instrument.symbol);""",
        ),
    ),
    ("generic 4xx rejection", lambda s: insert(s, "400..=499")),
    ("PLACE 404 rejected", lambda s: insert(s, "BrokerRejected PLACE404")),
    ("CANCEL 400 rejected", lambda s: insert(s, "BrokerRejected CANCEL400")),
    ("CANCEL 404 rejected", lambda s: insert(s, "BrokerRejected CANCEL404")),
    ("CANCEL 409 rejected", lambda s: insert(s, "BrokerRejected CANCEL409")),
    ("CANCEL 410 rejected", lambda s: insert(s, "BrokerRejected CANCEL410")),
    ("429 retry", lambda s: insert(s, "RetryAllowed 429")),
    ("Retry-After authority", lambda s: insert(s, "RetryAfter")),
    ("500 maintenance", lambda s: insert(s, "Maintenance 500")),
    ("503 maintenance", lambda s: insert(s, "Maintenance 503")),
    ("504 definitely-not-sent", lambda s: insert(s, "DefinitelyNotSent 504")),
    ("timeout definitely-not-sent", lambda s: insert(s, "DefinitelyNotSent timeout")),
    ("body failure definitely-not-sent", lambda s: insert(s, "DefinitelyNotSent body")),
    ("connect error definitely-not-sent", lambda s: insert(s, "DefinitelyNotSent connect")),
    ("unknown 2xx accepted", lambda s: replace(s, "201..=299 => {", "201..=299 => blocked_decision(")),
    ("missing PLACE id accepted", lambda s: insert(s, "accept_missing_place_order_id")),
    ("empty PLACE id accepted", lambda s: replace(s, "_ => return reconciliation_decision(Stage8a3ReconciliationReason::MissingBrokerOrderId),", "_ => return blocked_decision(Stage8a3SemanticCategory::PlaceAcceptedCandidate),")),
    ("PLACE mismatch accepted", lambda s: replace(s, "return reconciliation_mismatch();", "return blocked_decision(Stage8a3SemanticCategory::PlaceAcceptedCandidate);")),
    ("status-only PLACE 400 reject", lambda s: insert(s, "BrokerRejected status400")),
    ("free-text PLACE 400 reject", lambda s: insert(s, "BrokerRejected invalid trading parameters")),
    ("CANCEL 204 accepted", lambda s: insert(s, "CancelAcceptedCandidate 204")),
    ("empty CANCEL any status accepted", lambda s: insert(s, "CancelAcceptedCandidate empty")),
    ("CANCEL 200 flat", lambda s: insert(s, "FlatConfirmed")),
    ("CANCEL 401 ordinary reject", lambda s: insert(s, "BrokerRejected CANCEL401")),
    ("CANCEL 401 retry", lambda s: insert(s, "same_request_retry cancel401")),
    ("ambiguous retry", lambda s: insert(s, "same_request_retry ambiguous")),
    ("raw body export", lambda s: insert(s, "raw_response_body")),
    ("raw broker id export", lambda s: insert(s, "raw_broker_order_id")),
    ("raw client/account export", lambda s: insert(s, "raw_client_order_id raw_account_id")),
    ("ProvenNoMatch", lambda s: insert(s, "ProvenNoMatch")),
    ("Stage8A4 reconciliation", lambda s: insert(s, "Stage8a4Reconciliation")),
    ("reqwest send", lambda s: insert(s, "reqwest send")),
    ("M3d2 transport", lambda s: insert(s, "M3d2RealOrderEndpointTransport")),
    ("EndpointGateApproved", lambda s: insert(s, "EndpointGateApproved")),
    ("actual one-shot", lambda s: insert(s, "m3j16_actual_one_shot")),
    ("Redis consumer", lambda s: insert(s, "RedisCommandConsumer redis::")),
    ("dispatch/runtime-live", lambda s: insert(s, "BrokerDispatch RuntimeLive")),
    ("real strategy order", lambda s: insert(s, "RealStrategyOrder")),
    ("protective/multi-leg", lambda s: insert(s, "StopLoss Sltp BracketOrder ReplaceOrder MultiLeg")),
]


def main() -> int:
    if len(CASES) != 44:
        raise SystemExit(f"negative inventory drift: {len(CASES)}")
    source = (scanner.ROOT / scanner.MODULE).read_text()
    copied = scanner.ALLOWED_CHANGED_PATHS | {str(scanner.STAGE8A1), str(scanner.STAGE8A2)}
    for index, (name, mutate) in enumerate(CASES, 1):
        with tempfile.TemporaryDirectory(prefix="stage8a3-negative-") as raw:
            root = Path(raw)
            for relative in copied:
                origin = scanner.ROOT / relative
                if origin.is_file():
                    target = root / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(origin, target)
            (root / scanner.MODULE).write_text(mutate(source))
            try:
                scanner.check(
                    root,
                    git_scope=False,
                    pin_hashes=False,
                    exact_successor=False,
                )
            except Exception:
                print(f"PASS {index:02d} {name}")
            else:
                print(f"FAIL {index:02d} {name}: mutation accepted")
                return 1
    print("stage8a3-r2-negative: PASS cases=44/44")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
