# Stage 6 durable identity and journal contract

Status: transition-gate design; no Stage 6 implementation is authorized yet.

## Canonical identity chain

The core chain is broker-neutral and lossless:

```text
StrategyRequestId
  -> ClientOrderId (stable idempotency identity)
  -> zero or one BrokerOrderId per accepted order placement
  -> zero or many BrokerTradeId observations
```

Every durable request record must also bind `BrokerAccountId`, `InstrumentId`,
strategy ID, owner, cycle ID, role, action and an explicit causal/creation
sequence. Broker order and trade IDs remain opaque non-empty strings. Numeric
surrogates and FINAM-specific IDs are forbidden as core identity authorities.

One strategy request has one stable `ClientOrderId`. A retry caused by an
unknown transport result keeps the same request and idempotency identity and is
blocked from blind redispatch until broker-truth reconciliation. A changed
business action, quantity, side, role, target order or cycle requires a new
`StrategyRequestId` and therefore a new `ClientOrderId`. Cancel is its own
strategy request and causally references the immutable target `BrokerOrderId`.

## Minimum journal record

A future versioned record must contain:

```text
schema_version, journal_record_id, lifecycle_sequence, causal_parent_id,
strategy_request_id, client_order_id, broker_order_id?, broker_trade_ids[],
account_id, instrument_id, strategy_id, owner, cycle_id, role, action,
event_kind, canonical_payload_sha256, source_evidence_sha256
```

The record model is append-only (or equivalently auditable), canonically
serialized and replayed in monotonic lifecycle sequence. Exact replay is
idempotent; a conflicting replay under the same record/request/idempotency
identity fails closed. History cannot be rewritten in place. Wall-clock time
may be diagnostic metadata but never ordering authority. A broker ID cannot be
created before authenticated broker evidence supplies it.

## Restart source of truth

After restart, authority is the authenticated Stage 5D runtime snapshot plus
the validated append-only journal suffix. The journal projection may advance
only from its authenticated causal parent. Fresh complete broker truth resolves
ambiguous dispatch/acceptance windows through the accepted Stage 5G
reconciliation rules; absence in incomplete broker truth never proves absence.
No second restart authority or independent runtime persistence model is
permitted.
