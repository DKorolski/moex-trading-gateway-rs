# M4-3a dual-broker shadow parity foundation

Status: source-only / no-live parity foundation.

M4-3a starts the controlled transition from the ALOR-centered live contour to
the new broker-neutral FINAM contour. It does not run two live trading systems
at once. The intended deployment shape is:

```text
ALOR mature contour / oracle ─┐
                              ├─ parity comparator → evidence/diff report
FINAM new contour / shadow ───┘
```

Only one broker may be active for live trading at a time. During M4-3, ALOR may
remain the operational oracle while FINAM runs in read-only shadow mode.

## Trading boundary

M4-3a must not:

- send FINAM `POST /orders`;
- send FINAM `DELETE /orders/{id}`;
- enable command-consumer-to-real-FINAM;
- enable continuous runtime live;
- open or close a position;
- enable Stop/SLTP/bracket/replace/multi-leg;
- make FINAM cutover automatic.

The comparator reports `live_order_authorized = false` by construction. A
`cutover_safe = true` or `bars_synchronized = true` result is evidence only; it
does not authorize order submission.

## What is being checked

The first parity layer is broker-neutral and instrument-scoped.

For broker truth, M4-3a compares:

- target position quantity;
- target flat/non-flat state;
- target active orders;
- target unknown orders;
- account-wide active orders as a safety guard;
- account-wide unknown orders;
- account-wide orphan orders;
- other-symbol active orders;
- received timestamp skew;
- target instrument spec compatibility across brokers.

For bars, M4-3a compares final bars by:

- target instrument;
- finality;
- timeframe;
- open timestamp;
- close timestamp;
- OHLCV;
- source kind.

`source_kind` may differ diagnostically (`LiveStream` for ALOR, `HistoricalPoll`
or `ReadOnlyPoll` for FINAM) as long as the final canonical bar is identical.

## Why this is not a two-system trading hack

M4-3 is a temporary synchronization and semantic validation stand:

1. ALOR keeps acting as the mature oracle.
2. FINAM publishes broker-neutral shadow health/readiness/truth/market data.
3. The comparator classifies diffs before runtime attachment.
4. Once FINAM is stable, ALOR is removed from the live operational path.

The target steady state is:

```text
Strategy runtime
  -> broker-neutral contract
  -> FINAM gateway
  -> FINAM API
```

ALOR may remain only as archived fixtures or a regression oracle.

## VPS shadow rollout scope

The VPS contour may run `finam-gateway-shadow-loop` with:

- read-only token;
- Redis shadow streams namespaced away from the current ALOR live streams;
- explicit account/symbol configuration supplied by local `.env` or CLI flags;
- initial `TIME_FRAME_M1` bars for the seed shadow feed;
- `order_placement_enabled = false`;
- `cancel_enabled = false`;
- `command_consumer_enabled = false`;
- `stop_sltp_bracket_enabled = false`.

The shadow loop may publish:

- `Health`;
- `Readiness`;
- `PortfolioSnapshot`;
- `OrderSnapshot`;
- `MarketData`.

It must not consume order commands.

The production strategy parity target is still 10-minute closed-bar behavior.
M4-3a does not assume that FINAM exposes a broker-native `TIME_FRAME_M10`
history endpoint for every instrument. M4-3b first moves FINAM shadow
market-data input to WebSocket `LiveStream`. If direct M10 fetch/stream is
unavailable, the next parity step must build a canonical M1-to-10m final-bar
aggregator and compare those derived 10m bars against the ALOR oracle stream
before any strategy runtime cutover.

## Instrument rollout order

Recommended order:

1. IMOEXF — first because both FINAM micro evidence and ALOR live bar evidence
   already exist.
2. USDRUBF — second, after identity/price-step/lot-size mapping is stable.
3. RI/RTS — last, because freeze/intent semantics are most sensitive and prior
   incidents showed one-bar decision replacement risk.

## Cutover criteria

FINAM can be considered for active runtime only after a reviewed package proves:

- repeated parity windows have zero blocking truth diffs;
- final bars are synchronized for every target strategy timeframe;
- strategy decision journal emits the same decisions from the same canonical
  bars;
- FINAM no-send intents match ALOR active/oracle intents;
- target positions reconcile instrument-scoped flat/non-flat state;
- account-wide active/unknown/orphan orders are clean or explicitly blocked;
- stale quote/bar/readiness conditions block entry;
- no blind retry after ambiguous send exists;
- kill switch and emergency cancel policy are reviewed separately.

## M4-3a implementation artifacts

M4-3a adds broker-core source-level comparators:

- `compare_broker_truth_for_instrument(...)`;
- `compare_final_bars_for_instrument(...)`;
- `BrokerTruthParityReport`;
- `BrokerBarParityReport`;
- `BrokerParityIssueKind`.

These are pure functions over canonical broker-neutral data. They do not know
FINAM secrets, ALOR secrets, Redis URLs, or HTTP endpoints.

## Acceptance

M4-3a is ready for review when:

- broker-core parity tests pass;
- M4-3a evidence script reports all source markers present;
- forbidden-surface scanners remain green;
- VPS shadow example config contains only synthetic placeholders;
- handoff archive excludes `.env`, `.git`, `target`, `tmp`, `reports`, logs, and
  local deployment overrides.
