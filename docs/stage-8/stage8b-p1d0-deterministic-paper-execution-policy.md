# Stage 8B-P1-d0 deterministic paper execution policy

Status: design/policy review candidate. No provider implementation or
operational Redis DB 0 activation is authorized by this document.

Accepted predecessor:
`3d08f84a4a01d08265120def697584c3e60bcd3c` (Stage 8B-P1-c governance
closure).

## Purpose

P1-d will add a deterministic paper provider and canonical
order/trade/position feedback to the accepted P1-c command-publication
boundary. The accepted semantic-commit addendum requires execution timing,
market/limit/cancel behavior, partial fills, fees, slippage and paper IDs to be
frozen before provider code is written. This P1-d0 slice freezes those choices.

The policy is versioned as:

```text
moex.stage8b.p1d.execution-policy.v1
```

P1-d0 is design-only. The current `Stage7aPaperOutcomeProvider` and
`Stage7bRedisService` production implementations remain byte-identical to the
accepted P1-c source.

## Authority and ownership

Stage 7B remains the sole durable command-lifecycle authority. P1-d must not
add a second writable command journal, independently mutable Hybrid runtime or
legacy P0 settlement authority.

The provider owns one broker-neutral paper-book projection derived from:

1. an already accepted canonical `BrokerCommand`;
2. its Stage 6 durable request identity;
3. one exact canonical final M10 execution-bar authority;
4. the preceding canonical paper-book projection.

The projection is persisted only inside the existing Stage 5G/6/7 recovery
chain. The legacy `PaperLedgerSnapshot` may be used as a read-only comparator,
but it cannot settle a P1 command or advance P1 state.

## Initial command surface

Policy v1 supports only:

- `PlaceOrder` with `OrderType::Market` and `TimeInForce::Day`;
- `PlaceOrder` with `OrderType::Limit` and `TimeInForce::Day`;
- `CancelOrder` for an exact known P1 paper order.

Stop, stop-limit, take-profit, replace, bracket, multi-leg and every non-Day
place order remain unsupported. They fail closed before paper dispatch and do
not produce a successful ACK.

The initial operational identity remains single-account, single-strategy and
single-instrument: `finam-paper` / `hybrid_imoexf` / `IMOEXF@RTSX`.

## Canonical chronology

Let `B[n]` be the canonical final M10 on which the Hybrid callback emits a
place command. That command is never evaluated against `B[n]`.

Its first execution candidate is `B[n+1]`, defined as the first later
canonical final M10 admitted by the same broker-neutral schedule authority.
Any interval between the bars must be either:

- a complete expected M10 interval; or
- explicitly ineligible according to the same accepted schedule-window
  evidence, for example a clearing break.

An unproven missing eligible interval is `ExecutionBarGap`. It leaves the
command pending and forbids dispatch, provider invocation, ACK, trade,
position mutation and source-M10 XACK.

### Schedule eligibility authority

P1-d must reuse the already accepted opaque Stage 5E sequence authority. The
only allowed positive classifications are the existing
`Stage5eScheduleSequenceClassification::Contiguous` and
`Stage5eScheduleSequenceClassification::ApprovedNonTradableBoundary` results
minted from the retained `Stage5eScheduleProjectionBridgeInput` by the existing
Stage 5E classifier.

P1-d1 may add one narrow crate-private bridge that consumes that retained
projection and returns an opaque execution-bar eligibility capability bound to
the exact predecessor bar, candidate canonical M10 identity, command identity,
classification and boundary fingerprint. Redis composition and the paper
provider must not parse calendars, session config or raw schedule rows, mint a
classification, or reconstruct this capability from serialized fields.

Unavailable, expired, ambiguous, uncovered or cross-trading-day schedule
evidence is fail-closed and cannot authorize dispatch. Cross-session Day-order
expiry is resolved only by the accepted schedule owner; it is not inferred
from a UTC date comparison or wall-clock time.

History and warmup bars never execute operational paper commands. The
operational provider accepts only canonical `Live` final M10 bars. A separate
explicit replay test mode may consume immutable fixtures but cannot share an
operational namespace or authority.

For each accepted final M10 the order is:

