# Stage 8A-3 R1 endpoint-classifier entry contract

Stage 8A-2 R1 is independently accepted and closed at
`16180ac4f8eab761b3b055c1f5515f62cd94bfb9`. Stage 8A-3 may add only a
deterministic endpoint-specific classifier over explicit local PLACE or CANCEL
context and synthetic HTTP observations.

The official PLACE and CANCEL Markdown documents were retrieved again on
2026-08-15. Their byte lengths and SHA-256 values exactly match the accepted
Stage 8A-0 snapshot, so no material contract drift blocks this slice.

The Stage 8A-3 classifier is distinct from the historical M3d2 classifier. It
must not call either historical `classify_order_endpoint_local_http_response*`
function and must not reuse its generic rejection, timeout, maintenance or
rate-limit decisions as Stage 8 authority.

The implementation rules are:

- endpoint context is always explicit;
- HTTP 200 is only an endpoint-specific accepted candidate after documented
  decoding and exact correlation;
- PLACE 400 remains reconciliation-required because no exact provenance-pinned
  broker error-body decoder exists;
- CANCEL 200 with an empty/malformed body remains reconciliation-required
  because the current official contract documents a material response body;
- all undocumented 2xx, transient, timeout, disconnect, response-loss and
  body-read failures require reconciliation;
- no retry authority, `DefinitelyNotSent`, `ProvenNoMatch`, flatness, transport,
  Redis, broker dispatch or runtime-live surface may be introduced;
- acceptance may open Stage 8A-4 only.

