# Stage 5G-e-c R4 — authenticated lifecycle commitment

Reviewed predecessor: `2394dbcd15d953e1799e07f7c903fdb3b072fc3f`
(`Stage 5G-e-c R3`, rejected as submitted).

R4 chooses threat-model A from the remediation assignment: the canonical
Stage 5D restart package is authenticated with HMAC-SHA256 under an
operator-managed 32-byte key. The key is supplied to export and restore by the
operator boundary, is opaque and non-cloneable in the public API, and is never
serialized into the restart package. The package retains the R3 unkeyed anchor
as an internal consistency field; that field is explicitly not the trust root.

The HMAC authenticates the complete source-authority anchor. That anchor binds
strategy/account/instrument identity, config fingerprints, Stage 5D payload and
lifecycle watermarks, riskgate identity, revision/generation, runtime state,
the sole canonical lifecycle summary, checkpoint, order-position projection,
and the complete TimerReady Stage 5C settlement authority. Restore validates
the strict projection, the in-package anchor and the operator-keyed HMAC before
any runtime state is reconstructed or mutated.

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

The acceptance-defining adversarial witness coherently changes summary,
recovery projection and identity, history and settled batch, and a valid later
checkpoint. It then reseals every unkeyed lifecycle digest, the Stage 5D
anchor and envelope checksum, the extension checksum and outer package
checksum. Restore returns the exact typed
`AuthenticatedLifecycleCommitmentMismatch` because the attacker cannot
recompute the operator-keyed HMAC.

Key rotation defines a new operator commitment epoch: a package authenticated
under an older key fails against the newer key. Within one key epoch, storage
rollback prevention remains an operator/storage responsibility; R4 does not
claim a monotonic external store.

Fresh BrokerTruth reconciliation, GRST01–GRST12, Stage 5G-f, Redis/FINAM/HTTP,
broker execution, runtime-live, real orders, main merge and deployment remain
closed.
