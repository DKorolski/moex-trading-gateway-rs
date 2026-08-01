# Stage 5G-c — order/trade/position convergence

## R2-b semantic hardening boundary

R2-a authority recovery was independently accepted at
`c6ae2bdaea2575dd41e6da00acad5c231f3c7572`. R2-b executes the accepted
checker from that detached Git object and byte-pins the R2-a checker, negative
harness, projection descriptor, Stage 5G-a inventory, Stage 5C and frozen Stage
5F. A rehash-aware mutation proves that changing a Stage 5C region, descriptor
hashes and the local checker together is still rejected by detached authority.

Stage 5G-c R1 is the single successor of rejected review candidate
`dba5362444ec279391eed92ff28ebb4ceb729c09`. It does not open Stage 5G-d.

The lifecycle coordinator now consumes a crate-private projection derived only
from accepted Stage 5C source records: request identity, typed intent class,
typed base action, side, source target quantity, pre-position quantity and
expected attribution. These fields are not caller supplied.

Market Entry converges only at the exact signed source target and every partial
snapshot must be non-decreasing in absolute quantity. Market Exit converges
only at flat and every partial snapshot must be non-increasing without changing
sign. A target Market order row, when present, is authoritative: New, Working
and PartiallyFilled await; Unknown blocks; Rejected, Canceled and Expired are
explicit non-execution; Filled requires exact trade and position truth. A
position can never mask a non-terminal order.

Terminal candidate state is transactional. The candidate crosses the single
Stage 5C-j callsite before it is committed. A pre-callback Stage 5C block
reconstructs the linear ACK-resolved capability with the pre-candidate state,
so corrected evidence can retry. Success with remaining Stage 5C lifecycle
expectations fails closed.

Canceled or expired orders with a positive fill require exact correlated
trades and an exact intent-relative target position. Rejected orders carrying
a fill block. Every populated trade identity must match, quantity must be
strictly positive, and changed replay under the same BrokerTradeId conflicts.

Fingerprint schema v2 binds the complete continuation state, including source
projection, exact domain-hashed IDs, ordered orders, trades, target position,
terminal flags, replay ledger, sequence/duplicate counters, and all broker
event-time watermarks. Broker truth and component chronology are non-decreasing
and fail closed. Per-slot order/trade watermarks advance only from exact
correlation. Input vectors are canonicalized, so vector order does not change
the evidence fingerprint or accepted chronology.

Production witnesses use source-reachable BO entry and F04-style BO exit through
the public Stage 5G-b ACK wrapper, canonical BrokerTruthSnapshot and Stage 5C-j.
The matrix covers exact Entry, partial-to-exact Entry, Exit-to-flat, rejected
Exit with an existing position, and corrected retry after a Stage 5C preflight
block.

Status: R2-b implementation review candidate. Date: 2026-08-01.

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

The source-reachable public matrix now exercises Market entry and exit through
the real strategy callback and Stage 5C source projection. Pure broker-neutral
tests additionally exercise target Market order rows and adversarial chronology.
The accepted projection contains exact action, side, quantity, pre-position and
attribution, but it does not contain source limit price or Replace fields.
R2-b therefore validates Market-vs-Limit shape and positive Limit price without
claiming exact source-price equality. Adding those missing authorities requires
a separate projection review; R2-b does not expand Stage 5C.

## Acceptance matrix

All 16 frozen `GOP01`–`GOP16` cases from
`stage5g-lifecycle-entry-inventory.json` are executable in the focused Rust
matrix. The contract is machine-readable in `stage5g-c-contract.json`.

## Still closed

Redis command consumption, consumer groups, FINAM transport, HTTP POST/DELETE,
broker dispatch/execution, runtime-live, real orders, protective completion,
Stage 5G-d, Stage 6, main merge and deployment remain closed.
