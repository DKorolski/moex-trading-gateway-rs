# Stage 8B-P1-e R2 deployable paper supervisor design correction

Status: R2 design-only review candidate. Source implementation is not
authorized until independent acceptance of this exact R2 package.

Accepted immutable predecessor: `c2a9e1246dfdd59f3a6297268de907dedcb19903`
(Stage 8B-P1-d4 governance closure R1).

R1 design commit `693fab351f099b5f16ebb73d4956918b34d8ea1e` is
`HOLD / SUPERSEDED BY R2`. Its independent review SHA-256 is
`eb451af76c9b4030705d900ae0b96fabf62452dde47810e4b007cd4410d58cca`.
R0 remains superseded. R2 preserves all accepted direction from R1 and changes
only the five findings in that review.

## 1. Boundary and inherited contracts

This correction is documentation, immutable data contracts, checkers and
evidence only. It changes no Rust, Cargo, workflow, service unit, deployed
configuration or runtime. The R1 88-row acceptance contract remains mandatory
and is byte-bound as inherited input; the R2 48-row amendment adds requirements
without weakening R1.

The following remain closed: P1-e source implementation, P1-f activation,
operational Redis DB0/VPS, non-loopback Redis, FINAM transport/POST/DELETE,
broker dispatch, runtime-live, real orders, partial fills and protective orders.

R2 adds four normative artifacts:

| Contract | Canonical/file SHA-256 |
| --- | --- |
| `Stage8bP1FirstBootTransactionV1` | `d3f5f8a3aabacf4b5640deae0ce6d87fcf9bc10947367f45676fc9587e474f59` |
| `Stage8bP1eRedisRuntimePolicyV1` | `a3657dfbd10743f93478c1727e118b996c3ad9db5e71986e4378912ab3fdc6f7` |
| 22-row restart outer dispatch v2 | `79936494cc20c460f7a4249ca759fc43da3cca0a31c51d1ad39c26d3a03b79df` |
| 54-row operational continuation matrix v2 | `8160c7a072d2aa10ea1d75677d7a20efd65b76f7098901ee812028e606444739` |

JSON canonicalization is the R1 sorted-key compact UTF-8 projection. CSV
hashes cover exact file bytes. The checker recomputes every digest.

## 2. Crash-safe first-boot transaction

R2 chooses an authenticated incomplete-bootstrap ceremony, not an unchecked
delete/retry policy. The fixed transaction contract is
`stage8b-p1e-first-boot-transaction-v1.json`.

Before creation of the canonical root, `bootstrap` must create, fsync, rename,
parent-fsync and nofollow-reread an HMAC-authenticated transaction marker. It
binds the operational identity, runtime profile and actual config fingerprint,
source bundle hash/generation, source-plan hash, history and riskgate hashes,
candidate semantic ID, canonical root basename and bootstrap-attempt generation.
Ordinary bootstrap is legal only when root, marker, fixed marker temp and
accepted receipt are all absent and its attempt generation exceeds every
authenticated quarantine record for the same identity. Any existing marker,
temp or root requires the exact `bootstrap-recover` ceremony; repeating
ordinary bootstrap is forbidden.

The authoritative classification is the intersection of the HMAC marker and
an fd-anchored nofollow inspection of root, journal, seal, provenance and
receipt. Marker phase alone is never trusted:

| Classification | Sole legal next action |
| --- | --- |
| `NoRoot` | ordinary bootstrap starts one new transaction |
| `UnpublishedMarkerTemp` | unlink only the exact inspected fixed temp and fsync parent |
| `PreparedWithoutRoot` | resume the same authenticated transaction |
| `RootWithoutJournal` | atomically quarantine the proven incomplete root |
| `JournalWithoutSeal` | atomically quarantine the proven incomplete root |
| `CommittedRootResponseLost` | fresh authenticated restart and adoption |
| `QuarantinedIncompleteRoot` | fsync quarantine and move its marker into that exact directory |
| `AdoptedCommittedRoot` | ordinary restart-only `run` |
| `CorruptOrIdentityMismatch` | preserve evidence and fail closed |

Quarantine never calls recursive delete. The exact direct-child canonical root
must be a nonsymlink directory on the same filesystem and its complete shape
must match a closed allowlist. A committed seal or valid accepted receipt makes
quarantine illegal. The operation is `renameat2(..., RENAME_NOREPLACE)` into the
fixed quarantine parent followed by fsync of both parents and movement of the
active marker into that exact quarantine directory. A crash between rename and
marker movement is `QuarantinedIncompleteRoot` and resumes only that finalization.
Quarantined material is retained as nonauthoritative evidence and P1-e never
deletes it.

