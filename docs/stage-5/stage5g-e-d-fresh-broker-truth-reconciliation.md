# Stage 5G-e-d — fresh mock BrokerTruth reconciliation

## e-d-a contract boundary

This slice defines and validates the input contract only. It does not reconcile
or mutate a runtime and cannot invoke a strategy callback. The validated package
is crate-private, linear, non-serializable and carries no Redis, FINAM, HTTP,
broker-dispatch or runtime-live authority.

The package wraps the accepted broker-neutral `BrokerOrderSnapshot`,
`BrokerTradeSnapshot` and `BrokerPositionSnapshot` rows. It does not introduce a
second order/position domain model.

## Freshness and identity

Validation requires all of the following before a later reducer may inspect the
package:

- schema version 1;
- non-empty package identity and snapshot epoch;
- snapshot epoch distinct from the pre-restart epoch;
- `captured_at` strictly after clean-restore completion;
- exact typed operational identity match;
- exact account identity on every broker row;
- row receipt time not later than package capture time;
- status/lifecycle consistency and unique canonical row identities.

The operational identity binds broker, account, strategy definition, strategy
instance, deployment, deployment generation, gateway instance, config
fingerprint, instrument-map fingerprint, market-data generation,
command-consumer generation and full target `InstrumentId`. A free-form source
label is not accepted as identity authority.

`orders_complete`, `trades_complete` and `positions_complete` are independent
facts. An empty incomplete section means “truth unavailable”, not “the broker
has no rows”. e-d-a preserves that distinction; e-d-b must map it to
`AwaitFreshBrokerTruth` or a stronger fail-closed disposition before any
callback.

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

## Next slices

1. e-d-b: consume the accepted clean-restart capability and validated fresh
   package in a deterministic, mutation-safe reducer; execute GRST01–12.
2. e-d-c: deterministic export/restart/reconcile/re-export evidence, negative
   matrix and immutable handoff.

Stage 5G-f, Redis consumer groups, FINAM transport, HTTP POST/DELETE,
broker dispatch, runtime-live and real orders remain closed.

## Verification

The review gate is `bash scripts/stage5g_ed_gate.sh`. It runs the e-d-a source
checker, the mutation harness, formatting, focused Rust tests, clippy with
warnings denied, and the inherited accepted Stage 5G-e-c gate from a detached
`b9db87947723cf9c50e64b5fcc3b5ab30e857fd1` Git worktree (including its current
Git-object-bound no-rg forbidden-surface gate).

The older standalone Stage 5D additive and repository-wide forbidden scanners
remain historical hash-pinned tools: on the accepted Stage 5G predecessor they
already report compilation/source hash drift introduced by later accepted
Stage 5E/5F/5G work. Their baselines are not rewritten by e-d-a. The applicable
Stage 5G-e-c inherited gate remains green.
