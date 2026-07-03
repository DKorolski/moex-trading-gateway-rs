# M3d+ operational parity roadmap after ALOR-to-FINAM audit v2

Status: accepted planning input from the 2026-07-03 engineering audit
`audit_alor_to_finam_full_report_v2_peer_review_2026-07-03.md`.

The audit confirms the current project state as a safe M3/M3d pre-endpoint
implementation-transition gate, not a live-ready gateway. ALOR remains the
behavioral and operational oracle; FINAM must reach parity by contract and
evidence, not by directly copying ALOR-specific broker code.

## Current decision

```text
Design / safety boundary: Green
Implementation readiness: Amber
Live readiness: Red
Next work: M3d-1 FINAM contract alignment + hardening
Not next: live enablement, strategy attachment, SLTP/bracket
```

M3d-0 is accepted as transition preparation only. It does not authorize real
order endpoint calls. The next implementation work must close FINAM contract
alignment before any executable order `POST` / `DELETE` source is introduced.

## Required sequence to operational parity

```text
M3d-1  FINAM contract alignment before endpoint implementation
M3d-2  protected exact-two-route endpoint vertical slice, disabled by default
M3e    command consumer and ACK lifecycle, initially without strategies
M3f    broker-truth reconciliation loop and dirty-startup policy
M3g    market-data / own-order stream parity or proven polling substitute
M3h    runtime shadow integration and ops parity
M3i    first live micro: one account, one instrument, MARKET/LIMIT/CANCEL only
M4     stop/SLTP/bracket research after clean live micro
M5     strategy migration: simple system first, RI/RTS last
```

## P0 before any real endpoint call

- CI and scanners green: fmt, tests, clippy, forbidden-surface scan, negative
  harness, Redis shadow smoke, runtime bridge dry smoke.
- `TimeInForce` mapper aligned to FINAM enum semantics.
- Explicit FINAM `OrderStatus` classifier with terminal, pending,
  cancel-pending, policy, degraded/manual, and unknown buckets.
- `InstrumentRegistryValidator` as a `LiveReady` blocker.
- Pinned FINAM enum/status/spec fixtures and drift harness.
- Local mock endpoint that asserts actual HTTP method/path/header/body.
- Auth policy separating safe pre-send refresh from ambiguous post-send
  no-blind-retry cases.
- Gateway-wide FINAM rate limiter shared across read-only, reconciliation, and
  future order endpoints.
- Durable attempt journal committed before actual send.
- Exact two-route transport remains behind an unforgeable endpoint gate and is
  disabled by default until separately reviewed.

## P1 before command consumer / operational gateway parity

- Redis command consumer for broker-neutral `cmd.orders` only after endpoint
  gate and durable journal are accepted.
- ACK lifecycle with XACK after durable outcome and ACK publication.
- Protected mapping store for `request_id -> client_order_id -> broker_order_id`.
- Broker-truth reconciliation loop for order, trades, positions, and uncertain
  cancel/place outcomes.
- Crash/restart recovery matrix for SQLite/WAL order path.
- Daemon failures degrade readiness instead of panic/unwrap/expect exits.
- `/healthz`, `/readyz`, metrics/runbook parity with the ALOR reference.

## P2 before runtime shadow / paper

- Live bar timestamp/finality contract: open/close timestamp, final/updating,
  historical/live differences, session boundaries, dedupe/backfill.
- Own orders/trades stream or REST polling substitute with measured SLA and
  reconciliation proof.
- First-live-bar gate before any strategy live decision.
- Runtime adapter to broker-core protocol.
- Replay/state-restore/dedupe tests using ALOR behavioral oracle scenarios.

## P3 before first live micro

- One-shot operator arm bound to account, symbol, operation, max quantity, and
  TTL.
- One account, one symbol, minimal safe quantity.
- Kill switch, max orders, max loss, and automatic disarm on degraded state.
- Fresh redacted read-only evidence.
- No unknown active orders and flat or explicitly expected position at start.
- End-of-day redacted reconciliation evidence.

## Release Gate R1 anti-regression checklist

Every item must be closed or explicitly waived with an owner before first live
micro:

- No live decision before broker-truth orders snapshot.
- No live decision before broker-truth positions snapshot.
- No live decision before first live bar.
- Gateway readiness blocks entries on stale/degraded state.
- Market closed blocks entries.
- Close-only passthrough works separately from entry blocking.
- Strategy command `request_id` is known before strategy mutates pending state.
- Dropped or non-emitted intent rolls back strategy state.
- Duplicate `request_id` cannot create duplicate broker order.
- Command TTL expiry prevents broker call.
- XACK occurs only after durable outcome and ACK publication.
- Redis DLQ contains safe metadata, not raw sensitive payload.
- Timeout after place order enters unknown pending, not retry loop.
- Accepted response without broker order id requires reconciliation.
- Cancel ACK is not treated as terminal cancel.
- Broker order id mismatch forces manual intervention.
- Unknown broker order status blocks readiness.
- Startup dirty broker-truth state blocks `LiveReady`.
- Instrument params are validated before `LiveReady`.
- Schedule/maintenance blocks `LiveReady` and trading.
- Rate limit disarms order endpoint if unsafe.
- Unauthorized/token expiry does not cause blind retry.
- End-of-day report reconciles orders, trades, positions, and fees where
  available.
- Strategy state cannot restore ALOR live state into FINAM live without
  explicit migration.
- Stop/SLTP/brackets remain disabled until their own contract tests pass.

## Strategy migration order

1. Simple USDRUBF-like market/entry-exit lifecycle.
2. IMOEXF no-overlap hybrid / MR-priority only after owner attribution and
   partial-fill proof.
3. IMOEXF bracket lifecycle only after M4 stop/SLTP/bracket semantics.
4. RI/RTS last, after event-risk kill switch, freeze-intent semantics,
   partial-fill controls, and tail-risk runbook.

## Still not allowed

- Real FINAM order endpoint calls before M3d-1 and the later M3d-2 endpoint
  implementation review.
- Runtime strategies connected to real order emission.
- `LiveReady`.
- Stop/SLTP/bracket routes.
- Blind retry after timeout, connection reset, `408`, `504`, or ambiguous
  `401`.
- Treating cancel accepted as terminal canceled without broker truth.
