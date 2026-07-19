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
- MR pending entry Long;
- MR pending entry Short;
- BO pending entry Long;
- BO pending entry Short;
- pending exit;
- deferred entry;
- deferred exit;
- partial entry;
- safe-mode close-only;
- known-order index;
- pending-request index;
- working protective order hints;
- already-complete recovery plan;
- source-callback current-shadow Long;
- source-callback current-shadow Short;
- source-callback realized-PnL;
- source-callback rolling-sum/riskgate update;
- source-callback lifecycle notification boundary.

The r3a-r1 pending-entry rows are executable and owned by the accepted
`stage5d_final_r3a_source_pending_entry_full_restart_matrix` test. The new
`stage5d_final_r3_resumption_inventory_and_r3a_r1_reuse` test parses the r3
inventory, verifies the closed-surface contract, verifies the exact 21-case
positive id set, and executes the r3a-r1 MR/BO pending-entry source-produced
restart cases again.

Rows that are not yet source-produced remain marked `todo_source_produced`.
That marker is intentional: it prevents this slice from being confused with
full r3 closure.

## Gates

The additive freeze checker now validates the r3 inventory and fails if:

- the inventory is missing;
- the stage/status is changed;
- closed-surface booleans drift;
- the 21 positive case IDs drift;
- r3a-r1 executable rows stop pointing to the accepted source-produced test;
- all rows are marked complete prematurely.

The Stage 5D negative harness adds direct mutations for:

- removing the r3 resumption inventory;
- removing the r3a-r1 reuse marker from the source test.

## Next step

Continue Stage 5D-final-restart-r3 by implementing the remaining source-produced
positive cases in bounded groups, then add the real crash/checkpoint simulator,
package-negative matrix and checked-in golden vectors. Stage 5E remains closed.
