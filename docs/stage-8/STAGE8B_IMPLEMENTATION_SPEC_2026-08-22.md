# Stage 8B-S R3 — bounded engineering effect implementation specification

Status: corrective specification/checker-only candidate; independent acceptance required.

## 1. Accepted predecessor and scope

Stage 8B-D R2 was independently accepted at
`f296d0be782b8aa550a20e27600ba16826214349` and merged without changing its tree
to `main` by `50ed5382fdbe2d62ed253d65a312f951e2a267ff`. The accepted tree is
`f40e2e5f40d7e3ed1dd5f5a252832734265094df`; the R2 handoff SHA-256 is
`ac351d9c03c98d59e90affeb423dbb7fff2cd3722b3d601889c53ae90c6cc06b`.

Stage 8B-S R1 at `a675a772e02fa6da1a33973127542696019eb2f7` retained the
architecture but was not frozen. R2 at
`831eec8f830fa57e4ada8c135d803c34bea29298` closed those findings but was not
frozen because its adapter qualification followed exact 8B-P authorization.
R3 retains every R2 correction and fixes only that phase-order/build-binding
contradiction. This specification is
strictly additive over the exact accepted Stage 8B-D R2 authority file SHA-256
`83e85722fcf41e6abdd215569c4337f6c83994baeafbae47c5ad80bb9e935d09`.
No S field can weaken, override or replace an R2 invariant.

This Stage 8B-S package specifies future API, type-state, durability and test
topology. It changes no Rust, Cargo manifest, workflow, FINAM transport, Redis
consumer, broker dispatch or runtime-live path. Acceptance may open only Stage
8B-I no-send implementation and deterministic crash/replay rehearsal.

## 2. One public operator facade and one private composition root

Because `broker-cli` and `finam-gateway` are separate crates, the sole callable
cross-crate entry is one narrow public, non-authority-bearing facade:

```text
broker-cli
  -> finam_gateway::invoke_stage8b_operator_once(Stage8bOperatorInvocationRequest)
       -> Result<Stage8bOperatorDiagnostic, Stage8bOperatorFacadeError>
       -> pub(crate) compose_stage8b_effect_authority(...)
```

`Stage8bOperatorInvocationRequest` contains only reviewed local package/manifest
references and one opaque invocation ID. It cannot contain account IDs, URL,
method, headers, body, token, HTTP client, transport, capability, arm or raw
request parts. The public facade returns only a bounded redacted diagnostic; it
cannot mint, return, reconstruct or expose an authority value.

The future implementation has exactly one crate-private authority composition
root, `compose_stage8b_effect_authority`, inside `finam-gateway`. It consumes:

- the accepted `Stage8a1CurrentlyAuthorizedCapability`;
- a current Stage 7B durable owner/request authority;
- trusted local build, account-binding and contract evidence;
- an independently accepted, exact immutable Stage 8B run specification.

It must not import or call historical M3d2 real transport constructors,
`EndpointGateApproved`, arbitrary `reqwest::Client`, arbitrary URL/method/header
builders, Redis execution authority or any second send wrapper. `broker-cli`
can call only the public facade; the private root and every linear type must fail
cross-crate compilation. Runtime and strategy crates cannot depend on Stage 8B
effect types. Stage 8B-I must provide one positive facade integration fixture,
private-root/type compile-fail fixtures and exact single-facade/single-root
source inventory.

## 3. Exact future type-state topology

All authority-bearing values below are `pub(crate)`, have private fields, and do
not implement `Clone`, `Copy`, `Default`, `Debug`, `Serialize` or `Deserialize`.
Every transition consumes its input. Public output is redacted diagnostic data
only and cannot be converted back into transport input.

```text
Stage8bK1ControlApproved
  + Stage8bAcceptedRunSpec
  -> append/fsync/seal unique durable arm issuance
  -> Stage8bOperatorArm                     [K1]

Stage8bOperatorArm
  + Stage8a1CurrentlyAuthorizedCapability
  + Stage7bStage8bDurableRequestAuthority
  + Stage8bExecutionQualifiedBuild
  + Stage8bKeyedAccountBinding
  + Stage8bFreshContractAuthority
  + all exact K2 current source authorities
  -> Stage8bFreshPreflightApproved           [K2]
       (owns exact arm ID and exact run ID; no later substitution)

Stage8bFreshPreflightApproved
  -> consume arm and max-one budget
  -> append exact DispatchAttemptRecorded
  -> fsync
  -> authenticate covering Stage 7B seal
  -> Stage8bSealedAttemptCommitted           [K3]

Stage8bSealedAttemptCommitted
  -> immutable binding recheck
  -> Stage8bExactTransportPermit             [K4]

Stage8bExactTransportPermit
  -> exactly one private boundary invocation
  -> Stage8bPossibleEffectOwner

Stage8bPossibleEffectOwner
  -> fresh broker-truth reconciliation only [K5]
  -> Stage8bDurableClosureOwner
  -> exactly one durable closure classification
```

