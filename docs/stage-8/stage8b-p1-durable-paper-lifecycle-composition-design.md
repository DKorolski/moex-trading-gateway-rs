# Stage 8B-P1 durable paper lifecycle composition design

Status: architecture, P1-a and the semantic-commit R1A addendum are accepted.
P1-b is an implementation review candidate; P1-c remains closed. No P1 Redis
consumer or VPS activation is authorized by this document.

## Decision

Keep the paid isolated VPS and continue paper/shadow work on it. A failed
native-installation proof is not evidence that the host is compromised and is
not a reason to destroy the VPS. Generation 2 and the production authorization
path remain inactive and separate from this paper composition.

The accepted Stage 7B implementation must be reused as the durable paper
command authority. It must not be replaced with another Redis consumer,
another journal, or an ad-hoc in-memory retry loop.

## Proven P0 boundary

The deployed P0 process currently proves only this chain:

```text
final M1 Envelope<MarketDataEvent::Bar>
  -> canonical complete in-process M10 RuntimeBarInput
  -> paper hybrid projection
  -> atomic paper runtime-state batch in Redis
```

The isolated DB 15 smoke passed with 10 source entries, 10 XACKs, one runtime
batch, zero pending entries and zero DLQ entries. Its evidence is in
`stage8b-paper-shadow-db15-smoke-evidence.json`.

The P0 projection is not the accepted full Hybrid strategy host:

- `finam-paper-runtime-consume` passes an empty intent vector to the adapter;
- `--strategy-invocation-shadow` populates the compatibility projection but
  does not emit canonical `BrokerCommand` envelopes;
- the canonical M10 is consumed in process and is not separately persisted by
  this P0 command; P1 selects a new, separately persisted canonical M10 stream;
- it has no durable ACK/order/position lifecycle owner.

These limitations are explicit. They must not be relabelled as strategy paper
parity.

## Existing accepted authority to compose

Stage 7B is independently accepted at
`a1044e0dbe324c722b637498ca80ffafd9f0cbee`. The reusable production-paper
component is `Stage7bRedisService<P>`.

It already provides:

- one file-backed Stage 6 journal authority;
- a kernel-held single-writer lease;
- authenticated recovery seals;
- canonical command decode and profile checks;
- durable admission before paper-provider invocation;
- atomic ACK/DLQ publication and XACK;
- PEL reclaim and restart handling;
- composite paper readiness;
- no FINAM transport and no real-order capability.

It is currently a library/test composition. The repository has no deployable
binary that can safely construct its first-boot or restart inputs.

## Why a thin CLI wrapper is not yet sufficient

`Stage7bRecoveryReadyOwner::first_boot` requires all of the following as one
consistent authority set:

1. `Stage7bDurableRootAuthority` bound to the operational identity;
2. `Stage6dOperationalIdentityConfig`;
3. one linear `Stage6dFirstBootAuthorization`;
4. source-produced authenticated Stage 5G clean-restart bytes;
5. the exact `Stage5gLifecycleCommitmentKey` that authenticated those bytes;
6. a matching freshly constructed `HybridIntradayRuntimeStrategy`.

The P0 runtime-state JSON is not a Stage 5G clean-restart source and must never
be converted into a fabricated seed. Redis state, operator JSON and test-only
fixtures are not first-boot authority.

The Stage 7B provider returns durable command outcomes and identifiers. The P0
paper ledger independently models fills, prices, orders, trades and positions.
Connecting both without a single ownership rule would create two lifecycle
authorities. Therefore a direct `PaperIntent -> BrokerCommand` translator plus
a synthetic provider is forbidden until the ownership and feedback path below
is implemented and accepted.

## Selected P1 composition

Use one paper process with a single durable lifecycle owner:

```text
read-only FINAM WS M1
  -> canonical final M10 admission
  -> accepted Stage 5C/5F/5G Hybrid semantic continuation
  -> canonical BrokerCommand publication
  -> Stage7bRedisService
       -> file-backed Stage 6 lifecycle
       -> paper outcome provider
       -> canonical ACK / DLQ / XACK
  -> accepted ACK/order/position continuation
  -> source-produced Stage 5G checkpoint for restart
  -> redacted health/readiness/runtime-state evidence
```

