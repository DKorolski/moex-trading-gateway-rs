from __future__ import annotations

import json
import math
import subprocess
import urllib.parse
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import matplotlib.pyplot as plt
import pandas as pd


ROOT = Path(__file__).resolve().parent
ANALIZ_ROOT = ROOT.parent
SOURCE_CACHE = (
    ANALIZ_ROOT
    / "moex_session_policy_audit_2026_07_17"
    / "IMOEXF_2023-10-01_2026-07-17_combined_raw_10m.parquet"
)
OUT_CACHE = ROOT / "IMOEXF_2023-11-14_2026-07-22_raw_10m.parquet"
OUT_TRADES = ROOT / "imoexf_session_recut_trades.csv"
OUT_DAILY = ROOT / "imoexf_session_recut_daily.csv"
OUT_SUMMARY = ROOT / "imoexf_session_recut_summary.csv"
OUT_SESSIONS = ROOT / "imoexf_session_inventory.csv"
OUT_EARLY = ROOT / "imoexf_early_session_features.csv"
OUT_ANCHORS = ROOT / "imoexf_anchor_drift.csv"
OUT_CHART = ROOT / "imoexf_session_recut_equity.png"
OUT_REPORT = ROOT / "imoexf_session_recut_report.md"

EARLY_EFFECTIVE = pd.Timestamp("2026-07-14")
REQUEST_END = pd.Timestamp("2026-07-22")
ANALYSIS_START = pd.Timestamp("2026-07-14")
TICK_SIZE = 0.5
POINT_VALUE_RUB = 10.0
BROKER_ROUNDTRIP_RUB = 1.0
TAKER_FEE_RATE = 0.000066


@dataclass(frozen=True)
class SessionPolicy:
    policy_id: str
    session_start: str
    mr_entry_start: str
    mr_entry_end_high180: str
    mr_entry_end_author41: str
    bo_wait_hours: float
    note: str


POLICIES = (
    SessionPolicy(
        "legacy09",
        "09:00",
        "09:00",
        "11:59",
        "12:00",
        3.0,
        "Current live/control: early bars are invisible to model state.",
    ),
    SessionPolicy(
        "shadow07_current_code",
        "07:00",
        "07:00",
        "11:59",
        "12:00",
        3.0,
        "What current Rust overrides do: 07:00 state, hard-coded old MR cutoffs.",
    ),
    SessionPolicy(
        "canonical07_phase_recut",
        "07:00",
        "07:00",
        "09:59",
        "10:00",
        3.0,
        "Intended phase translation: opening MR window shifted two hours earlier.",
    ),
    SessionPolicy(
        "canonical07_compromise11",
        "07:00",
        "08:00",
        "10:59",
        "11:00",
        4.0,
        "One-hour compromise between phase time and the 09:00 liquidity ramp.",
    ),
    SessionPolicy(
        "canonical07_liquidity12",
        "07:00",
        "09:00",
        "11:59",
        "12:00",
        5.0,
        "Canonical 07:00 state and anchors, but old liquid trading clock.",
    ),
)


MODEL_IDS = ("high180_hybrid", "author41_short_hybrid")


def fetch_moex_candles(start: str, end: str) -> pd.DataFrame:
    base = (
        "https://iss.moex.com/iss/engines/futures/markets/forts/boards/"
        "RFUD/securities/IMOEXF/candles.json"
    )
    rows: list[list[Any]] = []
    columns: list[str] = []
    offset = 0
    while True:
        query = urllib.parse.urlencode(
            {
                "interval": 10,
                "from": start,
                "till": end,
                "iss.meta": "off",
                "start": offset,
            }
        )
        raw = subprocess.check_output(
            [
                "curl",
                "-L",
                "--retry",
                "5",
                "--retry-delay",
                "2",
                "--connect-timeout",
                "10",
                "--max-time",
                "60",
                "-sS",
                f"{base}?{query}",
            ]
        )
        payload = json.loads(raw)["candles"]
        columns = payload["columns"]
        batch = payload["data"]
        rows.extend(batch)
        if len(batch) < 500:
            break
        offset += len(batch)
    frame = pd.DataFrame(rows, columns=columns)
    if frame.empty:
        raise RuntimeError("MOEX ISS returned no IMOEXF bars")
    frame["timestamp"] = pd.to_datetime(frame["begin"])
    frame = frame.set_index("timestamp").sort_index()
    return frame.rename(
        columns={"open": "Open", "high": "High", "low": "Low", "close": "Close", "volume": "Volume"}
    )[["Open", "High", "Low", "Close", "Volume"]]


