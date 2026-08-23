# Stage 8B-IT-TLS — controlled rustls qualification

Status: R1 implementation candidate. Accepted predecessor and current `main` at
slice entry: `14e01a9f838080e196ece5945a7796f2bd2600bc` (Stage 8B-IT R3).

## Scope

This slice qualifies the exact Stage 8B adapter semantics through a controlled
local TLS endpoint. It is a test-only qualification, not a production endpoint
or an execution authorization. `Stage 8B-P`, `Stage 8B-XE`, FINAM credentials,
broker effects, Redis execution, dispatch and runtime-live remain closed.

The accepted R3 adapter has one additive test-only successor seam. Both normal
and TLS qualification clients are built through the same `exact_client_builder`
that freezes `retry::never`, redirect denial, no proxy, fixed timeouts and no
idle connection reuse. The TLS-only constructor consumes an opaque local
authority, disables built-in roots, installs one ephemeral local CA and resolves
the fixed reserved name `stage8b-it.invalid` to one explicit loopback socket.
No production endpoint constructor exists.

## Pinned client and backend

- reqwest `0.12.24`, default features disabled;
- hyper-rustls `0.27.9`;
- rustls `0.23.41` with the `ring` provider;
- tokio-rustls `0.26.4` for the test-only server;
- no native-tls package in the production graph;
- no dangerous certificate or hostname verifier API;
- test CA generation uses rcgen `0.14.7` only as a dev dependency.

## Controlled qualification matrix

The test server binds `127.0.0.1:0`, presents a certificate signed by an
ephemeral local CA, and advertises only ALPN `h2`. The exact reqwest client uses
the reserved DNS name for certificate/SNI validation while an explicit resolver
maps it to that loopback socket.

Positive cases prove:

1. valid CA + exact hostname negotiates HTTP/2;
2. PLACE arrives as the exact POST route and accepted JSON body;
3. CANCEL arrives as the exact DELETE route with no body;
4. the synthetic authorization header is present without exporting its value;
5. the response is classified through the accepted Stage 8A-3 seam.

Negative cases prove that wrong CA, wrong SAN, expired and not-yet-valid
certificates never produce an HTTP request. Post-handshake response loss and
timeout remain `Disconnect`/`Timeout` classified observations with one transport
attempt and no retry. Non-loopback resolution and malformed root certificates
are rejected before client construction.

## Drift and promotion rule

The authority freezes the accepted predecessor source hash, TLS successor
source/harness hashes, Cargo manifests/lock and normalized production and
qualification dependency graphs. Any material source, dependency, feature,
policy, route, body, classifier or binary drift requires requalification.

Acceptance of this slice does not automatically open Stage 8B-P. P still
requires reviewed change control, a fresh official FINAM contract snapshot,
immutable action/toolchain/build identities and proof that this exact
TLS-qualified adapter build has not drifted. P remains GET-only/read-only design
and still cannot authorize a POST/DELETE effect.
