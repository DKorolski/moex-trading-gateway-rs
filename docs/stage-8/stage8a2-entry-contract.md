# Stage 8A-2 entry contract — builder composition, no send

Status: authorized for implementation after independent Stage 8A-1 R3
acceptance. This document does not authorize Stage 8A-3 or any network send.

## Predecessor

Stage 8A-1 R3 is independently accepted and closed at
`1ff04154ba4b7a5ee060a73b853ce89bd7442f44`. Its immutable archive SHA-256 is
`02a332af7ccf1815a79de0f9a025be9d31094160f7977cd3e34de270497138a0`.

Stage 8A-2 must preserve the accepted authority chain and may consume only a
freshly revalidated opaque `Stage8a1CurrentlyAuthorizedCapability`. Possession
of an older minted capability without current-state revalidation is
insufficient.

The accepted predecessor remains immutable evidence. One narrowly reviewed
successor hook may be added inside the Stage 8A-1 privacy domain solely to
consume that continuation and match its already-owned approved command. This
successor delta does not alter the historical acceptance and must be separately
allowlisted, pinned and mutation-tested.

## Allowed implementation

- Compose the existing `broker_finam::build_place_order_request()` for
  broker-neutral MARKET + DAY and LIMIT + DAY commands.
- The only authorized PLACE invocation is
  `build_place_order_request(&approved, None)`; Stage 8A-2 has no independent
  outgoing-comment authority.
- Compose the existing `broker_finam::build_cancel_order_request()` for the
  exact durable CANCEL target mapping.
- Preserve the explicit durable `ClientOrderId` and exact command identity.
- Return an opaque internal request-shape witness to an in-memory or loopback
  mock/no-send sink.
- Expose only redacted diagnostics; raw request paths, headers and bodies must
  not cross the public boundary.
- Add golden builder-parity tests, malformed-input negatives, compile-fail
  opacity checks and semantic source mutations for the new boundary.
- Re-run inherited Stage 8A-1 source/hash/closed-surface checks from its exact
  accepted predecessor.

## Authorized consuming seam

Exactly one seam inside the existing Stage 8A-1 privacy domain may:

1. accept `Stage8a1CurrentlyAuthorizedCapability` by value;
2. consume its owned `Stage8ExecutionCapability` exactly once;
3. match only the private PLACE or CANCEL approved-command arm;
4. call only the corresponding accepted FINAM builder;
5. convert the raw builder result immediately into a private opaque witness;
6. move the witness into the deterministic in-memory no-send sink; and
7. return only a redacted diagnostic.

The seam must not expose an approved-command getter, `into_approved`, raw
request accessor or semantic equivalent. Neither `PreflightApproved*` nor
`Finam*RequestSpec` may cross the privacy boundary.

## Forbidden implementation

- FINAM POST or DELETE, including hidden or feature-gated production calls.
- Any non-loopback broker order endpoint or transport adapter.
- Automatic HTTP retry or an HTTP outcome classifier.
- Broker reconciliation or a constructible `ProvenNoMatch` result.
- Redis live command consumption, broker dispatch or runtime-live attachment.
- Real strategy orders, unattended execution, STOP, SLTP, bracket, replace or
  multi-leg behavior.
- A second FINAM serializer or builder that duplicates the accepted
  `broker_finam` builders.
- Any bypass, clone or reconstruction of the Stage 8A-1 continuation.
- Any non-`None` `OutgoingOrderComment` supplied by Stage 8A-2.
- Any import or invocation of `M3d2RealOrderEndpointTransport`,
  `EndpointGateApproved`, historical PLACE/CANCEL execution methods, or the
  `m3j16-actual-one-shot` feature from the Stage 8A-2 path.
- Raw request body, path segments, URL, account/order/client identifiers,
  headers, tokens or transport-ready request representation escaping the
  private witness.

## Required proof groups

1. Accepted-predecessor binding and exact changed-path inventory.
2. Linear continuation consumption with PLACE/CANCEL command binding.
3. MARKET + DAY, LIMIT + DAY and exact CANCEL golden parity.
4. Durable `ClientOrderId` preservation and malformed-input rejection.
5. Opaque request-shape witness and redacted diagnostics.
6. Semantic negative mutations for serializer duplication, capability bypass,
   path/body extraction, `None` to `Some(...)` comment drift, and both new and
   historical transport escape paths.
7. A Stage 8A-2-scoped closed-surface scanner; historical Stage 5 topology
   baselines must not be presented as current Stage 8 authority.
8. Debug/release workspace tests, doctests, clippy and formatting.
9. Immutable handoff with source-tree manifest, gate evidence and archive
   safety verification.

## Exit rule

Only an independent `ACCEPTED` verdict for the exact Stage 8A-2 handoff may
open Stage 8A-3. Acceptance of this entry contract alone opens no later slice.
