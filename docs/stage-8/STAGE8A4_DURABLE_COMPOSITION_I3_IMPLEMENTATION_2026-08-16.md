# Stage 8A-4 durable composition I3 R5

## Scope and lineage

I2 R3 remains accepted at `90f46052cc31cea012437eddb59fb7c3ca5c2320`.
I3 R1 at `a490bbe700c51f0e9c6debd2a007cb9b5061c3d8` was not accepted because a
caller could bypass the private reconciliation authority through a raw batch
writer, current truth/control were not exact request bound, and post-write I/O
failure was not sticky at the owner boundary. I3 R2 at
`62e5e0509adb9cceb1d9947b5b3f92120e2f19ea` preserved those mechanics but was
not accepted: it exported a hidden raw core append, regressed the accepted
Stage8A1 R3 owner authority, inverted the broker-neutral dependency direction,
and made restart depend on the lost I2 process object. I3 R3 closes those
authority-topology findings while preserving V2-first, exact suffix and
covering S1 behavior.

R3 was not accepted because its public sealer remained forgeable and an
incomplete V2/suffix tail advanced ordinary Ready/S1. R4 authenticates the
FINAM issuer with Ed25519 against a public key pinned by the Stage 6/7
operational identity, then creates the lifecycle-key HMAC seal only inside
strategy-runtime-core. Incomplete or complete-but-uncovered restart remains in
a typed Pending owner, and source/truth/control evidence is bound into the
one-shot writer commitment.

R4 at `44030688053c41a2179bb0f7bc59458c408348fd` was not accepted because its
restart entry retained the pre-crash `Stage8ExecutionCapability` and
`Stage8a1OperationalAuthorityIssuer`. R5 removes both process-local inputs.
The Pending owner now reopens the pinned Stage8A1 authority root, recomputes
the deterministic historical arm nonce, reads the existing immutable arm
registration, verifies its Ed25519 signature against the operational-identity
pinned issuer key, reconstructs private post-effect provenance, and only then
permits exact suffix recovery. It never registers or remints an arm.

I4 remains separately review-gated. ACK/readiness, Redis live consumption,
FINAM POST/DELETE, broker dispatch, runtime-live and real orders remain closed.

## Sealed writer authority

The production Stage7 writer no longer accepts a caller-provided
`Stage6Stage8a4DurableBatch`. It consumes a broker-neutral validated writer
entry whose Ed25519 signature covers the exact request, command, batch,
operational identity, runtime config, S0 seal and source/truth/control
bindings. Apply compares the attesting public key with the authenticated
operational identity, then mints and verifies the core-private
`Stage6Stage8a4SealedWriteAuthority` using a domain-separated lifecycle-key
HMAC. Neither layer has a caller-controlled `issue`/`seal`, `Clone`, `Debug`,
`Serialize` or `Deserialize` path.

The Stage8A1 issuer reads a pinned regular signing-key file, rejects symlinks,
requires no group/other permission bits, derives its public key and exact
compares it with accepted configuration and the Stage 6/7 operational
identity. Diagnostics and orchestration never receive the private key or a raw
batch writer.

The sole private issuer consumes all of:

- the private I2 candidate;
- current Stage6 request/command authority;
- fresh exact-request FINAM truth;
- current post-effect control evidence;
- current operational identity, runtime config and recovery seal facts.

The callable no-send production operation is
`reconcile_persist_and_cover_stage8a4`. It obtains current Stage7B authority,
builds the private I2 candidate from the accepted record and current
reconciliation admission, issues the sealed I3 authority, writes V2 plus the
exact suffix and returns only after authenticated covering S1. No public API
accepts a raw V2 transition or `Stage6Stage8a4DurableBatch` for storage.

The dependency direction is `finam-gateway -> runtime-durable-service`.
The runtime and strategy core remain broker-neutral and contain no FINAM,
reqwest or broker transport dependency. `cargo tree` still reports the
pre-existing broker-neutral `broker-core::order_path` SQLite store; it is not
introduced through FINAM and splitting that baseline persistence is outside
this narrow R3 topology correction. The low-level raw append is
`pub(crate)` and is not re-exported. A separate temporary external compile-fail
crate proves that neither the current nor historical raw Stage8A4 append name
is callable outside strategy-runtime-core.

