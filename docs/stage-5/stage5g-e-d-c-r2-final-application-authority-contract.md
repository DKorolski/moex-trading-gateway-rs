# Stage 5G-e-d-c R2 - final application authority closure

R2 is a narrow successor to `67e13aeecd3bf0dc33e570770b0e4b90f5fec0cf`.
It preserves the accepted R1 architecture and closes the remaining evidence
provenance and final post-package authority gaps.

The application path still consumes exactly one `Stage5gFreshTruthReduction`
and applies an owned candidate through the canonical order/position transition.
Before `Stage5gValidatedPostApplication` can be issued, a private linear
`Stage5gFreshTruthApplicationSourceProof` is constructed directly from the
consumed e-d-b reduction, clean-restart parent and fresh broker-truth package.
Application evidence is compared field-by-field against that proof, including
fresh package fingerprint, pre-restart fingerprint, reduction pre-semantic
fingerprint, history counts and fresh `captured_at`.

The post-application package is finalized in two phases. The clean-restart
exporter first derives the canonical post-package fingerprint from the parent
snapshot binding, fresh package identity, candidate semantic fingerprint and
post order/position state. Only after that fingerprint is written into evidence
is the final application authority SHA/HMAC calculated. Restore independently
recomputes the post-package fingerprint from persisted evidence and restored
state before accepting the package.

The fourteen failure points remain test-only, but R2 makes the previously
nominal points real: serialization failure is returned from a serializer
adapter call, restore failure is returned from the runtime reconstruction
adapter call, and Policy B records explicit disabled-replay classification
phases. ExactReplay remains disabled until a separately reviewed authenticated
durable tuple ledger exists.

This stage proves one exclusive in-memory candidate-application authority,
source-bound application provenance, a final authenticated post-package
fingerprint and an authenticated persistable package boundary. It does not
implement external durable storage, fsync, CAS, anti-rollback, Redis, FINAM,
broker dispatch, strategy callbacks, runtime-live or real orders. Stage 5G-f
and Stage 6 remain closed.
