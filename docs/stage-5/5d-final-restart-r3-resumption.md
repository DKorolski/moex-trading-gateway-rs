# Stage 5D-final-restart-r3 — resumption inventory gate

Status: historical/superseded. The authoritative current status is
[Stage 5D-final-restart-r3 aggregate closure r2](5d-final-restart-r3-aggregate-closure-r2-review-summary.md).

Status: review candidate, no-I/O. This is a
resumption/current-shadow-r1-r1 slice after accepted
Stage 5D-final-restart-r3a-r1 and positive-core-r1b, not full r3 closure.

The goal of this slice is to restart the larger r3 closure work without
overclaiming evidence. It records the full required positive matrix, reuses the
accepted source-produced pending-entry proof from r3a-r1, and keeps the
remaining source-produced cases explicitly marked as TODO.

The trading boundary remains closed:

- no Redis;
- no FINAM;
- no broker transport;
- no dispatch/send;
- no runtime-live;
- no broker execution.

## What is pinned

The new inventory is:

```text
docs/stage-5/stage5d-final-restart-r3-scenario-inventory.json
```

It pins the 21 mandatory positive restart cases from the r3 assignment:

- clean flat;
- open Long;
- open Short;
- current-shadow Long;
- current-shadow Short;
- current-shadow realized-PnL;
- MR pending entry Long;
- MR pending entry Short;
- BO pending entry Long;
- BO pending entry Short;
- partial entry;
- pending exit;
- deferred entry;
- deferred exit;
- safe-mode close-only;
- known-order index;
- pending-request index;
- working protective order hints;
- single pending riskgate finalization;
- ordered multi-row pending riskgate finalizations;
- already-complete recovery plan;

The r3a-r1 pending-entry rows are executable and owned by the accepted
`stage5d_final_r3a_source_pending_entry_full_restart_matrix` test. The
positive-core-r1b clean flat and broker-consistent open Long/Short rows are
executable and owned by
`stage5d_final_r3_positive_core_source_produced_full_restart_matrix`. They are
produced through actual source runtime lifecycle callbacks, not direct
persistence/envelope position mutation. The
current-shadow-r1-r1 Long/Short/realized-PnL rows are executable and owned by
`stage5d_final_r3_current_shadow_r1_source_produced_full_restart_matrix`. They
are produced through current-shadow source callbacks, strict canonical package
decode, fresh runtime restore, exact post-apply equality and Stage 5C
continuation.

The
`stage5d_final_r3_resumption_inventory_and_r3a_r1_reuse` test parses the r3
inventory, verifies the closed-surface contract, verifies the exact 21-case
positive id set, and executes the r3a-r1 MR/BO pending-entry source-produced
restart cases again.

Exactly ten rows are executable/accepted after current-shadow-r1-r1. The other
eleven rows remain marked `todo_source_produced` and must not claim an
`owning_test`.
That marker is intentional: it prevents this slice from being confused with full
r3 closure.

Current-shadow Long/Short/realized-PnL were promoted only after the executable
discovery proof localized the first mismatch as materialized riskgate state:
`risk_gate_mr_enabled_current_session`, `risk_gate_rolling_sum_lb120`,
`risk_gate_last_finalized_session_date` and `risk_gate_ledger_rows_count`.
Before correction, the source runtime state had empty/default materialized
current-shadow values while the ledger-derived package evidence rebuilt
`mr_enabled=true`, `rolling_sum=47.0`, `last_finalized_session_date=2026-01-05`
and `ledger_rows_count=61`. The owning layer is now the approved Stage 5D
validated materialized-apply boundary before canonical package export/injection.
Canonical restart export now fails fast on the old stale-source sequence, so a
committed strict-decodable package cannot be produced if it would
deterministically fail at authoritative riskgate injection. This does not
authorize source `set_state()` correction.

## Gates

The additive freeze checker now validates the r3 inventory and fails if:

- the inventory is missing;
- the stage/status is changed;
- closed-surface booleans drift;
- the 21 positive case IDs drift;
- r3a-r1 executable rows stop pointing to the accepted source-produced test;
- accepted executable IDs are anything other than the four r3a-r1 rows plus the
  three positive-core-r1b rows plus the three current-shadow-r1 rows;
- positive-core accepted rows lack runtime-callback producer metadata,
  source-object destruction, strict decode, fresh runtime or Stage 5C
  continuation proof;
- current-shadow accepted rows lack runtime-callback producer metadata,
  materialized-apply boundary, source-object destruction, strict decode, fresh
  runtime, exact post-apply equality or Stage 5C continuation proof;
- TODO IDs are anything other than the remaining eleven rows;
- any TODO row claims an owning test;
- any accepted row lacks its accepted owning test;
- Stage 5E closed marker is removed.

The Stage 5D negative harness adds direct mutations for:

- removing the r3 resumption inventory;
- removing the r3a-r1 reuse marker from the source test;
- prematurely promoting clean flat or current-shadow rows;
- assigning unapproved retained status;
- assigning nonexistent or false owning tests;
- reducing the TODO set;
- downgrading an accepted r3a row;
- removing the Stage 5E closed marker;
- adding an owner to a TODO row;
- removing an owner from an accepted row.
- dropping the current-shadow full-path proof;
- drifting realized current-shadow PnL/trade-count/session semantics;
- skipping or moving the materialized-apply boundary;
- reusing the source runtime instead of a fresh restored runtime;
- opening Stage 5E or any closed Redis/FINAM/transport/dispatch/runtime-live
  surface.
- removing the production materialized-apply boundary or turning it into a
  `#[cfg(test)]` helper;
- allowing raw envelope authority, raw strategy extraction, partial mutation on
  block, or stale package commit.

## Next step

Continue Stage 5D-final-restart-r3 by implementing the remaining source-produced
positive cases in bounded groups, then add the real crash/checkpoint simulator,
package-negative matrix and checked-in golden vectors. Stage 5E remains closed.
