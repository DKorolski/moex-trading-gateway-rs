# Stage 5G-e-d-b R2 — restart-owned fresh BrokerTruth reducer hardening

Accepted Stage 5G-e-d-a R6 predecessor: `4ece2c7c83ca5575dbca306b5fa29a48dae2bd47`.
Rejected R1 base repaired by this direct successor:
`b0ede8bbdfa99e7b2b06fd7f4f04db128d5f625b`. The original rejected e-d-b
lineage starts at `8a02f2a6b6e27587539d1e4e4717301bf010e6a1`.

R2 keeps the original e-d-b scope: deterministic classification and opaque
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
- pre-restart checkpoint identity plus explicitly untrusted current/historical
  replay hints.

The operational authorizer consumes a separate linear
`Stage5gReviewedOperationalIdentityAuthority`, not the package raw DTO. R2
ships no production issuer for that capability; only a `#[cfg(test)]` issuer
exists for owning evidence. A real config/deployment issuer requires the next
separate review, so broker input cannot mint its own expected identity.

The reducer recomputes both operational and restart-replay commitments before
classification. e-d-b has no authenticated fresh-truth replay ledger, so
caller-provided exact current/historical tuples are hints only and cannot
produce `ExactReplay`. Exact current hints return
`ReplayTupleNotInRestartLedger`; exact historical hints return
`HistoricalReplayNotAccepted`; changed tuples return
`ReplayFingerprintConflict`. Durable exact replay remains deferred to e-d-c.

## Canonical order/position semantics

The restart slot retains typed intent class (`Entry`, `Exit`,
`ProtectiveRepair`, `CancelCleanup`), exact `Decimal` target and pre-position,
request/client/broker IDs and optional attribution commitment. Expected
position is always:

```text
pre_position_qty + signed_fill
```

The same pure helpers are used by the accepted Stage 5G order/position logic
and this reducer. Source runtime quantity authority is still `f64`; therefore
R2 admits only finite integral lots and blocks fractional source quantities
with `SourceNumericAuthorityUnsupported`. No `f64 -> String -> Decimal`
conversion is presented as exact business authority.

Before target filtering, all account orders pass the shared active/unknown/
ambiguous ownership guard. Semantic target matches with a non-exact canonical
`InstrumentId` block as `TargetInstrumentIdentityConflict`. MARKET, LIMIT and
CANCEL retain their source action; a fresh row cannot reinterpret that action.

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

Committed order status/fill quantity and committed trade IDs/payloads are
monotonic. GRST06 requires exact terminal order, trade ledger, position and
ownership identity equality. Safe added fills/trades are GRST11, while any
regression is GRST10/12 and cannot form a candidate.

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

The mandatory gate runs the R2 checker, at least 160 named negative mutations,
preseal, debug/release focused tests, the full runtime-core suite, formatting,
clippy with warnings denied and detached exact R6 acceptance gate.

Stage 5G-e-d-c remains closed until independent R2 acceptance. Stage 5G-f,
Redis consumer groups, FINAM transport, HTTP POST/DELETE, broker dispatch,
runtime-live, real orders and Stage 6 remain closed.
