# Stage 8B-P1-e R4 deployable paper supervisor design correction

Status: design-only review candidate. P1-e source implementation remains
unauthorized until independent acceptance of this exact R4 package.

The immutable accepted predecessor is
`c2a9e1246dfdd59f3a6297268de907dedcb19903` (P1-d4 closure). R4 is a direct
child of the held R3 candidate
`913424b73c5a83df2131a1f9a2901b78035bfc4c`. The independent R3 review is
bound by SHA-256
`e34d3992d40bdc0a488ef3c491d96c05e3793763464f676780f2594a5e66427b`.

R4 preserves the accepted R3 direction and closes its four P1 and two P2
findings. It changes design documents, acceptance data, checkers, evidence,
status and handoff tooling only. It changes no production Rust, Cargo file,
workflow, active systemd unit, deployed config, Redis or FINAM implementation,
or governance authority.

## 1. Sole active contract

`Stage8bP1eAcceptanceContractV4` is the only active merge. It retains every
non-conflicting inherited requirement, excludes each conflicting row exactly
once, and activates all 56 R4 rows. The resulting set is:

```text
R1  83 active of 88
R2  34 active of 48
R3   9 active of 43
R4  56 active of 56
--------------------
   182 active REQUIRED rows
```

The exact supersession map is machine checked. Cross-version meaning is not
inferred from row identifiers. `SemanticAuthorityRegistryV4` names concrete
keys such as `systemd.User`, `filesystem.MarkerOwner`,
`redis.NonReadyAcquisitionOwner`, `ordering.SourceBeforeTimer` and
`classifier.UnlistedTupleDisposition`. Every required key has exactly one
active byte-identical value. Conflicting inactive R3 values remain visible as
historical evidence.

## 2. One executable deployment identity

All modes share one identity:

| Boundary | Exact value |
| --- | --- |
| User / Group | `moex-p1-paper` / `moex-p1-paper` |
| ordinary unit | `moex-finam-p1-paper.service` |
| bootstrap unit | `moex-finam-p1-paper-bootstrap.service` |
| recovery unit | `moex-finam-p1-paper-bootstrap-recover@.service` |
| binary | `/usr/local/libexec/moex/stage8b-p1-paper-supervisor` |
| config | `/etc/moex-finam-p1-paper/supervisor.json` |
| credential | `/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key` |
| durable parent | `/var/lib/moex-finam-p1-paper` |
| mutable state | `/var/lib/moex-finam-p1-paper/state` |

The parent is preprovisioned `root:moex-p1-paper` mode `0750`. The state,
transaction, canonical-root and quarantine directories are
`moex-p1-paper:moex-p1-paper` mode `0700`. Journal, seal, marker, marker temp,
receipt and receipt temp are service-owned mode `0600`. The non-root service is
never required to manufacture a root-owned file.

Creation is fd-relative with `O_NOFOLLOW`. Immediate and reread `fstat`
validation requires the exact service UID/GID, regular file type, exact mode,
one link and no group/world write bits. No second deployment identity or
alternate executable/config/credential path is valid. The full table and all
three command grammars are in `stage8b-p1e-deployment-identity-v1.json`.

## 3. Complete marker response-loss model

`Stage8bP1FirstBootTransactionV3` retains the ten base classes and adds four
separate authenticated temp-pending classes:

1. `PreparedToRootPublishedMarkerTempPending`;
2. `RootPublishedToJournalDurableMarkerTempPending`;
3. `JournalDurableToSealCommittedMarkerTempPending`;
4. `SealCommittedToAdoptedMarkerTempPending`.

For each class, the old marker, next marker temp, transaction and deployment
identity, generation increment, root identity, file custody and corresponding
already-durable phase effect must all agree. Recovery validates these facts,
renames only the exact temp over the old marker, fsyncs the parent, rereads and
authenticates it, then continues the same transaction. It does not repeat root,
journal, initial-seal or receipt creation and does not silently remove the temp.

A cross-transaction, cross-identity, incompatible-phase, wrong-generation,
unknown-version or custody-conflicting temp exits before mutation and preserves
evidence. Each temp-sync-before-rename frontier has a required release
subprocess/SIGKILL/wait/restart/idempotent-completion test.

