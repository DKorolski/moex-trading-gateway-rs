# Stage 5D-final-restart-r3 — resumption inventory gate

Status: review candidate, no-I/O. This is a resumption slice after accepted
Stage 5D-final-restart-r3a-r1, not full r3 closure.

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
`stage5d_final_r3a_source_pending_entry_full_restart_matrix` test. The new
`stage5d_final_r3_resumption_inventory_and_r3a_r1_reuse` test parses the r3
inventory, verifies the closed-surface contract, verifies the exact 21-case
positive id set, and executes the r3a-r1 MR/BO pending-entry source-produced
restart cases again.

Exactly four rows are executable/accepted in this slice. The other seventeen
rows remain marked `todo_source_produced` and must not claim an `owning_test`.
That marker is intentional: it prevents this slice from being confused with full
r3 closure.

## Gates

The additive freeze checker now validates the r3 inventory and fails if:

- the inventory is missing;
- the stage/status is changed;
- closed-surface booleans drift;
- the 21 positive case IDs drift;
- r3a-r1 executable rows stop pointing to the accepted source-produced test;
- accepted executable IDs are anything other than the four r3a-r1 rows;
- TODO IDs are anything other than the remaining seventeen rows;
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

## Next step

Continue Stage 5D-final-restart-r3 by implementing the remaining source-produced
positive cases in bounded groups, then add the real crash/checkpoint simulator,
package-negative matrix and checked-in golden vectors. Stage 5E remains closed.