The exact future types are:

- `Stage8bExecutionQualifiedBuild`;
- `Stage8bKeyedAccountBinding`;
- `Stage8bFreshContractAuthority`;
- `Stage8bAcceptedRunSpec`;
- `Stage8bK1ControlApproved`;
- `Stage8bOperatorArm`;
- `Stage8bFreshPreflightApproved`;
- `Stage8bSealedAttemptCommitted`;
- `Stage8bExactTransportPermit`;
- `Stage8bPossibleEffectOwner`;
- `Stage8bDurableClosureOwner`;
- `Stage8bClosureReceipt`.

No type exposes raw account ID, secret, rendered route, request body, bearer
token, raw response, retry handle or a reusable transport object.

K2 cannot be minted before the exact durable arm exists. The K2 witness owns
that arm, binds its ID and run ID, and is consumed directly into durable attempt
recording. Arm swap, K2 reuse, a second arm and restart reconstruction of
preflight/send authority are structurally forbidden.

## 4. Causal execution-build provenance (S-01)

`Stage8bExecutionQualifiedBuild` can be minted only by a local verifier that:

1. extracts the independently accepted source archive into a new empty root;
2. verifies archive digest, member manifest, modes, absence of unsafe members and
   embedded source ref;
3. builds from that extracted root with network dependency fetch disabled after
   an independently captured dependency-vendor/cache preparation step;
4. verifies `Cargo.lock`, all workspace/package manifests and the source tree
   immediately before and after build;
5. records exact command, profile, package, binary target, target triple,
   Cargo version, toolchain channel and immutable executable SHA-256, plus every
   `rustc -Vv` field: release, commit hash, commit date, host and LLVM version;
6. records the fully resolved Cargo feature graph for every package, including
   default features and both legacy `m3j16-actual-one-shot=false` facts;
7. computes dependency identity from a canonical cargo-metadata projection that
   removes machine-local absolute paths, sorts objects/arrays under a documented
   schema and serializes canonical UTF-8 JSON;
8. binds runtime config, policy, instrument contract, API snapshot, endpoint
   renderer and canonical body-schema identities into one aggregate.

A dirty worktree build, an unbound binary, declaration-only feature evidence,
unknown feature state, mutable toolchain/action pin or path-dependent metadata
cannot mint the type.

## 5. Privacy-safe endpoint and account identity (S-02)

`Stage8bKeyedAccountBinding` implements exactly the accepted domain-separated
HMAC-SHA256 contract. The key remains outside Git, CI, logs and handoff. The
binding stores only the HMAC and opaque key-generation ID and verifies in
constant time against exact UTF-8 account bytes.

The machine authority represents the byte sequence without human escape
notation: ASCII domain `moex-stage8b-account-binding-v1`, suffix hex `00`, then
`u32be(account_utf8_length)` and exact account UTF-8 bytes. Literal `\\0`,
literal `\\u0000`, a removed separator, little-endian length and normalized
account bytes are different messages and fail closed.

The frozen non-secret golden vector is:

```text
key_hex     = 000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
account_hex = 4143435f544553545f30303031
message_hex = 6d6f65782d737461676538622d6163636f756e742d62696e64696e672d7631000000000d4143435f544553545f30303031
hmac_sha256 = 60106309bd530bd0cec76c3fa78fa4b7004ef34e44447fb7cd78fdda87444435
```

The review-safe endpoint identity is:

```text
method + route_template_id + keyed_account_binding + endpoint_renderer_sha256
```

where `route_template_id` is exactly one of `PlaceOrderV1` or `CancelOrderV1`.
No artifact publishes SHA-256 of a rendered path containing the raw account ID.
The rendered path and canonical body exist only inside the non-serializable
private request parts created after `Stage8bExactTransportPermit` is consumed.

## 6. Exact run and reachable action

`Stage8bAcceptedRunSpec` selects one effect only: PLACE or CANCEL. PLACE is
LIMIT/DAY/one lot/IMOEXF@RTSX. CANCEL identifies one currently working order by
exact durable broker order ID and proves it belongs to the same request/client
identity and lifecycle. No account-wide or caller-selected order may be used.

The spec binds request/client identity, process boot, build, config, policy,
instrument, keyed account, action, side, quantity, order type, TIF, exact decimal
price, exact canonical decimal `max_notional` or cancel target, body and
endpoint-template identities, expiry, kill-switch generation, ownership lease
and max-one budget generation. Price × quantity/notional is checked against the
bound maximum before attempt recording and again immediately before transport.
Silent action,
quantity, type, TIF or protective-role rewriting is forbidden.