## 4. Redis acquisition model B

R4 does not redesign the accepted P1-d continuation APIs. For every non-Ready
owner, S06 is observation only: it may inspect XPENDING/XINFO and validate the
source identity, but it cannot execute XAUTOCLAIM, XREADGROUP or XACK. The
already accepted matching `resume_*_with_redis` wrapper owns the sole reclaim.
Immediately after that reclaim it checks the shutdown latch before parsing or
performing any semantic effect.

Only Ready plus an existing PEL delivery and Ready plus an S08 fresh delivery
construct `Stage8bP1eClaimedM10DeliveryV2`. This linear, non-cloneable,
non-serializable owner carries the exact stream, group, Redis entry ID, owned
canonical payload bytes, payload hash, semantic and operational identities,
acquisition disposition, consumer identity and delivery generation.

`process_claimed_ready_source` and `process_claimed_working_limit` consume that
owner and contain no XPENDING, XAUTOCLAIM or XREADGROUP path. A shutdown latch
is checked both before acquisition and immediately after successful delivery.
If raised after delivery, the source remains pending and parse, callback,
provider, schedule and XACK counts remain zero. Pending-not-claimable remains
degraded and never falls through to a fresh read.

Instrumentation bounds successful source acquisition, callback, provider and
source XACK to at most one per source; zero remains valid for retained,
blocked or already-acknowledged paths.

## 5. Orthogonal source/timer arbitration

The exact precedence is `SOURCE_FIRST_TIMER_DEFERRED`. If one exact pending or
claimable source and a due Day timer coexist, the source continuation reaches
its authorized durable boundary first. Any normal source XACK remains XACK-last.
Only then is the same timer reclassified against the returned authenticated
owner.

Before source completion there is no expiry authority, expiry outcome/truth,
terminal book transition or new callback. The deferred timer is discarded as
stale when the order is filled, canceled, expired, replaced, absent or no
longer belongs to the current trading day. It executes only if the same exact
working Day order remains present and due.

The rule applies to Ready and every named source-bearing non-Ready owner. The
logical P1-d3 cancel-recovered state is represented by
`P1d3TruthCommitted + p1d3_s_cancel_recovered`; R4 does not invent a new Rust
enum variant. Any tuple neither listed in the 52-row V4 matrix nor exactly
derived by this rule exits class 67 before transition, without Ready or fresh
poll fallback.

## 6. Byte-exact derived digests

`stage8b-p1e-derived-digests-v1.json` defines one binary record framing for:

- transaction ID SHA-256;
- canonical root identity SHA-256;
- adoption-ready owner SHA-256;
- receipt V2 HMAC-SHA256 preimage.

The framing fixes magic, schema, domain and field-name lengths, fixed field
order, value lengths, unsigned big-endian integers, network-order UUID bytes,
raw 32-byte digest inputs, filesystem `st_dev`/`st_ino` as `u64be`, and
lowercase-hex output. The receipt binds transaction, deployment and root
identities, package V2 hash, first-boot provenance, seal generation and
commitment, adoption owner and receipt generation.

Checked-in golden fixtures contain exact canonical preimage hex and expected
SHA-256/HMAC values. The checker independently encodes and recomputes every
fixture rather than trusting stored preimages.

## 7. Verification and authorization boundary

The R4 gate verifies exact parent/scope, all source hashes and supersessions,
the semantic authority registry, identity/custody, 10+4 transaction classes,
all 19 non-Ready acquisition owners, all 52 operational rows, source-first
derivation, digest fixtures and status/roadmap consistency. Its redigested
negative harness mutates each material contract family, including all minimum
R4 review cases.

Acceptance of this exact R4 design may authorize one P1-e source
implementation slice against V4. It does not authorize installation, startup,
P1-f, operational Redis DB0/VPS, non-loopback Redis, FINAM transport or
POST/DELETE, broker dispatch, runtime-live, real orders, partial fills, or
protective/bracket/multi-leg orders.

The inherited nonblocking source hardening remains deferred:
`0 < child_pid <= u32::MAX`.
