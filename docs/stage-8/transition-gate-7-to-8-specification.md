# Transition Gate 7→8 — protected FINAM execution specification

Status: Gate 7→8 R3 specification candidate pending independent review.

R1 `f7afc1c612c25de608783850ab2e8c0ae14b0687` closed the findings against
rejected base `4d1106e72bc1437d990a8bd949db4867d41c09b6`, but was not accepted because
CANCEL 401 and same-durable-request re-execution after `DefinitelyNotSent`
were not pinned. R2 `5da0db6cd1fd4c2c2eea6731d615ddd48a87b8ae`
closed those gaps but was not accepted because its post-acceptance transition
authority was contradictory. R3 closes only that transition-rule gap. It
remains docs/scripts-only and changes no production Rust, Cargo or GitHub
workflow surface.

This gate is a pre-implementation contract. It decides how the accepted Stage
7B durable paper service may later attach to a protected FINAM adapter without
weakening durability, identity, settlement or restart guarantees. This commit
contains no Stage 8 production implementation and opens no broker endpoint.

## 1. Frozen predecessor

The gate is based on:

- accepted Stage 7B source `a1044e0dbe324c722b637498ca80ffafd9f0cbee`;
- governance closure record `7c3ffffcfec012f3c96c65a3fcaf366c1740b88e`;
- independent review SHA-256
  `e66d87ae88cf1c1a8f2ec12ac5d4338374c26bfcc417624b0fa1f007a5c81bf2`.

Stage 7B remains the durable lifecycle and Redis settlement authority. Stage 8
must not introduce a second request journal, identity allocator, lifecycle
reducer or settlement owner.

## 2. Gate decision

Independent acceptance of this specification authorizes exactly one next
slice: **Stage 8A-0 current FINAM contract refresh/freeze**.

```text
8A-0 contract refresh/freeze  OPEN for docs/evidence/checkers only
8A-1 protected capability     CLOSED pending independent 8A-0 acceptance
8A-2 builder composition      CLOSED pending independent 8A-1 acceptance
8A-3 endpoint classifier      CLOSED pending independent 8A-2 acceptance
8A-4 reconciliation           CLOSED pending independent 8A-3 acceptance
8A-5 aggregate acceptance     CLOSED pending independent 8A-4 acceptance
```

Gate acceptance does not authorize Stage 8 production Rust, a protected
capability implementation, builder composition, classifiers, reconciliation
implementation or real FINAM POST/DELETE. Stage 8B remains closed and requires
a separate specification, independent acceptance and explicit operator
authorization for the exact micro run.

It does not authorize real FINAM POST/DELETE.

## 3. Execution capability boundary

Future production code must use an opaque linear capability, referred to here
as `Stage8ExecutionCapability`. The name is normative; the Rust type is not
implemented by this gate.

The capability must:

- have private fields and no public constructor;
- not implement `Clone`, `Copy`, `Serialize` or `Deserialize`;
- not expose raw credentials, routes or account identifiers through `Debug`;
- be created only after all preflight evidence below is validated;
- bind one durable request identity and one exact command;
- be consumed at the first operation that may reach broker transport;
- expire on TTL, restart, configuration drift, readiness loss or kill switch;
- never be reconstructed from Redis payloads, diagnostics or broker responses.

Required issuance inputs are:

1. recovered Stage 7B service authority and current recovery seal;
2. exact Stage 6 durable request and stable `ClientOrderId`;
3. one-shot operator arm bound to the same command;
4. accepted account, instrument and strategy allowlist entries;
5. validated quantity, price and notional limits;
6. persistent kill-switch mechanism that is available, fresh and readable,
   with the exact state `RunAllowed`;
7. fresh account-scoped and instrument-scoped broker truth;
8. zero unresolved ambiguous outcomes for the strategy/account scope;
9. available max-one engineering-micro budget;
10. proof that no other broker owns live execution for the same strategy.

The durable `DispatchAttemptRecorded` event and fsync-confirmed journal receipt
must exist before the capability can enter an operation that may send bytes.

## 4. Operator authorization

The operator arm is one-shot and non-renewing. It binds:

- durable request ID and stable client order ID;
- account digest;
- instrument identity and FINAM venue symbol digest;
- strategy ID, owner, cycle and action;
- command kind, side and quantity;
- exact LIMIT price or MARKET reference-price guard;
- maximum notional and slippage guard;
- build, configuration and endpoint-policy digests;
- issue time, expiry time and unique arm nonce.

