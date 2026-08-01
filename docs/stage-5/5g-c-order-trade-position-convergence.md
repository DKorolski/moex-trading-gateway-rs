# Stage 5G-c — order/trade/position convergence

Status: implementation review candidate. Date: 2026-08-01.

Accepted predecessor:
`92f57c7831d8a15fb2e37668d3b07f1ccea03af7` (Stage 5G-b R3).

## Boundary

Stage 5G-c adds one opaque, linear, paper-only capability around the accepted
ACK-resolved Stage 5G state. Input is a canonical `BrokerTruthSnapshot`; order,
trade and position data are not represented by a second local broker DTO.

The coordinator owns only admission, ordering, exact identity correlation and
paper reconciliation:

```text
Stage5gResolvedMockAckPaperStrategy
  -> canonical BrokerTruthSnapshot events
  -> active/partial accumulation without callback
  -> terminal-complete preflight
  -> existing resolve_stage5c_paper_broker_lifecycle (Stage 5C-j) once
```

Stage 5C-j remains the sole source callback and type-state authority. Broker
Core remains the status, lifecycle, instrument and broker-truth authority.
The pinned Stage 5C and Broker Core authority files are unchanged.

## Semantics

- Exact `StrategyRequestId`, `ClientOrderId` and `BrokerOrderId(String)` are
  preserved; client/broker IDs alone never resolve a strategy request.
- Working and partial snapshots are accumulated without mutating the strategy.
- Filled evidence requires exact correlated trade quantity and target-position
  confirmation before Stage 5C-j.
- Canceled, rejected and expired broker order states terminate without a
  position change.
- Unknown status, quantity regression, terminal regression, side mismatch,
  overfill, account/instrument mismatch and contradictory trade identity fail
  closed before callback.
- Target-instrument rows are lifecycle truth. Non-target account-wide active or
  unknown orders are safety blockers, never target settlement evidence.
- Exact replay is idempotent; the same evidence identity with a changed payload
  is a typed conflict.
- Paper evidence carries parsed Hybrid attribution separately because the
  canonical operational snapshot deliberately does not store strategy comments.
  Stage 5C-j validates it against source-owned attribution.

The accepted paper Stage 5F callback currently emits Market entry/exit intents.
Stage 5G-c does not forge a Place intent merely to make order tests reachable.
The canonical order/trade/position matrix therefore tests the coordinator as a
pure broker-neutral boundary, while the public source-owned wrapper witness
uses a terminal ACK requiring zero broker callbacks. Source-producible
protective Place lifecycle remains Stage 5G-f.

## Acceptance matrix

All 16 frozen `GOP01`–`GOP16` cases from
`stage5g-lifecycle-entry-inventory.json` are executable in the focused Rust
matrix. The contract is machine-readable in `stage5g-c-contract.json`.

## Still closed

Redis command consumption, consumer groups, FINAM transport, HTTP POST/DELETE,
broker dispatch/execution, runtime-live, real orders, protective completion,
Stage 5G-d, Stage 6, main merge and deployment remain closed.