def load_feed() -> pd.DataFrame:
    seed = pd.read_parquet(SOURCE_CACHE)
    seed.index = pd.to_datetime(seed.index)
    fresh = fetch_moex_candles("2026-07-17", REQUEST_END.strftime("%Y-%m-%d"))
    raw = pd.concat([seed, fresh]).sort_index()
    raw = raw[~raw.index.duplicated(keep="last")]
    raw = raw.rename(columns={"Adj Close": "Adj_Close"})
    raw = raw[["Open", "High", "Low", "Close", "Volume"]].dropna()
    raw.to_parquet(OUT_CACHE)
    return raw


def session_inventory(raw: pd.DataFrame) -> pd.DataFrame:
    work = raw.loc[raw.index.normalize() >= pd.Timestamp("2026-07-13")].copy()
    work["date"] = work.index.normalize()
    out = (
        work.groupby("date")
        .agg(first_bar=("Close", lambda s: s.index.min()), last_bar=("Close", lambda s: s.index.max()), bars=("Close", "size"), volume=("Volume", "sum"))
        .reset_index()
    )
    out["weekday"] = out["date"].dt.day_name()
    out["complete_regular_session"] = (
        (out["date"].dt.weekday < 5)
        & (out["first_bar"].dt.time <= pd.Timestamp("07:00").time())
        & (out["last_bar"].dt.time >= pd.Timestamp("23:30").time())
    )
    return out


def early_session_features(raw: pd.DataFrame, complete_end: pd.Timestamp) -> pd.DataFrame:
    work = raw.copy()
    work.columns = [c.lower() for c in work.columns]
    work = work[
        (work.index.normalize() >= ANALYSIS_START)
        & (work.index.normalize() <= complete_end)
        & (work.index.weekday < 5)
    ]
    rows: list[dict[str, Any]] = []
    for date, group in work.groupby(work.index.normalize()):
        full = group.between_time("07:00", "23:49")
        early = group.between_time("07:00", "08:59")
        legacy = group.between_time("09:00", "23:49")
        if early.empty or legacy.empty:
            continue
        full_range = float(full["high"].max() - full["low"].min())
        legacy_range = float(legacy["high"].max() - legacy["low"].min())
        rows.append(
            {
                "date": date,
                "early_bars": len(early),
                "early_volume_share": float(early["volume"].sum() / full["volume"].sum()),
                "early_range_points": float(early["high"].max() - early["low"].min()),
                "return_07_to_09_points": float(early["close"].iloc[-1] - early["open"].iloc[0]),
                "full_07_range_points": full_range,
                "legacy_09_range_points": legacy_range,
                "range_added_by_early_points": full_range - legacy_range,
                "open_07": float(early["open"].iloc[0]),
                "open_09": float(legacy["open"].iloc[0]),
            }
        )
    return pd.DataFrame(rows)


def anchor_drift(raw: pd.DataFrame, complete_end: pd.Timestamp) -> pd.DataFrame:
    contexts: dict[str, pd.DataFrame] = {}
    for policy in (POLICIES[0], POLICIES[2]):
        contexts[policy.policy_id] = build_daily_context(policy_filter(raw, policy))
    legacy = contexts["legacy09"][["prev_close", "prev_range"]].rename(
        columns={"prev_close": "legacy_prev_close", "prev_range": "legacy_prev_range"}
    )
    canonical = contexts["canonical07_phase_recut"][["prev_close", "prev_range"]].rename(
        columns={"prev_close": "canonical_prev_close", "prev_range": "canonical_prev_range"}
    )
    out = legacy.join(canonical, how="inner").reset_index(names="date")
    out = out[(out["date"] >= ANALYSIS_START) & (out["date"] <= complete_end)].copy()
    out["prev_close_delta"] = out["canonical_prev_close"] - out["legacy_prev_close"]
    out["prev_range_delta"] = out["canonical_prev_range"] - out["legacy_prev_range"]
    return out