```text
1. resolve orders already eligible before this bar against this final bar;
2. apply their exact order/trade/position feedback through the accepted owner;
3. run the Hybrid semantic callback for this bar;
4. persist/publish any new command for eligibility on a later bar.
```

A cancel generated at a bar close therefore cannot retroactively cancel a
fill proven by that same completed bar.
Fill-before-cancel is the frozen chronology for that case.

## Pre-dispatch execution eligibility

The existing Stage 7B service currently records `DispatchAttemptRecorded`
before calling `Stage7aPaperOutcomeProvider`. P1-d must add a narrow,
fail-closed execution-eligibility preflight before that transition.

Until an exact eligible execution bar is durable and bound to the command:

```text
command Redis entry       pending
RequestAccepted           may already be durable from P1-c
DispatchAttemptRecorded   forbidden
paper provider call       forbidden
ACK/DLQ/XACK              forbidden
```

`NoInput` and `AwaitingExecutionBar` are retryable no-effect states. They are
not `Uncertain` and do not consume the linear composition owner. A conflicting
bar identity, changed payload, schedule ambiguity or multiple eligible
candidates fails closed.

The later execution bar is observed read-only from the same canonical M10
stream by exact Redis ID, semantic identity and payload hash. This observation
must not use `XREADGROUP`, add the bar to a PEL, XACK it or invoke the Hybrid
callback. The independent market-data publisher may append `B[n+1]` while the
originating `B[n]` remains pending. After `B[n]` reaches terminal feedback and
is XACKed, `B[n+1]` is delivered normally through the semantic consumer group
and advances the Hybrid callback exactly once. Read-only execution observation
therefore avoids a wait-for-next-bar deadlock without creating a second
semantic-bar authority.

After the exact execution input is bound, Stage 7B alone may mint
`DispatchAttemptRecorded`; only then may the provider derive an outcome.
A crash after that record but before durable outcome remains reconciliation
required under the accepted Stage 7 rules. It must not blindly call the
provider again.

## Market fill policy

A valid market place command fills in full at the first eligible final M10
open:

```text
fill_price = execution_bar.open
fill_source_ts = execution_bar.open_ts
filled_qty = command.qty
remaining_qty = 0
order_status = Filled
```

`observed_at`, Redis entry IDs, wall-clock time and process boot identity are
not price or fill-time authorities.

## Limit fill policy

A new or previously working Day limit order is evaluated once per eligible
canonical final M10.

For a buy:

```text
if bar.low > limit:
    Working
else:
    fill_price = min(bar.open, limit)
```

For a sell:

```text
if bar.high < limit:
    Working
else:
    fill_price = max(bar.open, limit)
```

The formula allows deterministic opening-gap price improvement and never
fills worse than the limit. A touched limit fills the complete remaining
quantity. Its fill source timestamp is the final bar `close_ts`, because OHLC
does not prove the intrabar touch timestamp.

An untouched order becomes or remains `Working`. It is reconstructed from the
same Stage 5G/6/7 authority and evaluated on each later eligible final M10.
Day expiry is applied at the accepted session boundary after the last eligible
bar has been evaluated. Expiry produces no trade and no position change.

## Partial-fill policy

Synthetic partial fills are disabled in policy v1. Every place outcome is
exactly one of:

- full fill;
- active working order with zero fill;
- terminal rejected/expired order with zero fill.

`OrderStatus::PartiallyFilled`, nonzero filled quantity below order quantity,
or multiple synthetic trades for one v1 place request is an unsupported
projection and fails closed. Broker-truth partial-fill support elsewhere in
the project remains unchanged; this rule applies only to the deterministic P1
paper provider.

## Fee and slippage policy

Policy v1 uses explicit zero economics:

```text
market slippage adjustment = 0 ticks
limit slippage adjustment  = 0 ticks
commission                 = 0
fees                       = 0
```

Zero is materialized in canonical trade evidence rather than omitted. A
future nonzero model requires a new policy version and separate review; it
must not be enabled by configuration under v1.

## Deterministic identifiers

Paper IDs are derived from immutable durable identity, never from Redis IDs,
timestamps, vector order, consumer name or process epoch.

