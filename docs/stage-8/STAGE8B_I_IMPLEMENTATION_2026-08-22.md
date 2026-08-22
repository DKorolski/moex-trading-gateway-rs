# Stage 8B-I R2 — corrective no-send type-state and deterministic rehearsal

Status: corrective implementation candidate; independent acceptance required.

The original I candidate `a52fbcae5340d632ce8b983eda6ecb4b8dedabce`
was not accepted. R2 closes only the six findings in the independent review
whose SHA-256 is
`3f7b04caa6b402ab96432560c5ef5f48c7a0e77bbbc87c466c85054f15216399`.
Accepted Stage 8B-S R3 is not reopened.

Stage 8B-S R3 was accepted at `afecc2584593570b62cbe7f00ee81f64d4b9b26b`
and merged history-preservingly by `d1581962666aa82b993854d0642e67bd66624032`.
This slice implements only the accepted I boundary. It adds no real adapter,
FINAM request, Redis execution authority, dispatch, runtime-live or real order.

## Boundary

The only public FINAM-gateway entry is
`invoke_stage8b_operator_once(Stage8bOperatorInvocationRequest)`. Broker CLI
reaches it through `invoke_stage8b_no_send_from_cli`. The request has private
fields and accepts only an opaque invocation ID plus two local evidence
references. The result is `Stage8bOperatorDiagnostic`: hashes, bounded counts
and explicit `no_send=true`, `authority_constructed=false`. It exports no raw
path, account, body, token, client, arm or authority.

The sole crate-private root remains `compose_stage8b_effect_authority`. It
consumes Stage 8A-1 current/durable authority and the linear Stage 8B evidence
types. It performs K2 binding only and never invokes a request builder. The
fresh continuation is moved through sealed-attempt and exact-permit types.
Only `compose_stage8b_private_request_parts_from_stage8a2` consumes the exact
permit and invokes the accepted existing-builder-only Stage 8A-2 sink; its
private witness is then consumed by one local no-network boundary. No serializer
or reusable permit is added.
`classify_stage8b_transport_observation_with_stage8a3` delegates only to the
accepted Stage 8A-3 Model A classifier and grants no broker truth.

## Privacy and local evidence

Account binding uses HMAC-SHA256 over exact bytes:

`ASCII("moex-stage8b-account-binding-v1") || 00 || u32be(len(account)) || account_utf8`.

The minimum key is 32 bytes, key/message buffers are `Zeroizing`, and verification
uses `Mac::verify_slice`. The accepted golden digest is
`60106309bd530bd0cec76c3fa78fa4b7004ef34e44447fb7cd78fdda87444435`.

Evidence paths must be absolute and symlink-free. Package files must be regular,
single-linked and bounded. They are opened with `O_NOFOLLOW`; path and descriptor
identity are checked before and after the read. Manifest root is pinned as a
directory descriptor and its fixed child is opened with `openat(O_NOFOLLOW)`.
Symlink, hardlink, child-symlink and deterministic post-open path-swap tests fail
closed.

## Durability and restart rehearsal

The internal arm rehearsal accepts only a typed canonical lowercase 32-byte
binding, uses `openat(O_CREAT|O_EXCL|O_NOFOLLOW)`, mode 0600, file `fsync` and
directory `fsync`, and stores an HMAC-authenticated record covering the complete
opaque durable/run/account/build/config/policy/endpoint/body/control/K2 binding
and expiry. A two-process test proves exactly one winner.
Issuance returns only an issued-record receipt, not an accepted arm capability.
Only authenticated record verification creates the K2 arm type; K2 then binds
the authenticated record digest, requires its verification timestamp to equal
the exact fresh-source observation timestamp, and rejects expiry at that point.
Successful verification atomically creates a separate HMAC-authenticated,
fsync-backed consumed marker with `O_EXCL`; a second verifier after consumption
or restart is rejected, while a torn marker permanently fails closed.
No public or production operator-arm issuance path calls this rehearsal.

K1/K2/K3 faults close safe. K4/K5 are classified `OutcomeUnknown`; they never
retry or resend. A fsync-backed no-send journal is closed and reopened at all
six prefixes: before attempt, attempt/no transport, possible effect/no response,
response/no durable outcome, durable outcome/no publication, and completed
publication. The versioned durable outcome and publication records encode the
exact closure payload. Every one of the five closure classes survives restart
both before and after publication without normalization: `Stage8BClosedSafe`,
`ResidualWorkingOrder`, `ResidualPosition`, `OutcomeUnknown`,
`BrokerTruthConflict`. Torn, unknown, corrupt or mismatched payloads fail closed.

## Build and endpoint evidence

The private execution-build verifier binds accepted source ref/archive/member
manifest, Cargo.lock/manifests, identical pre/post source trees, canonical
path-free metadata identity, exact resolved feature graph, both legacy-send
features false, unknown-feature count zero, Cargo and complete rustc/toolchain
fields, release target/package/binary, config, policy, instrument, API snapshot,
endpoint renderer and body schema. The endpoint identity binds exactly method,
`PlaceOrderV1` or `CancelOrderV1`, keyed account binding and renderer digest;
it contains no rendered account path.

The R2 handoff requires the canonical current-tree gate, no-Redis smoke, full
workspace debug/release/all-target tests, doctests, all-feature clippy, Redis
shadow smoke and runtime bridge dry smoke in addition to the focused I-R2 gate.
Run the aggregate gate once on the final clean commit and retain its complete
stdout with `bash scripts/stage8b_i_gate.sh | tee reports/stage8b-i-r2-gate.log`.
The handoff maker consumes that exact-commit log and rejects a stale source ref
or missing canonical-regression marker; it does not silently rerun the hour-long
immutable predecessor replay while packaging.

## Closed surfaces

Stage 8B-I does not authorize 8B-IT, 8B-P, 8B-XE, a real adapter, FINAM
POST/DELETE, network send, Redis XADD/XACK/live consumption, ACK/readiness
publication, broker dispatch, runtime-live, real strategy orders or Stage 12.
