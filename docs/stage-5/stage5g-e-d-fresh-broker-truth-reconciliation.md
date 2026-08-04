# Stage 5G-e-d — fresh mock BrokerTruth reconciliation

## e-d-a R1 contract boundary

This slice defines and validates the input contract only. It does not reconcile
or mutate a runtime and cannot invoke a strategy callback. The validated package
is crate-private, linear, non-serializable and carries no Redis, FINAM, HTTP,
broker-dispatch or runtime-live authority.

The package wraps the accepted broker-neutral `BrokerOrderSnapshot`,
`BrokerTradeSnapshot` and `BrokerPositionSnapshot` rows. It does not introduce a
second order/position domain model.

## Freshness and identity

Validation requires all of the following before a later reducer may inspect the
package. The R1 chronology is exact:

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
constructor. Whitespace IDs, zero generations and malformed SHA-256 values are
rejected.

Replay authority is deliberately split. The pre-restart package/epoch prevents
reuse of stale startup evidence. The last-reconciled identity permits only an
exact immediate replay. A separate bounded accepted historical ledger may
permit an exact older replay. The same package ID with a changed canonical fingerprint
is a conflict and fails before any mutation; a known historical package outside
the accepted ledger is blocked.

Position uniqueness follows broker-core's accepted semantic instrument matcher,
not strict JSON equality. A wildcard venue collision (`venue_symbol=None`
against a matching canonical symbol/exchange/market) is rejected, including a
bridge between two otherwise distinct venue symbols.

`orders_complete`, `trades_complete` and `positions_complete` are independent
facts. An empty incomplete section means “truth unavailable”, not “the broker
has no rows”. e-d-a preserves that distinction; e-d-b must map it to
`AwaitFreshBrokerTruth` or a stronger fail-closed disposition before any
callback.

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
| `GRST06_RESTART_AFTER_TERMINAL_POSITION_APPLIED` | Filled terminal order plus applied target/flat position continues from the committed checkpoint. |
| `GRST07_RESTART_AT_TIMER_CHECKPOINT` | Exact timer checkpoint replay is single-consume and deterministic. |
| `GRST08_RESTART_WITH_GENERATED_INTENT_ESCROW` | Retryable block retains generated-intent escrow unchanged. |
| `GRST09_EXACT_REPLAY_IS_IDEMPOTENT` | Exact package replay is a no-op with an unchanged semantic fingerprint. |
| `GRST10_CONFLICTING_REPLAY_BLOCKS` | Contradictory rows/package identity never mutate runtime and require reconciliation or terminal handling. |
| `GRST11_FRESH_BROKER_TRUTH_OVERRIDES_STALE_HINT` | Fresh active/terminal truth overrides stale cancel/order hints; canceled, rejected and expired outcomes remain explicit. |
| `GRST12_MISSING_OR_AMBIGUOUS_TRUTH_REQUIRES_RECONCILIATION` | Missing, ambiguous or incomplete truth is never interpreted as broker absence. |

## Dispositions reserved for e-d-b

The typed disposition vocabulary is frozen in e-d-a:

- `ExactReplay`;
- `ContinueFromCommittedCheckpoint`;
- `ApplyOwnedCandidate`;
- `AwaitFreshBrokerTruth`;
- `ReconciliationRequired`;
- `ManualInterventionRequired`;
- `TerminalInconsistency`.

No function in e-d-a returns one of these dispositions. Classification and the
GRST01–12 executable reducer belong to e-d-b and require a separate review.
Stage 5G-e-d-b remains closed until independent acceptance of this R1 package.

## Next slices

1. e-d-b: consume the accepted clean-restart capability and validated fresh
   package in a deterministic, mutation-safe reducer; execute GRST01–12.
2. e-d-c: deterministic export/restart/reconcile/re-export evidence, negative
   matrix and immutable handoff.

Stage 5G-f, Redis consumer groups, FINAM transport, HTTP POST/DELETE,
broker dispatch, runtime-live and real orders remain closed.

## Verification

The R1 review gate is `bash scripts/stage5g_eda_r1_gate.sh`. It runs the R1
source checker and negative matrix, formatting, focused tests in debug and
release, the full `strategy-runtime-core` library suite and clippy with warnings
denied. It then runs the rejected predecessor's complete e-d-a gate from a
detached `f44b154753ea8b60a73cfb6ee3b5e487263dcb3b` worktree. That predecessor
gate in turn executes the inherited accepted Stage 5G-e-c gate from detached
`b9db87947723cf9c50e64b5fcc3b5ab30e857fd1` source.

The older standalone Stage 5D additive and repository-wide forbidden scanners
remain historical hash-pinned tools: on the accepted Stage 5G predecessor they
already report compilation/source hash drift introduced by later accepted
Stage 5E/5F/5G work. Their baselines are not rewritten by e-d-a. The applicable
Stage 5G-e-c inherited gate remains green.