The implementation may use cooperating tasks, but there is one process-level
composition owner and one mutable Hybrid runtime authority. No API may extract
`Stage6dDurableRuntimeRecovered`, the file journal or the writer lease for a
second owner.

The exact M10 stream, semantic batch identity, Stage 5G/6/7 commit ordering,
M10-last XACK rule and crash matrix are frozen in
`stage8b-p1-semantic-commit-protocol-addendum.md`. R1A acceptance authorizes
P1-b source implementation only; P1-c command publication remains on hold.

### Required narrow source change

The accepted recovery owner currently exposes command-lifecycle methods but no
production semantic-bar continuation. P1 therefore requires a reviewed narrow
facade owned by the same composition boundary. It must either:

- advance the accepted Stage 5C/5F/5G bar/timer continuation through the
  existing linear owner and return canonical command publications; or
- prove an equivalent single-owner composition that does not clone or run a
  second Hybrid strategy state.

A separate process that independently runs the strategy while Stage 7B owns a
different recovered copy is not accepted by this design.

## Bootstrap and restart contract

### First boot

First boot is an explicit offline administrative action:

1. validate the paper config and instrument-map fingerprint;
2. create one fresh Hybrid runtime using the accepted production constructor;
3. advance it only through accepted paper bootstrap/warmup facades;
4. export a source-produced Stage 5G clean-restart package;
5. authenticate it with a deployment-specific Stage 5G commitment key;
6. derive the expected Stage 7B identity directory name;
7. create that empty canonical directory with restrictive ownership;
8. mint one linear first-boot authorization;
9. call `Stage7bRecoveryReadyOwner::first_boot` exactly once;
10. persist the initial journal and recovery seal before attaching Redis.

No test feature, golden fixture, arbitrary runtime-state JSON, ALOR snapshot or
Redis payload may replace steps 2-5.

### Restart

Every later start:

1. loads the same deployment identity and commitment key;
2. opens only the exact identity-derived durable root;
3. calls `Stage7bRecoveryReadyOwner::restart`;
4. refuses Redis attachment unless the result is `Ready`;
5. reclaims eligible PEL with a fresh consumer name;
6. publishes `PaperReady` only after storage, seal, source poll, claim scan,
   settlement and zero-unresolved-state checks all pass.

No restart may silently recreate a missing journal or seed.

## Commitment-key custody

The Stage 5G lifecycle commitment key is unrelated to the Stage 8B Generation-2
production signing ceremony. P1 must not copy or reuse Generation-2 material.

The paper key must be:

- exactly 32 random bytes generated once for this paper deployment;
- supplied to the process through a root-owned systemd credential or an
  equivalent file-descriptor boundary;
- absent from Git, Redis, JSON/TOML configs, logs, handoff archives and process
  arguments;
- readable only by the service identity at start;
- zeroized by the existing key type on drop;
- backed up only under a separately documented paper recovery policy.

Key generation and the source-produced first-boot seed must be one reviewed
bootstrap workflow. Generating a new key on ordinary restart is forbidden.

## Fixed paper identity and namespace

The initial deployment is single instrument and single strategy:

- broker id: `finam-paper`;
- strategy id: `hybrid_imoexf`;
- instrument: `IMOEXF` / `IMOEXF@RTSX` / MOEX Futures;
- Redis prefix: `finam_imoexf_paper:`;
- Stage 7B streams must share one explicit Redis hash tag;
- durable root name must equal
  `Stage7bDurableRootAuthority::expected_directory_name(identity)`;
- account id and instrument-map fingerprint come from validated config, not
  from a consumed command.

RI and USDRUBF are later deployments with different identities, roots, hash
tags and consumer groups. They are not added to the first IMOEXF acceptance
run.

## Paper outcome ownership

P1 must define one paper execution reducer, not two.

The selected provider contract is:

- Stage 7B owns command admission, dispatch-attempt evidence, terminal outcome,
  canonical ACK and replay identity;
