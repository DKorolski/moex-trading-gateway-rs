# Stage 8B-I — no-send production types and deterministic rehearsal

Status: implementation candidate; independent acceptance required.

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
types. It is unreachable from the public facade in I and cannot produce or call
a transport adapter. `compose_stage8b_private_request_parts_from_stage8a2`
uses the accepted existing-builder-only Stage 8A-2 sink; no serializer is added.
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

The internal arm rehearsal uses `openat(O_CREAT|O_EXCL|O_NOFOLLOW)`, mode 0600,
file `fsync` and directory `fsync`. A two-process test proves exactly one winner.
No public or production operator-arm issuance path calls this rehearsal.

K1/K2/K3 faults close safe. K4/K5 are classified `OutcomeUnknown`; they never
retry or resend. A fsync-backed no-send journal is closed and reopened at all
six prefixes: before attempt, attempt/no transport, possible effect/no response,
response/no durable outcome, durable outcome/no publication, and completed
publication. Impossible record order is rejected. The five closure classes stay
distinct: `Stage8BClosedSafe`, `ResidualWorkingOrder`, `ResidualPosition`,
`OutcomeUnknown`, `BrokerTruthConflict`.

## Closed surfaces

Stage 8B-I does not authorize 8B-IT, 8B-P, 8B-XE, a real adapter, FINAM
POST/DELETE, network send, Redis XADD/XACK/live consumption, ACK/readiness
publication, broker dispatch, runtime-live, real strategy orders or Stage 12.
