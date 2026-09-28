# Stage 8B-P1-e I1A — authenticated schedule source design

Status: **DESIGN REVIEW CANDIDATE — IMPLEMENTATION NOT AUTHORIZED**.

This amendment closes the schedule-source design gap identified during review of the
I1 supervisor foundation. It does not add a production publisher, a supervisor source
adapter, a Stage5E facade, a durable record, or an operational Redis key. Those changes
are permitted only after independent acceptance of this design package.

The immutable predecessors remain P1-e I0
`afda87a98ae3b0d0f4506292a162310f4b9068c0`, P1-e R10
`d34e000c39f439ae981f9573c8b203a3dc8e3e85`, and P1-d4 governance closure
`c2a9e1246dfdd59f3a6297268de907dedcb19903`. Foundation R1 source candidate
`a0b07f6ef16ac8e43004204f1e52184bc615fa97` closes only F01/F02 and is not an
I1A implementation.

## 1. Selected architecture and ownership

The selected source is a separate, pre-provisioned, broker-neutral Redis Stream in the
isolated paper DB15:

```text
finam_imoexf_paper:{finam-imoexf-p1}:market-schedule
```

It is not an M10 field and does not alter the canonical M10 wire schema. It has no
consumer group and no PEL/XACK lifecycle. The future supervisor is verify-only: it may
perform `TYPE` and bounded `XREVRANGE`, but may not create, repair, trim, append, delete,
or acknowledge this stream.

The sole planned production writer is
`finam-gateway::Stage8bP1eReadonlySchedulePublisherV1`. It maps a FINAM GET-only
`AssetScheduleResponse`, an independently accepted redacted broker-neutral
`Stage4BootstrapEvidenceReport`, and the exact instrument-registry projection into the
signed envelope. Neither the supervisor nor strategy-runtime may manufacture the
source. A controlled fixture publisher is allowed only in isolated tests before the
operational producer is accepted.

Provisioning belongs to a later administrative boundary. The producer may not create
the stream implicitly. Missing or wrong Redis key type therefore fails closed rather
than silently creating a second authority source.

## 2. Trust and canonical envelope

Redis DB15 is transport, not a trust anchor. The envelope is authenticated by the
Generation-2 `schedule` public key already pinned by:

```text
manifest: docs/stage-8/stage8b-p-r2b-trust-rebind-generation-2-trust-manifest.json
manifest sha256: dfe61ddb944df042cdf9514f56c14131e4a45bc732435ff89658ceaceb92d4ee
key id: schedule-ed25519-v1
key generation: 2
public key: 432a274889bc2b3a4492b3a1510f7356e1c2878337263e28472335f8600e4d99
public-key sha256: feff4cf8ddce79666c0dd71f0f3e634a396f2b0f479a2d5b12da230330c543b3
```

This selection activates only verification by an existing public key. It does not
activate Generation-2 execution, authorize installation of a private key, or authorize
the future publisher operationally. Private material must never enter Git, handoff,
the supervisor process, or runtime configuration. Installing the signing key and
starting the producer require a separate operational acceptance gate.

The wire contract is
`stage8b-p1e-i1a-schedule-envelope-v1.schema.json`. One Redis field named `payload`
contains the exact UTF-8 canonical JSON bytes. Recursive duplicate and unknown keys,
floating-point values, malformed decimal strings, non-canonical timestamps, uppercase
hex, trailing data, and any decode/re-encode byte difference are rejected.

The signature preimage is:

```text
sha256(
  "moex.stage8b.p1e.schedule-envelope.signature.v1"
  || 0x00
  || canonical_unsigned_envelope_bytes
)
```

It is signed with Ed25519. `payload_sha256` binds the exact canonical payload bytes;
the envelope also binds producer contract, source generation/sequence, operational
identity, runtime configuration, instrument map, key identity, and publication time.

## 3. Schedule and Stage4 evidence validation