def policy_filter(raw: pd.DataFrame, policy: SessionPolicy) -> pd.DataFrame:
    work = raw.copy()
    work.columns = [c.lower() for c in work.columns]
    idx = work.index
    old = idx.normalize() < EARLY_EFFECTIVE
    times = pd.Series(idx.time, index=idx)
    starts = pd.Series(
        [pd.Timestamp("09:00").time() if is_old else pd.Timestamp(policy.session_start).time() for is_old in old],
        index=idx,
    )
    keep = (times >= starts) & (times <= pd.Timestamp("23:49").time())
    keep &= idx.weekday < 5
    return work.loc[keep.to_numpy(), ["open", "high", "low", "close", "volume"]].copy()


def build_daily_context(bars: pd.DataFrame) -> pd.DataFrame:
    grouped = bars.groupby(bars.index.normalize()).agg(
        open=("open", "first"), high=("high", "max"), low=("low", "min"), close=("close", "last")
    )
    grouped["range"] = grouped["high"] - grouped["low"]
    grouped["prev_close"] = grouped["close"].shift(1)
    grouped["prev_range"] = grouped["range"].shift(1)
    grouped["day_before_close"] = grouped["close"].shift(2)
    grouped["prev_return"] = (grouped["prev_close"] - grouped["day_before_close"]) / grouped["day_before_close"]
    return grouped


def mr_cost_points() -> float:
    return BROKER_ROUNDTRIP_RUB / POINT_VALUE_RUB


def bo_cost_points(entry: float, exit_: float) -> float:
    return BROKER_ROUNDTRIP_RUB / POINT_VALUE_RUB + TAKER_FEE_RATE * (entry + exit_)


def gross_points(side: str, entry: float, exit_: float) -> float:
    return exit_ - entry if side == "long" else entry - exit_


def close_position(pos: dict[str, Any], ts: pd.Timestamp, price: float, reason: str) -> dict[str, Any]:
    gross = gross_points(pos["side"], pos["entry_price"], price)
    cost = mr_cost_points() if pos["component"] == "MR" else bo_cost_points(pos["entry_price"], price)
    return {
        "model_id": pos["model_id"],
        "policy": pos["policy"],
        "component": pos["component"],
        "side": pos["side"],
        "entry_ts": pos["entry_ts"],
        "exit_ts": ts,
        "entry_price": pos["entry_price"],
        "exit_price": price,
        "gross_points": gross,
        "cost_points": cost,
        "net_points": gross - cost,
        "exit_reason": reason,
    }


def high180_signal(
    ts: pd.Timestamp,
    row: Any,
    prev_close: float,
    prev_range: float,
    day_high: float,
    day_low: float,
    entry_start: str,
    entry_end: str,
) -> dict[str, Any] | None:
    if not (pd.Timestamp(entry_start).time() <= ts.time() <= pd.Timestamp(entry_end).time()):
        return None
    if not math.isfinite(prev_close + prev_range):
        return None
    close = float(row.close)
    rel_range = prev_range / close if close else math.nan
    if not 0.005 < rel_range < 0.050:
        return None
    midpoint = (day_high + day_low) / 2.0
    if close < prev_close and close > prev_close - 0.085 * prev_range and midpoint > close:
        distance = midpoint - close
        return {"side": "long", "take": midpoint, "stop": close - 7.0 * distance}
    if close > prev_close and close < prev_close + 0.090 * prev_range and midpoint < close:
        distance = close - midpoint
        return {"side": "short", "take": midpoint, "stop": close + 7.0 * distance}
    return None


def author41_signal(
    ts: pd.Timestamp,
    row: Any,
    prev_close: float,
    prev_range: float,
    entry_start: str,
    entry_end: str,
) -> dict[str, Any] | None:
    if not (pd.Timestamp(entry_start).time() <= ts.time() <= pd.Timestamp(entry_end).time()):
        return None
    if prev_close <= 0 or prev_range <= 0:
        return None
    rel_range = prev_range / prev_close
    close = float(row.close)
    if not 0.005 < rel_range < 0.075:
        return None
    if not (close > prev_close and close < prev_close + 0.16 * prev_range):
        return None
    take = prev_close - 0.02 * prev_range
    if (close - take) / TICK_SIZE < 2.0:
        return None
    return {"side": "short", "take": take, "stop": prev_close + 0.58 * prev_range}


