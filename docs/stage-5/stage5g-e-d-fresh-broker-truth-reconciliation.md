# Stage 5G-e-d — fresh mock BrokerTruth reconciliation

## e-d-a R6 accepted boundary and e-d-b R3 reducer

Stage 5G-e-d-a was independently accepted at
`4ece2c7c83ca5575dbca306b5fa29a48dae2bd47`. Its validated package is
crate-private, linear, non-serializable and carries no Redis, FINAM, HTTP,
broker-dispatch or runtime-live authority.

The first e-d-b implementation at
`8a02f2a6b6e27587539d1e4e4717301bf010e6a1` was rejected because restart
binding, replay ownership, trade linkage and source-relative position semantics
were incomplete. R1 at `b0ede8bbdfa99e7b2b06fd7f4f04db128d5f625b`
remained rejected on restart authority and parity findings. R2 at
`c5f84bbcf7c1b44c1eac9c2e99857834d333a4c4` closed those findings but was
rejected on LIMIT price authority, cancel correlation, historical-row
partitioning and semantic refresh. e-d-b R3 is its one direct repair successor. It consumes the
accepted clean-restart capability and a non-serializable restart-bound package,
classifies one frozen GRST case, and may construct one opaque in-memory
candidate. The reducer retains both owning authorities in its linear result. It
cannot apply the candidate, invoke a callback, mutate runtime or persistence,
publish Redis data, call FINAM/HTTP or dispatch an order.

The package wraps the accepted broker-neutral `BrokerOrderSnapshot`,
`BrokerTradeSnapshot` and `BrokerPositionSnapshot` rows. It does not introduce a
second order/position domain model.

## Freshness and identity

Validation requires all of the following before a later reducer may inspect the
package. The R3 chronology is exact:

```text
clean_restore_completed_at < section_observed_at <= captured_at
clean_restore_completed_at <= row.received_ts <= section_observed_at
source_ts <= received_ts
```

In particular:

- schema version 1;
- non-empty package identity and snapshot epoch;
- snapshot epoch distinct from the pre-restart epoch;
- `captured_at` strictly after clean-restore completion;
- exact typed operational identity match;
- exact account identity on every broker row;
- an explicit post-restore observation time for each order, trade and position
  section, including a complete empty section;
- row receipt time bounded by clean restore and that section's observation;
- status/lifecycle consistency and unique canonical row identities.

The operational identity binds broker, account, strategy definition, strategy
instance, deployment, deployment generation, gateway instance, config
fingerprint, instrument-map fingerprint, market-data generation,
command-consumer generation and full target `InstrumentId`. A free-form source
label is not accepted as identity authority. The validated identity types have
no unchecked `Deserialize`; JSON first enters a raw DTO and must pass the typed
constructor. The canonical identity-token grammar requires a non-empty,
already-trimmed UTF-8 token with no Unicode whitespace or control character
anywhere. Visible hyphen and colon characters are allowed. This policy applies
to Stage 5G string identities, package/snapshot identity, account, target symbol
and optional venue symbol. Zero generations and malformed lowercase-hex SHA-256
values are rejected.

Package validation consumes a separate linear reviewed deployment/config
capability. R3 intentionally has no production raw-DTO issuer for it; the only
issuer is test-only. This prevents the broker package from supplying both the
actual and expected identity. Production config issuance remains closed for a
separately reviewed integration step.

Replay data supplied to e-d-b is deliberately typed as untrusted hints and
committed to the package/restart binding only to prevent post-bind mutation.
No authenticated fresh-truth ledger exists in the Stage 5D restart envelope,
so exact immediate and historical replay are conservatively disabled. Exact
current tuples become `ReplayTupleNotInRestartLedger`; exact historical tuples
become `HistoricalReplayNotAccepted`; changed tuples become
`ReplayFingerprintConflict`. Exact replay may be enabled only after e-d-c adds
and authenticates the durable tuple ledger.

