# Stage 8B-P1-e R3 deployable paper supervisor design correction

Status: R3 design-only review candidate. Source implementation is not
authorized until independent acceptance of this exact R3 package.

Accepted immutable predecessor: `c2a9e1246dfdd59f3a6297268de907dedcb19903`
(Stage 8B-P1-d4 governance closure R1).

R2 design commit `3aaed81da4f1a558b4d31f4d3a169ddceca61e6f` is
`HOLD / SUPERSEDED BY R3`. The independent R2 review SHA-256 is
`ea5828a52e2098da1fc2f4eeb52c37e40f9328e634f42840e64dc703e2b15b26`.
R1 and R0 remain superseded. R3 retains the accepted R2 direction and changes
only the four P1 findings in that review.

## 1. Boundary and active contract

R3 changes documentation, immutable data contracts, checkers, evidence and
handoff tooling only. It changes no Rust, Cargo file, workflow, systemd unit,
deployed configuration or runtime state.

R1 and R2 matrices remain immutable evidence; they are not simultaneously
normative in full. The sole active merge is
`Stage8bP1eAcceptanceContractV3`, encoded by
`stage8b-p1e-active-acceptance-contract-v3.json`:

```text
all R1 rows except the exact R1 superseded set
+ all R2 rows except the exact R2 superseded set
+ all 43 R3 rows
= 160 active REQUIRED rows
```

Every excluded row appears exactly once in the supersession map and every
replacement is active. The checker constructs the active row set and a
semantic-key map. Two active rows with the same semantic key and unequal
semantic values are rejected. This explicitly resolves:

- `P1ER1-043` (`Ready` has no continuation) in favor of composite observable
  classification followed by at most one quiescent S08 poll;
- `P1ER1-074` (300-second start-limit window) and `P1ER2-044` in favor of the
  one active exact value `StartLimitIntervalSec=600`, `StartLimitBurst=5`;
- the R2 all-88 inheritance claim;
- the R2 result-discriminating 54-row operational matrix;
- initial-seal-only provenance storage.

The R1 mode rows remain active and compatible: `validate-config`, ordinary
`bootstrap` and ordinary `run` retain their exact meaning. R3 adds a fourth,
strictly administrative `bootstrap-recover` mode with its own reviewed
systemd credential boundary; it does not broaden any of the first three.

The following remain closed: P1-e source implementation, P1-f activation,
operational Redis DB0/VPS, non-loopback Redis, FINAM transport/POST/DELETE,
broker dispatch, runtime-live, real orders, partial fills and protective
orders.

Canonical/file digests of the R3 contracts are:

| Contract | SHA-256 |
| --- | --- |
| active acceptance contract V3, canonical JSON | `6447082e81e0adee7aed5c3336231e44ee040b5d0ab47e454a47b0de0725cd90` |
| R3 acceptance matrix, exact bytes | `7da8c390d6474e32b0b680892cce335eb528d0d633fcc1e6f7ddedf0d1c7b342` |
| first-boot transaction V2, canonical JSON | `8008789da8020e0e6470177706031eeac19a64050de091bc94dcfa3328646f51` |
| first-boot receipt V1, canonical JSON | `0241146a81f0e8fdd51e78c1584e3b2ffe03337b07e976dd05417b16c990464f` |
| authenticated restart package V2, canonical JSON | `114a2c746ade4ee024b5bf0f81441d712549172fe0ab7bc2724949ca73725b1a` |
| source acquisition seam V1, canonical JSON | `05aaee3c1e27f0b9765c4e4755077a3722a1949c6fa5753234a2690ad4ef2ce0` |
| 52-row pre-transition matrix V3, exact bytes | `e24860e874d264ab0dec35ddf3e91e169204b24076347beed34d4134c0d70ce0` |

JSON canonicalization is sorted-key compact UTF-8. CSV hashes cover exact
file bytes.

## 2. Exact first-boot recovery execution boundary

`Stage8bP1FirstBootTransactionV2` supersedes transaction V1. Its classifier is
the intersection of an authenticated marker and fd-anchored nofollow
inspection of marker temp, root, journal, seal, package V2, provenance,
receipt temp, receipt and quarantine. Its ten classifications each name one
legal action and one response-loss interpretation.

Ordinary bootstrap remains:

```text
stage8b-p1-paper-supervisor bootstrap CONFIG CREATE_NEW_STAGE8B_P1_DURABLE_ROOT
```

Recovery has exactly this binary grammar:

```text
stage8b-p1-paper-supervisor bootstrap-recover \
  /etc/moex/stage8b-p1-paper-supervisor.json \
  TRANSACTION_ID.ACTION \
  RECOVER_EXISTING_STAGE8B_P1_FIRST_BOOT_V2
```