The exact network policy is part of the run authority: TLS to host
`api.finam.ru`; PLACE uses POST with `PlaceOrderV1`; CANCEL uses DELETE with
`CancelOrderV1`. Redirects, proxies, alternate hosts, arbitrary URLs/methods/
headers, generic request APIs and automatic transport retry are forbidden.

## 7. Five current kill-switch boundaries (S-03)

The same persistent control source is freshly reread and its generation/revision
is bound at all five boundaries:

- `K1`: immediately before operator-arm issuance;
- `K2`: at final fresh preflight, before arm/budget consumption and attempt append;
- `K3`: after attempt append/fsync and authenticated covering seal;
- `K4`: immediately before the private transport boundary may write bytes;
- `K5`: before post-effect continuation, reconciliation application or closure
  publication.

Every read must be fresh, readable, exact `RunAllowed`, unexpired and consistent
with immutable run bindings. Failure before possible send closes consumed
authority with no retry. Failure after possible send enters reconciliation-only
ownership; it never proves absence and never permits resend.

## 8. Frozen freshness budgets and current readiness (S-03)

Before Stage 8B-P, immutable numeric maximum-age/skew budgets are independently
accepted for trusted clock, readiness, current control, ownership, schedule,
instrument, account/orders/positions/trades/exact-order sources and API snapshot.
Missing, future-dated, stale, incomplete or caller-selected values fail closed.

Historical ACK, historical readiness or an earlier successful GET cannot imply
current readiness. Current readiness is minted from fresh opaque source
authorities only and is destroyed when consumed by preflight.

The K2 constructor explicitly requires one FINAM execution owner, zero
ambiguity, zero unresolved lifecycle, fresh broker truth, fresh readiness, fresh
schedule and no caller-built or cached authority. These are semantic constructor
predicates, not diagnostic fields.

## 8.1 Durable one-use arm record

Arm issuance is an append/fsync/covering-seal transition keyed by the exact
durable request ID, client order ID, accepted run digest and account binding.
The durable states are `NeverIssued`, `IssuedUnconsumed`, `Consumed`,
`AttemptCommitted` and `Closed`. Only `NeverIssued` may issue once. Expiry,
consumption, crash or closure can never return to `NeverIssued`.

The arm binds command, build, config, policy, endpoint-template, body, keyed
account, expiry and run identity. Restart reconstructs only observation and
reconciliation authority. It cannot reconstruct a process-local arm, K2
preflight or send authority, and the uniqueness record forbids a replacement
arm for the same durable request.

## 9. Six distinct durable crash windows (S-03)

The implementation and deterministic rehearsal preserve these states without
normalization:

1. `BeforeAttempt` — no durable attempt, definitely no send;
2. `AttemptCommittedNoTransport` — arm/budget consumed, no retry;
3. `PossibleSendNoResponse` — reconciliation required;
4. `ResponseNoDurableOutcome` — recover from broker truth only, never resend;
5. `DurableOutcomeNoPublication` — recover settlement/publication only;
6. `RestartAtEveryBoundary` — reconstruction grants only the authority legal for
   the persisted state.

An HTTP timeout, disconnect, partial write, lost/malformed response, 429, 5xx or
2xx lacking required broker identity is not downgraded to no-send. A response is
not durable truth. A durable terminal outcome cannot be sent again merely because
publication failed.

## 10. Reconciliation, settlement and safe closure

The only post-boundary owner can call accepted Stage 8A4 fresh-truth admission,
reducer and Stage 7B durable application. It cannot call transport. Broker truth
cannot rewrite request/client/order identity.

Exactly one durable classification is produced:

- `Stage8BClosedSafe`;
- `ResidualWorkingOrder`;
- `ResidualPosition`;
- `OutcomeUnknown`;
- `BrokerTruthConflict`.

`Stage8BClosedSafe` requires every R2 predicate, including exact terminal target
lifecycle, approved position baseline restoration, clean account guard,
journal/seal consistency and operator signoff. Residual, unknown or conflict
states durably disarm the slice. Resolution is manual plus fresh reconciliation;
there is no automatic cleanup, CANCEL follow-up, retry or re-arm.

## 11. Stage 11 evidence sufficiency (S-04)

Before the acceptance series begins, its calendar, active-session definition,
exclusions, thresholds, ALOR oracle source/build/binary/config/profile hashes,
FINAM paper source/build/config/profile hashes and reachable-action inventory are
frozen. A complete elapsed session with no representative lifecycle activity is
time evidence only, not sufficient coverage.

The three-session minimum must cover representative strategy intents and order,
ACK, fill, position, timer, restart and reconciliation transitions reachable from
the frozen configuration. Unobserved reachable paths require additional sessions
or independently accepted deterministic replay. A blocking semantic/runtime/config
fix resets the consecutive-session counter. Recovery qualification remains a
separate test and cannot replace a normal session.

