# Stage 8B-P1-d3 working LIMIT/CANCEL/expiry source

Status: R1 source-correction review candidate.

Accepted design predecessor:
`df330b2424199739ceb7c261321a5e5ee381c332`.

Accepted P1-d2 predecessor:
`bcd8db546104968dd0e48ab041e02acf6869d224`.

## Scope

This slice implements the broker-neutral deterministic paper lifecycle frozen
by the accepted P1-d3 R1 design:

```text
authenticated P1-d2 S_truth
  -> one-time quiescent P1-d3 migration
  -> initial LIMIT Working/Filled/Expired
  -> later M10 evaluation or Day expiry
  -> CANCEL with target-first race resolution
  -> replacement package persisted and reread
  -> exact source XACK last
```

Operational Redis DB 0 and VPS activation remain closed. Redis tests launch a
disposable loopback-only server with a generated namespace. The implementation
attaches no FINAM POST/DELETE transport, broker network dispatch, runtime-live
or real-order capability.

## Canonical working book

`Stage8bP1d3WorkingBookProjectionV1` is embedded in the authenticated Stage 5G
replacement package. It is not a Redis sidecar. Its registry is bounded at
`P1D3_MAX_ORDER_RECORDS_PER_GENERATION = 1024`, rejects duplicate broker order,
request, durable client-order and deterministic order identities, and encodes
records in bytewise UTF-8 `BrokerOrderId` order.

Genesis and every transition use separate domain-separated hashes. A
quiescent accepted P1-d2 truth package migrates exactly once to an empty
ordinal-zero book without a sequence allocation, callback, provider call,
dispatch, ACK, truth or XACK.

## Deterministic outcomes

The pure reducer implements the eight accepted shapes:

1. initial Working;
2. initial Filled;
3. initial Expired;
4. later Filled;
5. later Expired;
6. CancelCanceled;
7. CancelExecutionObserved;
8. CancelAlreadyTerminalNonExecution.

These are independent outcome branches, not one linear lifecycle. Their
durable continuation matrix is:

| Branch | Exact ACK | Replacement | Broker truth | Final source effect |
| --- | --- | --- | --- | --- |
| initial LIMIT -> Working | Accepted / no reason | S_ack -> S_working | Working order | XACK after replacement reread |
| initial LIMIT -> Filled | Accepted / no reason | S_ack -> S_terminal | Filled order + trade + position | XACK after replacement reread |
| initial LIMIT -> Expired | Accepted / no reason | S_ack -> S_terminal | Expired order only | XACK after replacement reread |
| Working -> later Filled | none; autonomous | S_terminal | Filled order + trade + position | XACK after replacement reread |
| Working -> autonomous Expired | none; autonomous | S_terminal | Expired order only | XACK after replacement reread |
| Working -> CancelCanceled | Accepted / no reason | S_ack -> S_terminal | Canceled order only | XACK after replacement reread |
| terminal target -> CancelExecutionObserved | Recovered / RecoveredByBrokerTruth | S_cancel_recovered | no new truth; retained Filled target truth | only XACK after seal reread |
| terminal target -> CancelAlreadyTerminalNonExecution | Recovered / RecoveredByBrokerTruth | S_cancel_recovered | no new truth; retained terminal target truth | only XACK after seal reread |

Command branches emit an exact Stage 7 `RequestFinalized`; autonomous later
fill/expiry branches do not fabricate a request or ACK. Every row is bound to
its named Stage 6 V3 outcome, its golden shape, and fresh/recovery byte
equality in the source evidence.

Buy limits fill when `low <= limit` at `min(open, limit)`. Sell limits fill
when `high >= limit` at `max(open, limit)`. Candidate-bar outcomes use the
exact schedule-approved bar close. Expiry uses only the consumed opaque Stage
5E Day-boundary authority. Wall clock, Redis receive time and inferred
intrabar time are never transition authorities. Partial fills remain rejected.

Every shape has a checked-in fresh/recovery canonical byte vector and SHA-256
in `fixtures/stage8b-p1d3/outcome-golden-v1.json`. Recovery authenticates the
full `Stage8bP1d3OutcomeEvidenceV1` bytes embedded in one Stage 6 V3 record and
recomputes the complete transition. It cannot select a replacement bar,
schedule authority, provider result or caller-supplied sequence.