`TRANSACTION_ID` is exactly 64 lowercase hexadecimal characters. `ACTION` is
one of:

```text
remove-marker-temp
resume-prepared
quarantine-root
finalize-quarantine
remove-receipt-temp-and-adopt
adopt-committed-root
```

The only accepted production invocation is:

```text
systemctl start \
  stage8b-p1-paper-bootstrap-recover@TRANSACTION_ID.ACTION.service
```

The installed `stage8b-p1-paper-bootstrap-recover@.service` has the fixed
`ExecStart` from the transaction contract. It obtains only
`stage8b-p1-lifecycle.key` through `LoadCredential` and
`CREDENTIALS_DIRECTORY`, permits only `AF_UNIX`, uses `PrivateNetwork=true`,
and can write only the canonical durable parent. Absence or invalidity of the
credential exits 66 before filesystem mutation. Direct invocation, an
environment-selected key, arbitrary config path and caller-selected
credential path are not accepted deployment boundaries.

The command always performs a fresh authenticated classification first. Both
the transaction ID and action selector must match the sole action for that
classification before mutation. A stale selector is never reinterpreted as a
new action. Every response-loss restart reclassifies from disk and executes
only the next missing idempotent step.

Quarantine retains R2's accepted rules: no recursive or automatic deletion,
same-filesystem direct-child proof, closed shape, no committed seal or valid
receipt, `renameat2(..., RENAME_NOREPLACE)`, parent fsync and retained
nonauthoritative evidence.

## 3. Authenticated receipt and exact adoption predicate

The authoritative receipt is `Stage8bP1FirstBootReceiptV1`, frozen in
`stage8b-p1e-first-boot-receipt-v1.json`. Its HMAC covers canonical fields that
bind:

- transaction ID and bootstrap-attempt generation;
- operational identity and canonical root identity;
- authenticated package V2 bytes;
- byte-identical first-boot provenance;
- committed seal generation and commitment;
- Stage 6 checkpoint;
- adoption predicate version and exact ready-owner digest.

It uses one fixed `0600`, nofollow, single-link, root-owned temp; `write_all`,
`sync_all`, `renameat2(..., RENAME_NOREPLACE)`, parent fsync and nofollow
reread/full cross-validation. Rename plus parent fsync is the commit point.
An exact existing receipt is idempotent success; any nonexact existing receipt
is corruption and blocks before ordinary run and Redis.

A crash after receipt-temp sync but before rename is
`CommittedRootReceiptTemp`. Recovery may only authenticate and inspect that
fixed temp, unlink it, fsync the parent and perform a new fresh adoption. A
crash after receipt rename plus parent fsync is already
`AdoptedCommittedRoot`, even if marker phase remains `seal_committed`.

The fresh final-path restart must return
`Stage7bRestartOutcome::Ready(Box<Stage7bRecoveryReadyOwner>)`. The exact
adoption predicate is not merely the word `TimerReady`; it cross-validates:

```text
operational identity exact
authenticated package schema exactly V2
first-boot provenance exact
committed seal generation and commitment exact
Stage 6 checkpoint exact
authenticated embedded Stage5G phase TimerReady
pending lifecycle owner count 0
pending request count 0
pending deferred timer count 0
journal_mutation_uncertain false
seal_commit_uncertain false
```

The domain-separated digest of those fields is stored in the receipt. No
other `Stage7bRestartOutcome` variant is adoptable. Adoption performs no Redis
operation and never invokes first boot again.

## 4. Durable provenance lifetime through replacement packages

P1-e roots use `Stage6dAuthenticatedRestartPackageV2`, frozen in
`stage8b-p1e-authenticated-restart-package-v2.json`. V2 adds both
`first_boot_provenance_v1` canonical bytes and their SHA-256 to the package.
The restart commitment and lifecycle-key HMAC cover that digest together with
the Stage5G package, Stage 6 checkpoint and operational identity digests.

`seal_stage6d_restart_package_v2` requires authenticated first-boot
provenance. `advance_stage6d_restart_package_v2` and every Stage5G replacement
constructor copy the exact canonical provenance bytes from the authenticated
current V2 package. Callers cannot provide, rebuild, omit or substitute the
record.

P1-e `run` accepts only V2. Missing, duplicate, unknown-version, changed,
cross-identity or downgraded provenance blocks before Redis. The required
round trip is:

```text
bootstrap V2
→ commit S_eval replacement
→ commit S_ack replacement
→ commit S_truth replacement
→ process restart
→ byte-identical provenance and digest validated before Redis
```

