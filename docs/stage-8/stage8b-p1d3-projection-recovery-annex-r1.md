# Stage 8B-P1-d3 projection and pre-seal recovery annex R1

Status: R1 design-only review candidate. This annex corrects the three P1 and
one P2 findings against R0 `74696d1eefc0453c41440f79b087cafebd0d7ab0`.
It authorizes no production source or operational activation.

Accepted immutable predecessor:
`bcd8db546104968dd0e48ab041e02acf6869d224`.

Normative domains:

```text
moex.stage8b.p1d3.outcome-evidence.v1
moex.stage8b.p1d3.working-book.v1
moex.stage8b.p1d3.book-genesis.v1
moex.stage8b.p1d3.book-transition.v1
```

## Exact symbols and clocks

The tables use these exact values:

```text
RID_P   = place StrategyRequestId
RID_C   = cancel StrategyRequestId
DCID_P  = place Stage6 durable request-level ClientOrderId
DCID_C  = cancel Stage6 durable request-level ClientOrderId
TCID    = target order's original place ClientOrderId
BOID    = deterministic BrokerOrderId of the LIMIT
BTID    = deterministic BrokerTradeId of its only possible fill
Q       = exact positive integral command quantity bytes
L       = exact positive limit-price Decimal bytes
Z       = Decimal::ZERO with positive sign and scale zero
F       = exact deterministic fill-price Decimal bytes
```

For Buy, `F` is the open bytes when `open < L`, otherwise the limit bytes. For
Sell, `F` is the open bytes when `open > L`, otherwise the limit bytes. Numeric
ties deliberately choose the exact limit bytes. No arithmetic rescaling is
permitted for `Q`, `L` or `F`.

Every new status observation has one transition clock:

```text
T_CANDIDATE = exact schedule-approved final candidate M10 close_ts
T_BOUNDARY  = exact consumed Day-expiry authority boundary timestamp
```

| Shape | `T_transition` | Stage7 `observed_at` |
|---|---|---|
| initial Working | `T_CANDIDATE` of first candidate | same value |
| initial Filled | `T_CANDIDATE` of first candidate | same value |
| initial Expired | `T_BOUNDARY` | same value |
| later Filled | later `T_CANDIDATE` | no new request finalization |
| later autonomous Expired | `T_BOUNDARY` | no new request finalization |
| CancelCanceled | cancel `T_CANDIDATE` after target evaluation | same value |
| CancelExecutionObserved | cancel `T_CANDIDATE`; never old target clock | same value |
| CancelAlreadyTerminalNonExecution | cancel `T_CANDIDATE`; never old target clock | same value |

For every newly materialized order/trade/position row,
`T_source = T_receipt = T_transition`. `BrokerTruthSnapshot.received_ts` and
`CommandAck.received_ts` are also exactly `T_transition`. These clocks come
only from consumed schedule evidence. `Utc::now()`, process time, Redis read
time, the command creation clock and an older target terminal clock are
forbidden projection authorities.

## Exact `CommandAck` fields

Later autonomous Filled/Expired transitions emit no ACK. Request outcomes emit
exactly one ACK:

| Field | Initial Working/Filled/Expired | CancelCanceled | CancelExecutionObserved / CancelAlreadyTerminalNonExecution |
|---|---|---|---|
| `request_id` | `RID_P` | `RID_C` | `RID_C` |
| `client_order_id` | `Some(DCID_P)` | `Some(DCID_C)` | `Some(DCID_C)` |
| `broker_order_id` | `Some(BOID)` | `Some(BOID)` | `Some(BOID)` |
| `status` | `CommandAckStatus::Accepted` | `CommandAckStatus::Accepted` | `CommandAckStatus::Recovered` |
| `reason` | `None` | `None` | `Some(CommandAckReason { code: RecoveredByBrokerTruth })` |
| `received_ts` | exact shape `T_transition` | exact shape `T_transition` | exact cancel `T_transition` |

`DCID_C` is `ClientOrderId::from_strategy_request(RID_C)` retained by the
Stage6 cancel identity. It is distinct from optional `TCID` in the cancel
command. `TCID` identifies the target and never occupies the cancel ACK slot.

