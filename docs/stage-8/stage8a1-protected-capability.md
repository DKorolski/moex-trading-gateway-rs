# Stage 8A-1 R1 — protected execution capability and authority provenance

Status: implementation candidate; independent acceptance pending.

Accepted predecessor:
`c949d7f83aa87cf990204a5b8ae66e5ca37c9f1d` (Stage 8A-0 R1).

R0 base candidate:
`29b868621361c5beff16e3008864e66d2efdafef`.

Independent R0 review SHA-256:
`a36c9cc254c0ff7ca22c4b1c89484a20adb359f27e0b5db607668282d6dcf82d`.

## Purpose

R1 keeps Stage 8A-1 strictly no-send and fixes the R0 authority defect: public,
caller-constructible facts can no longer authorize capability minting. The
capability and every proof used at the minting boundary are opaque, linear and
non-serializable. No request extraction, FINAM builder, transport, Redis
consumer or runtime-live path is introduced.

## Durable authority chain

The exact command is first authorized by the current recovered Stage 6 durable
journal. That authority proves an exact `RequestAccepted` record, durable
identity, canonical command digest and authenticated checkpoint. The current
Stage 7B recovery-ready owner then binds it to the active operational identity,
recovery-seal generation and seal commitment.

`Stage8a1DurableRequestAuthority::from_stage7b_owner()` is the only Stage 8
bridge. A raw `PlaceOrder` or `CancelOrder` cannot create this authority. The
binding includes request/client IDs, account, instrument, strategy attribution,
action and the full durable command snapshot, including CANCEL target IDs.

## Opaque authorities

Capability issuance consumes these private-field authorities:

- exact-command operator arm with a unique nonce, command, policy,
  build/config/endpoint digests and bounded validity;
- frozen execution policy containing the complete broker-neutral arithmetic
  policy and fixed TTL/freshness limits;
- trusted clock and readiness;
- durable `RunAllowed` kill switch;
- exactly one FINAM broker owner in the same scope;
- zero unresolved order/delivery/reconciliation ambiguity;
- fresh account and target-instrument broker truth;
- eligible trading schedule;
- unused max-one engineering-micro budget.

All proofs bind the same scope derived from durable identity, operational
identity, recovery seal and frozen build/config/policy provenance. Their fields
are private, they have no public literal constructors and they cannot be
cloned. Production issuers for the non-durable proof authorities remain closed
for a later separately reviewed slice.

The operator arm and frozen policy bind the exact side, quantity, limit price
or market reference context, strategy attribution, notional/slippage/reference
guards and endpoint provenance. Consuming the arm by value makes a second mint
from the same authority impossible. Compile-fail tests cover private
construction, cloning and reuse after move.

## Existing arithmetic validator

The accepted `broker_core::OrderPreflightPolicy` remains the arithmetic
validator; Stage 8 does not introduce a second quantity/price/notional engine.
Its complete relevant surface is owned by and hashed into the opaque frozen
policy:

- exact account and venue allowlists;
- MARKET/LIMIT only and DAY only;
- min/step/max quantity and price step;
- max market quantity;
- non-optional per-order and per-run notional limits;
- limit-deviation and reference-age limits;
- no cancel-without-mapping bypass;
- one-shot broker-core arm.

## Capability and audit binding

`Stage8ExecutionCapability` has private fields and implements neither `Clone`,
`Copy`, `Debug`, `Serialize` nor `Deserialize`. Its redacted diagnostic exposes
only scope, expiry, recovery generation and an audit fingerprint. The
fingerprint binds the exact durable command, arm nonce, frozen policy, Stage 7B
seal and every evidence digest. No raw account, order or strategy identifiers
are exported.

The capability expires at the earliest authority expiry. It cannot survive a
restart and has no method that exposes a request or feeds a serializer or
transport.

## CANCEL policy

CANCEL additionally requires an exact durable `OrderPathRecord`. Account,
instrument, broker order ID and optional client order ID must match the durable
command and mapping. Cancel-by-broker-ID without mapping remains forbidden. An
already-terminal record returns `AlreadyTerminal` and never mints a capability.

## Acceptance evidence

- exact 58-row R1 acceptance matrix;
- exact 52-case fail-closed negative inventory and executable harness;
- focused authority, mutation, PLACE and CANCEL tests;
- compile-fail capability/arm/proof construction and linearity tests;
- exact SHA-pinned Rust surface, including all touched `lib.rs` and authority
  bridge files;
- serialized workspace tests/doctests, fmt, clippy and inherited scanners;
- source-manifest and SHA-bound immutable handoff evidence.

## Deliberately closed

- `broker_finam::build_place_order_request()` composition;
- `broker_finam::build_cancel_order_request()` composition;
- request/body extraction and mock send seams;
- HTTP outcome classification and broker reconciliation implementation;
- FINAM POST/DELETE;
- Redis live command consumption and broker dispatch;
- runtime-live, real strategy orders and Stage 8B;
- Stage 8A-2 through Stage 8A-5.

Only independent acceptance of this exact R1 slice may open Stage 8A-2.
