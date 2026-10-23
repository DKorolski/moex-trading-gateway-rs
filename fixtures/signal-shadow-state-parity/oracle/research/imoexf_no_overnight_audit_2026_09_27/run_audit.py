"""Read-only research replay; never connects to a broker or changes live configs."""
from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import subprocess
import sys
import urllib.parse

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import pandas as pd

ROOT = Path(__file__).resolve().parent
SOURCE = ROOT.parent / "imoexf_session_recut_2026_07_22/run_imoexf_session_recut_ab.py"
spec = importlib.util.spec_from_file_location("frozen_imo_formulas", SOURCE)
model = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = model
spec.loader.exec_module(model)
TICK = 0.5
TRANSITION = pd.Timestamp("2026-07-14")


@dataclass(frozen=True)
class Profile:
    name: str
    start: str = "07:00"
    mr_end: str = "09:59"
    wait: float = 3.0
    mr: str = "author41"
    weekend_state: bool = True


PROFILES = (
    Profile("author41_weekend07"),
    Profile("author41_weekend09", "09:00", "11:59"),
    Profile("author41_weekend07_wait4", "07:00", "10:59", 4.0),
    Profile("bo_only_weekend07", mr="off"),
    Profile("bo_only_weekend09", "09:00", mr="off"),
    Profile("bo_only_weekend07_wait4", wait=4.0, mr="off"),
    Profile("bo_only_weekday07_control", mr="off", weekend_state=False),
    Profile("high180_weekend07_ungated_reference", mr="high180"),
)


def tick(price):
    return math.floor(price / TICK + 0.5) * TICK


def fetch(start, end, cache):
    if cache.exists():
        return pd.read_parquet(cache)
    rows, offset = [], 0
    base = "https://iss.moex.com/iss/engines/futures/markets/forts/boards/RFUD/securities/IMOEXF/candles.json"
    while True:
        query = urllib.parse.urlencode(dict(interval=10, **{"from": start}, till=end,
                                           start=offset, **{"iss.meta": "off"}))
        pages = cache.parent / (cache.stem + "_pages")
        pages.mkdir(exist_ok=True)
        page = pages / f"{offset}.json"
        if page.exists():
            raw = page.read_bytes()
        else:
            raw = subprocess.check_output(["curl", "-fsSL", "--retry", "3", "--retry-all-errors",
                                           "--connect-timeout", "10", "--max-time", "60", base + "?" + query])
            json.loads(raw)["candles"]
            page.write_bytes(raw)
        block = json.loads(raw)["candles"]
        batch = block["data"]
        if not batch:
            break
        rows.extend(batch)
        offset += len(batch)
        print(f"Fetched {offset} bars", flush=True)
        if len(batch) < 500:
            break
    frame = pd.DataFrame(rows, columns=block["columns"])
    if frame.empty:
        raise RuntimeError("Empty MOEX history")
    frame.index = pd.to_datetime(frame.pop("begin"))
    frame = frame[["open", "high", "low", "close", "volume"]].sort_index()
    assert not frame.index.has_duplicates, "Duplicate source bars"
    frame.to_parquet(cache)
    return frame


def prepare(raw, profile):
    starts = ["09:00" if t.normalize() < TRANSITION else profile.start for t in raw.index]
    keep = [s <= t.strftime("%H:%M") <= "23:49" and
            (profile.weekend_state or t.weekday() < 5) for t, s in zip(raw.index, starts)]
    return raw.loc[keep].copy()


def validate_history(raw, start, end):
    if raw.index.has_duplicates or not raw.index.is_monotonic_increasing:
        raise ValueError("Non-unique or unsorted history")
    if raw[["open", "high", "low", "close", "volume"]].isna().any().any():
        raise ValueError("Null OHLCV")
    if ((raw.high < raw[["open", "low", "close"]].max(axis=1)) |
        (raw.low > raw[["open", "high", "close"]].min(axis=1))).any():
        raise ValueError("Invalid OHLC")
    missing = pd.bdate_range(start, end).difference(raw.index.normalize().unique())
    if len(missing):
        raise ValueError(f"Missing weekday sessions: {missing.tolist()}")