The broker-neutral payload is restricted to IMOEXF / `IMOEXF@RTSX`, MOEX futures,
MIC `RTSX`, board `FUT`, tick size `0.5`, Europe/Moscow, and timeframe 600 seconds.
The registry version and registry identity hash are mandatory.

Known session types are exactly:

- `tradable_open`;
- `break_or_clearing`;
- `maintenance`.

Sessions are non-empty, ordered by start/end/type, non-overlapping, and endpoint
unambiguous. At least one `tradable_open` interval is required. A non-tradable gap is
accepted only when every affected M10 grid endpoint is explicitly covered by a known
non-tradable interval. Unknown session types and inferred eligibility are rejected.

The Stage4 member carries the exact canonical report bytes as lowercase hex, their
SHA-256, explicit Stage4 source observation/expiry times, and signed schedule state
`open`. The default-feature verifier must decode and validate the actual report, not
trust copied summary flags. It must prove all of the following:

- report status is `Accepted` and every required section is accepted;
- safety boundary remains closed and no live authorization is present;
- manual intervention is false and the reason chain is empty;
- schedule source is present, fresh, required for bootstrap, and not blocked;
- signed Stage4 schedule state is `Open`;
- expiry is recomputed from every required report section;
- explicit Stage4 observation/expiry fields equal the canonical report values;
- report, normalized schedule, registry, instrument, and identity bindings agree.

Normalized schedule and Stage4 evidence remain independent proofs. Matching one hash
or freshness clock cannot stand in for the other.

## 4. Freshness, ordering, and lookup

The trusted clock allows at most 250 ms future skew. Maximum ages are:

| Evidence | Maximum age |
|---|---:|
| Signed transport envelope | 5,000 ms |
| Normalized FINAM schedule | 86,400,000 ms |
| Stage4 schedule evidence | 5,000 ms |
| Cross-source observation skew | 5,000 ms |

Effective expiry is the earliest of transport expiry, normalized source expiry,
recomputed Stage4 required-source expiry, and trust-key validity. Re-serialization or
republishing never refreshes old source observations.

The future producer uses exact bounded retention `MAXLEN = 4096`. The supervisor reads
at most 64 newest rows with `XREVRANGE`. The newest row is the candidate; malformed or
untrusted newest data blocks the source and never causes fallback to an older row.
The bounded window is used to detect sequence conflicts and to cross-check the durable
high-water mark, not to search for a convenient valid row.

Within one generation, sequence is strictly increasing. Equal generation/sequence and
equal bytes are idempotent. Equal generation/sequence with different bytes is a
terminal conflict. Rollback, reorder, unexplained gap, or a generation change without
a reviewed trust/config amendment blocks issuance. A durable high-water observation is
updated only as part of the authenticated transition commit.

Acquisition uses 12 attempts within a 60-second total deadline, a 2-second Redis
operation timeout, and deterministic exponential backoff from 250 ms to 5 seconds.
Missing/stale input before the deadline is Degraded and retryable. Deadline exhaustion
is exit 67. Identity, schema, signature, replay conflict, wrong key type, or unavailable
atomic durability is exit 66. All such exits retain the owner and exact M10 PEL.

## 5. Stage5E facade and authority issuance

The only future source-to-authority route is a narrow default-feature facade owned by
`strategy-runtime-core::stage5e_no_io_lifecycle::schedule_window_evidence`. Validation
order is fixed:

1. strict decode and canonical-byte equality;
2. payload/report hashes;
3. trust manifest, key generation, validity and Ed25519 signature;
4. generation/sequence replay rules and all freshness clocks;
5. operational/runtime/instrument/registry bindings;
6. full Stage4 accepted-report chain and signed Open state;
7. normalized schedule structure;
8. existing Stage5E registry and schedule classifier;
9. transition-exact authority issuance.

The facade must compile under default features. `artifact-fixtures` is prohibited from
deployable binaries. It may not return raw schedule rows, booleans, or accepted flags;
it may not expose existing private constructors. Authority values are opaque, non-
Clone, non-Copy, non-Serde, have no raw-parts constructor, and are consumed once.

