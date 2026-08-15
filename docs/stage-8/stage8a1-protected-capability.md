# Stage 8A-1 — protected execution capability and strict preflight

Status: implementation candidate; independent acceptance pending.

Accepted predecessor:
`c949d7f83aa87cf990204a5b8ae66e5ca37c9f1d` (Stage 8A-0 R1).

Independent predecessor review SHA-256:
`574876211e0c896cc9d61f9f2d078059e54fd471a9b97e94a3c9c8c81930879b`.

## Purpose

Stage 8A-1 adds only the no-send authority boundary required before request
builder composition. It does not build FINAM payloads and cannot reach a
network, Redis command consumer or runtime-live path.

The implementation is
`finam_gateway::stage8a1_execution_capability`. Its
`Stage8ExecutionCapability` has private fields and intentionally implements
neither `Clone`, `Copy`, `Debug`, `Serialize` nor `Deserialize`. No method in
this slice extracts the approved command or consumes the capability for an
endpoint operation.

## Existing authority reused

Stage 8A-1 does not create a second quantity/price/notional/slippage validator.
It calls the accepted broker-neutral `OrderPreflightPolicy` and requires its
policy surface to equal the Stage 8 allowlists and narrow policy:

- exact account set;
- exact venue-symbol set derived from the instrument allowlist;
- MARKET/LIMIT only;
- DAY only;
- one-shot, unattempted legacy arm bound to the same session, expiry and
  configuration fingerprint;
- existing quantity, step, price, reference freshness, notional and limit-band
  checks remain authoritative.

The existing FINAM request builders remain untouched and are not called.

## Capability prerequisites

One capability is bound to one command request, command kind, account,
instrument, strategy, restart generation and configuration fingerprint.
Issuance requires all of the following at the same preflight instant:

1. a consumed one-shot operator-arm input with exact request binding and a
   bounded TTL;
2. exact non-empty account, instrument and strategy allowlists without
   duplicates;
3. MARKET/LIMIT and DAY-only broker-neutral preflight;
4. durable kill-switch evidence whose state is exactly `RunAllowed`;
5. one and only one active broker owner, exactly `BrokerKind::Finam`;
6. zero unresolved orders, unknown-delivery observations and
   reconciliation-required items;
7. fresh evidence with valid lowercase SHA-256 bindings;
8. identical non-zero restart generation across arm and all evidence;
9. identical current configuration fingerprint across arm and all evidence.

The capability expiry is the earliest expiry among the operator arm,
kill-switch evidence, broker-ownership evidence and ambiguity evidence.
Capabilities are not serializable, so they cannot be restored across a
process restart. A later consuming slice must re-check current generation,
configuration and kill-switch state immediately before any builder use.

## CANCEL policy

CANCEL requires an existing durable `OrderPathRecord`. Account,
`broker_order_id`, optional `client_order_id` and instrument identity must match
the existing record. Cancel-by-broker-id without a mapping is forbidden.

An already-terminal record returns `AlreadyTerminal` and never issues a
capability.

## Deliberately closed

- `broker_finam::build_place_order_request()` composition;
- `broker_finam::build_cancel_order_request()` composition;
- any request/body extraction method;
- mock or real send seam;
- HTTP outcome classification;
- broker reconciliation;
- FINAM POST/DELETE;
- Redis live command consumption;
- broker dispatch, runtime-live and real strategy orders;
- Stage 8A-2 through Stage 8B.

Only independent acceptance of this exact slice may open Stage 8A-2.
