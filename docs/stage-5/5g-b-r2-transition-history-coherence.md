# Stage 5G-b R2 — transition-history coherence

R2 is a single successor to
`00d158978904c177828ff2a330b1f3c1bfb4bb10`. It closes only the findings from
the independent R1 review. It does not open Stage 5G-c or any live surface.

## Lifecycle rules

An exact no-send proof (`Expired` + `ExpiredCommand`, without a broker order
ID) is accepted only directly from `Waiting`, or after an unproved `Expired`
without reason and without broker ID. It cannot erase `Submitted`, `Accepted`,
`Recovered`, `Timeout`, `UnknownPending`, `Error`, manual intervention,
resolution, or any observed broker identity. Contradictory evidence is retained
in a manual-intervention capability, consumes its sequence/time watermark, and
never enters Stage 5C.

After broker order ID `A` has been observed, a callback-safe terminal ACK must
carry exact `A`. A missing or conflicting ID blocks before callback and retains
the prior identity.

ACK `total_sequence` remains strictly increasing. ACK receive time is
non-decreasing: equal timestamps are allowed, reversed timestamps block. The
watermark is part of schema/fingerprint v3.

## Production integration evidence

Four structural tests use a real Stage 5C settled Market batch and the public
Stage 5G API. They cover direct acceptance, submitted/recovered resolution,
pre-callback ownership retention, contradictory no-send evidence, and exact
duplicate replay. Controlled current time is used only to keep the public
Stage 5C capability fresh; it is excluded from deterministic golden evidence.
No constructor or bypass was added to frozen Stage 5C.

## Boundary

Redis command consumption, FINAM/HTTP transport, dispatch, order/trade/position
lifecycle, runtime-live, real orders, Stage 5G-c, and Stage 6 remain closed.
Independent acceptance of R2 is required before the next transition.
