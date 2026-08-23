# Stage 8B-IT — adapter-qualified no-effect integration

Status: implementation candidate. Accepted predecessor:
`0af222f252cdc2b4c763c9e04935a5cb5f0c6d65` (Stage 8B-I R3).

## Purpose and phase boundary

Stage 8B-IT implements and qualifies one exact FINAM order adapter without a
broker effect. The immutable phase order remains
`8B-D → 8B-S → 8B-I → 8B-IT → 8B-P → 8B-XE`.
Acceptance of IT may open only exact adapter-qualified build authorization in
8B-P. It does not issue a production operator arm and cannot send to FINAM.

## Permit-only request composition

The adapter receives only `Stage8bApprovedRequestParts`, a crate-private,
non-Debug, non-Serialize, non-Clone type created by the sole bridge
`compose_stage8b_private_request_parts_from_stage8a2`. The bridge consumes
`Stage8bExactTransportPermit`, retains the accepted Stage 8A-2 no-send
diagnostic and invokes only `build_place_order_request` or
`build_cancel_order_request` for the exact approved command cloned from the
already cross-bound linear continuation. The accepted Stage 8A-2 and Stage
8A-3 source files remain byte-identical to their frozen hashes.

No public facade can construct the permit, private parts, adapter,
qualification endpoint or qualification token. There is no raw route, body,
token, response, request-builder or retry getter.

## Single adapter and network policy

`crates/finam-gateway/src/stage8b_adapter.rs` owns the single IT transport
surface. It has exactly one fixed `.post`, one fixed `.delete` and one common
`.send` call. It uses `reqwest::Client::builder()` with:

- `redirect(Policy::none())`;
- `.no_proxy()`;
- fixed two-second connect timeout;
- fixed three-second request timeout;
- no idle connection reuse;
- no automatic retry or resend loop.

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

Every local observation is passed through the accepted sole classifier seam
`classify_stage8b_transport_observation_with_stage8a3`. Classifier output is
candidate/diagnostic evidence only. It is not broker truth and cannot close an
effect; later possible effects still require accepted Stage 8A-4 fresh broker
truth and durable closure.

Controlled tests cover PLACE, CANCEL, redirect-not-followed, response loss,
timeout and connection failure. Captured local wire evidence proves the exact
method and route. No test resolves, connects to or writes to `api.finam.ru`.

## Adapter-qualified identity

The future 8B-P package must bind the exact independently accepted IT source,
archive, Cargo manifests/lock, resolved feature/dependency graphs, toolchain,
config/policy, instrument, API snapshot, endpoint renderer, body schema and
executable SHA-256. Any drift requires relevant IT requalification before a
new P package. P cannot precede IT and cannot refresh automatically.

## Closed surfaces

Stage 8B-IT keeps closed production endpoint authority, production operator-arm
issuance, FINAM POST/DELETE effect, broker network send, Redis live consumer,
XADD/XACK, ACK/readiness publication, broker dispatch, runtime-live, real
strategy orders, MARKET/Stop/SLTP/bracket/replace/multi-leg, Stage 8B-P,
Stage 8B-XE and Stage 12.