## Exact `BrokerOrderSnapshot` fields

Initial and later rows share the target's exact account, instrument and
original place identities. Recovered cancel outcomes emit no new order row;
their already-terminal book row remains byte-identical.

| Field | Working | Filled (initial or later) | Expired (initial or later) | CancelCanceled | Recovered cancel outcomes |
|---|---|---|---|---|---|
| `account_id` | exact place account as `BrokerAccountId` | same | same | same | no new row |
| `broker_order_id` | `Some(BOID)` | `Some(BOID)` | `Some(BOID)` | `Some(BOID)` | no new row |
| `client_order_id` | `Some(DCID_P)` | `Some(DCID_P)` | `Some(DCID_P)` | `Some(DCID_P)` | no new row |
| `instrument` | exact place `InstrumentId` | same | same | same | no new row |
| `side` | exact place side | same | same | same | no new row |
| `order_type` | `OrderType::Limit` | `OrderType::Limit` | `OrderType::Limit` | `OrderType::Limit` | no new row |
| `time_in_force` | `Some(TimeInForce::Day)` | same | same | same | no new row |
| `status` | `OrderStatus::Working` | `OrderStatus::Filled` | `OrderStatus::Expired` | `OrderStatus::Canceled` | no new row |
| `lifecycle` | `BrokerOrderLifecycle::Active` | `BrokerOrderLifecycle::Terminal` | terminal | terminal | no new row |
| `qty` | exact `Q` | exact `Q` | exact `Q` | exact `Q` | no new row |
| `filled_qty` | exact `Z` | exact `Q` | exact `Z` | exact `Z` | no new row |
| `remaining_qty` | `Some(Q)` | `Some(Z)` | `Some(Q)` | `Some(Q)` | no new row |
| `limit_price` | `Some(L)` | `Some(L)` | `Some(L)` | `Some(L)` | no new row |
| `broker_asset_id` | `None` | `None` | `None` | `None` | no new row |
| `board` | `None` | `None` | `None` | `None` | no new row |
| `expiration_date` | `None` | `None` | `None` | `None` | no new row |
| `source_ts` | `Some(T_transition)` | `Some(T_transition)` | `Some(T_transition)` | `Some(T_transition)` | no new row |
| `received_ts` | `T_transition` | `T_transition` | `T_transition` | `T_transition` | no new row |

Because v1 has no partial fill, a Working target canceled by CANCEL has
`filled_qty = Z` and `remaining_qty = Some(Q)`. CANCEL never changes the row's
original place client ID, side, quantity, limit or metadata.

## Exact fill trade and resulting position

Only initial Filled and later Filled emit a trade and position. Their tables
are identical except for the exact candidate `T_transition` and pre-position.

| `BrokerTradeSnapshot` field | Exact value |
|---|---|
| `account_id` | exact place account as `BrokerAccountId` |
| `broker_trade_id` | exact `BTID` |
| `broker_order_id` | `Some(BOID)` |
| `client_order_id` | `Some(DCID_P)` |
| `instrument` | exact place `InstrumentId` |
| `side` | exact place side |
| `qty` | exact `Q` |
| `price` | exact `F` |
| `gross_amount` | `None` |
| `commission` | `Some(Z)` |
| `broker_asset_id` | `None` |
| `board` | `None` |
| `expiration_date` | `None` |
| `source_ts` | `T_transition` |
| `received_ts` | `T_transition` |

| `BrokerPositionSnapshot` field | Exact value |
|---|---|
| `account_id` | exact place account as `BrokerAccountId` |
| `instrument` | exact place `InstrumentId` |
| `qty` | exact checked `q1 = q0 + signed(Q)` |
| `avg_price` | P1-d2 scale-8 `MidpointNearestEven` result; `None` iff `q1 == 0` |
| `unrealized_pnl` | `None` |
| `source_ts` | `Some(T_transition)` |
| `received_ts` | `T_transition` |

The accepted P1-d2 prior-position presence, arithmetic, checked overflow,
explicit flat row and average-price byte rules are inherited without change.
No Working, Expired, Canceled or recovered-cancel outcome emits a position.

## Exact event-scoped truth shapes