```text
BrokerOrderId =
  "P1D-O-" + hex(sha256(
    "moex.stage8b.p1d.order-id.v1" || NUL ||
    operational_identity_sha256 || NUL ||
    StrategyRequestId || NUL ||
    canonical_command_sha256))

BrokerTradeId =
  "P1D-T-" + hex(sha256(
    "moex.stage8b.p1d.trade-id.v1" || NUL ||
    BrokerOrderId || NUL ||
    execution_bar_semantic_id_sha256 || NUL ||
    "1"))
```

The complete lowercase 64-hex digest is retained. Fill ordinal is fixed to
one because v1 forbids partial fills. Replaying the same durable inputs must
produce byte-identical IDs and projection bytes.

## Cancel ordering and outcomes

Cancel accepts only an exact target `BrokerOrderId` already correlated with
the same account, instrument, strategy attribution and client-order identity.

After all fills proved by the completed bar are applied:

- active Working target -> `CancelCanceled`, target becomes `Canceled`;
- already Filled target -> `CancelExecutionObserved`;
- already Canceled/Expired/Rejected target ->
  `CancelAlreadyTerminalNonExecution`;
- absent, foreign, ambiguous or identity-conflicting target -> fail closed;
- incomplete target truth -> `Inconclusive`, remains pending.

Cancel never emits a trade or position change by itself.

## Canonical outcome and feedback bundle

One provider decision produces one opaque, linear P1-d outcome bundle. It
contains the exact `Stage6dPaperOutcome` plus canonical projections derived
from the same inputs:

- one `BrokerOrderSnapshot`;
- zero or one `BrokerTradeSnapshot`;
- one target `BrokerPositionSnapshot` only when a fill changes position;
- exact request/client/order/trade/attribution and execution-bar bindings.

Stage 6/7 durable settlement consumes the outcome first. The exact canonical
ACK then resolves only the matching `StrategyRequestId`. Order/trade/position
feedback is applied to the same Hybrid owner through the accepted Stage 5G
order-position/fresh-truth boundary. Duplicate bundles are idempotent;
conflicting duplicates fail closed before callback or position mutation.

Position arithmetic uses exact decimal quantities:

- same-side fill increases quantity and uses weighted average price;
- opposite-side fill reduces the existing side at its existing average;
- flat sets `avg_price = None`;
- a flip values the residual side at the new fill price.

Account-wide position row counts remain diagnostic. Lifecycle truth and flat
checks use the exact target instrument plus nonzero quantity.

## Restart and retention

The recovery seal must bind the current paper-book projection, execution-bar
identity and every active order needed for later limit/cancel evaluation.
Restart does not use Redis output rows as authority and does not invoke the
Hybrid callback or provider merely to reconstruct state.

Until terminal feedback and the post-feedback recovery seal are durable and
reread, the originating M10 remains unacknowledged. XACK is always last.

## Implementation sequence after policy acceptance

1. **P1-d1 provider core:** opaque execution-bar eligibility and deterministic
   market outcome/ID bundle, with no Redis DB0 activation.
2. **P1-d2 feedback composition:** exact ACK plus canonical
   order/trade/position application to the same Stage 5G owner.
3. **P1-d3 working limit/cancel book:** later-bar evaluation, expiry and
   restart recovery under the same seal authority.
4. **P1-d4 crash/replay closure:** pre-dispatch wait, post-dispatch uncertain,
   duplicate/conflict and post-feedback pre-XACK matrices on isolated Redis.
5. A governance-only current-tree authority rebind follows accepted source
   implementation. Operational DB0 activation remains deferred to P1-f.

Each source slice requires independent review.
P1-d0 acceptance authorizes only P1-d1 source implementation.

The current-tree authority intentionally remains pinned to accepted P1-c while
this design candidate is reviewed. Rebinding it for documentation-only commits
would create governance churn without expanding production authority. It must
be rebound only after the corresponding P1-d source implementation is
independently accepted.

## Explicitly closed

- operational Redis DB 0 activation;
- P1 paper VPS service activation;
- FINAM HTTP POST/DELETE;
- broker network dispatch;
- real orders;
- runtime-live / `LiveReady`;
- protective/stop/bracket/replace/multi-leg execution;
- Generation-2 production authorization.