Three distinct issuance operations are required:

1. Market execution → `Stage8bP1d1ExecutionScheduleAuthority`.
2. Working LIMIT or Cancel step → `Stage8bP1d3ScheduleStepAuthority`.
3. Day expiry → `Stage8bP1d3DayExpiryAuthority`.

Each binds the exact operational identity, schedule fingerprint, trading day,
instrument/timeframe, transition hash, active request/order/book as applicable, and
exact predecessor/candidate M10 identities. Day expiry additionally binds the proven
last eligible M10 and boundary timestamp. A schedule snapshot may support several
different transitions while fresh, but each authority is separately transition-bound
and single-use.

## 6. Durable binding and restart

Before first authority issuance the future implementation must append
`Stage8bP1eScheduleEvidenceBoundV1` to the authenticated Stage6 journal, include it in
the covering seal, fsync, reread, and cross-validate it. The record retains the exact
signed envelope bytes and hash, Redis stream ID, generation/sequence, schedule
fingerprint, authority kind, transition binding, and bind timestamp.

For the identical incomplete durable transition after crash/restart, the exact
historical envelope may reissue semantically equivalent authority even when it has
left Redis retention or is no longer fresh for new work. This replay may not invoke a
second strategy callback, paper-provider call, or already committed seal. The newest
schedule may never replace the bound historical envelope mid-transition.

Historical evidence cannot authorize a new transition. Every new transition requires
a currently fresh source and a new durable binding. Schedule processing has no direct
effect on M10 XACK; accepted Stage7 XACK-last rules remain the sole source-ack authority.

## 7. Missing source and timer precedence

Missing, stale, future, untrusted, conflicting, identity-mismatched, or Stage4-blocked
source produces explicit Degraded readiness. It does not imply session eligibility,
does not release the linear owner, does not XACK the exact M10, does not acquire a
second fresh M10, and does not execute paper-provider or broker effects.

Timer policy is `SOURCE_FIRST_TIMER_DEFERRED`:

1. classify a due timer without executing it;
2. check the shutdown latch;
3. complete S06 pending/reclaim scan;
4. perform at most one S08 fresh M10 poll;
5. process that exact M10 to an authenticated boundary if present;
6. reclassify the timer against the returned owner/book;
7. obtain or reissue the exact day schedule binding;
8. check the latch after acquisition;
9. check the latch immediately before timer execution;
10. execute timer as a separate effect.

An unavailable schedule can never invent Day expiry. A signal at either checkpoint
retains the exact owner, schedule binding state, and M10 PEL according to the accepted
shutdown latch contract.

## 8. Versioned amendment and closed surfaces

Implementation, after design acceptance, must add new versioned Redis deployment,
runtime-policy, and supervisor-config artifacts. Historical R10/I0 artifacts are
immutable and must not be rewritten as though they already contained the stream.
Every new artifact hash must be pinned consistently in source, checker, evidence, and
handoff. The prospective source allowlist is
`stage8b-p1e-i1a-implementation-scope-v1.json`.

This package leaves all operational surfaces closed:

- Redis DB0/VPS and DB15 schedule activation;
- schedule private-key installation;
- FINAM POST/DELETE and broker dispatch;
- runtime-live and real orders;
- Generation-2 execution activation.

## 9. Acceptance boundary

The executable matrix is
`stage8b-p1e-i1a-schedule-source-acceptance-matrix-v1.csv`. It covers positive Market,
Working, Cancel, Day and approved-gap paths; authentication/freshness/identity/session
negatives; durable replay and retention; timer/latch ordering; compile privacy; and
closed surfaces.

Acceptance of this design authorizes only the bounded implementation slice listed by
the implementation-scope artifact. It does not accept that future source code. After
design acceptance the next review target is the production facade/source adapter plus
fixture-backed tests. Only after that source acceptance may F00–F17 owner-loop wiring,
process supervision, the new crash/restart matrix, and final I1 closure proceed.
