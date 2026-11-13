#!/usr/bin/env python3
"""One bounded FINAM catalog read. No WS, bars, strategy or order endpoint.

Fixed-origin/no-proxy/no-redirect/auth-expiry policy follows the existing
read-only preparation collectors. Credentials stay in memory; only successful
public catalog bodies or explicitly labelled public projections are retained.
"""
import argparse
import datetime as dt
import hashlib
import json
from pathlib import Path
import re
import shlex
import time
import urllib.error
import urllib.parse
import urllib.request

ORIGIN = "https://api.finam.ru"
PUBLIC_ASSET = ("board", "id", "ticker", "mic", "isin", "type", "name", "decimals",
                "min_step", "lot_size", "expiration_date", "quote_currency", "future_details")
PUBLIC_PARAMS = ("symbol", "trade_lot_size", "is_tradable", "tradeable", "price_type")
PRIVATE_KEYS = {"token", "access_token", "secret", "account_id", "account_ids",
                "client_id", "authorization", "password"}


class Hold(Exception):
    pass


def need(condition, reason):
    if not condition:
        raise Hold(reason)


def enc(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def now_ms():
    return time.time_ns() // 1_000_000


def put(path, data):
    with path.open("xb") as f:
        f.write(data)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def credentials(path):
    # Do not source a shell file, expand variables or export credentials.
    selected = {}
    for line in path.read_text().splitlines():
        m = re.fullmatch(r"\s*(?:export\s+)?(FINAM_SECRET_TOKEN|FINAM_ACCOUNT_ID)\s*=\s*(.*)", line)
        if m:
            need(m[1] not in selected, "DUPLICATE_ENV_KEY")
            values = shlex.split(m[2], comments=True, posix=True)
            need(len(values) == 1, "ENV_VALUE_SHAPE")
            selected[m[1]] = values[0]
    secret, account = selected.get("FINAM_SECRET_TOKEN", ""), selected.get("FINAM_ACCOUNT_ID", "")
    need(secret and not any(c.isspace() for c in secret), "CREDENTIAL_FORMAT")
    need(re.fullmatch(r"[A-Za-z0-9_.-]{1,128}", account), "ACCOUNT_FORMAT")
    return secret, account


def auth_check(details, account, at):
    need(details.get("readonly") is True and account in details.get("account_ids", []), "READONLY_ACCOUNT")
    need(any(p.get("mic") == "RTSX" and p.get("delay_minutes") == 0
             for p in details.get("md_permissions", [])), "RTSX_PERMISSION")
    expires = dt.datetime.fromisoformat(details["expires_at"].replace("Z", "+00:00"))
    need(expires.tzinfo is not None, "EXPIRY_TIMEZONE")
    expiry = int(expires.timestamp() * 1000)
    need(120_000 < expiry - at <= 86_400_000, "ACTUAL_EXPIRY_MARGIN")
    return expiry


def public_check(value):
    if isinstance(value, dict):
        need(not any(k.lower() in PRIVATE_KEYS for k in value), "PRIVATE_RESPONSE_FIELD")
        for v in value.values():
            public_check(v)
    elif isinstance(value, list):
        for v in value:
            public_check(v)


def candidates(rows):
    needles = {"IMOEXF": ("IMOEXF",), "USDRUBF": ("USDRUBF",), "RIZ6": ("RIZ6", "RTS-12.26")}
    found = {}
    for model, aliases in needles.items():
        found[model] = [r for r in rows if r.get("mic") == "RTSX" and r.get("is_archived") is not True
            and any(a == str(r.get("ticker", "")).upper()
                    or a == str(r.get("symbol", "")).upper().split("@")[0]
                    or a in str(r.get("name", "")).upper() for a in aliases)]
    return found


def collect(env_file, output, catalog_source="all-active", resume=None):
    output.mkdir(parents=True, exist_ok=False)
    result = dict(schema="finam.shadow.catalog.collection.v1", status="COLLECTING",
        started_at_ms=now_ms(), requests=[], auth={}, candidate_rows={}, instruments={},
        activation=False, orders=False, websocket=False, bars=False, redis=False,
        raw_auth_retained=False, raw_account_retained=False)
    result["catalog_source"] = catalog_source
    sensitive = []
    deadline = time.monotonic() + 240
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    expiry = None
    stage = "credentials"

    def request(label, path, payload=None, token=None):
        nonlocal stage
        stage = label
        need(time.monotonic() < deadline and len(result["requests"]) < 80, "HTTP_BUDGET")
        parsed = urllib.parse.urlsplit(path)
        if payload is not None:
            need(path in ("/v1/sessions", "/v1/sessions/details"), "AUTH_ROUTE")
        else:
            need(token is not None and expiry is not None and expiry - now_ms() > 120_000, "CURRENT_AUTH")
            need(parsed.path in ("/v1/assets", "/v1/assets/all") or re.fullmatch(r"/v1/assets/[A-Za-z0-9_.%\-]+(?:/params)?", parsed.path),
                 "CATALOG_GET_ROUTE")
        need(not parsed.scheme and not parsed.netloc and not parsed.fragment, "FIXED_ORIGIN")
        record = dict(label=label, method="POST" if payload is not None else "GET",
            path=parsed.path, query_keys=sorted(urllib.parse.parse_qs(parsed.query)),
            requested_at_ms=now_ms())
        if parsed.path in ("/v1/assets", "/v1/assets/all"):
            record["public_catalog_query"] = urllib.parse.parse_qs(parsed.query)
        result["requests"].append(record)
        headers = {"Accept": "application/json"}
        if token:
            headers["Authorization"] = "Bearer " + token
        if payload is not None:
            headers["Content-Type"] = "application/json"
        req = urllib.request.Request(ORIGIN + path, data=None if payload is None else enc(payload),
                                     headers=headers, method=record["method"])
        try:
            with opener.open(req, timeout=20) as response:
                raw = response.read(32 * 1024 * 1024 + 1)
                record.update(http_status=response.status, received_at_ms=now_ms(), bytes=len(raw))
                declared = response.headers.get("Content-Length")
        except urllib.error.HTTPError as error:
            record.update(http_status=error.code, received_at_ms=now_ms())
            raise Hold("HTTP_STATUS_" + label + "_" + str(error.code)) from None
        record["http_content_length"] = None if declared is None else int(declared)
        need(record["http_status"] == 200 and len(raw) <= 32 * 1024 * 1024, "HTTP_RESPONSE_BOUND")
        need(declared is None or int(declared) == len(raw), "HTTP_TRUNCATED")
        value = json.loads(raw)
        if payload is None:
            record.update(response_sha256=sha(raw), transport_complete=True,
                          http_content_length=None if declared is None else int(declared))
        return value, raw, record

    try:
        secret, account = credentials(env_file)
        sensitive.append(secret)
        auth, _, _ = request("auth", "/v1/sessions", payload={"secret": secret})
        token = auth["token"]
        need(isinstance(token, str) and token, "AUTH_TOKEN_SHAPE")
        sensitive.append(token)
        details, _, _ = request("details", "/v1/sessions/details", payload={"token": token})
        expiry = auth_check(details, account, now_ms())
        result["auth"] = dict(readonly=True, selected_account_membership_verified=True,
            rtsx_zero_delay_verified=True, expires_at=details["expires_at"],
            token_retained=False, account_retained=False)
        need(catalog_source in ("available", "all-active"), "CATALOG_SOURCE")
        rows, cursor, cursors, first_page = [], None, set(), 0
        if resume is not None:
            need(catalog_source == "all-active", "RESUME_SOURCE")
            prior_bytes = (resume / "collection.json").read_bytes()
            prior = json.loads(prior_bytes)
            need(prior["catalog_source"] == "all-active" and prior["status"] == "HOLD", "RESUME_STATE")
            prefix = prior.get("catalog_prefix_requests", []) + [r for r in prior["requests"]
                if r.get("original_raw_body_retained") and r.get("transport_complete") and r.get("file")]
            need(0 < len(prefix) < 64, "RESUME_PREFIX_BOUND")
            for page, old in enumerate(prefix):
                name = "catalog-page-" + str(page + 1).zfill(3) + ".raw.json"
                need(old["file"] == name and old["path"] == "/v1/assets/all"
                     and old["public_catalog_query"].get("cursor", [None])[0] == cursor, "RESUME_CAUSAL_CURSOR")
                raw = (resume / name).read_bytes()
                need(sha(raw) == old["response_sha256"], "RESUME_HASH")
                value = json.loads(raw); public_check(value)
                need(not any(s in raw.decode() for s in sensitive), "RESUME_SECRET_GUARD")
                put(output / name, raw)
                rows.extend(value["assets"])
                cursor = value.get("next_cursor")
                need(isinstance(cursor, str) and cursor not in cursors, "RESUME_CURSOR")
                cursors.add(cursor)
            first_page = len(prefix)
            result.update(catalog_prefix_requests=prefix, resumed_collection_sha256=sha(prior_bytes))
        for page in range(first_page, 64):
            query = {"only_active": "true"} if catalog_source == "all-active" else {}
            if cursor:
                query["cursor"] = cursor
            path = "/v1/assets/all" if catalog_source == "all-active" else "/v1/assets"
            if query:
                path += "?" + urllib.parse.urlencode(query)
            catalog, raw, record = request("catalog_" + str(page + 1), path, token=token)
            public_check(catalog)
            need(not any(s in raw.decode() for s in sensitive), "CATALOG_SECRET_GUARD")
            need(isinstance(catalog.get("assets"), list), "CATALOG_SHAPE")
            filename = "catalog-page-" + str(page + 1).zfill(3) + ".raw.json"
            put(output / filename, raw)
            record.update(original_raw_body_retained=True, file=filename)
            rows.extend(catalog["assets"])
            cursor = catalog.get("next_cursor")
            if catalog_source == "available" or cursor in (None, "", "0", 0):
                result.update(catalog_complete=True, catalog_page_count=page + 1)
                break
            if all(len(v) == 1 for v in candidates(rows).values()):
                result.update(catalog_complete=False, catalog_page_count=page + 1,
                              catalog_target_subset_found=True)
                break
            need(isinstance(cursor, str) and len(cursor) <= 128 and cursor not in cursors,
                 "CATALOG_CURSOR_LOOP_OR_SHAPE")
            cursors.add(cursor)
            time.sleep(0.35)
        need(result.get("catalog_complete") is True or result.get("catalog_target_subset_found") is True,
             "CATALOG_PAGE_BOUND")
        result["catalog_rows"] = len(rows)
        result["candidate_rows"] = candidates(rows)
        for model, matches in result["candidate_rows"].items():
            need(len(matches) == 1, "CATALOG_AMBIGUOUS_OR_MISSING_" + model)
            row = matches[0]
            symbol = row["symbol"]
            need(isinstance(symbol, str) and re.fullmatch(r"[A-Za-z0-9_.-]+@RTSX", symbol), "CATALOG_SYMBOL_SHAPE")
            encoded = urllib.parse.quote(symbol, safe="")
            query = urllib.parse.urlencode({"account_id": account})
            item = {"catalog_row": row}
            for label, suffix, keys in [("asset", "", PUBLIC_ASSET), ("params", "/params", PUBLIC_PARAMS)]:
                value, raw, record = request(model + "_" + label, "/v1/assets/" + encoded + suffix + "?" + query, token=token)
                if "account_id" in value:
                    need(value["account_id"] == account, "PARAMS_ACCOUNT_MISMATCH")
                if label == "params":
                    need(value.get("account_id") == account, "PARAMS_ACCOUNT_BINDING")
                projection = {k: value[k] for k in keys if k in value}
                public_check(projection)
                need(not any(s in enc(projection).decode() for s in sensitive), "PROJECTION_SECRET_GUARD")
                filename = model.lower() + "-" + label + ".projection.json"
                put(output / filename, enc(projection))
                record.update(original_raw_body_retained=False, projection_file=filename,
                              projection_sha256=sha(enc(projection)), scope="public_projection_not_original_raw")
                item[label] = projection
                if label == "params":
                    item["selected_account_matches"] = True
            result["instruments"][model] = item
        result["status"] = "COLLECTED_NOT_VALIDATED"
    except Exception as error:
        result.update(status="HOLD", stage=stage,
                      reason=str(error) if isinstance(error, Hold) else type(error).__name__,
                      provider_error_body_withheld=True)
    result["finished_at_ms"] = now_ms()
    need(not any(s and s in enc(result).decode() for s in sensitive), "OUTPUT_SECRET_GUARD")
    put(output / "collection.json", enc(result))
    put(output / "SHA256SUMS", "".join(sha(p.read_bytes()) + "  " + p.name + "\n"
        for p in sorted(output.iterdir()) if p.is_file()).encode())
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--env-file", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--catalog-source", choices=("available", "all-active"), default="all-active")
    p.add_argument("--resume-prefix", type=Path)
    a = p.parse_args()
    result = collect(a.env_file.resolve(), a.output.resolve(), a.catalog_source, a.resume_prefix)
    print(json.dumps({k: result[k] for k in ("status", "candidate_rows", "instruments", "auth")
                      if k in result}, ensure_ascii=False))
    if result["status"] == "HOLD":
        print(json.dumps({k: result.get(k) for k in ("stage", "reason")}))
        raise SystemExit(1)


if __name__ == "__main__":
    main()