An arm is consumed on the first possible transport attempt, not on a successful
HTTP response. Restart never re-arms it. Any mismatch, stale truth, readiness
loss, existing unresolved lifecycle or active kill switch disarms it.

## 5. Current FINAM contract and sole serializer boundary

The official FINAM REST order contract was refreshed at
`2026-08-14T15:00:32Z` from `https://api.finam.ru/docs/rest/`, normalized in
`finam-rest-order-contract-snapshot-2026-08-14.json`, and compared with the
current project builder and pinned enum fixture in
`finam-rest-order-contract-evidence-2026-08-14.json`. Material drift blocks
this gate; it never silently authorizes mapper or policy changes.

The pinned endpoint paths are:

- PLACE: `POST /v1/accounts/{account_id}/orders`;
- CANCEL: `DELETE /v1/accounts/{account_id}/orders/{order_id}`.

The official PLACE body documents `symbol`, `quantity`, `side`, `type`,
`time_in_force`, `limit_price`, `stop_price`, `stop_condition`, `legs`,
`client_order_id`, `valid_before` and `comment`. Initial Stage 8 policy permits
only MARKET/LIMIT fields and prohibits stop, conditional and multi-leg fields.

Stage 8A must compose the existing sole vetted serializers:

- `broker_finam::build_place_order_request()`;
- `broker_finam::build_cancel_order_request()`.

A second Stage 8 FINAM JSON/request serializer is forbidden. Initial protected
PLACE allows only `TimeInForce::Day -> TIME_IN_FORCE_DAY`; every other TIF
fails before the existing builder is called. Every PLACE explicitly sends the
exact Stage 6 durable `ClientOrderId`: non-empty, at most 20 characters, with
no broker-generated or transport-generated fallback.

## 6. Command and FINAM mapping contract

Only these broker-neutral commands are initially representable:

| Command | Required mapping | Rejected conditions |
| --- | --- | --- |
| PLACE MARKET | account, instrument, side, positive quantity, stable client order ID; no limit price | stale reference price, unsupported side/quantity, any price field requiring LIMIT semantics |
| PLACE LIMIT | MARKET fields plus positive canonical Decimal price aligned to loaded FINAM tick | missing/non-positive price, float-derived ambiguity, tick mismatch, stale instrument parameters |
| CANCEL | account, instrument, stable cancel client ID and exact Stage 6-correlated target broker order ID | unknown/foreign/terminal target without reconciliation, symbol/account mismatch |

The adapter must not invent IDs or use numeric surrogates. The original
`StrategyRequestId`, derived stable `ClientOrderId`, broker-native string order
ID and exact instrument identity remain distinct.

Unknown FINAM enum values, undocumented required fields or an unrecognized
successful status are not coerced into acceptance. They become typed blocked
or reconciliation-required outcomes.

REPLACE, Stop, SLTP, bracket and multi-leg are outside the initial capability
and fail before transport.

## 7. Endpoint-specific broker outcome classification

The first boundary distinguishes:

- `DefinitelyNotSent`: local validation or connect failure with proof that no
  bytes left the process; capability remains consumed permanently for that
  durable request;
- `BrokerRejected`: authenticated broker rejection with stable classified
  response and no evidence of acceptance;
- `AcceptedObserved`: accepted broker order identity is durably recorded;
- `AmbiguousAfterPossibleSend`: timeout, disconnect, 5xx, malformed/truncated
  body, response loss or accepted response without usable broker order ID;
- `ReconciliationRequired`: durable state entered for every ambiguous result.

PLACE classification is endpoint-specific:

| PLACE response | Required result |
| --- | --- |
| decoded 200 with exact usable broker identity and correlation | `AcceptedObserved` |
| safely decoded documented 400 invalid trading parameters | `BrokerRejected` |
| malformed or contradictory 400 | `ReconciliationRequired` |
| 401 | disarm and authentication/readiness block; no blind retry |
| documented 404 account/instrument not found | configuration/instrument block; no blind retry |
| 429, 500, 503, 504 or default | `ReconciliationRequired` |
| malformed, truncated or unknown 2xx, or 2xx without usable broker order identity | `ReconciliationRequired` |

CANCEL classification is separately endpoint-specific:

| CANCEL response | Required result |
| --- | --- |
| decoded 200 with exact target correlation | accepted cancellation observation only; it does not prove flatness or no intervening fill |
| documented 400 already executed | `ReconciliationRequired` |
| documented 401 expired/invalid authentication | disarm and authentication/readiness block; target order remains unresolved; `ReconciliationRequired` hold until fresh read-only broker truth; no blind or same-request CANCEL retry |
| documented 404 account/order not found | `ReconciliationRequired` |
| undocumented 409 or 410 | defensive `ReconciliationRequired` |
| 429, 500, 503, 504 or default | `ReconciliationRequired` |
| malformed or contradictory 2xx | `ReconciliationRequired` |

A generic `all 4xx -> BrokerRejected` classifier is forbidden. Endpoint
context always overrides HTTP status class.

No outcome after a possible send is automatically retried. HTTP method
idempotency assumptions, transport library retries and a repeated operator arm
must not bypass reconciliation.

Only a pre-send/local connect failure with proof that no bytes could leave may
be classified as `DefinitelyNotSent`; timeout alone is never that proof.

Once `DispatchAttemptRecorded` exists and a `Stage8ExecutionCapability` enters
an operation that may send bytes, that durable request's execution allowance
is consumed permanently. `DefinitelyNotSent` may prove that the attempt caused
no broker effect, but it never permits a second send-capable capability, arm or
execution attempt for the same `StrategyRequestId` or durable request. Any
later execution requires a durable terminal/no-effect disposition for the old
request plus a NEW `StrategyRequestId`, NEW derived `ClientOrderId`, NEW
operator arm and NEW `Stage8ExecutionCapability`. Same-request retry remains
CLOSED unless a later separately accepted durable retry protocol opens it.

## 8. Reconciliation authority

Reconciliation is broker truth, not transport response interpretation. It must
query current FINAM read-only sources using account and instrument scope and
correlate, in order:

1. stable `ClientOrderId` or exact broker-native correlation;
2. broker order ID when already known;
3. account + instrument + side + quantity + bounded event time;
4. trades and resulting target-instrument position as supporting evidence.

Orders, trades and positions are normalized into the broker-neutral canonical
snapshot model before lifecycle decisions. Account-wide active orders are a
safety guard; target-instrument active orders are lifecycle truth. An empty
response, stale snapshot or missing position row is not proof of rejection or
flatness.

Reconciliation produces one of:

- exact accepted working order;
- exact partially/fully filled order and trades;
- exact terminal rejected/cancelled/expired order;
- conflict/multiple candidates;
- still unknown.

`ProvenNoMatch` is CLOSED and unconstructible throughout Stage 8A. Empty,
missing, stale or merely absent truth always remains `StillUnknown` or
`ReconciliationRequired`. Conflict and still-unknown states block new live
commands; multiple candidates mean conflict and no new live command.
Reconciliation never redispatches an old ambiguous request; any new
send requires a new durable request, a fresh capability and a new operator arm.

## 9. Max-one micro authority

Stage 8B may later authorize at most one real engineering command. The budget
is durable, account/strategy scoped and consumed before possible send. PLACE
followed by CANCEL is not implicitly two free commands: a LimitCancel exercise
must be an explicitly reviewed two-action scenario with sequential identities,
or remain out of scope for the first one-command micro.

The accepted Stage 7A/7B invariant remains stronger at runtime: no second
non-final lifecycle for one strategy. A source-correlated CANCEL is considered
only after the PLACE request has a final command disposition and exposes an
exact working broker order target.

## 10. Allowlist and limits

All entries are deny-by-default and exact-match:

- one broker account;
- one canonical instrument and exact FINAM venue symbol;
- one strategy ID and owner;
- one command kind and side for a micro run;
- maximum absolute quantity;
- minimum/maximum LIMIT price;
- maximum notional;
- maximum reference-price age and allowed slippage for MARKET;
- trading-session/schedule eligibility;
- maximum one unresolved pending lifecycle, with zero required before entry.

Quantity and price use canonical decimal/integer-lot representations and
current broker asset parameters. Overflow, precision loss or missing contract
multiplier fails closed.

## 11. Kill switch

The persistent kill-switch mechanism must be available, fresh and readable,
and its state must be exactly `RunAllowed` before PLACE. `StopRequested`, stale
state, unreadable state or a generation conflict blocks PLACE fail-closed. It
is checked:

1. before capability issuance;
2. after durable attempt recording;
3. immediately before transport;
4. before any later continuation.

Emergency CANCEL policy must be independently scoped to an exact owned working
order and cannot become a general execution bypass. Stage 8B also requires the
same kill-switch mechanism and a separately accepted run contract.