Position uniqueness inside e-d-a follows broker-core's accepted semantic
instrument matcher. The e-d-b operational boundary is stricter: the target
`InstrumentId`, including venue symbol, must exactly equal the authenticated
restart target; wildcard venue fallback is forbidden.

`orders_complete`, `trades_complete` and `positions_complete` are independent
facts. An empty incomplete section means “truth unavailable”, not “the broker
has no rows”. e-d-a preserves that distinction; e-d-b must map it to
`AwaitFreshBrokerTruth` or a stronger fail-closed disposition before any
callback.

Before target filtering, the full account order set is checked with the same
shared guard as accepted Stage 5G order/position logic. Non-owned active or
unknown rows and ambiguous/conflicting owned rows block. Unrelated terminal
orders and unrelated trades are ignored as historical rows and counted in
redacted evidence. MARKET/CANCEL source action is retained and checked; source
LIMIT is fail closed until canonical Decimal/tick price authority exists.
Cancel command identity is distinct from target-order identity. Source/fresh order and trade facts may only advance
monotonically. Source runtime quantities are accepted only for finite integral
lots until the source model migrates from `f64` to canonical `Decimal`.

Order rows preserve canonical lifecycle rules: status and lifecycle must agree,
remaining quantity must be explicit and exact, `Filled` requires a complete
fill, active zero-remaining rows are inconsistent, native IDs must be canonical,
and `Unknown` remains explicit rather than becoming active or terminal.

## Frozen GRST mapping

The identifiers from `stage5g-lifecycle-entry-inventory.json` are immutable.
The completion obligations are attached without renaming or removing a case:

| Frozen ID | e-d semantic obligation |
|---|---|
| `GRST01_RESTART_BEFORE_ACK` | Pre-ACK restart with no authoritative broker row remains blocked awaiting fresh truth. |
| `GRST02_RESTART_AFTER_ACK_BEFORE_ORDER` | Accepted order discovered by exact client ID can become an owned candidate only after full validation. |
| `GRST03_RESTART_WITH_WORKING_ORDER` | Post-ACK working order remains active and retains exact request/client/broker identity. |
| `GRST04_RESTART_AFTER_PARTIAL_FILL` | Partial fill and matching target position converge monotonically; mismatch blocks. |
| `GRST05_RESTART_FILLED_BEFORE_POSITION` | Filled order before a complete matching position section waits for fresh position truth. |
| `GRST06_RESTART_AFTER_TERMINAL_POSITION_APPLIED` | Exact terminal order, trades, position and ownership identity continue from the committed checkpoint. |
| `GRST07_RESTART_AT_TIMER_CHECKPOINT` | Exact timer checkpoint replay is single-consume and deterministic. |
| `GRST08_RESTART_WITH_GENERATED_INTENT_ESCROW` | Retryable block retains generated-intent escrow unchanged. |
| `GRST09_EXACT_REPLAY_IS_IDEMPOTENT` | Caller exact-replay hints are semantic no-ops but remain blocked until an authenticated fresh ledger exists. |
| `GRST10_CONFLICTING_REPLAY_BLOCKS` | Contradictory rows/package identity never mutate runtime and require reconciliation or terminal handling. |
| `GRST11_FRESH_BROKER_TRUTH_OVERRIDES_STALE_HINT` | Fresh active/terminal truth overrides stale cancel/order hints; canceled, rejected and expired outcomes remain explicit. |
| `GRST12_MISSING_OR_AMBIGUOUS_TRUTH_REQUIRES_RECONCILIATION` | Missing, ambiguous or incomplete truth is never interpreted as broker absence. |

## Executable e-d-b dispositions

The typed disposition vocabulary is frozen in e-d-a:

- `ExactReplay`;
- `ContinueFromCommittedCheckpoint`;
- `ApplyOwnedCandidate`;
- `AwaitFreshBrokerTruth`;
- `ReconciliationRequired`;
- `ManualInterventionRequired`;
- `TerminalInconsistency`.

