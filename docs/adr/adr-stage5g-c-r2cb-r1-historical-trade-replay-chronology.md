# ADR: Stage 5G-c R2-c-b R1 historical trade replay chronology

Status: review candidate

## Context

FINAM read-only reconciliation returns full trade snapshots. Each poll repeats historical trades with their immutable broker source timestamp and a fresh observation `received_ts`. The rejected R2-c-b implementation compared a known historical trade against the newest source timestamp of a different trade whenever its refreshed receipt made the full struct unequal. A partial A, partial A+B, filled A+B+C sequence could therefore fail on Poll 3 with `TradeTimeRegression`.

## Decision

Chronology classifies a correlated trade by `BrokerTradeId` before applying the global trade watermark.

- A known ID must retain the exact immutable payload, including its original `source_ts`. Its new observation receipt must be greater than or equal to the committed receipt. It bypasses the global source watermark and updates only its retained observation receipt to the maximum.
- A changed immutable payload fails with `TradeIdentityConflict`.
- A known ID with an earlier observation receipt fails with `TradeTimeRegression`.
- Only a previously unseen ID is compared with the global source and receipt watermarks.
- A late-discovered unseen ID older than the newest committed source timestamp remains fail closed with `TradeTimeRegression`. Reordering is not authorized in this stage.

Trade quantity is accumulated only by immutable broker identity, so repeated full snapshots do not count A or B twice.

## Evidence boundary

The native FINAM fixture now contains three polls: partial A, partial A+B, and filled A+B+C. A connector-neutral golden projection is consumed both by the FINAM mapper test and by a public runtime witness:

```text
accepted Stage 5F Market intent
→ accepted Stage 5G-b ACK
→ Poll 1
→ Poll 2
→ Poll 3
→ one Stage 5C lifecycle convergence
```

The runtime crate does not depend on `broker-finam`.

## Replay identity carry-forward

The package receipt remains exact to milliseconds. Stage 5G-c currently receives REST-style full snapshots at a cadence that prevents two broker packages for one request/account from intentionally sharing the same receipt millisecond. The existing request/account/receipt identity is retained for this narrow repair because changing replay-key semantics would broaden the accepted surface.

Before Stage 5G-d, stream-driven reuse, or any faster ingestion path is authorized, evidence identity must bind a connector package sequence or canonical fingerprint in a collision-safe replay key. Receipt-millisecond uniqueness must not be assumed for live streams.

## Closed surfaces

This remains deterministic paper reconciliation only. Stage 5G-d, Redis live consumer/groups, FINAM transport, HTTP POST/DELETE, broker dispatch/execution, runtime-live, real orders, Stage 6, `main` merge, and deployment remain closed.