## 12. Fault matrix

| Fault | Required result |
| --- | --- |
| timeout/disconnect before known send | no automatic retry; classify conservatively unless transport proves no bytes left |
| timeout/disconnect after possible send | durable reconciliation required |
| PLACE decoded documented 400 | broker rejection only when safely decoded |
| PLACE 401/404 | disarm/auth or configuration block; no blind retry |
| CANCEL 400/404/409/410 | reconciliation required |
| HTTP 429/5xx | ambiguous after possible send; no blind retry |
| malformed/truncated 2xx | reconciliation required |
| 2xx without broker order ID | reconciliation required |
| response accepted then lost | recover only from fresh broker truth |
| duplicate invocation after restart | same durable identity; no second send |
| multiple broker matches | conflict, kill/disarm, manual intervention |
| stale/empty broker truth | remains unknown, no proof of flat/rejected |
| journal/seal failure | no capability or transport |
| Redis settlement failure after broker outcome | preserve durable broker outcome; retry settlement only, never broker send |
| operator arm expires during attempt | no continuation; ambiguous send still reconciled |
| kill switch activates before transport | no send |
| kill switch activates after possible send | reconcile; do not infer no order |

Stage 8A tests must inject every row before real transport can be proposed.

## 13. Redis and durability inheritance

Stage 8 receives a command only through the accepted Stage 7B owner. It may not
XACK before a durable terminal lifecycle and committed recovery seal. Broker
response loss, Redis response loss and process crash must preserve the same
Stage 6 identity and prevent a second effect.

Redis markers remain settlement/publication state and never become broker
execution authority. A transport adapter cannot allocate an alternative
journal, request ID, order lifecycle or retry queue.

## 14. Single-broker ownership

ALOR and FINAM must not both have live execution authority for the same
strategy/account/instrument scope. Shadow market data and read-only comparison
are permitted, but only one broker ownership lease may enable order emission.
Missing, stale or conflicting ownership evidence blocks execution.

## 15. Evidence required before Stage 8B

The Stage 8A handoff must provide exact source/archive binding and independently
reproducible evidence for:

- opaque-capability construction and compile-fail privacy tests;
- one-shot consumption and restart non-reconstructibility;
- durable attempt-before-transport ordering;
- exact MARKET/LIMIT/CANCEL mapping goldens;
- stable client-order-ID reuse during reconciliation only;
- complete fault matrix in debug and release;
- response-loss/restart simulations with zero duplicate sends;
- account/instrument/strategy allowlist negatives;
- quantity/price/notional/session and kill-switch negatives;
- fresh broker-truth lookup/reducer fixtures;
- Stage 7B inherited full gate;
- source scan proving no autonomous runtime attachment or native protective
  commands;
- explicit proof that real endpoint calls remain disabled in Stage 8A.

Before any Stage 8B real micro, a second independently reviewed package must
add real read-only preflight evidence, exact operator authorization, endpoint
and build hashes, one-command blast radius, rollback/recovery runbook and
post-run broker reconciliation evidence requirements.

## 16. Gate acceptance rule

This specification is accepted only when all 69 mandatory rows in
`GATE7_TO_8_R3_ACCEPTANCE_MATRIX_2026-08-14.csv` pass, all exact 36 negative
mutations are rejected, the current FINAM contract evidence remains hash-bound,
and an independent reviewer records acceptance against an exact commit and
immutable archive.

The historical Stage 5 `forbidden_surface_scan.sh` is not rebaselined here and
cannot be the sole Stage 8 authority. Gate R3 instead proves zero production,
Cargo and `.github` delta from reviewed R2 `5da0db6`. Stage 8A-0 must define a
new Stage 8-specific closed-surface scanner before later implementation slices.

Before independent R3 acceptance every Stage 8 slice is closed. After
independent R3 acceptance the explicit transition is:

```text
8A-0 contract refresh/freeze  OPEN for docs/evidence/checkers only
8A-1                          CLOSED pending 8A-0 acceptance
8A-2                          CLOSED pending 8A-1 acceptance
8A-3                          CLOSED pending 8A-2 acceptance
8A-4                          CLOSED pending 8A-3 acceptance
8A-5                          CLOSED pending 8A-4 acceptance
Stage 8 production Rust       CLOSED
FINAM POST/DELETE             CLOSED
broker dispatch               CLOSED
runtime-live                  CLOSED
real strategy orders          CLOSED
native protective orders      CLOSED
Stage 8B                      CLOSED
```