Every emitted truth is event-scoped and contains no unrelated account data.
Vector order below is canonical; the existing Stage5G canonicalizer must
produce the same bytes.

| Shape | `orders` | `positions` | `cash` | `trades` | `instruments` | `received_ts` |
|---|---|---|---|---|---|---|
| initial Working | `[Working order]` | `[]` | `None` | `[]` | `[]` | `T_transition` |
| initial Filled | `[Filled order]` | `[result position]` | `None` | `[fill trade]` | `[]` | `T_transition` |
| initial Expired | `[Expired order]` | `[]` | `None` | `[]` | `[]` | `T_transition` |
| later Filled | `[Filled order]` | `[result position]` | `None` | `[fill trade]` | `[]` | `T_transition` |
| later autonomous Expired | `[Expired order]` | `[]` | `None` | `[]` | `[]` | `T_transition` |
| CancelCanceled | `[Canceled order]` | `[]` | `None` | `[]` | `[]` | `T_transition` |
| CancelExecutionObserved | no new `BrokerTruthSnapshot` | n/a | n/a | n/a | n/a | n/a |
| CancelAlreadyTerminalNonExecution | no new `BrokerTruthSnapshot` | n/a | n/a | n/a | n/a | n/a |

No row may default `None` to empty text, invent `broker_asset_id`/`board`, infer
`expiration_date`, derive `gross_amount`, omit explicit commission zero, add a
cash/instrument row or encode a numerically equal zero with alternate Decimal
bytes.

## Exact sequence and replacement phases

```text
initial Working/Filled/Expired:
  seq_truth = checked_add(seq_ack, 1)

CancelCanceled:
  seq_truth = checked_add(seq_ack, 1)

later autonomous Filled/Expired:
  one exact next seq_truth; no ACK sequence

CancelExecutionObserved / CancelAlreadyTerminalNonExecution:
  one exact seq_ack; no truth sequence
```

For initial and CancelCanceled paths, `S_ack` contains the ACK and the reserved
truth continuation; `S_working` or `S_terminal` contains the truth and book.
For recovered cancel paths, `S_ack` is not terminal: the required post-ACK seal
is `S_cancel_recovered`, containing the updated Stage5G ACK package and the
byte-identical already-terminal book.

```text
before S_cancel_recovered persisted+reread:
  byte-identical recovered ACK replay is the only continuation

after S_cancel_recovered persisted+reread:
  exact source XACK is the only continuation
```

Immediate XACK after an in-memory recovered ACK is forbidden. Recovered ACK
replay after the seal and duplicate target truth before or after the seal are
forbidden. The seal is fsynced, reread, MAC-validated and cross-bound to exact
Stage6/7 records before it grants source-XACK authority.

## `Stage8bP1d3OutcomeEvidenceV1`

Every transition caused by a consumed schedule capability is converted into a
full versioned write-ahead fact before the capability is dropped. The complete
fact, not only its digest, is embedded in the same authenticated Stage6 journal
record as its `Stage6dPaperOutcome` or autonomous order transition.

The fixed-order wire struct contains exactly:

1. `schema_version = 1` and domain;
2. outcome kind and transition ordinal;
3. operational identity, package generation, account, instrument and
   attribution fingerprints;
4. request ID, durable request client ID, canonical command hash, accepted
   command payload hash and optional target place client ID/order ID;
5. source M10 Redis ID/semantic hash/payload hash/open/close clocks;
6. candidate M10 Redis ID/semantic hash/payload hash/open/close clocks, or
   explicit `None` fields for a boundary-only expiry;
7. consumed witness kind, schedule fingerprint, trading-day identity, last
   eligible M10 identity and exact boundary timestamp;
8. pre-transition book generation/hash and expected post-transition book hash;
9. exact BOID, optional BTID, side, `Q.serialize()`, `L.serialize()`, optional
   `F.serialize()`, and all exact transition clocks;
10. optional reserved `seq_ack` and `seq_truth` with their allocation frontier;
11. Stage6 dispatch/outcome record IDs, Stage6 frontier/checkpoint hashes and
    exact Stage7 `RequestFinalized` record ID/fingerprint when request-scoped;
