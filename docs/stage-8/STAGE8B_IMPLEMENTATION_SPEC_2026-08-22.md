# Stage 8B-S — bounded engineering effect implementation specification

Status: specification/checker-only candidate; independent acceptance required.

## 1. Accepted predecessor and scope

Stage 8B-D R2 was independently accepted at
`f296d0be782b8aa550a20e27600ba16826214349` and merged without changing its tree
to `main` by `50ed5382fdbe2d62ed253d65a312f951e2a267ff`. The accepted tree is
`f40e2e5f40d7e3ed1dd5f5a252832734265094df`; the R2 handoff SHA-256 is
`ac351d9c03c98d59e90affeb423dbb7fff2cd3722b3d601889c53ae90c6cc06b`.

This Stage 8B-S package specifies future API, type-state, durability and test
topology. It changes no Rust, Cargo manifest, workflow, FINAM transport, Redis
consumer, broker dispatch or runtime-live path. Acceptance may open only Stage
8B-I no-send implementation and deterministic crash/replay rehearsal.

## 2. Sole composition root and dependency direction

The future implementation has one crate-private composition root in
`finam-gateway`. It consumes, by value:

- the accepted `Stage8a1CurrentlyAuthorizedCapability`;
- a current Stage 7B durable owner/request authority;
- trusted local build, account-binding and contract evidence;
- an independently accepted, exact immutable Stage 8B run specification.

It must not import or call historical M3d2 real transport constructors,
`EndpointGateApproved`, arbitrary `reqwest::Client`, arbitrary URL/method/header
builders, Redis execution authority or any second send wrapper. `broker-cli` may
only invoke the composition root; it cannot mint capabilities or render a raw
route. Runtime and strategy crates cannot depend on Stage 8B effect types.

## 3. Exact future type-state topology

All authority-bearing values below are `pub(crate)`, have private fields, and do
not implement `Clone`, `Copy`, `Default`, `Debug`, `Serialize` or `Deserialize`.
Every transition consumes its input. Public output is redacted diagnostic data
only and cannot be converted back into transport input.

```text
Stage8a1CurrentlyAuthorizedCapability
  + Stage7bStage8bDurableRequestAuthority
  + Stage8bExecutionQualifiedBuild
  + Stage8bKeyedAccountBinding
  + Stage8bFreshContractAuthority
  + Stage8bAcceptedRunSpec
  -> Stage8bFreshPreflightApproved

Stage8bFreshPreflightApproved
  + Stage8bOperatorArm                       [K1/K2]
  -> Stage8bAttemptCommitOwner

Stage8bAttemptCommitOwner
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
- `Stage8bOperatorArm`;
- `Stage8bFreshPreflightApproved`;
- `Stage8bAttemptCommitOwner`;
- `Stage8bSealedAttemptCommitted`;
- `Stage8bExactTransportPermit`;
- `Stage8bPossibleEffectOwner`;
- `Stage8bDurableClosureOwner`;
- `Stage8bClosureReceipt`.

No type exposes raw account ID, secret, rendered route, request body, bearer
token, raw response, retry handle or a reusable transport object.

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
   toolchain and immutable executable SHA-256;
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
price or cancel target, body and endpoint-template identities, expiry, kill-switch
generation, ownership lease and max-one budget generation. Silent action,
quantity, type, TIF or protective-role rewriting is forbidden.

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

## 14. Closed surfaces

Stage 8B-S keeps closed: production implementation; operator arming; FINAM
POST/DELETE; network send; Redis XADD/XACK/live consumer; ACK/readiness
publication; broker dispatch; retry/resend/re-arm; runtime-live; real orders;
MARKET/Stop/SLTP/bracket/replace/multi-leg; Stage 8B-P/X; Stage 11 execution
promotion; Stage 12.
