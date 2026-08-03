# Stage 5G-e-c R3 — independently anchored TimerReady source authority

Reviewed predecessor: `f2f5b1508171632d2e4b211eae79ee6bf3b18178`
(`Stage 5G-e-c R2`, rejected as submitted).

R3 preserves the accepted R2 implementation while separating the trust
domains. The Stage 5G source-authority digest is now committed into the
canonical Stage 5D persistence envelope before the final Stage 5G projection
is serialized. The anchor therefore participates in the Stage 5D payload,
envelope and package checksums; it is not stored only inside the rehashable
Stage 5G extension.

The anchored source projection covers the source-owned binding, lifecycle
kind, callback and zero-intent authority, runtime-state fingerprint, complete
summary, checkpoint, order-position state and TimerReady Stage 5C settlement.
For TimerReady this includes the settled zero-intent batch, ordered history,
recovery-receipt identity and source checkpoint. Restore recomputes this digest
from the strict Stage 5G projection and compares it with the independent Stage
5D envelope anchor before any runtime mutation.

The TimerReady validator also requires a nonempty history whose final row is
the exact settled batch, exact request/intent cardinality, canonical lowercase
SHA-256 identities and monotonic event/history timestamps. These checks reject
impossible source projections independently of the outer anchor.

The package mutation harness now reseals both checkpoint envelopes, the full
source-authority digest, lifecycle checkpoint, source lifecycle commit,
lifecycle proof, Stage 5G extension checksum, Stage 5D envelope checksum and
outer package checksum. Coherent mutations of both summary copies, recovery
identity, settled history, both checkpoint copies and a same-instance complete
extension graft still fail because the original Stage 5D anchor is unchanged.

Compatibility is additive: ordinary Stage 5D packages omit the optional anchor
and retain their prior serialized form. A Stage 5G clean restart requires the
anchor and fails closed when it is absent, malformed or inconsistent.

Fresh BrokerTruth reconciliation, GRST01–GRST12, Stage 5G-f, Redis/FINAM/HTTP,
broker execution, runtime-live, real orders, main merge and deployment remain
closed.