Throughout parity ALOR is the sole execution owner/oracle and FINAM POST/DELETE
and dispatch remain disabled. Every accepted session requires the same final M10
decision boundary, zero unexplained blocking divergence and no semantic CLI
overrides. Any blocking semantic/runtime/config change resets the full
three-session series.

## 11.1 Accepted Stage 8A-2 and Stage 8A-3 successor seams

Stage 8B-I must reuse the accepted Stage 8A-2 builders at ref
`16180ac4f8eab761b3b055c1f5515f62cd94bfb9`, including source SHA-256
`1026a24962bf45de8653c80ba095f892af35523da58f4fa4fccad706fb023653`.
The sole crate-private successor bridge is
`compose_stage8b_private_request_parts_from_stage8a2`. It consumes the accepted
Stage 8A continuation and exact transport permit, invokes only
`build_place_order_request` or `build_cancel_order_request`, and keeps raw
request specs, approved-command fields and body inside the existing privacy
domain. A second serializer or byte-equivalent independent builder is forbidden.

Stage 8B selects classifier Model A. A single private seam,
`classify_stage8b_transport_observation_with_stage8a3`, converts the local
boundary observation through the accepted Stage 8A-3 endpoint classifier at ref
`012c9bfa51c1d6206fbd9a7e1f06f1fc90fdf30d`, source SHA-256
`f34c9fef5e219dad15b0a00ce1eaf63311ec9f77d1997e422b977e5c8ffe47b3`.
Classifier output is candidate/diagnostic evidence, never execution truth; all
possible effects still reach durable Stage 8A4 broker-truth reconciliation.
No second or third classifier is permitted.

## 12. Fresh FINAM contract and governance gates (S-05/S-06)

Immediately before Stage 8B-P, the official FINAM contract is fetched again and
hash-bound. Drift in routes, methods, fields, enums, TIF, quantity, identifiers,
status or error semantics blocks promotion pending review.

Before Stage 8B-P, `main` must have branch protection or an independently
documented equivalent reviewed-change rule, immutable action/toolchain pins and
an accepted current authority manifest. Stage 8B-S does not change repository
settings or workflows.

## 13. Stage 8B-I proof obligations

If this specification is independently accepted, Stage 8B-I may add only no-send
production types and deterministic fixtures. It must prove:

- privacy and construction compile-fail contracts for every linear capability;
- resolved feature/build provenance and endpoint-identity fixtures;
- exact five-boundary kill-switch mutation coverage;
- exact six-window crash/restart matrix;
- one private request-parts constructor and one boundary interface whose test
  implementation records locally but performs no network I/O;
- no alternate transport, Redis authority, ACK publication, broker dispatch,
  runtime-live or real order path;
- safe-closure and residual-state deterministic fixtures.

Stage 8B-I acceptance still does not authorize Stage 8B-P or any broker effect.

## 13.1 Real adapter qualification before exact run authorization

The immutable phase order is `8B-D → 8B-S → 8B-I → 8B-IT → 8B-P → 8B-XE`:

- `8B-IT`: implement and independently qualify the exact real FINAM transport adapter against
  local/controlled non-broker endpoints; prove host/TLS/method/route policy,
  no redirects/proxy/retry and permit-only reachability. FINAM POST/DELETE effect
  remains forbidden.
- `8B-P`: only after accepted IT, perform the fresh official contract/preflight
  and authorize the exact accepted adapter-qualified source, Cargo/lock/feature
  graph, toolchain, dependencies, config/policy/instrument/API snapshot,
  endpoint renderer, body schema and executable identities.
- `8B-XE`: only after accepted IT, the exact matching 8B-P package and a fresh
  operator authorization, permit at most one broker effect followed by durable
  reconciliation and safe closure.

The first review of real adapter code and the first broker effect can therefore
never be the same acceptance event. A P package issued before adapter
qualification is invalid. From accepted IT through P to XE, the P-bound build is
immutable. Any source, Cargo manifest/lock, resolved feature graph, toolchain,
dependency, config, policy, instrument, API snapshot, endpoint renderer,
request-body schema or executable drift invalidates and discards P. Relevant
adapter qualification must then be repeated, followed by fresh FINAM contract
preflight and a new exact P package. Automatic P refresh, authority carry-over
and use of a different build at XE are forbidden.

## 14. Closed surfaces

Stage 8B-S R3 keeps closed: production implementation; operator arming; FINAM
POST/DELETE; network send; Redis XADD/XACK/live consumer; ACK/readiness
publication; broker dispatch; retry/resend/re-arm; runtime-live; real orders;
MARKET/Stop/SLTP/bracket/replace/multi-leg; Stage 8B-I/IT/P/XE; Stage 11 execution
promotion; Stage 12.