def replay(raw, profile, slip=0.0, allow_incomplete=False):
    bars = prepare(raw, profile)
    context = model.build_daily_context(bars)
    trades, inventory = [], []
    for day, group in bars.groupby(bars.index.normalize()):
        start = "09:00" if day < TRANSITION else profile.start
        clock = day + pd.Timedelta(hours=int(start[:2]), minutes=int(start[3:]))
        complete = group.index.min() <= clock + pd.Timedelta(minutes=10) and any(
            t.strftime("%H:%M") == "23:30" for t in group.index)
        inventory.append(dict(profile=profile.name, date=str(day.date()), bars=len(group),
                              first=str(group.index.min()), last=str(group.index.max()),
                              weekend=day.weekday() >= 5, complete_for_eod=complete))
        if day.weekday() >= 5:
            continue  # State/anchors were built above; weekend signals never run.
        if not complete and not allow_incomplete:
            raise ValueError(f"Incomplete weekday data: {profile.name} {day.date()}")
        ctx = context.loc[day]
        pc, pr, ret = float(ctx.prev_close), float(ctx.prev_range), float(ctx.prev_return)
        if not math.isfinite(pc + pr):
            continue
        pos = None
        was_long = was_short = False
        high, low = -math.inf, math.inf
        for row in group.itertuples():
            ts, close = pd.Timestamp(row.Index), float(row.close)
            high, low = max(high, row.high), min(low, row.low)
            hhmm = ts.strftime("%H:%M")
            closed = False
            if pos is not None:
                reason, price, is_limit = None, close, False
                ambiguous = False
                if pos["component"] == "MR":
                    long = pos["side"] == "long"
                    hit_stop = row.low <= pos["stop"] if long else row.high >= pos["stop"]
                    hit_take = row.high >= pos["take"] if long else row.low <= pos["take"]
                    ambiguous = hit_stop and hit_take
                    if hit_stop:
                        reason = "mr_stop"
                        price = min(pos["stop"], row.open) if long else max(pos["stop"], row.open)
                    elif hit_take:
                        reason, price, is_limit = "mr_take", pos["take"], True
                    elif profile.mr == "high180" and ts - pos["entry_bar"] >= pd.Timedelta(minutes=180):
                        reason = "mr_hold180"
                    elif profile.mr == "author41" and hhmm >= "20:00":
                        reason = "mr_time20"
                else:
                    if pos["side"] == "long":
                        if close < pc - 0.35 * pr:
                            reason = "bo_stop2"
                        elif ts.minute == 50 and close < pc + 0.51 * pr:
                            reason = "bo_stop1"
                    else:
                        if close > pc + 0.35 * pr:
                            reason = "bo_stop2"
                        elif ts.minute == 50 and close > pc - 0.51 * pr:
                            reason = "bo_stop1"
                # EOD is a forward bar event, never a retrospective last-bar fill.
                if hhmm >= "23:30" and reason is None:
                    reason = "eod_2330"
                if reason:
                    sign = 1 if pos["side"] == "long" else -1
                    fill = tick(price) if is_limit else tick(price) - sign * slip
                    gross = sign * (fill - pos["entry_price"])
                    cost = model.BROKER_ROUNDTRIP_RUB / model.POINT_VALUE_RUB
                    # Entry is market for every component; only TP is a maker limit.
                    cost += model.TAKER_FEE_RATE * (pos["entry_price"] + (0 if is_limit else fill))
                    trades.append(dict(profile=profile.name, slippage_per_market_side=slip,
                                       component=pos["component"], side=pos["side"],
                                       entry_bar=pos["entry_bar"], exit_bar=ts,
                                       entry_event=pos["entry_bar"] + pd.Timedelta(minutes=10),
                                       exit_event=ts + pd.Timedelta(minutes=10),
                                       entry_price=pos["entry_price"], exit_price=fill,
                                       take=pos.get("take", math.nan), stop=pos.get("stop", math.nan),
                                       exit_reason=reason, ambiguous_touch=ambiguous,
                                       gross_points=gross, cost_points=cost, net_points=gross-cost))
                    pos, closed = None, True
            if pos is not None or closed or hhmm >= "23:30":
                continue
            signal = None
            if profile.mr == "author41":
                signal = model.author41_signal(ts, row, pc, pr, start, profile.mr_end)
            elif profile.mr == "high180":
                signal = model.high180_signal(ts, row, pc, pr, high, low, start, profile.mr_end)
            component = "MR"
            if signal:
                signal = dict(signal, take=tick(signal["take"]), stop=tick(signal["stop"]))
                sign = 1 if signal["side"] == "long" else -1
                if sign * (signal["take"] - tick(close)) < 2*TICK:
                    signal = None
            if signal is None:
                component = "BO"
                signal = model.bo_signal(ts, row, clock, pc, pr, ret, was_long, was_short, profile.wait)
            if signal:
                sign = 1 if signal["side"] == "long" else -1
                if component == "BO":
                    was_long |= signal["side"] == "long"
                    was_short |= signal["side"] == "short"
                pos = dict(signal, component=component, entry_bar=ts,
                           entry_price=tick(close) + sign*slip)
        if pos is not None:
            raise ValueError(f"Open position at data end: {profile.name} {day}")
    result = pd.DataFrame(trades)
    if not result.empty:
        assert (result.entry_event.dt.date == result.exit_event.dt.date).all()
        assert (result.entry_event.dt.weekday < 5).all()
        assert (result.exit_event.dt.strftime("%H:%M") <= "23:40").all()
        assert (result.exit_bar > result.entry_bar).all()
    return result, pd.DataFrame(inventory)


