# Stage 5G-e-d-b R1 — restart-bound fresh BrokerTruth reducer

Accepted Stage 5G-e-d-a R6 predecessor: `4ece2c7c83ca5575dbca306b5fa29a48dae2bd47`.
Rejected e-d-b base repaired by this direct successor:
`8a02f2a6b6e27587539d1e4e4717301bf010e6a1`.

R1 keeps the original e-d-b scope: deterministic classification and opaque
candidate construction only. It does not apply a candidate, invoke a strategy
callback, mutate runtime or Stage 5D persistence, publish Redis data, call
FINAM/HTTP, dispatch an order or open runtime-live.

## Restart-bound authority

`bind_stage5g_fresh_truth_to_clean_restart` is the only production constructor
for `Stage5gRestartBoundFreshBrokerTruthPackage`. The type is non-Clone and
non-serializable. Its domain-separated commitments cover:

- authenticated source lifecycle and lifecycle-authority commitments;
- the complete restart checkpoint and replay ledger;
- all twelve operational identity fields: broker, account, strategy definition
  and instance, deployment and generation, gateway instance, config and
  instrument-map fingerprints, market-data and command-consumer generations,
  and exact target `InstrumentId`;
- pre-restart, last-reconciled and bounded historical replay authority.

The reducer recomputes both operational and restart-replay commitments before
classification. Exact current or historical replay requires an exact
package/epoch/fingerprint tuple. A changed fingerprint maps to GRST10 with
`ReplayFingerprintConflict`; unknown historical evidence remains blocked.

## Canonical order/position semantics

The restart slot retains typed intent class (`Entry`, `Exit`,
`ProtectiveRepair`, `CancelCleanup`), exact `Decimal` target and pre-position,
request/client/broker IDs and optional attribution commitment. Expected
position is always:

```text
pre_position_qty + signed_fill
```

The same pure helpers are used by the accepted Stage 5G order/position logic
and this reducer. No `f64 -> String -> Decimal` conversion is an authority.

Trade linkage is exact only when at least one present client/broker ID matches
and no present ID conflicts. `None == None` is not a match. Selection and
candidate self-consistency use the same helper.

## Status and completeness matrix

- TimerReady continues only with complete positions, no target orders/trades
  and exact committed target quantity.
- New/Working with any fill or linked trade is a terminal contradiction.
- PartiallyFilled requires complete position truth, exact trade sum, exact
  source-relative position and compatible intent class.
- Filled waits when positions are incomplete; complete empty positions are
  accepted only when the expected post-position is exactly flat.
- Rejected with a fill or linked trade is inconsistent.
- Canceled/Expired with a fill require complete exact post-position; zero fill
  requires unchanged pre-position.

Incomplete sections never mean broker absence. Candidate self-consistency
rechecks commitment, linkage, intent class, source-relative quantity, terminal
rules and required section completeness.

## Executable evidence

GRST01–GRST12 remain frozen and covered by the pure reducer matrix. Owning
boundary tests additionally run authenticated export → byte decode/restore →
fresh package validation → restart binding → owning reduction for every GRST
outcome. They cover Entry partial/fill, Exit-to-flat, terminal partials,
rejected-with-fill, TimerReady contradictions, exact current/historical replay,
full operational mismatch and generated-intent escrow retention.

Canonicalization is exercised with more than one trade row. Identical owning
inputs produce byte-identical redacted evidence in sequential, reversed-row and
parallel runs. Exact replay keeps pre/post semantic fingerprints identical.

The mandatory gate runs the R1 checker, at least 120 named negative mutations,
preseal, debug/release focused tests, the full runtime-core suite, formatting,
clippy with warnings denied and detached exact R6 acceptance gate.

Stage 5G-e-d-c remains closed until independent R1 acceptance. Stage 5G-f,
Redis consumer groups, FINAM transport, HTTP POST/DELETE, broker dispatch,
runtime-live, real orders and Stage 6 remain closed.