12. previous outcome-evidence hash. The current record digest is stored by the
    Stage6 envelope alongside, not recursively inside, these wire bytes.

The wire representation is `serde_json::to_vec` of one declared Rust struct in
the order above. It contains no maps. Every Decimal is a `[u8; 16]` returned by
`Decimal::serialize()`, every optional field is explicitly serialized as a
value or JSON `null`, and every timestamp is signed UTC epoch milliseconds.
Unknown or missing fields and alternate ordering fail schema validation.

The record digest is:

```text
SHA256(
  b"moex.stage8b.p1d3.outcome-evidence.v1\0" ||
  u64_be(canonical_wire_len) || canonical_wire_bytes
)
```

The Stage6 journal append and outcome become durable atomically under the
existing journal owner. A digest without the full authenticated bytes cannot
mint recovery. Request-scoped evidence is appended before Stage7 finalization;
autonomous later-fill/expiry evidence is appended before Stage5G truth.

After crash before the first replacement seal, a crate-private recovery-only
constructor rereads this exact record, authenticates the Stage6 checkpoint and
Stage7 finalization where applicable, recomputes all IDs/projections/hashes and
returns a linear continuation. It has no provider, Hybrid callback, Redis
candidate selection, schedule-capability issuer, dispatch or wall-clock API.
It cannot select another currently valid boundary. Changed candidate bytes,
boundary, schedule fingerprint, old target state, book hash, sequence or
record identity hard-conflicts before ACK, truth, callback or XACK.

## Canonical book, transition hash and P1-d2 migration

The registry vector is sorted by bytewise UTF-8 bytes of exact
`BrokerOrderId::as_str()`. Within one generation each of these indexes is
unique: BOID, original `RID_P`, original `DCID_P`, and deterministic order
fingerprint. Sorting by insertion order, locale, hash-map iteration or request
time is forbidden.

The ordinal-zero previous-hash seed is:

```text
GENESIS = SHA256(
  b"moex.stage8b.p1d3.book-genesis.v1\0" ||
  raw32(accepted_p1d2_package_commitment_sha256) ||
  raw32(operational_identity_sha256) ||
  u64_be(package_generation)
)
```

For transition ordinal `n >= 1`:

```text
H_n = SHA256(
  b"moex.stage8b.p1d3.book-transition.v1\0" ||
  u64_be(n) || raw32(H_(n-1)) ||
  u64_be(outcome_evidence_len) || outcome_evidence_bytes ||
  u64_be(post_book_len) || post_book_canonical_bytes
)
```

The book wire encoding is fixed-order `serde_json::to_vec` with no maps and
Decimal `[u8; 16]` values. It binds the exact sorted registry, active BOID or
null, ordinal, previous hash, total-sequence frontier, operational identity and
generation.

The sole P1-d2 migration bridge consumes an authenticated accepted P1-d2
replacement package only when its request is terminal, source resolution is
complete, no P1 request is pending and no active order exists. It emits an
empty sorted registry, null active BOID, ordinal zero and exact `GENESIS` under
the same operational identity and next checked package generation. It allocates
no Stage5G sequence and performs no callback, provider call, Stage6 dispatch,
ACK, truth or Redis XACK. Migration replay is byte-identical; a second migration
or nonquiescent input hard-conflicts.

## Required source golden and negative tests

Source acceptance must include checked-in golden canonical bytes and SHA-256
for all eight shapes listed above, both fresh and recovery paths. It must also
prove two logically identical terminal registries built through different
insertion/restart paths serialize identically.

At minimum mutations must reject: wall-clock Working ACK, old-target clock for
recovered cancel, alternate zero scale, omitted metadata field, reordered
vectors, digest-only outcome evidence, changed expiry boundary, provider use in
recovery, insertion-order registry, duplicate index, changed genesis seed,
changed length prefix, nonquiescent migration, immediate XACK after in-memory
recovered ACK and duplicate target truth.

## Explicitly closed

- P1-d3 production source implementation before R1 design acceptance;
- P1-d4 exhaustive crash/replay closure;
- operational Redis DB0/VPS and deployable supervisor;
- FINAM POST/DELETE, broker dispatch, real orders and runtime-live;
- partial fills, nonzero fee/slippage and protective order completion.