def bo_signal(
    ts: pd.Timestamp,
    row: Any,
    session_start: pd.Timestamp,
    prev_close: float,
    prev_range: float,
    prev_return: float,
    was_long: bool,
    was_short: bool,
    wait_hours: float,
) -> dict[str, Any] | None:
    if ts - session_start < pd.Timedelta(hours=wait_hours) or prev_range < 1.01:
        return None
    can_long = not (math.isfinite(prev_return) and prev_return < -0.025)
    can_short = not (math.isfinite(prev_return) and prev_return > 0.025)
    close = float(row.close)
    if can_long and not was_long and close > prev_close + 0.53 * prev_range:
        return {"side": "long"}
    if can_short and not was_short and close < prev_close - 0.53 * prev_range:
        return {"side": "short"}
    return None


def replay_hybrid(bars: pd.DataFrame, policy: SessionPolicy, model_id: str) -> pd.DataFrame:
    context = build_daily_context(bars)
    trades: list[dict[str, Any]] = []
    pos: dict[str, Any] | None = None
    current_day: pd.Timestamp | None = None
    day_high = day_low = math.nan
    session_start = pd.Timestamp.min
    was_bo_long = was_bo_short = False

    for row in bars.itertuples():
        ts = pd.Timestamp(row.Index)
        day = ts.normalize()
        if current_day is None or day != current_day:
            if pos is not None:
                trades.append(close_position(pos, pos["last_ts"], pos["last_close"], "forced_session_flatten"))
                pos = None
            current_day = day
            day_high = float(row.high)
            day_low = float(row.low)
            session_start = ts
            was_bo_long = was_bo_short = False
        else:
            day_high = max(day_high, float(row.high))
            day_low = min(day_low, float(row.low))

        if day not in context.index:
            continue
        ctx = context.loc[day]
        prev_close = float(ctx.prev_close)
        prev_range = float(ctx.prev_range)
        prev_return = float(ctx.prev_return)
        if not math.isfinite(prev_close + prev_range):
            continue

        closed_this_bar = False
        if pos is not None:
            pos["last_ts"] = ts
            pos["last_close"] = float(row.close)
            reason = None
            exit_price = float(row.close)
            if pos["component"] == "MR" and ts > pos["entry_ts"]:
                if pos["side"] == "long":
                    if float(row.low) <= pos["stop"]:
                        reason, exit_price = "bracket_stop", pos["stop"]
                    elif float(row.high) >= pos["take"]:
                        reason, exit_price = "bracket_take", pos["take"]
                else:
                    if float(row.high) >= pos["stop"]:
                        reason, exit_price = "bracket_stop", pos["stop"]
                    elif float(row.low) <= pos["take"]:
                        reason, exit_price = "bracket_take", pos["take"]
                if reason is None and model_id == "high180_hybrid" and ts - pos["entry_ts"] >= pd.Timedelta(minutes=180):
                    reason = "mr_time_180m"
                if reason is None and model_id == "author41_short_hybrid" and ts.time() >= pd.Timestamp("20:00").time():
                    reason = "mr_time_20_00"
            elif pos["component"] == "BO":
                close = float(row.close)
                if ts.time() == pd.Timestamp("23:30").time():
                    reason = "bo_eod"
                elif pos["side"] == "long":
                    if close < prev_close - 0.35 * prev_range:
                        reason = "bo_stop2_long"
                    elif ts.minute == 50 and close < prev_close + 0.51 * prev_range:
                        reason = "bo_stop1_long"
                else:
                    if close > prev_close + 0.35 * prev_range:
                        reason = "bo_stop2_short"
                    elif ts.minute == 50 and close > prev_close - 0.51 * prev_range:
                        reason = "bo_stop1_short"
            if reason is not None:
                trades.append(close_position(pos, ts, exit_price, reason))
                pos = None
                closed_this_bar = True

        if pos is not None or closed_this_bar:
            continue

        if model_id == "high180_hybrid":
            signal = high180_signal(
                ts,
                row,
                prev_close,
                prev_range,
                day_high,
                day_low,
                policy.mr_entry_start,
                policy.mr_entry_end_high180,
            )
        else:
            signal = author41_signal(
                ts,
                row,
                prev_close,
                prev_range,
                policy.mr_entry_start,
                policy.mr_entry_end_author41,
            )
        if signal is not None:
            pos = {
                "model_id": model_id,
                "policy": policy.policy_id,
                "component": "MR",
                "side": signal["side"],
                "entry_ts": ts,
                "entry_price": float(row.close),
                "take": signal["take"],
                "stop": signal["stop"],
                "last_ts": ts,
                "last_close": float(row.close),
            }
            continue

        signal = bo_signal(
            ts,
            row,
            session_start,
            prev_close,
            prev_range,
            prev_return,
            was_bo_long,
            was_bo_short,
            policy.bo_wait_hours,
        )
        if signal is not None:
            was_bo_long |= signal["side"] == "long"
            was_bo_short |= signal["side"] == "short"
            pos = {
                "model_id": model_id,
                "policy": policy.policy_id,
                "component": "BO",
                "side": signal["side"],
                "entry_ts": ts,
                "entry_price": float(row.close),
                "last_ts": ts,
                "last_close": float(row.close),
            }

    if pos is not None:
        trades.append(close_position(pos, pos["last_ts"], pos["last_close"], "forced_data_cutoff"))
    return pd.DataFrame(trades)