The e-d-b R3 reducer returns exactly one of these dispositions with a closed typed
reason. GRST01–12 execute once in frozen order in focused debug/release tests.
Replay hints are semantic no-ops, incomplete truth never means broker absence,
and contradiction never produces a candidate. Restart slots preserve typed
intent class and exact Decimal pre-position; expected post-position is
`pre_position_qty + signed_fill`. Shared exact trade linkage rejects a
secondary-ID conflict even when the other ID matches, and never treats two
missing IDs as equal authority.

R6 is final e-d-a compilation-control acceptance closure. It does not change the
production validation semantics accepted as substantively correct in R2–R5.
No Rust source, test, Cargo topology or dependency semantic changed.
All strategy-runtime-core Rust source remains byte-frozen to R4. This is bound through the exact
19-file source manifest in
`stage5g-e-d-a-r5-runtime-core-source-freeze.json`. The production fresh
BrokerTruth validator also remains frozen at the exact prefix SHA-256 recorded
in `stage5g-e-d-a-r4-production-freeze.json`. The
explicit current-HEAD inventory in
`stage5g-e-d-a-r3-current-head-invariants.json` binds every inherited package,
chronology, row-authority, quantity, shape, duplicate and replay-ledger guard to
a production anchor, focused Rust witness and named negative mutation.
`implemented_restart_case_ids remains empty`; no GRST case executes in e-d-a.

## Next slices

1. e-d-b R3: consume the accepted clean-restart capability and restart-bound
   validated fresh package in a deterministic, mutation-safe reducer; execute
   GRST01–12 through pure and owning-boundary fixtures.
2. e-d-c: deterministic export/restart/reconcile/re-export evidence, negative
   matrix and immutable handoff.

Stage 5G-f, Redis consumer groups, FINAM transport, HTTP POST/DELETE,
broker dispatch, runtime-live and real orders remain closed.

## Verification

The accepted predecessor gate is `bash scripts/stage5g_eda_r6_gate.sh` from a
detached worktree at exact `4ece2c7...`. The rejected e-d-b R2 repair base is
`c5f84bb...`. The current-head e-d-b R3 gate is
`bash scripts/stage5g_edb_r3_gate.sh`.

The R6 checker is the controlling strict current-HEAD superset. It freezes the
complete accepted R5 project tree outside the exact ten-file R6 allowlist,
root/package Cargo manifests, lockfile, workspace membership, all runtime Rust
targets and the absence of repository-local Cargo compiler overrides. It also
retains the R5 freeze of the
complete accepted fresh-truth source file and every Rust source under
`strategy-runtime-core`, so suffix code, sibling reducers, new files and module
registration drift all fail closed. It retains the accepted production-prefix
hash and also
compares the complete contract truth map and exact closed-surface key/value map,
and retains alias-aware reducer diagnostics. It compares the exact ordered
12 GRST IDs, seven reconciliation dispositions and twelve operational identity
fields in JSON and Rust source, including the exact ordered
`Stage5gRestartScenarioId::ALL` array. It independently pins every inherited
package/row guard and rejects an actual compilable reducer inserted into the
Rust source, not merely documentation drift. The gate runs focused tests in
debug/release, the full
`strategy-runtime-core` suite, formatting and clippy with warnings denied.

The R6 checker also binds the exact ordered current gate command inventory and
the immutable handoff builder hash. For lineage evidence the R6 gate runs the
complete R5 gate from detached
`c84ee07c2700f04b5c070eab713598777d5195b6`; that gate runs detached R4,
R3, R2, R1, `f44b154` and accepted `b9db879` predecessors. Detached gates prove
provenance in addition to, never instead of, the accepted R6 boundary and
current-head e-d-b invariants.

The historical `stage5g_ed_*` scripts are predecessor-only snapshot tools for
`f44b154`, not the documented HEAD command.

The older standalone Stage 5D additive and repository-wide forbidden scanners
remain historical hash-pinned tools: on the accepted Stage 5G predecessor they
already report compilation/source hash drift introduced by later accepted
Stage 5E/5F/5G work. Their baselines are not rewritten by e-d-a. The applicable
Stage 5G-e-c inherited gate remains green.
