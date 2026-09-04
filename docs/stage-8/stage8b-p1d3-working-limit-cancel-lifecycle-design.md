# Stage 8B-P1-d3 working LIMIT/CANCEL/expiry lifecycle design

Status: design-only review candidate. No P1-d3 production source, operational
Redis DB 0 activation, VPS service, FINAM transport or live execution is
authorized by this document.

Accepted predecessor:
`bcd8db546104968dd0e48ab041e02acf6869d224` (Stage 8B-P1-d2 governance
closure, CLOSED / ACCEPTED).

The canonical contracts introduced by this design are:

```text
moex.stage8b.p1d3.working-book.v1
moex.stage8b.p1d3.book-transition.v1
```

## Purpose and scope

P1-d2 proves one deterministic Market request through durable outcome,
RequestFinalized, ACK, replacement `S_ack`, broker truth, replacement
`S_truth` and source XACK. P1-d3 extends that accepted chain only far enough
to support:

- a Day LIMIT that fills on its first eligible M10;
- a Day LIMIT that becomes Working and is evaluated on later eligible M10s;
- deterministic full fill of a previously Working LIMIT;
- deterministic Day expiry under opaque schedule authority;
- an exact CANCEL of a known P1 paper order;
- restart reconstruction from the authenticated working book.

Synthetic partial fills remain forbidden. P1-d4 owns the exhaustive
subprocess crash/replay matrix. P1-e owns the deployable supervisor, and P1-f
owns isolated operational acceptance.

## Sole authority and split lifecycles

Stage 7B remains the sole durable command-request lifecycle authority. The
Stage 5G/6/7 replacement package remains the sole runtime and paper-book
recovery authority. P1-d3 must not add a second writable command journal, a
Redis-derived order ledger, or an independently mutable Hybrid runtime.

P1-d3 explicitly separates two lifecycles:

1. **request lifecycle** — a LIMIT request is durably settled once its exact
   Accepted ACK and first exact broker truth (`Working`, `Filled` or
   `Expired`) are sealed and reread;
2. **order lifecycle** — a `Working` order remains active inside the
   authenticated paper book until a later fill, exact cancel or schedule-owned
   Day expiry is sealed and reread.

In the accepted P1-d0 phrase "terminal feedback", terminal means the terminal
request-processing seal, not necessarily `BrokerOrderLifecycle::Terminal`.
Therefore an initial Working result may XACK its originating command M10 only
after replacement `S_working` is persisted and reread. The active order is not
discarded or inferred from Redis output rows.

## Canonical authenticated working book

The proposed source type is `Stage8bP1d3WorkingBookProjectionV1`. It is carried
inside the existing authenticated replacement package, never as a sidecar.
Its canonical bytes bind:

- schema version and both P1-d3 domains;
- operational identity fingerprint and package generation;
- exact account, instrument and Hybrid attribution;
- original `StrategyRequestId`, durable place `ClientOrderId`, canonical
  command hash and accepted Stage 6 identity;
- deterministic `BrokerOrderId(String)`;
- side, exact quantity, exact LIMIT price and `TimeInForce::Day`;
- exact order status, lifecycle, filled quantity and remaining quantity;
- decision M10 identity and first execution-observation identity;
- last evaluated M10 Redis ID, semantic hash, payload hash and close time;
- opaque schedule-window identity fingerprint and trading-day identity;
- latest canonical order projection hash;
- zero or one deterministic trade identity;
- the total-sequence frontier used by the matching Stage 5G package;
- a transition ordinal and previous-transition hash.

The v1 operational identity has at most one active order. The durable registry
may retain terminal rows needed to resolve later exact cancels, but it may not
silently evict, overwrite or reuse an order ID. The implementation must use a
compile-time bound:

```text
P1D3_MAX_ORDER_RECORDS_PER_GENERATION = 1024
```