Response-loss adoption does not invoke first boot again. It authenticates the
transaction, seal and provenance, drops all old process capabilities, performs
a fresh restart from the final canonical path, requires the exact source-produced
`TimerReady` owner and no Redis contact, then atomically persists/rereads the
accepted receipt. The receipt rename plus parent fsync is the adoption commit
point; changing marker phase to `adopted` is best-effort diagnostic state.
Therefore response loss after the receipt but before the marker update is still
classified as `AdoptedCommittedRoot`, not as corruption. Until the receipt
commit succeeds, ordinary `run` is forbidden. Thus no interruption can create
a second durable authority.

The source implementation acceptance must use the release/default-feature
binary and subprocess SIGKILL at three exact hooks:

1. after canonical-root parent fsync and before journal creation;
2. after journal fsync and before initial-seal commit;
3. after seal persist/reread and before bootstrap reports success.

Each restart must prove the sole legal next action, unchanged source/identity
bindings, and absence of duplicate authority.

## 3. Durable first-boot provenance

`Stage8bP1FirstBootProvenanceV1` is an HMAC-covered field of the initial durable
package and first committed seal. It contains exactly:

```text
schema_version
domain
operational_identity_sha256
runtime_profile_sha256
runtime_config_fingerprint_sha256
source_bundle_sha256
source_bundle_generation
source_plan_sha256
history_bars_sha256
riskgate_session_observations_sha256
candidate_semantic_id_sha256
```

It is built only from already authenticated F00-F16 inputs. Bootstrap receipt
and transaction marker repeat the same hashes but cannot replace the durable
record. Every `run` compares every configured binding with this provenance
after local restart and before Redis contact. Missing, duplicate, unknown,
mismatched or unauthenticated provenance blocks startup before Redis.

## 4. Two-level restart dispatch

The R1 enum-only matrix was insufficient because `Stage7bRestartOutcome::Ready`
can coexist with pre-WAL PEL work or a due deterministic timer. R2 replaces it
with two exact levels:

1. `stage8b-p1e-restart-continuation-matrix-v2.csv` is an exhaustive 22-row
   match over every current enum variant with no wildcard;
2. `stage8b-p1e-operational-continuation-matrix-v2.csv` classifies the local
   owner plus authenticated package phase plus PEL/source state plus timer state.

Every row fixes the starting boundary, authority reissue, first transition,
returned owner, completed S06R boundary, source disposition, XACK legality,
fresh-poll legality and readiness legality.

The future `Stage8bP1eCompositeClassifierV2` is itself a closed exhaustive
nested match over the 22 local variants and typed PEL/timer observations. It
may return only one of `OC01..OC54` or one of these closed failures:

```text
MissingExpectedSource
SourceBindingMismatch
UnexpectedAlreadyAcknowledgedSource
AmbiguousPel
TimerBindingMismatch
FreshDeliveryWhileUnresolved
ClaimDeadlineOrCursorExhausted
```

Every failure exits 67 before authority issue, callback, XACK or fresh read.
There is no catch-all route to `Ready`, and adding an enum variant without a
new reviewed matrix row is a compile/checker failure.

### Ready is a composite state

`Ready + no PEL + no due timer` alone reaches one bounded S08 poll. The accepted
P1-d4 pre-WAL states instead behave as follows:

- `P1D4C-031/032`: claim the exact later M10, reissue only the exact schedule
  authority and commit `S_eval` plus terminal zero-intent replacement before
  source XACK;
- `P1D4C-036/037`: same claim/reissue, then preserve the single callback and
  generated-Market chain through its exact `S_truth` before source XACK;
- `P1D4C-041/042`: claim and deterministically reexecute the later-fill path
  through exact terminal truth before source XACK;
- `P1D4C-047`: with no source, reissue only the exact Day-boundary authority and
  commit/reread terminal expiry; there is no XACK;
- `P1D4C-049`: the terminal expiry already exists, so no authority is reissued
  and no XACK occurs.

An exact pending source has priority over a simultaneously due Day boundary.
The source reaches terminal truth and XACK-last first; the timer is then
reclassified against the new authenticated owner. Day expiry executes only if
that owner still has the exact working order and the boundary remains due;
otherwise the timer is proven not applicable. Fresh polling remains forbidden
until the source and this exact timer classification are complete.

An unclaimable exact source stays Degraded under the bounded claim policy. More
than one PEL entry is ambiguous and exits 67 with no XACK or fresh read. Missing
or already-acknowledged source is legal only for rows explicitly naming a
continuous group-frontier proof; all other such combinations fail closed.

### Exact post-evidence boundaries