The old V1 package remains historical source evidence; it is not accepted as
a P1-e deployed-root package and cannot silently migrate during ordinary
`run`.

## 5. Observable pre-transition classifier

The active matrix is
`stage8b-p1e-operational-pretransition-matrix-v3.csv`. It contains 52 unique
cells. The outer 22-variant matrix V2 remains active and exhaustive; V3 changes
only its composite Ready refinement.

The pre-transition classifier may inspect only:

```text
authenticated local restart variant
authenticated package phase
PEL count and exact source binding
pending age relation to claim threshold
claim cursor terminality
due timer binding
shutdown latch
```

It cannot inspect or manufacture `zero_intent`, `one_intent`, `later_filled`,
a callback result, a cloned runtime or speculative state. Consequently, the
old OC02/OC03/OC04 result-discriminating cells are replaced by one OC02:

```text
Ready + one exact claimable later M10 + no due timer
```

Two source entries with identical observable Redis shape always select the
same cell even if their later strategy outcomes differ. Branching occurs only
after the one real callback returns a typed outcome.

## 6. Linear already-acquired delivery owner

The exact seam is `Stage8bP1eClaimedM10DeliveryV1`, frozen in
`stage8b-p1e-source-acquisition-seam-v1.json`. It is neither `Clone`, `Copy`,
serializable nor reconstructible.

S06 owns the bounded `XPENDING`/threshold policy and performs at most the one
legal `XAUTOCLAIM` that creates this owner. S08 performs one `XREADGROUP` and
moves the returned delivery directly into the same linear owner. New narrow
facades consume it exactly once:

```text
process_claimed_working_limit(..., Stage8bP1eClaimedM10DeliveryV1)
process_claimed_ready_source(..., Stage8bP1eClaimedM10DeliveryV1)
```

Neither facade may call `XPENDING`, `XAUTOCLAIM` or `XREADGROUP`. Existing
high-level methods that acquire internally cannot be called after S06/S08 has
acquired a delivery.

Only after the delivery is moved into a facade may its one real callback
produce `ZeroIntentTerminal`, `OneIntentPrepublication`,
`LaterFilledTerminal` or a closed blocked outcome. No preview callback exists.
Instrumentation must prove one acquisition and one callback total from S06 or
S08 entry through the returned terminal/retained boundary.

If the shutdown latch is set after S08 delivery, the owner is consumed only by
`retain_after_shutdown_latch`: the same entry remains pending and there is no
parse, callback, provider, schedule, XACK or second acquisition.

For simultaneous pending source and due Day timer, the linear source reaches
its exact terminal boundary and XACK-last first. The timer is then
reclassified against the returned authenticated owner and runs only if the
same working order still exists and remains due.

All accepted exact terminal rules remain: each AckCommitted continuation
persists/rereads its exact `S_truth`, and cancel continuation persists/rereads
exact `S_cancel_recovered`, before source XACK-last.

## 7. Targeted R3 negative evidence

The R3 negative harness redigests mutated artifacts and must reject at least:

1. reactivation of `P1ER1-043` beside its unequal Ready replacement;
2. reactivation of `P1ER1-074` beside the 600-second policy;
3. unmanaged/direct recovery or missing `CREDENTIALS_DIRECTORY`;
4. stale recovery selector and cross-transaction/cross-attempt receipt;
5. altered seal/provenance binding in the receipt;
6. provenance dropped after an S_eval/S_ack/S_truth replacement;
7. package V2 downgraded to V1;
8. `zero_intent` or preview callback added to pre-transition classification;
9. a claimed continuation allowed to call `XAUTOCLAIM` or `XREADGROUP`;
10. S08 delivery processed through an acquiring high-level method.

The inherited R1 and R2 negative harnesses remain evidence for their
non-superseded contracts. R3's active-contract checker is authoritative for
supersession and semantic conflict.

## 8. Source implementation boundary after acceptance

Independent acceptance of this exact R3 design may authorize one P1-e source
slice implementing the active V3 contract. That later slice must include
release/default-feature subprocess SIGKILL coverage, receipt and provenance
round trips, exact systemd template evidence, exhaustive 22-row outer and
52-row pre-transition dispatch, linear acquisition instrumentation, no-preview
callback tests, inherited P1-d4 gates, workspace debug/release, doctests,
strict clippy, no-Redis and isolated-Redis tests.

This R3 does not authorize that implementation. P1-f and every operational or
live surface remain separate later gates.

The deferred bound `0 < child_pid <= u32::MAX` remains nonblocking and needs
separately authorized source hardening; it is not hidden inside R3.
