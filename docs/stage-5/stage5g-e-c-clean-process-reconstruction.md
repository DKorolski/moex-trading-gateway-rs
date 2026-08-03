# Stage 5G-e-c R1 — source-bound lifecycle reconstruction authority

Base commit: `4296f0621249875f7a2f8cccaa2fbe069cb4bccf`.

R1 preserves the single Stage 5D restart document. `Stage5gCleanRestartSource`
is consumed, its projection is embedded as a versioned checksummed extension
of the accepted Stage 5D canonical restart package, and only package bytes are
returned. Strategy/account/instrument and both runtime config fingerprints are
derived from the consumed source rather than public export input. Restore
strictly decodes both sections, validates the Stage 5G projection and its
cross-binding against the still-unconsumed Stage 5D envelope and fresh runtime,
and only then mutates the fresh runtime with semantic/private/riskgate state.

Four source capabilities remain accepted: timer-ready zero-intent, raw awaiting,
committed exact replay and committed new-package awaiting. The three durable
order/position shapes are intentionally collapsed into the one honest
`order_position_awaiting_committed` lifecycle kind because their persisted
state cannot prove the transient producer label. Timer-ready has exact
zero-intent/callback authority; committed awaiting has exact pre-callback zero
authority. Unknown kinds, self-authorized callback counts, missing replay state,
cross-binding drift, regressive continuation and mismatched slots fail closed.

The projection preserves replay-ledger order/fingerprints/current identity,
package receipt and discriminator, millisecond watermark, sequence, duplicate
counter, continuation checkpoint, callback count, order/trade/position slots,
source attribution and exact broker Decimal serialization.

Focused evidence contains 27 tests: four public export/drop/copy/fresh-restore
round trips, eight package-level rehash-aware semantic negatives and the prior
15 projection/Stage 5D tests, plus compile-fail witnesses for source reuse and
reconstructed-capability cloning. A crate-private redacted observation proves
the restored capability has the fields needed by the next reconciliation step.
Fresh post-restore BrokerTruth,
GRST01–GRST12, Stage 5G-f and every live surface remain deferred.
