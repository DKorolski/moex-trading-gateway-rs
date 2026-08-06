# Stage 5G-e-d-b R5 — terminal idempotency and monotonic supersession closure

Accepted Stage 5G-e-d-a R6 predecessor: `4ece2c7c83ca5575dbca306b5fa29a48dae2bd47`.
Rejected R2 base repaired by this direct successor:
`c5f84bbcf7c1b44c1eac9c2e99857834d333a4c4`. Rejected R1 was
`b0ede8bbdfa99e7b2b06fd7f4f04db128d5f625b`. The original rejected e-d-b
lineage starts at `8a02f2a6b6e27587539d1e4e4717301bf010e6a1`.
R3 at `f9bc372f7ad5a56514ce1d6ad7ffd4f54097bb28` was rejected because harmless
account history was handled only after slot selection, flat had two competing
representations, cancel target client authority could be inferred from the
cancel command, and non-terminal target order payload drift was not fully
sealed. R4 is one direct repair successor to that exact R3 commit.
R4 at `66c5fbd2518ec2e7398c88bb59cc7e4dae3ce1bd` closed all submitted R3
findings but was rejected because exact terminal idempotency was nested only
under Filled, safe same-status Canceled/Expired late fills could not advance,
and missing-owned outcomes lost already classified history counts. R5 is one
direct repair successor to that exact R4 commit.

R5 keeps the original e-d-b scope: deterministic classification and opaque
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
`Stage5gReviewedOperationalIdentityAuthority`, not the package raw DTO. R4
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
R4 admits only finite integral lots and blocks fractional source quantities
with `SourceNumericAuthorityUnsupported`. No `f64 -> String -> Decimal`
conversion is presented as exact business authority.

Before target filtering, all account orders pass the shared active/unknown/
ambiguous ownership guard and the pure ownership classifier. Exact owned rows
may form a candidate; partial-ID conflicts block; unrelated terminal history is
ignored and counted; non-owned active/unknown rows remain account-wide safety
blocks. Semantic target matches with a non-exact canonical `InstrumentId`
block as `TargetInstrumentIdentityConflict`.

The account-wide safety pass is followed by a global history partition before
GRST01/GRST07 or slot selection. No-slot terminal orders and unrelated trades
are harmless history only after complete position truth is exact. Their counts
belong to the reduction/evidence itself, so candidate-free GRST01, GRST06 and
GRST07 outcomes retain them. Active, unknown or ambiguous rows remain blocking.

Command identity and target-order identity are separate. Place uses its command
client ID as target client ID. Cancel retains its own command request/client ID
while correlating only to the target broker order ID and, when committed, the
target client order ID. Cancel target-client authority is produced only by an
accepted target-order row through the production order/position boundary and
is action-scoped in the restart projection. The cancel command client ID can
never become target-order authority.

MARKET and CANCEL continue through the reducer. Source LIMIT is deliberately
fail closed with `SourceLimitPriceAuthorityUnsupported` until source price has
a reviewed canonical Decimal/tick authority; a positive broker LIMIT price is
not accepted as source price evidence.

Trade linkage is exact only when at least one present client/broker ID matches
and no supplied ID conflicts. No match is unrelated historical evidence, not a
conflict. `None == None` is not a match. Selection and candidate
self-consistency use the same helper.

## Status and completeness matrix

- TimerReady continues only with complete positions, no owned current
  order/trade evidence, all other rows classified as harmless historical, and
  exact committed target quantity.
- New/Working requires complete position truth equal to committed pre-position;
  any fill or linked trade is a terminal contradiction.
- PartiallyFilled requires complete position truth, exact trade sum, exact
  source-relative position and compatible intent class.
- Filled waits when positions are incomplete; complete empty positions are
  accepted only when the expected post-position is exactly flat.
- Rejected with a fill or linked trade is inconsistent.
- Canceled/Expired with a fill require complete exact post-position; zero fill
  requires unchanged pre-position.

Committed order status/fill quantity and committed trade IDs/payloads are
monotonic. Exact re-observation of Filled, Rejected, Canceled or Expired is one
status-independent GRST06 rule and never creates a candidate. Receipt
timestamps and volatile unrealized PnL are excluded from semantic equality,
while independent post-restore chronology remains mandatory. A same-status
Canceled or Expired order may advance to GRST11 only when added immutable
trades exactly explain the larger fill and the complete canonical position
converges. Rejected positive fills, Filled additional fills, status changes,
trade disappearance/payload drift and fill regression remain blocking.

Every source-to-fresh order comparison first verifies the exact immutable order
payload: account, exact instrument, broker/client IDs, side, type, TIF,
quantity, limit price and native asset/board/expiration fields. Only lifecycle,
status and fills may advance monotonically. The action-scoped cancel authority
also commits this immutable payload when target-order evidence is available.

Complete empty target positions and complete explicit `qty = 0` rows are the
same canonical flat observation. Flat equality ignores `avg_price` and
unrealized PnL; non-flat equality keeps authoritative quantity and average
price. An incomplete position section can never imply flat or absence.

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

The mandatory gate runs the R5 checker, at least 265 named negative mutations,
preseal, debug/release focused tests, the full runtime-core suite, formatting,
clippy with warnings denied and the detached exact R4 predecessor gate, which
retains the complete R3→R2→R1→R6 lineage.

Stage 5G-e-d-c remains closed until independent R5 acceptance. Stage 5G-f,
Redis consumer groups, FINAM transport, HTTP POST/DELETE, broker dispatch,
runtime-live, real orders and Stage 6 remain closed.
