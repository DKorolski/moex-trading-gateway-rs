#!/usr/bin/env python3
"""Local synthetic six-arm integration probe. No network, secrets or VPS.

Uses a separately supplied binary. The old ALOR differential drivers remain the
independent numerical oracles; this verifier checks wire/persistence/CLI claims.
"""
import argparse
import datetime as dt
import hashlib
import json
from pathlib import Path
import re
import sqlite3
import subprocess

VERSION = "finam.shadow.local-six-arm-session.v1"
SYMBOLS = ("IMOEXF", "USDRUBF", "RIZ6")


def encode(v):
    return json.dumps(v, ensure_ascii=False, separators=(",", ":")).encode()


def digest(v):
    return hashlib.sha256(encode(v)).hexdigest()


def write(p, v):
    p.write_bytes(encode(v) + b"\n")


def ms(day, h, m):
    return int(dt.datetime.fromisoformat(day).replace(hour=h, minute=m,
        tzinfo=dt.timezone(dt.timedelta(hours=3))).timestamp() * 1000)


def ohlcv(price="100.1"):
    return dict(open=price, high=price, low=price, close=price, volume="1")


def candle(binding, seq, start, purpose):
    return dict(instrument=binding["instrument"], source_sequence=seq,
        provider_id=binding["provider_id"], binding_sha256=digest(binding),
        start_ms=start, end_ms=start+600000, received_ms=start+600137,
        durable_ms=start+600251, source_processed_ms=start+600999, purpose=purpose,
        data=ohlcv(), construction=dict(kind="external_m10_v1", raw_sha256="a"*64))


