# Stage 5F-c R2 — source-reachability closure

Status: review required before Stage 5F-d

Predecessor: `824fcff3adfcda15b5442f00004f604a58e10236`

## Outcome

R2 corrects the ten source-reachability findings without changing production
strategy formulas or the accepted R1 callback/settlement contour. The seven
R1 candidate outputs remain byte-identical and non-golden.

The corrected rows are:

```text
F03 F05 F12 F13 F14 F15 F16 F17 F19 F26
```

- F03 and F17 now occur after the three-hour BO wait boundary.
- F05 is an explicit short stop2 exit with `close > 101.4`.
- F12–F15 prove that High180 target/stop price movement creates no new bar
  exit intent; actual protective completion remains owned by future Stage 5G
  broker feedback.
- F16 is a structural active-profile invariant: BO starts at 12:00:00 MSK,
  after the High180 entry window closes at 11:59:59 MSK.
- F19 has both a source-valid BO candidate and a recent open MR cycle, so zero
  BO entry is attributable to owner/open-position suppression.
- F26 retains stale pending entry state only because explicit synthetic
  working-order evidence activates the production GC guard.

## Authority and checks

`scripts/stage5f_source_reachability_check.py` binds the exact production
sources and target config, computes the BO thresholds/windows, High180 cutoff
and max hold, owner/cycle age and pending timeout/working-order relation, and
checks Stage 5F versus Stage 5G ownership.

`scripts/stage5f_source_reachability_negative_harness.py` mutates each reviewed
rule, including exact-boundary and timeout-plus-one cases, and requires every
mutation to fail closed.

The old-to-new row semantics are recorded in
`docs/stage-5/stage5f-c-r2-row-semantics-mapping.json`; the revised 34-row
classification is recorded in both the R2 inventory and the B0 reachability
inventory.

## Closed scope

No Redis, FINAM transport, HTTP POST/DELETE, dispatch, broker execution,
runtime-live, ACK/order/position/timer/restart feedback or protective-order
implementation is added. Stage 5F-d, Stage 5G and Stage 5H remain closed until
independent acceptance of this R2 package.