The accepted Stage8A1 R3 topology is restored unchanged: durable authority is
derived through the current `Stage7bRecoveryReadyOwner`, lifecycle key and
current disk seal; readiness uses `Stage7bCompositeReadinessSnapshot`; PLACE
and CANCEL continuation both revalidate through current owner authority. There
is no caller-supplied seal/op-id constructor or local readiness lookalike.

## Exact request truth and control binding

Writer-entry truth binds the exact request ID, account, instrument, durable
request binding and canonical truth digest. Its validity ends at the oldest
source response plus the admitted freshness allowance; the writer compares the
current clock to `writer_entry_valid_until`. Identical counts from another
account or request therefore cannot authorize persistence. Changed unknown,
orphan, order or position state changes the private safety projection and is
rejected.

Control evidence is exact-compared against the current owner using operational
identity, runtime-config fingerprint, accepted command digest, authority scope,
arm-registry binding and current-control binding. `StopRequested` and
stale/unreadable control may still permit persistence of already-established
truth, but never grant send, retry, ACK or readiness authority.

## Durable protocol retained

Under the held writer lease, S0 is reread and authenticated, Stage6 is
refreshed, the exact accepted command and sole dispatch are reconstructed, and
the four-field CAS checks frontier/checkpoint, seal generation, seal
fingerprint and request-state fingerprint. A new batch is written V2-first,
then only the exact verified missing manifest suffix. Every frame append is
fsync-backed. Same stable key and same payload resumes; a different payload is
a hard conflict. Success requires complete replay, F1, covering S1, and an
authenticated reread of S1.

Restart recovery remains narrow: only S0 followed by one exact I3 V2 and zero
or more exact manifest-prefix V1 records can be covered. This includes an
already complete F1 whose covering S1 was not committed. The production
restart entry reconstructs the exact V1 suffix from persisted canonical V2,
verifies every persisted prefix record against the manifest, appends only the
missing records, and commits/rereads S1. It therefore does not require the lost
I2 candidate, private outcome, pre-crash execution capability, retained issuer
object or a test writer. The recovery-only issuer is rebuilt from the Pending
owner and current pinned authority root. Historical provenance comes from a
versioned canonical arm-registration record selected by the deterministic
one-arm nonce. Its issuer signature covers durable identity, accepted command,
operational/runtime identity, authority scope and frozen policy. Missing,
replaced, non-canonical, unsigned or mismatched registration fails closed
without suffix append or S1. A second V2, unrelated V1,
wrong durable binding, stale precondition, torn frame or foreign suffix is
never covered.

Direct integration tests traverse the real production issuers and entries.
The normal path proves RequestAccepted + Dispatch through current read-only
truth to V2, two-record exact suffix, F1 and S1. Recovery tests create
deterministic crash boundaries through a feature-gated I/O fault injector, then
call the actual production recovery entry for V2-only, partial-suffix and
complete-before-S1 states. Repeated restart stays Pending; successful repair
returns ordinary Ready. The fault injector cannot append a batch or construct
a writer authority.

The mandatory R5 witness crosses an actual process boundary. Process A creates
the normal one-shot arm, writes V2 and is killed while its owner and issuer are
still live. Process B receives no Rust object from A; it reopens the durable
root and authority files, observes Pending, reuses the sole existing arm file,
appends the two missing suffix records, commits/rereads S1 and restarts Ready.
Focused negative tests remove or structurally preserve but cryptographically
alter that arm file and prove the durable state remains Pending. Registry entry
count is unchanged by every recovery.

## Sticky mutation uncertainty

The file backend marks durability uncertain before the first write attempt.
A pure `BeforeFrameWrite` failure is classified as no mutation and does not
poison the owner. Failures after header, partial payload, frame hash, before
sync, during sync, or during post-write rescan are classified as
`JournalMutationMayHaveOccurred`.

The Stage7 owner then sets `journal_mutation_uncertain` sticky. In that process
`recovery_ready` is false and command authority, another append, seal advance
and all lifecycle admission fail. Only close/reopen and authoritative restart
scan can resolve disk state. The fault matrix covers both V2 and suffix writes;
existing seal-commit uncertainty remains sticky as well.

## Closed surfaces

The durable receipt grants no ACK or readiness capability. I4 remains
separately review-gated. Redis command consumption, XACK, FINAM POST/DELETE,
transport send, retry/resend/re-arm, broker execution, runtime-live, real
orders, Stage 8A-5+ and Stage 8B remain closed.