Every `*AckCommitted` starts at `S_ack`, invokes only its exact ACK resume, then
must commit/reread its exact `S_truth` before XACK-last. It does not complete at
`S_ack`. `P1d3CancelContinuationPending` starts at the committed target
`S_terminal`, invokes `resume_stage8b_p1d3_cancel_continuation_with_redis`, then
must commit/reread exact `S_cancel_recovered` before source XACK-last. Generic
`S_truth` wording is not accepted for that row.

### S08 delivery

S08 may issue one successful `XREADGROUP ... COUNT 1 BLOCK 1000 ... >` only
after S06R is quiescent and the shutdown latch is clear. If it returns an M10,
that same delivery is processed in the same ownership invocation to an
authenticated retained-source boundary or exact terminal replacement/XACK.
`PaperReady` and a second fresh read are both forbidden while it is unresolved.
If the post-read shutdown latch is set, the R1 event rule wins: the delivery
remains pending with no parse, callback, provider or schedule operation.

## 5. Exact Redis runtime policy

`Stage8bP1eRedisRuntimePolicyV1` is hash-bound by supervisor config and maps
explicitly to `Stage8bP1RedisConfig`; `paper_default_auto`, ordinary environment
overrides, unchecked defaults and runtime-selected values are forbidden.

Normative values are:

```text
read_count                 1
claim_count                2
claim_idle_ms              30000
max_claim_pages            1
retention_floor            4096
Redis operation timeout    2000 ms
fresh poll timeout         1000 ms
maximum PEL count          1
claim attempts             12
startup deadline           60000 ms
backoff                    250 ms exponential capped at 5000 ms
```

Configuration validation before Redis contact enforces:

```text
claim_idle_ms
+ maximum_backoff_ms
+ maximum_commands_after_threshold * redis_operation_timeout_ms
<= startup_total_deadline_ms
```

With the frozen values, `30000 + 5000 + 4*2000 = 43000 <= 60000`; an entry
abandoned at process death becomes claimable inside the S06 budget when Redis
responds within the reviewed deadline. Every Redis operation is wrapped by the
global deadline and its 2000-ms per-operation timeout. A nonterminal claim
cursor after the one allowed page exits 67. No timeout or page exhaustion may
fall through to `XREADGROUP >`.

S06 uses `XPENDING` summary, rejects count greater than one, obtains at most two
details, waits without XAUTOCLAIM below the threshold, and executes one
`XAUTOCLAIM ... 0-0 COUNT 2` only at or above the threshold. Zero, multiple,
wrong-binding or nonterminal-cursor results fail closed. Boundary tests use age
`claim_idle_ms-1`, exact `claim_idle_ms` and `claim_idle_ms+1`.

## 6. Finite restart and stale-consumer hygiene

Exit 64 and 66 are in exact `RestartPreventExitStatus`; deployment/config
mistakes do not storm. Other failures use `Restart=on-failure`, `RestartSec=5`,
`StartLimitIntervalSec=600` and `StartLimitBurst=5`. Five full 60-second claim
attempts plus delay still fall inside the 600-second window; the next start is
throttled and the unit remains failed until explicit operator reset.

`XINFO CONSUMERS` is capped at 64 entries; a larger inventory exits 67 without
deletion or fresh read. At most 16 are examined per boot in the exact order:

```text
pending count ascending
then idle milliseconds descending
then consumer name bytes ascending
```

Only zero-pending entries idle for at least 24 hours may be deleted. Successful
deletions disappear, so repeated boots advance through the next deterministic
prefix even when the initial inventory exceeds 16. Nonzero-pending consumers
are never deleted and move only through the accepted claim path.

## 7. Required implementation evidence after R2 acceptance

R2 acceptance may authorize one source slice only. That later slice must add:

- real release/default-feature SIGKILL first-boot transaction tests;
- durable-provenance round-trip and pre-Redis mismatch negatives;
- exhaustive enum and 54-cell operational dispatch tests, including all named
  P1-d4 cells, exact `S_cancel_recovered` and every ACK-to-truth transition;
- claim age minus/exact/plus threshold, incompatible policy, PEL ambiguity,
  timeout/page exhaustion and no-fresh-read proofs;
- S08 delivery/no-second-read and post-latch delivery proofs;
- finite restart-throttle and deterministic `>16` stale-consumer progression;
- inherited P1-d4, workspace debug/release, doctest, strict clippy, no-Redis and
  isolated-Redis gates.

This R2 does not authorize that implementation. Only independent acceptance of
the immutable R2 package can do so. P1-f and every operational/live surface
remain separate later gates.

The previously deferred bound `0 < child_pid <= u32::MAX` remains nonblocking
and requires separately authorized source hardening; it is not hidden inside
this design-only correction.
