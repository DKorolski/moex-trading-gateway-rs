# Stage 5G-e-c R5 — full canonical restart package MAC coverage

Reviewed predecessor: `3a9fd1106a064ed6c29b1a378cbc02da90b2efc1`
(`Stage 5G-e-c R4`, rejected as submitted because its HMAC covered only the
Stage 5G source projection).

R5 keeps the accepted R4 operator-key boundary and authenticates one versioned
canonical semantic projection of the complete Stage 5D + Stage 5G restart
package. The external key remains exactly 32 bytes, opaque, non-cloneable and
non-serializable; its bytes are zeroized on drop and are never stored in the
package.

`Stage5gAuthenticatedRestartPackageCommitmentV1` binds package schema and
checkpoint state, snapshot identity/revision/generation/timestamp, timestamp
policy, complete Stage 5D envelope semantics, source build identity, runtime
semantic/private state, lifecycle watermarks, recovery indexes, riskgate
persistence and exact riskgate evidence. It also binds the complete Stage 5G
source projection, summary, checkpoint, order-position authority and
TimerReady settlement/recovery/history projection.

Only circular transport-integrity fields are excluded: the Stage 5D payload
checksum is cleared, the HMAC field is absent, and extension/outer package
checksums are outside the canonical projection. Export computes the HMAC first
and transport checksums afterward. Restore reconstructs the same projection
from strictly decoded package data and verifies the HMAC before runtime state
reconstruction or mutation.

TimerReady no longer persists duplicate `source_summary` or
`source_checkpoint` values. Its sole summary and checkpoint are produced from
the linear source capability during export and are covered by the authenticated
source projection. The source authority also contains a versioned recovery
receipt projection. Restore recomputes `recovery_receipt_identity_sha256` from
all frozen recovery fields instead of accepting a free 64-hex string.

History remains nonempty, ordered and source-bound, and must end in the exact
settled batch. The authenticated projection binds its complete rows and state
fingerprints. The same commitment binds replay ledger/current identity,
sequence and duplicate counters, BrokerTruth receipt/discriminator data, and
the continuation checkpoint.

Twelve acceptance-defining attacks coherently mutate watermarks, revision,
generation, persisted time, a compatible source build, runtime-private state,
recovery indexes, riskgate evidence/persistence, package identity, or the
complete envelope plus extension. Every ordinary checksum and unkeyed
lifecycle hash is resealed while the operator tag remains unchanged. All
twelve reach and fail at the exact typed
`AuthenticatedLifecycleCommitmentMismatch` boundary.

Key rotation defines a new operator commitment epoch: a package authenticated
under an older key fails against the newer key. Within one key epoch, storage
rollback prevention remains an operator/storage responsibility; R5 does not
claim a monotonic external store.

Fresh BrokerTruth reconciliation, GRST01–GRST12, Stage 5G-f, Redis/FINAM/HTTP,
broker execution, runtime-live, real orders, main merge and deployment remain
closed.