Capacity exhaustion fails closed before a new dispatch. Compaction,
generation rotation and runtime-configurable widening are outside P1-d3.

No public constructor, serde input or Redis payload may mint a working-book
authority. Public diagnostics are redacted evidence only.

## Schedule authority

P1-d3 reuses the accepted Stage 5E schedule owner. It may add exactly two
narrow crate-private opaque capabilities:

- `Stage8bP1d3ScheduleStepAuthority`, bound to the exact last-evaluated and
  candidate canonical M10 pair;
- `Stage8bP1d3DayExpiryAuthority`, bound to the exact trading day, last
  eligible M10, schedule fingerprint and Day boundary timestamp.

Both types have private fields, are non-`Clone`, non-serializable and are
consumed once. Redis composition may carry them but cannot parse calendars,
invent session boundaries or reconstruct them from persisted fingerprints.

An expiry authority is issued only when Stage 5E proves that the last eligible
bar has already been evaluated and that no omitted eligible M10 exists before
the exact Day boundary. UTC date comparison, wall clock, process time and the
first bar of a new day are not expiry authorities.

Unavailable, stale, future, ambiguous, cross-instrument or gap-bearing
schedule evidence is a no-effect fail-closed result.

## Per-bar ordering

For every canonical Live final M10 `B[k]`, the order is fixed:

```text
1. authenticate the current replacement package and working book;
2. resolve the exact already-active order against B[k], if not already
   evaluated against B[k];
3. apply any resulting order/trade/position truth;
4. persist, fsync, reread and cross-validate the replacement book seal;
5. invoke the Hybrid callback for B[k] exactly once;
6. persist/publish zero or one new command under the accepted P1-c chain;
7. XACK B[k] only through that existing terminal source owner.
```

The book evaluator has no M10 XACK method. A fill or expiry is visible to the
same Hybrid runtime before its callback for that bar.

The first candidate `B[n+1]` may be observed read-only while command source
`B[n]` remains pending, as frozen by P1-d0/P1-d1. If a LIMIT becomes Working,
`S_working` records `B[n+1]` as already evaluated. When `B[n+1]` is later
delivered through the semantic consumer group, the exact match is a no-op
book evaluation followed by its one Hybrid callback; it cannot fill twice.

## Initial LIMIT evaluation

Only an exact `PlaceOrder` with `OrderType::Limit`, one positive integral
quantity, `Some(positive limit_price)`, `TimeInForce::Day` and `ttl_ms=None`
is supported.

The deterministic order ID is the accepted P1-d0 formula using the full
domain-separated SHA-256. It is allocated once from durable command identity;
bar identity, Redis ID, wall clock, consumer name and process epoch do not
affect it.

For a buy LIMIT on an eligible M10:

```text
bar.low > limit  -> Working
bar.low <= limit -> Filled at min(bar.open, limit)
```

For a sell LIMIT:

```text
bar.high < limit  -> Working
bar.high >= limit -> Filled at max(bar.open, limit)
```

Touch fills the complete remaining quantity. Fill price can improve at the
open and can never be worse than the limit. Limit fill source and receipt
timestamps are both the exact final execution-bar `close_ts`; OHLC does not
prove an intrabar touch time.

If the opaque Day-boundary authority proves that no first eligible bar remains
in the same trading day, the place result is terminal `Expired`, with a
deterministic order ID and zero fill. The P1-d3 implementation may add the
explicit `Stage6dPaperOutcome::LimitExpired { broker_order_id }` variant;
reusing `LimitPending` for terminal expiry is forbidden.

## Initial LIMIT feedback shapes

All three initial outcomes first durably apply the exact Stage 6 outcome and
finalize the exact Stage 7 request before any Stage 5G mutation.

### Working

- ACK: `Accepted`, reason `None`, exact request-level durable client ID and
  exact broker order ID;
- order: LIMIT/Day, `Working`, Active, exact qty, filled zero, remaining qty,
  exact limit price;