def fixtures(implementation):
    seeds = []
    for symbol in SYMBOLS:
        b = dict(instrument=symbol, provider_id="fixture:"+symbol, catalog_sha256="b"*64,
                 basis="fixture_only", tick="0.01", lot=1, construction="external_m10_v1")
        history = []
        for day, weekend in (("2026-10-22", False), ("2026-10-23", False),
                             ("2026-10-24", True), ("2026-10-25", True)):
            start, count = (ms(day, 9, 50),55) if weekend else (ms(day,7,0),101)
            for i in range(count):
                c = candle(b,len(history)+1,start+i*600000,"context_history")
                c["data"] = ohlcv("100")
                c["data"].update(high="102", low="98")
                history.append(c)
        seeds.append(dict(binding=b,history=history,declared_consumed_through_ms=ms("2026-10-26",7,0)))
    manifest = dict(schema=VERSION,generation="cli-synthetic-six-arm-session",
        implementation_sha256=implementation,evidence="declared_clock_replay",session_date="2026-10-26",
        freeze_at_ms=ms("2026-10-26",7,0),seeds=seeds)
    events = [candle(s["binding"],len(s["history"])+i+1,ms("2026-10-26",7,0)+i*600000,"model")
              for i in range(101) for s in seeds]
    return manifest,events


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--binary",type=Path,required=True)
    p.add_argument("--output",type=Path,required=True)
    args = p.parse_args()
    binary = args.binary.resolve(strict=True)
    binary_sha = hashlib.sha256(binary.read_bytes()).hexdigest()
    out = args.output.resolve()
    out.mkdir(parents=True,exist_ok=False)

    def call(*a, success=True):
        r = subprocess.run([str(binary),*map(str,a)],capture_output=True,text=True,timeout=300)
        if success and r.returncode != 0:
            raise AssertionError(r.stderr)
        if not success and r.returncode == 0:
            raise AssertionError("negative accepted: "+repr(a))
        return r

    implementation = call("implementation").stdout.strip()
    assert re.fullmatch("[0-9a-f]{64}",implementation)
    manifest,events = fixtures(implementation)
    manifest_path = out/"manifest.json"
    events_path = out/"candles.json"
    write(manifest_path,manifest); write(events_path,events)
    db = out/"session.sqlite"
    result = json.loads(call("replay",manifest_path,db,events_path).stdout)
    write(out/"export.json",result)
    assert result["manifest_sha256"] == digest(manifest)
    assert result["revision"] == 303 and len(result["results"]) == 606
    assert len(result["states"]) == 6 and len(result["inputs"]) == 303
    assert [r["candle"] for r in result["inputs"]] == events
    assert all(s["ready"] and s["full_grid_flat"] and s["bars_processed"] == 101 for s in result["states"])
    assert result["order_emission_allowed"] is False and result["observed_clean_session"] is False
    assert result["external_delivery_exactly_once"] is False and result["operational_activation"] is False
    assert all(not r["order_emission_allowed"] and not r["observed_clean_session"]
               and r["evidence"] == "declared_clock_replay" for r in result["results"])
    marks = 0
    ids = set()
    for i,row in enumerate(result["inputs"]):
        c = row["candle"]
        pair = result["results"][i*2:i*2+2]
        for r in pair:
            # Exported JSON Values sort object keys; the wire checksum uses the
            # typed Candle field order, retained in the original events vector.
            input_sha = digest(events[i])
            expected_id = digest([VERSION,result["manifest_sha256"],r["arm"],c["source_sequence"],input_sha])
            assert r["input_sha256"] == input_sha and r["result_id"] == expected_id
            assert expected_id not in ids
            ids.add(expected_id)
            for k,m in enumerate(r["marks"]):
                assert m["id"] == digest([expected_id,k])
                marks += 1
    assert marks > 0
    with sqlite3.connect("file:"+str(db)+"?mode=ro",uri=True) as conn:
        assert conn.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        assert not conn.execute("PRAGMA foreign_key_check").fetchall()
        assert conn.execute("SELECT count(*) FROM inputs").fetchone()[0] == 303
        assert conn.execute("SELECT count(*) FROM outcomes").fetchone()[0] == 606
        assert conn.execute("SELECT count(*) FROM arms").fetchone()[0] == 6
        for text,sha in conn.execute("SELECT result,result_sha FROM outcomes"):
            assert hashlib.sha256(text.encode()).hexdigest() == sha
        for blob,sha,text,state_sha in conn.execute("SELECT checkpoint,checkpoint_sha,state,state_sha FROM arms"):
            assert hashlib.sha256(blob).hexdigest() == sha
            assert hashlib.sha256(text.encode()).hexdigest() == state_sha

    # Reopen and response-loss redelivery: explicit lookup, no marks in receipt.
    assert json.loads(call("export",manifest_path,db).stdout) == result
    one = out/"one.json"; write(one,events[0])
    receipt = json.loads(call("apply",manifest_path,db,one).stdout)
    assert receipt["status"] == "already_committed" and "marks" not in receipt
    assert receipt["result_ids"] == [r["result_id"] for r in result["results"][:2]]
    assert json.loads(call("export",manifest_path,db).stdout) == result
    bad = dict(events[0]); bad["received_ms"] += 1
    write(one,bad); conflict = call("apply",manifest_path,db,one,success=False)
    write(one,events[0])
    bad_manifest = dict(manifest); bad_manifest["generation"] += "-wrong"
    wrong = out/"wrong-manifest.json"; write(wrong,bad_manifest)
    mismatch = call("export",wrong,db,success=False)
    exists = call("create",manifest_path,db,success=False)
    assert json.loads(call("export",manifest_path,db).stdout) == result
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == binary_sha
    summary = dict(schema="finam.local-session.verification.v1",verdict="PASS",
        implementation_sha256=implementation,binary_sha256=binary_sha,
        instruments=3,policy_arms=6,inputs=303,outputs=606,model_marks=marks,
        millisecond_clocks_retained=True,duplicate_did_not_emit=True,reopen_equal=True,
        negative_cases=3,negative_messages=[r.stderr.strip() for r in (conflict,mismatch,exists)],
        sqlite_integrity="ok",numeric_parity_proof="separate accepted-fixture differential gates",
        source_authority="declared_unattested",live_mapping_verified=False,
        linux_release=False,operational_activation=False,observed_clean_sessions=0)
    write(out/"summary.json",summary)
    write(out/"sha256.json",{f.name:hashlib.sha256(f.read_bytes()).hexdigest()
        for f in sorted(out.iterdir()) if f.is_file() and f.name!="sha256.json"})
    print(json.dumps(summary,ensure_ascii=False))


if __name__ == "__main__":
    main()