def summarize(trades: pd.DataFrame, complete_end: pd.Timestamp) -> tuple[pd.DataFrame, pd.DataFrame]:
    work = trades.copy()
    work["entry_ts"] = pd.to_datetime(work["entry_ts"])
    work["exit_ts"] = pd.to_datetime(work["exit_ts"])
    complete = work[(work["entry_ts"].dt.normalize() >= ANALYSIS_START) & (work["exit_ts"].dt.normalize() <= complete_end)].copy()
    summary = (
        complete.groupby(["model_id", "policy", "component"], as_index=False)
        .agg(
            trades=("net_points", "size"),
            net_points=("net_points", "sum"),
            gross_points=("gross_points", "sum"),
            avg_points=("net_points", "mean"),
            wins=("net_points", lambda s: int((s > 0).sum())),
            losses=("net_points", lambda s: int((s <= 0).sum())),
            worst_trade=("net_points", "min"),
        )
    )
    combo = (
        complete.groupby(["model_id", "policy"], as_index=False)
        .agg(
            trades=("net_points", "size"),
            net_points=("net_points", "sum"),
            gross_points=("gross_points", "sum"),
            avg_points=("net_points", "mean"),
            wins=("net_points", lambda s: int((s > 0).sum())),
            losses=("net_points", lambda s: int((s <= 0).sum())),
            worst_trade=("net_points", "min"),
        )
    )
    combo["component"] = "COMBO"
    summary = pd.concat([summary, combo], ignore_index=True).sort_values(["model_id", "policy", "component"])
    complete["date"] = complete["exit_ts"].dt.normalize()
    daily = complete.groupby(["date", "model_id", "policy", "component"], as_index=False)["net_points"].sum()
    combo_daily = complete.groupby(["date", "model_id", "policy"], as_index=False)["net_points"].sum()
    combo_daily["component"] = "COMBO"
    daily = pd.concat([daily, combo_daily], ignore_index=True).sort_values(["model_id", "policy", "component", "date"])
    daily["equity_points"] = daily.groupby(["model_id", "policy", "component"])["net_points"].cumsum()
    return summary, daily


def make_chart(daily: pd.DataFrame) -> None:
    fig, axes = plt.subplots(2, 1, figsize=(12, 8), sharex=True)
    for ax, model_id in zip(axes, MODEL_IDS):
        sub = daily[(daily["model_id"] == model_id) & (daily["component"] == "COMBO")]
        for policy, group in sub.groupby("policy"):
            ax.plot(group["date"], group["equity_points"], marker="o", label=policy)
        ax.axhline(0, color="black", linewidth=0.8)
        ax.set_title(model_id)
        ax.set_ylabel("Net points")
        ax.grid(alpha=0.25)
        ax.legend()
    axes[-1].set_xlabel("Complete session exit date")
    fig.suptitle("IMOEXF session-policy A/B after 07:00 transition")
    fig.tight_layout()
    fig.savefig(OUT_CHART, dpi=160)
    plt.close(fig)


