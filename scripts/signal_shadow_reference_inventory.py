#!/usr/bin/env python3
"""Read-only historical inventory, NOT calendar authority or parity acceptance."""
import argparse
import csv
import hashlib
import io
import json
from collections import defaultdict
from datetime import datetime, timedelta, timezone
from decimal import Decimal
from pathlib import Path

PINS = {
    "imoexf_raw_10m_msk_utc.csv": "e4a30fb81a0abf5711a3702d19b918cad681820380f24422f6ba476b74f92cec",
    "baseline07_python_reference_trades.csv": "2accaf0eec6c81f3446832059c231471415094c6cf43dc91e6ddbba85ccfe384",
}
MSK = timezone(timedelta(hours=3))
SEED = ["2026-09-15", "2026-09-16", "2026-09-17", "2026-09-18"]
REPLAY = ["2026-09-21", "2026-09-22", "2026-09-23", "2026-09-24", "2026-09-25"]


def inspect_bars(raw):
    days = defaultdict(list)
    previous = None
    count = 0
    for row in csv.DictReader(io.StringIO(raw.decode("utf-8"))):
        label = datetime.fromisoformat(row["bar_start_utc"])
        available = datetime.fromisoformat(row["available_at_utc"])
        local = datetime.fromisoformat(row["bar_start_msk"]).replace(tzinfo=MSK)
        if (label.utcoffset() != timedelta(0)
                or available.utcoffset() != timedelta(0)
                or local != label or available != label + timedelta(minutes=10)
                or label.minute % 10 or label.second or label.microsecond
                or (previous is not None and label <= previous)):
            raise ValueError("timestamp/order contract")
        values = [Decimal(row[k]) for k in ("open", "high", "low", "close", "volume")]
        if not all(v.is_finite() for v in values):
            raise ValueError("nonfinite OHLCV")
        o, h, low, c, volume = values
        if min(o, h, low, c) <= 0 or volume < 0 or not low <= min(o, c) <= max(o, c) <= h:
            raise ValueError("OHLCV contract")
        count += 1
        previous = label
        if local.weekday() < 5 and 7 <= local.hour < 24:
            days[local.date().isoformat()].append(local.strftime("%H:%M"))
    if not count:
        raise ValueError("empty history")
    inventory = {}
    nominal = [f"{m // 60:02}:{m % 60:02}" for m in range(7 * 60, 23 * 60 + 41, 10)]
    for date in SEED + REPLAY:
        labels = days.get(date, [])
        if not labels:
            raise ValueError("selected session absent")
        inventory[date] = {
            "observed_count": len(labels),
            "first": labels[0], "last": labels[-1],
            "missing_nominal_labels": sorted(set(nominal) - set(labels)),
            "unexpected_labels": sorted(set(labels) - set(nominal)),
        }
    return count, inventory


def inventory(root):
    files = {}
    payloads = {}
    for name, expected in PINS.items():
        path = root / name
        if not path.is_file() or path.stat().st_size > 16 * 1024 * 1024:
            raise ValueError("missing/oversize fixture")
        raw = path.read_bytes()
        digest = hashlib.sha256(raw).hexdigest()
        if digest != expected:
            raise ValueError("frozen fixture hash mismatch: " + name)
        files[name] = {"sha256": digest, "bytes": len(raw)}
        payloads[name] = raw
    count, sessions = inspect_bars(payloads["imoexf_raw_10m_msk_utc.csv"])
    rounds = list(csv.DictReader(io.StringIO(
        payloads["baseline07_python_reference_trades.csv"].decode("utf-8"))))
    selected = [r for r in rounds if r["entry_bar"][:10] in REPLAY]
    if not selected or any(
        r["profile"] != "bo_only_weekday07_control" or r["component"] != "BO"
        or Decimal(r["slippage_per_market_side"]) != 0 for r in selected
    ):
        raise ValueError("reference profile")
    return {
        "schema_version": 1,
        "reference": "alor_freeze_2026_09_27",
        "reference_head": "a19124b98d601d7c583b3554d4beadad7ad82cdc",
        "status": "HISTORICAL_INPUT_INVENTORY_ONLY",
        "files": files, "raw_bar_count": count,
        "seed_dates": SEED, "replay_dates": REPLAY,
        "sessions": sessions, "selected_python_rounds": selected,
        "calendar_authority": False, "current_alor_live": False,
        "parity_accepted": False,
        "limitations": [
            "Nominal grid is diagnostic, not exchange-calendar coverage proof.",
            "Python rounds are independent historical reference, not current live fills.",
            "No quote stream: first-post-intent-quote feedback cannot be proven from M10.",
            "No bars after September 25; September 22-25 seed alone cannot replay September 28.",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", type=Path, required=True)
    args = parser.parse_args()
    try:
        result = inventory(args.fixtures)
    except (ValueError, OSError, KeyError, ArithmeticError) as error:
        parser.exit(1, f"inventory refused: {error}\n")
    print(json.dumps(result, ensure_ascii=False, sort_keys=True, indent=2))


if __name__ == "__main__":
    main()