- the provider receives only a durably admitted command;
- market/limit/cancel outcomes are derived from an accepted deterministic
  market-data snapshot or next-bar policy;
- provider-generated broker order/trade IDs are deterministic from durable
  request identity and cannot depend on Redis entry ID or process boot ID;
- order/trade/position projections are derived from that same provider outcome
  and Stage 6 facts;
- the legacy P0 paper ledger may remain a diagnostic comparator, but it cannot
  independently settle the same P1 command.

Uncertain outcome returns `Stage7aPaperProviderError::Uncertain` and leaves the
durable request pending for reconciliation. It is never guessed, resent or
converted to a successful ACK.

## Publication and feedback rules

- Semantic output is published as canonical `Envelope<BrokerCommand>` accepted
  by the existing Stage 7A decoder.
- `StrategyRequestId`, `ClientOrderId` and `BrokerOrderId(String)` keep their
  accepted distinct meanings.
- Command stream, ACK stream and DLQ stream use the accepted Stage 7B paper
  namespace and one Redis hash slot.
- Runtime pending state clears only on an exact matching canonical ACK.
- Duplicate ACK does not reapply a position transition.
- Order/trade/position publication is downstream evidence from the durable
  outcome, not a second execution authority.
- No output stream is named like or shared with the ALOR live contour.

## Health and readiness

The deployable service publishes redacted snapshots containing at least:

- boot mode (`first_boot` or `restart`);
- operational identity digest;
- durable checkpoint and recovery-seal generation;
- consumer alive, source-poll freshness and claim-scan freshness;
- settlement health;
- PEL count and blocked request count;
- last accepted semantic bar timestamp;
- last canonical ACK timestamp;
- `paper_only=true`;
- `finam_transport_attached=false`;
- `broker_network_dispatch_attached=false`;
- `runtime_live=false`;
- `real_orders=false`.

Readiness is `PaperReady`, not `LiveReady`.

## Implementation sequence after design acceptance

1. **P1-a — bootstrap/identity facade.** Implemented as a library-only review
   candidate. It adds deployable config types, source-produced first-boot
   package workflow and restart-only default and proves that missing roots,
   malformed credentials and wrong identities fail before Redis. See
   `stage8b-p1a-bootstrap-identity-facade.md`.
2. **P1-b — single-owner semantic continuation.** Attach canonical M10 and the
   accepted Hybrid bar/timer facade without exposing a second mutable runtime.
3. **P1-c — canonical command publication.** Publish exact Stage 7A command
   envelopes with deterministic IDs and exact ACK feedback.
4. **P1-d — deterministic paper provider/projections.** Reuse Stage 7B durable
   lifecycle and derive order/trade/position evidence from its outcomes.
5. **P1-e — deployable supervisor.** Add systemd units, credential handling,
   readiness streams and graceful shutdown.
6. **P1-f — isolated operational acceptance.** Use synthetic data and isolated
   Redis first, then readonly FINAM bars. Prove restart, PEL reclaim, duplicate,
   conflicting duplicate and uncertain-provider behavior.
7. Independent review is required before P1 service activation in operational
   Redis DB 0. Existing P0 read-only DB 0 operation is unaffected.

## Review decisions required

The reviewer should accept or reject these exact design choices before code:

1. one composition owner may gain a narrow semantic-bar continuation facade;
2. P1 uses source-produced Stage 5G first-boot material, never P0 projection
   JSON or test fixtures;
3. Stage 7B is the sole command lifecycle authority;
4. deterministic paper order/trade/position projections derive from the Stage
   7B provider outcome rather than the legacy P0 ledger;
5. the Stage 5G commitment key uses separate systemd credential custody and is
   unrelated to Generation 2;
6. operational DB 0 activation waits for independent P1 acceptance.

## Explicitly closed

- full-trade FINAM token on the paper VPS;
- FINAM order HTTP POST/DELETE;
- broker network dispatch;
- Redis-to-real-FINAM command consumer;
- runtime-live and unattended live execution;
- real orders;
- protective live orders;
- Generation-2 activation or authorization issuance.
