# Stage 8A-4 durable composition I3 R2

## Scope and lineage

I2 R3 remains accepted at `90f46052cc31cea012437eddb59fb7c3ca5c2320`.
I3 R1 at `a490bbe700c51f0e9c6debd2a007cb9b5061c3d8` was not accepted because a
caller could bypass the private reconciliation authority through a raw batch
writer, current truth/control were not exact request bound, and post-write I/O
failure was not sticky at the owner boundary. I3 R2 closes only those findings
while preserving the accepted V2-first, exact suffix and covering S1 protocol.

I4 remains separately review-gated. ACK/readiness, Redis live consumption,
FINAM POST/DELETE, broker dispatch, runtime-live and real orders remain closed.

## Sealed writer authority

The production Stage7 writer no longer accepts a caller-provided
`Stage6Stage8a4DurableBatch`. It consumes a linear opaque
`Stage8a4DurableWriteAuthority`. Its fields are private; it has no public
constructor and no `Clone`, `Debug`, `Serialize` or `Deserialize` path.

The sole private issuer consumes all of:

- the private I2 candidate;
- current Stage6 request/command authority;
- fresh exact-request FINAM truth;
- current post-effect control evidence;
- current operational identity, runtime config and recovery seal facts.

The dependency direction is `runtime-durable-service -> finam-gateway`, so the
runtime owner can consume the sealed FINAM capability without exposing a raw
cross-crate batch API. The low-level core append is hidden/internal and cannot
be combined with the public Stage7 covering-seal path. Compile-fail examples
pin both the removed raw Stage7 call and the nonconstructible opaque authority.

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
or more exact manifest-prefix V1 records can be covered. A second V2, unrelated
V1, wrong durable binding, stale precondition, torn frame or foreign suffix is
never covered.

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