The independent complete-projection oracle is checked in as
`fixtures/stage8b-p1d3/projection-golden-v1.json`. For each of the eight
shapes, fresh and recovery construction are compared separately with immutable
canonical bytes and SHA-256 for the exact ACK or absence, order rows, trade
rows, position rows, complete event-scoped truth and the aggregate projection.
The projection includes exact vector membership/order, all nested Decimal
`[u8; 16]` values, the reserved sequence pair/frontier and pre/post book
hashes. Equality of the two constructors is therefore not used as its own
oracle.

Filled position projection delegates to the accepted broker-neutral P1-d2
position reducer. That single reducer owns checked quantity arithmetic,
scale-8 nearest-even average bytes, same-side weighted average,
opposite-side reduction with the prior average retained, exact flattening,
crossing/flip semantics and strict `q0`/`avg0` presence consistency.

## Replacement phases

The runtime uses phase-specific linear owners:

- `S_ack`: exact request ACK is durable; only truth continuation exists;
- `S_working` / `S_terminal`: book and exact truth are durable; only source
  acknowledgement remains for the originating command;
- `S_eval`: later bar is already evaluated without sequence allocation; only
  the exact same-bar Hybrid callback remains;
- `S_cancel_recovered`: recovered cancel ACK and unchanged terminal book are
  durable; only the exact cancel source XACK remains.

Initial Working reserves consecutive ACK/truth sequence values. Later fill or
expiry allocates one truth sequence. Untouched evaluation allocates no
sequence. Fill-before-cancel first commits and rereads target terminal truth,
then derives the recovered cancel ACK without new market or schedule input.

The two narrow crash barriers around `S_cancel_recovered` are named:

- `p1d3-after-recovered-cancel-before-s-cancel-recovered`;
- `p1d3-after-s-cancel-recovered-before-source-xack`.

P1-d3 executes both boundaries in a real subprocess, sends `SIGKILL`, restarts
from the durable files and the isolated Redis PEL, and proves the typed
continuation split. Before `S_cancel_recovered`, only exact recovered-ACK
reconstruction is available. After its persisted+reread seal, only truth/source
XACK continuation is available. The source remains pending throughout both
crashes and is acknowledged only from the truth owner.

The subprocess identities also retain a checked collision regression. The
global mapper still deliberately remains outside this narrow slice and can
map distinct UUIDs whose distinguishing bits occur outside its encoded
12-byte prefix to the same value (`00000000000000000000`). P1-d3 therefore
fails closed before effects whenever a derived cancel DCID equals the target
place TCID, and recovery validates the same inequality from authenticated
outcome evidence. A regression uses two genuinely distinct colliding UUIDs;
the ordinary subprocess place/cancel UUIDs additionally encode as distinct
`000D608000000G00G000` and `000D60G000000G00G000`. All values are FINAM-safe,
stable through restart and independent of wall clock.

The complete exhaustive subprocess/SIGKILL frontier matrix remains P1-d4
scope. It is not implied by these two required P1-d3 boundary cases.

The one-intent replacement package is rebound to the deterministically
projected post-`RequestAccepted` Stage 6 checkpoint before export, then checked
against the actual fsync+reread checkpoint. Order-position restart authority
also preserves and overlays the pending P1 semantic slot, so a crash cannot
erase the LIMIT/CANCEL command while retaining its accepted journal row.

## Redis composition

The exact first successor is read with `XRANGE`, not consumer acquisition.
The isolated Redis proof verifies that this read-only observation leaves the
originating M10 as the sole PEL entry. `XACK` is possible only from a truth
owner obtained after replacement persistence and reread. Missing, malformed
or non-contiguous successor input fails closed.

For a later working-order bar, order evaluation and `S_eval`/`S_terminal`
commit happen before the same bar enters Hybrid. Restart resumes only the
authenticated same-bar callback and never evaluates the order twice.

## Deliberately closed

- current-tree authority rebind before independent source acceptance;
- operational Redis DB 0 and VPS paper activation;
- deployable supervisor and schedule-source adapter (P1-e);
- FINAM POST/DELETE and broker network dispatch;
- runtime-live and real orders;
- partial fills;
- exhaustive crash/replay closure (P1-d4).

Acceptance of this source candidate may authorize only its governance-only
authority rebind and then P1-d4. It does not authorize operational activation.