def write_report(
    summary: pd.DataFrame,
    sessions: pd.DataFrame,
    early: pd.DataFrame,
    anchors: pd.DataFrame,
    complete_end: pd.Timestamp,
) -> None:
    combo = summary[summary["component"] == "COMBO"].copy()
    policy_table = pd.DataFrame(
        [
            {
                "policy": policy.policy_id,
                "state_start": policy.session_start,
                "mr_entry_start": policy.mr_entry_start,
                "high180_entry_end": policy.mr_entry_end_high180,
                "author41_entry_end": policy.mr_entry_end_author41,
                "bo_wait_hours": policy.bo_wait_hours,
                "note": policy.note,
            }
            for policy in POLICIES
        ]
    )
    lines = [
        "# IMOEXF 07:00 Session Recut A/B",
        "",
        f"- Complete-session comparison: `{ANALYSIS_START.date()}..{complete_end.date()}`.",
        f"- `{REQUEST_END.date()}` is partial and excluded from the verdict.",
        "- Weekend bars are excluded because both live configs have `weekends_off=true`.",
        "- The 06:50 auction/service bar is excluded; canonical state starts at 07:00.",
        "- MR has priority while flat; an open MR position suppresses BO, matching orchestrator ownership.",
        "- MR exits use broker-bracket level touch with conservative stop-first handling on ambiguous 10m bars.",
        "- BO uses the Rust close-bar stop logic and `wait_hours=3` from the selected session start.",
        "",
        "## Window definitions",
        "",
        policy_table.to_markdown(index=False),
        "",
        "## Complete sessions",
        "",
        sessions.to_markdown(index=False),
        "",
        "## Early-session state",
        "",
        early.to_markdown(index=False, floatfmt=".4f"),
        "",
        "## Next-session anchor drift",
        "",
        anchors.to_markdown(index=False, floatfmt=".2f"),
        "",
        "## Combo comparison",
        "",
        combo.to_markdown(index=False),
        "",
        "## Critical implementation finding",
        "",
        "The existing `shadow07` config sets `mr_session_end_time=09:59`, but the active Rust MR overrides do not consume that value. High180 constructs `High180MrConfig::default()` with `entry_end_time=11:59:59`; Author41 uses hard-coded `12:00`. Therefore `shadow07_current_code` is the actual current behavior, while `canonical07_phase_recut` requires a Rust patch and replay parity before use.",
        "",
        "## Interpretation",
        "",
        "The unchanged-parameter sensitivity confirms that the selected clock window is now a material model parameter. In this short trend-heavy sample, BO eligibility at 10:00 was best, 11:00 was intermediate, and 12:00 was weakest. That ordering is not a valid parameter selection because it is dominated by the strong 15-16 July BO sessions.",
        "",
        "Do not recalibrate K/stops on these six sessions. Keep `legacy09` live, patch the MR override cutoff, and collect parallel 10:00/11:00/12:00 shadow decisions. Re-open a narrow frozen window/K plateau only after at least 20 complete weekday sessions; keep weekends and the 06:50 auction outside all model state.",
    ]
    OUT_REPORT.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> None:
    raw = load_feed()
    sessions = session_inventory(raw)
    sessions.to_csv(OUT_SESSIONS, index=False)
    complete_dates = sessions.loc[sessions["complete_regular_session"], "date"]
    complete_dates = complete_dates[(complete_dates >= ANALYSIS_START) & (complete_dates < REQUEST_END)]
    if complete_dates.empty:
        raise RuntimeError("no complete post-transition weekday sessions")
    complete_end = pd.Timestamp(complete_dates.max())
    early = early_session_features(raw, complete_end)
    early.to_csv(OUT_EARLY, index=False)
    anchors = anchor_drift(raw, complete_end)
    anchors.to_csv(OUT_ANCHORS, index=False)

    all_trades: list[pd.DataFrame] = []
    for policy in POLICIES:
        bars = policy_filter(raw, policy)
        for model_id in MODEL_IDS:
            all_trades.append(replay_hybrid(bars, policy, model_id))
    trades = pd.concat(all_trades, ignore_index=True)
    trades.to_csv(OUT_TRADES, index=False)
    summary, daily = summarize(trades, complete_end)
    summary.to_csv(OUT_SUMMARY, index=False)
    daily.to_csv(OUT_DAILY, index=False)
    make_chart(daily)
    write_report(
        summary,
        sessions[sessions["date"] >= pd.Timestamp("2026-07-13")],
        early,
        anchors,
        complete_end,
    )
    print(summary.to_string(index=False))
    print(f"complete_end={complete_end.date()} trades={len(trades)}")


if __name__ == "__main__":
    main()