def summarize(trades, start, end):
    out = []
    spans = [("full", start, end), ("initial_research", start, "2026-08-23"),
             ("post_selection", "2026-08-24", end), ("recent", "2026-09-07", end)]
    for period, lo, hi in spans:
        selected = trades[(trades.entry_bar >= pd.Timestamp(lo)) &
                          (trades.exit_bar < pd.Timestamp(hi) + pd.Timedelta(days=1))]
        dates = pd.bdate_range(lo, hi)
        for (profile, slip), g in selected.groupby(["profile", "slippage_per_market_side"]):
            for comp in ["ALL", "MR", "BO"]:
                sub = g if comp == "ALL" else g[g.component == comp]
                daily = sub.groupby(sub.exit_bar.dt.normalize()).net_points.sum().reindex(dates, fill_value=0)
                eq = daily.cumsum()
                dd = eq.cummax().clip(lower=0) - eq
                gains, losses = sub.loc[sub.net_points > 0, "net_points"].sum(), -sub.loc[sub.net_points < 0, "net_points"].sum()
                out.append(dict(period=period, profile=profile, slip=slip, component=comp,
                                trades=len(sub), gross_points=sub.gross_points.sum(),
                                net_points=sub.net_points.sum(), maxdd=dd.max(),
                                profit_factor=gains/losses if losses else None,
                                win_rate=(sub.net_points > 0).mean() if len(sub) else None,
                                best_day=daily.max(), worst_day=daily.min(),
                                ambiguous_touches=int(sub.ambiguous_touch.sum())))
    return pd.DataFrame(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--start", default="2026-07-14")
    ap.add_argument("--end", default="2026-09-25")
    args = ap.parse_args()
    cache = ROOT / f"IMOEXF_2026-07-01_{args.end}_raw_10m.parquet"
    raw = fetch("2026-07-01", args.end, cache)
    validate_history(raw, args.start, args.end)
    if raw.index.max().normalize() < pd.Timestamp(args.end):
        raise RuntimeError("History does not reach requested end")
    trades, inventories = [], []
    for profile in PROFILES:
        for slip in (0.0, 0.5, 1.0):
            t, inv = replay(raw, profile, slip)
            trades.append(t)
            if slip == 0:
                inventories.append(inv)
    all_trades = pd.concat(trades, ignore_index=True)
    all_trades = all_trades[all_trades.entry_bar >= pd.Timestamp(args.start)].copy()
    all_trades.to_csv(ROOT / "trades.csv", index=False)
    pd.concat(inventories).to_csv(ROOT / "session_inventory.csv", index=False)
    summary = summarize(all_trades, args.start, args.end)
    summary.to_csv(ROOT / "summary.csv", index=False)
    daily = all_trades.assign(date=all_trades.exit_bar.dt.normalize()).groupby(
        ["profile", "slippage_per_market_side", "date"]).net_points.sum().reset_index()
    daily.to_csv(ROOT / "daily.csv", index=False)
    manifest = dict(source="MOEX ISS RFUD IMOEXF 10m", timezone="Europe/Moscow",
                    raw_rows=len(raw), first_bar=str(raw.index.min()), last_bar=str(raw.index.max()),
                    raw_sha256=hashlib.sha256(cache.read_bytes()).hexdigest(),
                    formulas_source=str(SOURCE.relative_to(ROOT.parent)),
                    formulas_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
                    script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                    profiles=[vars(p) for p in PROFILES], start=args.start, end=args.end,
                    overnight_violations=int((all_trades.entry_event.dt.date != all_trades.exit_event.dt.date).sum()),
                    invalid_weekend_entries=int((all_trades.entry_event.dt.weekday >= 5).sum()),
                    eod_contract="23:30 labelled close, available at 23:40 MSK; no entries >=23:30",
                    broker_roundtrip_rub=model.BROKER_ROUNDTRIP_RUB,
                    taker_fee_rate=model.TAKER_FEE_RATE, rub_per_point=model.POINT_VALUE_RUB,
                    slippage_scenarios=[0, 0.5, 1.0], tick_size=TICK)
    (ROOT / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    fig, ax = plt.subplots(figsize=(13, 7))
    for name, g in daily[daily.slippage_per_market_side == 0.5].groupby("profile"):
        if "high180" not in name:
            series = g.set_index("date").net_points.reindex(pd.bdate_range(args.start, args.end), fill_value=0)
            ax.plot(series.index, series.cumsum(), label=name)
    ax.axvline(pd.Timestamp("2026-08-24"), color="gray", linestyle="--")
    ax.axhline(0, color="black", linewidth=0.7)
    ax.set(title="IMOEXF: no overnight, costs + 0.5 point / market side", ylabel="Net points / contract")
    ax.legend(fontsize=8); ax.grid(alpha=0.2)
    fig.tight_layout(); fig.savefig(ROOT / "equity.png", dpi=150); plt.close(fig)
    print(summary[(summary.component == "ALL") & (summary.slip == 0.5)].to_string(index=False))


if __name__ == "__main__":
    main()
