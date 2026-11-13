#!/usr/bin/env python3
"""Offline catalog/mapping verifier. No network, auth or execution."""
import argparse
import datetime as dt
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import re
import urllib.parse

ROOT = Path(__file__).resolve().parents[1]
ACCEPTED_MODELS = "3755928dd3e3cdc7a63c303561769f5faf2a7a40"
RI_PROFILE_SHA = "30e74209c058e288ab171afb7d18cba4d9bb93a0e4927cf9a6062325e96fe989"
MSK = dt.timezone(dt.timedelta(hours=3))


def need(v, reason):
    if not v:
        raise ValueError(reason)


def sha(b):
    return hashlib.sha256(b).hexdigest()


def enc(v):
    return json.dumps(v, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def positive(wire):
    need(isinstance(wire, str) and len(wire) <= 48
         and re.fullmatch(r"[0-9]+(?:[.][0-9]{1,9})?", wire), "DECIMAL_WIRE")
    whole, _, fraction = wire.partition(".")
    v = Fraction(int(whole + fraction), 10**len(fraction))
    need(v > 0, "POSITIVE_DECIMAL")
    return v


def tick(min_step, decimals):
    need(isinstance(min_step, str) and re.fullmatch(r"[0-9]{1,18}", min_step)
         and int(min_step) > 0, "MIN_STEP")
    need(type(decimals) is int and 0 <= decimals <= 9, "DECIMALS")
    text = str(int(min_step)).zfill(decimals + 1)
    return (text[:-decimals] + "." + text[-decimals:]).rstrip("0").rstrip(".") if decimals else text


def bind(model, row, asset, params, ri):
    need(model in ("IMOEXF", "USDRUBF", "RIZ6"), "MODEL")
    expected_name = ri["contract_display_name"] if model == "RIZ6" else model
    need(row["ticker"] == model and row["name"] == expected_name and row["mic"] == "RTSX"
         and row["type"] == "FUTURES" and row["is_archived"] is False, "CATALOG_IDENTITY")
    need(row["symbol"] == model + "@RTSX" and isinstance(row["id"], str) and row["id"], "PROVIDER_ID")
    need(all(asset.get(k) == row[k] for k in ("id", "ticker", "mic", "name", "type")), "ASSET_IDENTITY")
    need(asset.get("board") == "FUT" and params.get("symbol") == row["symbol"], "BOARD_PARAMS_IDENTITY")
    price_tick = tick(asset["min_step"], asset["decimals"])
    # IMOEXF control proves the scaled-integer interpretation, not min_step=5.
    if model == "IMOEXF":
        need(price_tick == "0.5", "IMOEXF_CONTROL_TICK")
    reference_lot = positive(asset["lot_size"]["value"])
    trade_lot = positive(params["trade_lot_size"])
    need(trade_lot == 1, "ONE_CONTRACT_TRADE_LOT")
    size = positive(asset["future_details"]["contract_size"]["value"])
    expiration = None
    structured = asset.get("expiration_date")
    iso = asset.get("future_details", {}).get("expiration_date")
    if structured:
        expiration = dt.date(structured["year"], structured["month"], structured["day"]).isoformat()
    iso_date = None
    if iso:
        stamp = dt.datetime.fromisoformat(iso.replace("Z", "+00:00"))
        need(stamp.tzinfo is not None, "EXPIRY_TIMEZONE")
        iso_date = stamp.astimezone(MSK).date().isoformat()
        need(expiration is None or expiration == iso_date, "EXPIRY_DATE_DISAGREEMENT")
        expiration = iso_date
    if model == "RIZ6":
        need(expiration == ri["actual_expiry_date"] and structured and iso, "PINNED_RI_EXPIRY")
    need(asset.get("quote_currency") in ("RUB", "USD"), "QUOTE_CURRENCY")
    return dict(model_instrument=model, provider_symbol=row["symbol"], broker_asset_id=row["id"],
        contract_display_name=row["name"], venue_mic=row["mic"], board=asset["board"],
        price_tick=price_tick, reported_min_step=asset["min_step"], reported_decimals=asset["decimals"],
        reference_lot_size=asset["lot_size"]["value"], trade_lot_size=params["trade_lot_size"],
        model_virtual_contract_qty=1, reported_contract_size=asset["future_details"]["contract_size"]["value"],
        quote_currency=asset["quote_currency"], expiration_date_msk=expiration,
        reported_future_expiration=iso, expiration_status="date_confirmed" if expiration else "not_reported_by_provider",
        reported_account_tradable=params.get("is_tradable", params.get("tradeable")),
        contract_rollover_automatic=False, lot_semantics="trade_lot_size determines contract order units; reference lot/contract size retained separately",
        # Keep exact rational sanity checks without inferring economic point value.
        reference_lot_positive=reference_lot > 0, reported_contract_size_positive=size > 0)


def verify(collection, prefix_record=None):
    report_bytes = (collection / "collection.json").read_bytes()
    report = json.loads(report_bytes)
    need(report["status"] == "COLLECTED_NOT_VALIDATED" and report["auth"]["readonly"] is True
         and report["auth"]["selected_account_membership_verified"] is True
         and report["auth"]["rtsx_zero_delay_verified"] is True, "COLLECTION_AUTH")
    need(all(report[k] is False for k in ("activation", "orders", "websocket", "bars", "redis",
                                         "raw_auth_retained", "raw_account_retained")), "CLOSED_SURFACES")
    profile_bytes = (ROOT / "crates/finam-shadow-models/profiles/ri.json").read_bytes()
    need(sha(profile_bytes) == RI_PROFILE_SHA, "RI_PROFILE")
    ri = json.loads(profile_bytes)
    need(ri["symbol"] == "RIZ6", "RI_MODEL")
    prefix = report.get("catalog_prefix_requests", [])
    if prefix:
        need(prefix_record is not None and sha(prefix_record.read_bytes()) == report["resumed_collection_sha256"], "PREFIX_RECORD_HASH")
        prior = json.loads(prefix_record.read_bytes())
        need(prior["status"] == "HOLD" and prior["auth"]["readonly"] is True, "PREFIX_STATUS")
        original = prior.get("catalog_prefix_requests", []) + [r for r in prior["requests"]
            if r.get("original_raw_body_retained") and r.get("transport_complete") and r.get("file")]
        need(prefix == original, "PREFIX_RECORDS_CHANGED")
    requests = report["requests"]
    need([r["path"] for r in requests[:2]] == ["/v1/sessions", "/v1/sessions/details"]
         and all(r["method"] == "POST" for r in requests[:2]), "AUTH_ROUTES")
    expiry = dt.datetime.fromisoformat(report["auth"]["expires_at"].replace("Z", "+00:00")).timestamp()*1000
    for r in requests:
        need(r["http_status"] == 200 and r["requested_at_ms"] <= r["received_at_ms"], "HTTP_CLOCK_STATUS")
        if r["method"] == "GET":
            need(expiry - r["received_at_ms"] > 120_000 and r["transport_complete"], "AUTH_OR_TRANSPORT")
        else:
            need(r in requests[:2], "POST_OUTSIDE_AUTH")
    pages = prefix + [r for r in requests if r.get("file")]
    need(len(pages) == report["catalog_page_count"] and 0 < len(pages) <= 64, "PAGE_COUNT")
    cursor, rows, files = None, [], {}
    for index, r in enumerate(pages):
        name = "catalog-page-" + str(index+1).zfill(3) + ".raw.json"
        need(r["file"] == name and r["path"] == "/v1/assets/all" and r["method"] == "GET"
             and r["http_status"] == 200 and r["transport_complete"], "CATALOG_ROUTE_STATUS")
        query = {"only_active": ["true"]}
        if cursor:
            query["cursor"] = [cursor]
        need(r["public_catalog_query"] == query and r["query_keys"] == sorted(query), "CURSOR_CHAIN")
        raw = (collection / name).read_bytes()
        need(sha(raw) == r["response_sha256"] and len(raw) == r["bytes"]
             and r["http_content_length"] in (None, len(raw)), "CATALOG_BODY")
        page = json.loads(raw)
        files[name] = sha(raw)
        rows.extend(page["assets"])
        cursor = page.get("next_cursor")
    need(len(rows) == report["catalog_rows"], "CATALOG_ROWS")
    need(report["catalog_complete"] is (cursor in (None, "", "0", 0)), "COMPLETENESS_CLAIM")
    bindings = []
    for model in ("IMOEXF", "USDRUBF", "RIZ6"):
        matches = [r for r in rows if r.get("ticker") == model and r.get("mic") == "RTSX"
                   and r.get("is_archived") is False]
        need(len(matches) == 1, "TARGET_NOT_UNIQUE_IN_SCANNED_CATALOG")
        item = report["instruments"][model]
        need(item["catalog_row"] == matches[0] and item["selected_account_matches"] is True, "TARGET_PROJECTION")
        for label in ("asset", "params"):
            filename = model.lower() + "-" + label + ".projection.json"
            raw = (collection / filename).read_bytes()
            need(json.loads(raw) == item[label], "PROJECTION_FILE")
            record = [r for r in requests if r["label"] == model + "_" + label]
            need(len(record) == 1, "PROJECTION_REQUEST")
            record = record[0]
            expected = "/v1/assets/" + urllib.parse.quote(matches[0]["symbol"], safe="") + ("/params" if label == "params" else "")
            need(record["method"] == "GET" and record["path"] == expected and record["query_keys"] == ["account_id"]
                 and record["projection_file"] == filename and record["projection_sha256"] == sha(raw)
                 and record["original_raw_body_retained"] is False
                 and record["scope"] == "public_projection_not_original_raw", "PROJECTION_PROVENANCE")
            need(re.fullmatch("[0-9a-f]{64}", record["response_sha256"]), "ORIGINAL_BODY_HASH")
            files[filename] = sha(raw)
        binding = bind(model, item["catalog_row"], item["asset"], item["params"], ri)
        binding.update(asset_projection_sha256=files[model.lower()+"-asset.projection.json"],
                       params_projection_sha256=files[model.lower()+"-params.projection.json"])
        bindings.append(binding)
    need(len([r for r in requests if r["method"] == "GET"]) == len(pages)-len(prefix)+6, "EXTRA_GET_SURFACE")
    files["collection.json"] = sha(report_bytes)
    return dict(schema="finam.shadow.mapping.snapshot.v1",validation_status="PASS_PENDING_REVIEW",
        accepted_models_ref=ACCEPTED_MODELS,evidence_kind="observed_readonly_finam_catalog_and_public_projections",
        catalog_scope="scanned active-catalog prefix plus exact target metadata, not a global registry audit",
        catalog_complete=report["catalog_complete"],catalog_pages=len(pages),catalog_rows=len(rows),
        collection_started_at_ms=report["started_at_ms"],collection_finished_at_ms=report["finished_at_ms"],
        observed_at_msk=dt.datetime.fromtimestamp(report["finished_at_ms"]/1000,MSK).isoformat(),
        bindings=bindings,ri_profile_sha256=RI_PROFILE_SHA,
        collector_source_sha256=sha((ROOT/"scripts/finam_shadow_catalog_readonly.py").read_bytes()),
        verifier_source_sha256=sha(Path(__file__).read_bytes()),evidence_files=files,
        auth_readonly_verified=True,account_identity_retained=False,auth_token_retained=False,
        asset_params_evidence="public projection; original response SHA retained, original body not retained",
        source_bars_admitted=False,operational_activation=False,real_orders=False,redis_db0=False,
        runtime_live=False,observed_clean_sessions_added=0)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--collection",type=Path,required=True)
    p.add_argument("--prefix-record",type=Path)
    p.add_argument("--output",type=Path,required=True)
    a=p.parse_args()
    result=verify(a.collection,a.prefix_record)
    a.output.parent.mkdir(parents=True,exist_ok=True)
    with a.output.open("xb") as f:
        f.write(enc(result)+b"\n")
    print(json.dumps({k:result[k] for k in ("validation_status","bindings","observed_at_msk","catalog_complete")},ensure_ascii=False))


if __name__=="__main__":
    main()
