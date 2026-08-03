# Stage 5G-e-c — canonical Stage 5D clean-process reconstruction

Base commit: `6995f8dd2ac226eff33b781f575927361fdc2c45`.

This slice introduces no second restart document. `Stage5gCleanRestartSource`
is consumed, its projection is embedded as a versioned checksummed extension
of the accepted Stage 5D canonical restart package, and only package bytes are
returned. Restore strictly decodes those bytes in a new ownership context,
requires a freshly configured runtime, validates its config binding, applies
the Stage 5D semantic state, private extension and authoritative riskgate, and
then returns a new linear `Stage5gCleanRestartedCapability`.

The initial authority supports timer-ready zero-intent, raw awaiting
order/position, committed exact-replay synchronized, and committed NewPackage
awaiting states. Unknown kinds, missing replay state, checksum drift,
regressive continuation, mismatched slots or a mismatched fresh runtime fail
closed without returning a partial capability.

The projection preserves replay-ledger order/fingerprints/current identity,
package receipt and discriminator, millisecond watermark, sequence, duplicate
counter, continuation checkpoint, callback count, order/trade/position slots,
source attribution and exact broker Decimal serialization.

Focused evidence contains 15 tests plus compile-fail witnesses for source reuse
and reconstructed-capability cloning. Fresh post-restore BrokerTruth,
GRST01–GRST12, Stage 5G-f and every live surface remain deferred.
