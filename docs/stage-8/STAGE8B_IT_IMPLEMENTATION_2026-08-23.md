# Stage 8B-IT — adapter-qualified no-effect integration

Status: corrective R3 implementation candidate. Accepted predecessor:
`0af222f252cdc2b4c763c9e04935a5cb5f0c6d65` (Stage 8B-I R3).
The first IT candidate `e44053917a928aeb4bc8e3330a58a693edc31fd3`
and R2 candidate `74d07c842f0ef3a02c4c30a919542a108304b52e`
were not accepted and are retained only as review lineage.
The accepted Stage 8B-I R3 checker is replayed immutably at `0af222f`; it is not
weakened or rebound to the controlled Stage 8A-2 successor source introduced by
IT R3. The current successor is independently digest-pinned by the IT checker.

## Purpose and phase boundary

Stage 8B-IT implements and qualifies one exact FINAM order adapter without a
broker effect. The immutable phase order remains
`8B-D → 8B-S → 8B-I → 8B-IT → 8B-P → 8B-XE`.
Acceptance of IT may open only exact adapter-qualified build authorization in
8B-P. It does not issue a production operator arm and cannot send to FINAM.

## Permit-only request composition

The adapter receives only `Stage8bApprovedRequestParts`, a non-Debug,
non-Serialize, non-Clone opaque type defined in the sibling
`stage8b_permit_capsule` module and created by the sole bridge
`compose_stage8b_private_request_parts_from_stage8a2`. The bridge consumes
`Stage8bExactTransportPermit`. K4 privately mints an opaque
`Stage8bA2PermitProof` bound to the exact attempt, covering seal, durable
request and continuation. The Stage 8A-2 extraction seam itself requires and
consumes that proof through `consume_stage8a2_request_capsule`; continuation
and sink alone cannot extract a raw request capsule. The shared Stage 8A-2
`compose_once` path invokes exactly one accepted `build_place_order_request` or
`build_cancel_order_request`, records the existing no-send diagnostic and
returns an opaque one-use capsule. There is no borrow/clone extraction seam.
The Stage 8A-3 source remains byte-identical to its frozen hash; the additive
Stage 8A-2 successor source is separately hash-bound by the R3 authority.

The adapter and permit capsule are sibling children of `stage8b_no_send`.
Proof and request-part fields are private to the permit-capsule sibling, so the
adapter cannot construct either type even though it can consume the opaque
input. No other sibling or public facade can construct the permit, parts, adapter,
qualification endpoint or qualification token. There is no raw route, body,
token, response, request-builder or retry getter.

## Single adapter and network policy

`crates/finam-gateway/src/stage8b_no_send/stage8b_adapter.rs` owns the single IT transport
surface. It has exactly one fixed `.post`, one fixed `.delete` and one common
`.send` call. It uses `reqwest::Client::builder()` with:

- `redirect(Policy::none())`;
- explicit `retry(reqwest::retry::never())`;
- `.no_proxy()`;
- fixed two-second connect timeout;
- fixed three-second request timeout;
- no idle connection reuse;
- no automatic protocol-NACK retry or application resend loop.

The frozen production policy accepts only TLS `https://api.finam.ru` on the
default/443 port, with no credentials, query, fragment or path prefix. IT has
no constructor for a production endpoint authority. Its only constructible
endpoint is an explicit numeric loopback IP with an explicit port, no path,
query, credentials or fragment. Therefore all IT writes terminate at a local
controlled non-broker process.

PLACE is exactly POST `PlaceOrderV1` and CANCEL is exactly DELETE
`CancelOrderV1`. Paths come only from the accepted FINAM specs. Empty, dot,
dot-dot and slash-containing path segments fail before write. Headers are
fixed by the adapter; there is no arbitrary URL/method/header interface.

## Observation and fault semantics

The adapter is consumed by one attempt. Its response body is capped at 64 KiB.
Connection failure, timeout, disconnect/response loss, malformed/oversized
body, redirect, 429 and 5xx never create retry authority. Once `.send` is
entered, the diagnostic conservatively records a possible write and exactly
one attempt.

Raw context and transport observation are private to the nested adapter. Every
success, error, timeout, disconnect and redirect path is consumed by the sole
`classify_raw_observation` transition, which invokes the accepted classifier
seam `classify_stage8b_transport_observation_with_stage8a3` before returning.
Only `Stage8a3ClassifiedObservation` plus redacted adapter diagnostics may
escape to the parent privacy module. Classifier output is
candidate/diagnostic evidence only. It is not broker truth and cannot close an
effect; later possible effects still require accepted Stage 8A-4 fresh broker
truth and durable closure.

Controlled tests cover PLACE, CANCEL, redirect-not-followed, response loss,
timeout and connection failure. Captured local wire evidence proves the exact
method and route. No test resolves, connects to or writes to `api.finam.ru`.

Same-crate compile-fail probes prove that sibling modules cannot name request
parts, access the nested adapter, export raw observations, extract without K4
proof, or fabricate the proof/request-parts from inside the adapter. The exact IT gate also runs the canonical full workspace
debug/release/doctest/clippy and isolated Redis/dry-bridge regression suite; its
complete log is source-ref-bound inside the handoff.

## Adapter-qualified identity

The future 8B-P package must bind the exact independently accepted IT source,
archive, Cargo manifests/lock, resolved feature/dependency graphs, toolchain,
config/policy, instrument, API snapshot, endpoint renderer, body schema and
executable SHA-256. Any drift requires relevant IT requalification before a
new P package. P cannot precede IT and cannot refresh automatically.

Controlled TLS handshake qualification for the exact adapter build remains a
blocking Stage 8B-P precondition. IT-R3 does not weaken certificate validation
and does not introduce a production endpoint constructor.

## Closed surfaces

Stage 8B-IT keeps closed production endpoint authority, production operator-arm
issuance, FINAM POST/DELETE effect, broker network send, Redis live consumer,
XADD/XACK, ACK/readiness publication, broker dispatch, runtime-live, real
strategy orders, MARKET/Stop/SLTP/bracket/replace/multi-leg, Stage 8B-P,
Stage 8B-XE and Stage 12.
