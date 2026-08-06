# Stage 5G-e-d-c R1 — application authority closure

R1 is a bounded correction based on `18240b26a5bea77ea71c851f72a644706a7e0b57`.
It proves one exclusive in-memory owner for fresh-truth candidate application
and authenticated post-application package creation.

The only entry consumes `Stage5gFreshTruthReduction`. A private constructor
creates `Stage5gValidatedPostApplication` only after preflight, the shared
canonical transition, independent candidate/post equality and global-state
invariants. The clean-restart exporter consumes that token by value. It no
longer accepts raw state, identifiers or caller-built evidence.

Candidate, post-state and restored-state semantic projections are constructed
independently. Application evidence is bound both into lifecycle hashes and an
inner domain-separated HMAC authority seal. This makes a coherently resealed
outer package insufficient to forge application authority.

Fourteen test-only failure points enter their named operation boundaries.
Serialization, authentication and restore failures occur inside narrow
adapters around the real operations. GRST01–12 each have one named full-chain
witness. Policy B remains in force: ExactReplay is disabled without an
authenticated durable tuple ledger.

This stage does not provide an external durable journal, fsync, CAS,
anti-rollback storage, Redis, FINAM transport, broker dispatch, callbacks,
runtime-live or real orders. Stage 5G-f and Stage 6 remain closed.