- truth: exactly one order, zero trades, zero positions, no cash/instruments;
- chronology: ACK uses `seq_ack`; truth uses `seq_truth = seq_ack + 1`;
- durability: replacement `S_ack` precedes truth and replacement `S_working`
  precedes command-source XACK.

### Filled

- ACK: `Accepted`, reason `None`;
- order: LIMIT/Day, `Filled`, Terminal, filled qty equals qty and remaining is
  exact positive scale-zero Decimal zero;
- exactly one deterministic trade and exactly one resulting target-position
  row, using the accepted P1-d2 scale-8 nearest-even arithmetic;
- replacement `S_ack` and `S_truth` preserve the P1-d2 ordering.

### Expired before first eligible bar

- ACK: `Accepted`, reason `None`;
- order: LIMIT/Day, `Expired`, Terminal, filled zero and remaining qty exact;
- zero trades and zero positions;
- source and receipt timestamps equal the exact schedule-owned boundary;
- replacement `S_ack` and terminal `S_truth` precede source XACK.

The ACK client ID is always the request-level durable client ID. A target
order client ID never replaces it.

## Later Working evaluation

Each later schedule-approved final M10 is evaluated exactly once against the
active row's complete remaining quantity.

- untouched: order remains Working; no Stage 5G event or total sequence is
  allocated, but the last-evaluated bar and transition hash are replacement
  sealed before the Hybrid callback;
- touched: allocate exactly one next total sequence, apply one Filled order,
  one deterministic trade and one resulting position, then replacement seal
  the terminal book before callback;
- same already-evaluated bar: byte-identical no-op with no sequence allocation;
- changed bytes under the same bar identity: hard conflict before any effect.

The deterministic trade ID binds the original order ID, exact fill-bar
semantic identity and fill ordinal `1`. Partial fill, a second trade or a
second terminal transition fails closed.

## CANCEL lifecycle

A CANCEL is admitted only when its Stage 6 identity exactly binds the same
account, instrument and attribution and its target `BrokerOrderId` resolves to
exactly one canonical P1-d3 registry row. `CancelOrder.client_order_id`, when
present, is the target order client ID; it is never the cancel request ACK
client ID.

The cancel command is not effective on its decision bar. At its first
schedule-approved later candidate the target LIMIT is evaluated first. This
freezes fill-before-cancel:

| Target after candidate evaluation | Stage 6 cancel outcome | ACK | New truth |
|---|---|---|---|
| Working | `CancelCanceled` | `Accepted` / no reason | target `Canceled` |
| Filled on this candidate or earlier | `CancelExecutionObserved` | `Recovered` / `RecoveredByBrokerTruth` | none; exact Filled truth already exists |
| Canceled, Expired or Rejected | `CancelAlreadyTerminalNonExecution` | `Recovered` / `RecoveredByBrokerTruth` | none; exact terminal truth already exists |
| incomplete target evidence | `Inconclusive` hold | none | none |
| absent, foreign, ambiguous or conflicting target | fail closed before dispatch | none | none |

Deterministic v1 does not emit `CancelRejected` for an exact known paper
target. An unsupported or identity-conflicting request fails before dispatch.

For `CancelCanceled`, ACK uses `seq_ack`, canceled truth uses
`seq_truth = seq_ack + 1`, and the target order timestamp is the exact
candidate-bar close. For terminal-observed outcomes, the recovered ACK alone
uses the next sequence; duplicate terminal truth is forbidden.

If candidate evaluation itself fills or expires the target, that target truth
uses the earlier sequence and is replacement sealed before the cancel ACK.
The cancel request then finalizes from the already-sealed target state.

Cancel never creates a trade or position mutation by itself.

## Day expiry of a Working order

The last eligible M10 is evaluated before expiry. If it touches, Filled wins.
Otherwise the consumed `Stage8bP1d3DayExpiryAuthority` transitions Working to
Expired at the exact boundary:

- one Expired terminal order row;
- filled quantity remains zero and remaining quantity remains exact;
- zero trades and zero positions;
- one next Stage 5G truth sequence;
- source and receipt timestamps equal the boundary timestamp;
- replacement terminal seal is persisted and reread before a next-session
  callback or any cancel resolution.

Expiry can never be inferred from `Utc::now()`, Redis idle time, TTL, a changed
calendar date or missing market data.

## Replacement seals and sequence ownership

P1-d3 extends the accepted replacement package; it does not write a sidecar.
Every committed phase binds both the complete Stage 5G state and the complete
working-book projection.

The required durable phases are:

```text
S_ack       request ACK committed; only its exact truth continuation exists
S_working   initial Working truth plus active book committed
S_eval      later untouched-bar frontier committed
S_terminal  Filled, Canceled or Expired truth plus terminal book committed
```

`S_working`, `S_eval` and `S_terminal` are logical phase names; implementation
may use one private enum inside a single schema. Each replacement is persisted,
fsynced, reread, MAC-validated and cross-bound to the Stage 6/7 frontier before
the next effect.

Sequence allocation rules are exact:

- initial Working/Filled/Expired: consecutive ACK/truth pair;
- later untouched evaluation: no sequence;
- later autonomous fill or expiry: one next truth sequence;
- successful cancel: consecutive cancel ACK/canceled-truth pair;
- cancel observing already-terminal truth: one recovered-ACK sequence;
- fill/expiry immediately before cancel: target truth sequence precedes the
  cancel ACK sequence.

Overflow, reuse, skipped reserved values or allocation from a different
Stage 5G package fails before mutation.

## Restart and idempotency

Restart authenticates the exact Stage 5G/6/7 package and P1-d3 book together.
It does not rebuild the book from Redis order/trade streams, rerun the Hybrid
callback, read wall clock, or blindly invoke the provider.

The minimum P1-d3 source tests must prove round-trip recovery for:

- initial Working before source XACK;
- Working after source XACK and before next bar;
- same already-evaluated bar replay;
- later Filled;
- later Expired;
- CancelCanceled;
- CancelExecutionObserved;
- CancelAlreadyTerminalNonExecution.

Byte-identical transition replay is idempotent. Same identity with changed
bar bytes, limit, quantity, target, timestamps, sequence, book generation or
prior transition hash is a hard conflict before callback, ACK, truth or XACK.

P1-d4 remains responsible for the complete SIGKILL/frontier matrix, including
pre-dispatch wait, outcome-before-finalization, ACK-before-seal,
truth-before-seal, book-seal-before-callback and source-XACK response loss.

## Implementation slices after design acceptance

1. Add broker-neutral P1-d3 book and pure LIMIT/CANCEL/expiry transition core.
2. Extend Stage 6 paper outcome only with exact variants required above.
3. Extend the Stage 5G replacement package with private P1-d3 book phases.
4. Compose isolated Redis read-only candidate observation and source lifecycle
   under the existing P1 owner.
5. Add pure, restart-roundtrip and isolated Redis tests required by this
   design.
6. Obtain independent source acceptance, then perform a governance-only
   current-tree authority rebind.
7. Open P1-d4 only after that source acceptance and governance closure.

## Explicitly closed

- P1-d3 production source implementation before design acceptance;
- P1-d4 exhaustive crash/replay closure;
- operational Redis DB 0 and VPS paper service activation;
- operational schedule-source adapter and deployable supervisor;
- FINAM HTTP POST/DELETE and broker network dispatch;
- real orders and runtime-live / `LiveReady`;
- partial fills and nonzero fee/slippage models;
- stop, stop-limit, take-profit, replace, bracket and multi-leg execution;
- protective order completion and Generation-2 production authorization.

The current-tree authority remains pinned to accepted P1-d2 while this
design-only candidate is reviewed.
