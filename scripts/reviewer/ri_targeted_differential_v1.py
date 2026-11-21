"""Reviewer-only deterministic RI complete-grid differential; no broker IO."""
from __future__ import annotations

import argparse
import hashlib
import json
from collections import Counter
from datetime import datetime, timedelta, timezone
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
MSK = timezone(timedelta(hours=3))


def start_minute(day):
    return 540 if day < "2026-07-14" else 420


def bar(day, minute, close, opening=None, high=None, low=None, origin="live"):
    opening = close if opening is None else opening
    ts = datetime.fromisoformat(day).replace(tzinfo=MSK) + timedelta(minutes=minute)
    return dict(symbol="RIZ6", close_time_utc=int(ts.timestamp()), close=float(close),
                o=float(opening), h=float(max(close, opening)+10 if high is None else high),
                l=float(min(close, opening)-10 if low is None else low), v=10., origin=origin)


def history(day, previous2=90000.):
    d = datetime.fromisoformat(day)
    days = []
    while len(days) < 2:
        d -= timedelta(days=1)
        if d.weekday() < 5:
            days.insert(0, d.date().isoformat())
    result = []
    for index, day in enumerate(days):
        close = previous2 if index == 0 else 90000.
        result.extend(bar(day, minute, close, high=max(91000., close), low=min(89000., close), origin="history")
                      for minute in range(start_minute(day), 1411, 10))
    return result


def events(day, mirror=False, sixth_extreme=False):
    # Two MR positions consume the shared quota, then three BO entries, strict
    # stop boundaries, stop-on-entry-bar, day-extreme reentry and same-close EOD.
    path = {0:(89900.,None), 1:(90011.,None), 2:(90100.,None), 3:(89959.,None),
            4:(90000.,None), 5:(94000. if sixth_extreme else 90000.,None),
            6:(90000.,None), 11:(91000.,None), 12:(91100.,91010.),
            13:(90360.,None), 14:(90359.,None), 15:(91200.,90020.),
            16:(91300.,None), 17:(90990.,91310.), 18:(91500.,90980.),
            19:(91600.,None), 20:(91600.,91610.)}
    price = 90000.
    result = []
    for index, minute in enumerate(range(start_minute(day), 1421, 10)):
        opening = None
        if index in path:
            price, opening = path[index]
        if minute == 1380:
            price, opening = 92500., 92400.
        raw = bar(day, minute, price, opening)
        if mirror:
            raw.update(close=180000.-raw["close"], o=180000.-raw["o"],
                       h=180000.-raw["l"], l=180000.-raw["h"])
        result.append(raw)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--handoff", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(exist_ok=False)
    specifications = [
        ("long_grid_quota_reentry_eod", "2026-10-13", False, 90000., False),
        ("short_grid_quota_reentry_eod", "2026-10-13", True, 90000., False),
        ("friday_mr_only", "2026-10-16", False, 90000., False),
        ("season_before_boundary", "2026-05-20", False, 90000., False),
        ("season_first_day", "2026-05-21", False, 90000., False),
        ("season_last_day", "2026-07-01", False, 90000., False),
        ("season_after_boundary", "2026-07-02", False, 90000., False),
        ("pretransition_last_day", "2026-07-13", False, 90000., False),
        ("posttransition_first_day", "2026-07-14", False, 90000., False),
        ("return_gate_long", "2026-10-13", False, 93000., False),
        ("return_gate_short", "2026-10-13", True, 87000., False),
        ("sixth_bar_extreme_disables_bo", "2026-10-13", False, 90000., True),
    ]
    fixture = dict(schema_version="ri_prospective_fixture_v1", evidence_kind="synthetic_model_clock",
                   cases=[dict(id="reviewer/"+name, day=day, warmup=history(day, prev2),
                               events=events(day, mirror, extreme))
                          for name, day, mirror, prev2, extreme in specifications])
    inp = out/"inputs.json"
    inp.write_text(json.dumps(fixture,separators=(",",":"),allow_nan=False)+"\n")
    command = [str(args.binary.resolve()), str(inp), str(out/"actual.json"), "reviewer-ri-targeted-differential-v1"]
    run = subprocess.run(command,capture_output=True,text=True,timeout=120)
    (out/"candidate.log").write_text(run.stdout+run.stderr)
    if run.returncode:
        raise RuntimeError(f"candidate exit {run.returncode}: {run.stderr}")
    sys.path.insert(0,str(args.handoff.resolve()/"reference_rust/scripts/shadow_state_parity"))
    from run_ri_fixture_parity import expected_rows, compare_rows
    expected = expected_rows(fixture)
    (out/"expected.json").write_text(json.dumps(expected,separators=(",",":"),allow_nan=False)+"\n")
    actual = json.loads((out/"actual.json").read_text())
    compare_rows(expected,actual["rows"])
    groups = {}
    for row in actual["rows"]:
        key = (row["case"],row["policy"])
        groups.setdefault(key,[]).append(row)
    details = []
    for (case,policy),rows in groups.items():
        terminal = rows[-1]["state"]
        assert terminal["phase"] == "flat" and terminal["mr"] is None and terminal["bo"] is None and terminal["pending"] is None
        assert all(not (r["state"]["mr"] and r["state"]["bo"]) for r in rows)
        details.append(dict(case=case,policy=policy,rows=len(rows),mr_entries=terminal["mr_total"],bo_entries=terminal["bo_total"],
                            exits=dict(Counter(a["reason"] for r in rows for a in r["state"]["actions"] if a["role"]=="exit"))))
    branches=Counter(b for row in actual["rows"] for b in row["state"]["branches"])
    result = dict(result="PASS_TARGETED_DIFFERENTIAL", cases=len(fixture["cases"]),policies=2,state_rows=len(expected),
                  input_sha256=hashlib.sha256(inp.read_bytes()).hexdigest(),
                  binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                  source_candidate="017698205c482087334ac6c4d13fd9b65a19422d",
                  oracle="Unmodified pinned ALOR run_ri_fixture_parity.expected_rows and compare_rows",
                  scope="Additional reviewer-only complete-grid synthetic scenarios; no observed session or operational complete() claim",branch_counts=dict(branches),details=details)
    (out/"verification.json").write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps({k:v for k,v in result.items() if k not in ("branch_counts","details")},indent=2))


if __name__ == "__main__":
    main()
