# Stage 5G-f — paper/mock MR protective completion

Stage 5G-f closes the protective target/stop completion seam for Mean
Reversion positions in paper/mock mode only.

It does not place FINAM native stops, SLTP, brackets, real orders, Redis
consumer work, broker dispatch, runtime-live, Stage 5G-g/h or Stage 6.

## R6 base and accepted predecessor

R6 stable cleanup observation closure is one direct successor to the reviewed Stage 5G-f R5
intermediate baseline:

`1f8d7f3d14aa9cd2cb0f522679cf66787d5dd8a8`

The accepted Stage 5G-f R4 lineage point remains:

`430bae6cd02f67844623f9d1b2112b1faedcc40a`

The accepted Stage 5G-f R3 lineage point remains:

`7dde2ac181c7a5d3a3312bfb463e384281062a8a`

The accepted Stage 5G-f R2 intermediate baseline remains:

`34ecc9595bdb83639415ddde1b3975b88ac2faa4`

The accepted Stage 5G-f R1 lineage point remains:

`a28cedd984d41bd2db4aeb7fd8c125c62ded4b28`

The accepted Stage 5G-e-d-c R3 predecessor remains:

`c38d2e44e083e39552ea716823e43ebae775b881`

The Stage 5G-f handoff runs a bounded predecessor R3 verification in a detached
worktree at that exact commit:

- `stage5g_edc_r3_check.py`;
- `stage5g_edc_r3_negative_harness.py`;
- `stage5g_edc_r3_preseal_check.py`;
- focused R3 debug/release tests.

This intentionally avoids recursively re-running the entire historical
Stage 5G-e-d-c → Stage 5G-e-d-b → Stage 5D/5C lineage on every Stage 5G-f
handoff. The accepted R3 commit itself remains immutable and separately
reviewed; Stage 5G-f verifies that it is using that exact boundary before
validating the new protective-completion surface.

## Frozen scenarios

The machine-readable authority is
`docs/stage-5/stage5g-f-protective-completion-contract.json`.

It freezes exactly eight scenarios:

1. `GPRT01_F12_MR_LONG_TARGET_COMPLETES_FLAT`
2. `GPRT02_F13_MR_SHORT_TARGET_COMPLETES_FLAT`
3. `GPRT03_F14_MR_LONG_STOP_COMPLETES_FLAT`
4. `GPRT04_F15_MR_SHORT_STOP_COMPLETES_FLAT`
5. `GPRT05_WRONG_OWNER_OR_CYCLE_BLOCKS`
6. `GPRT06_WRONG_INSTRUMENT_OR_ORDER_ID_BLOCKS`
7. `GPRT07_TRIGGER_WITHOUT_FLAT_POSITION_BLOCKS`
8. `GPRT08_NON_EXECUTION_TERMINAL_CANNOT_INVENT_EXIT`

## Source semantic boundary

Stage 5F F12–F15 remain no-bar-exit semantic rows:

- F12 MR long favorable extreme emits no target exit.
- F13 MR short favorable extreme emits no target exit.
- F14 MR long adverse extreme emits no stop exit.
- F15 MR short adverse extreme emits no stop exit.

Stage 5G-f is therefore not allowed to infer protective execution from OHLC
bar high/low. Completion is authorized only by broker/runtime protective
feedback and complete flat position truth.

## Completion policy

Target completion requires:

- exact MR owner/cycle attribution;
- `TakeProfit` role;
- exact `BrokerOrderId == tp_order_id`;
- side opposite the protected position;
- quantity equal to the protected position;
- `Filled` status;
- complete target-instrument position truth;
- final target position flat.

Stop completion requires:

- exact MR owner/cycle attribution;
- `StopLoss` role;
- exact `BrokerStopOrderId == sl_stop_order_id`;
- exact exchange `BrokerOrderId` when the source authority has one;
- side opposite the protected position;
- quantity equal to the protected position;
- one accepted execution-like status: `Filled`, `Executed`, `Triggered`,
  `Done`, or `Completed`;
- complete target-instrument position truth;
- final target position flat.

`Canceled`, `Cancelled`, `Expired`, and `Rejected` are non-execution terminal
statuses and cannot invent an exit.

Complete absent target-position row is flat. Incomplete absent target-position
row is not flat.

## Opaque path and owning lifecycle state

Production code admits `Stage5gProtectiveCompletionAuthority` only through
`prepare_stage5g_protective_completion(Stage5gCleanRestartedCapability)`.
The authority is source-owned by the authenticated clean-restart package; raw
caller fields are not exported as a production API.

R3 authenticated protective restart extends the accepted Stage 5G clean-restart
package with a versioned protective lifecycle projection. The projection binds:

- scenario and protective leg;
- strategy/account/instrument and MR cycle;
- protected side/quantity and TP/SL protective identifiers;
- accepted execution receipt ledger;
- position-truth disposition;
- post-runtime semantic fingerprint;
- generated cleanup batch and settled batch-history fingerprints;
- cleanup sibling identity;
- cleanup-pending/completed state.

`Stage5gProtectiveCompletionAuthority` is then consumed by the production
canonical evidence issuer `issue_stage5g_canonical_protective_evidence` and
`apply_stage5g_protective_completion`. The production apply boundary consumes
`Stage5gValidatedProtectiveEvidence`, not raw caller vectors.

Raw protective evidence structs and the crate-private canonical acceptor remain
test-only/internal. Production callers cannot mint validated protective evidence
from caller-provided `positions_complete`, raw position rows, terminal status
strings or arbitrary receipt fingerprints.

Standalone raw JSON restart of a protective transition is intentionally absent.
Restart continuity must use the accepted Stage 5D/Stage 5G authenticated package
boundary; this slice does not introduce a second durable store.

Successful continuation now owns canonical post-callback runtime state through
`Stage5gProtectiveCommittedState`.

The result partition is:

- `Completed`;
- `FlatCleanupPending`;
- `AwaitingPositionTruth`;
- `Blocked`.

Completed requires every generated cleanup request to reach terminal non-execution; sibling execution requires fresh position truth.

R6 stable cleanup observation closure adds the missing continuation after an authenticated
`FlatCleanupPending` restart:

- Stage 5C owns a reconstructable cleanup-batch restart projection with exact
  request IDs, target protective IDs, source timestamps and MR attribution;
- `FlatCleanupPending` restore reconstructs the owning `Stage5cPaperIntentBatch`
  through the Stage 5C verifier instead of restoring from summary/hash only;
- `apply_stage5g_protective_cleanup_completion` is the sole paper/mock cleanup
  settlement boundary;
- terminal non-execution cleanup truth settles exactly one cleanup request;
- `Completed` is reached only after every generated cleanup request reaches
  terminal non-execution;
- non-terminal cleanup truth keeps the continuation in `FlatCleanupPending`;
- execution-observed cleanup truth requires fresh position truth instead of
  completing the protective lifecycle;
- the cleanup settlement fingerprint is preserved in the completed restart
  projection.

`Completed` is used only when no sibling cleanup batch is generated or required,
or after a future cleanup-settlement continuation proves completion. If the
accepted Stage 5C broker lifecycle bridge emits cleanup intents after a flat
protective execution, Stage 5G-f returns `FlatCleanupPending`. That state
owns:
If the accepted Stage 5C broker lifecycle bridge emits cleanup intents after a
flat protective execution, Stage 5G-f returns `FlatCleanupPending`. That state
owns:

- the post-callback runtime;
- the exact generated `Stage5cPaperIntentBatch`;
- the generated batch summary;
- the settled batch history;
- the per-request cleanup settlement ledger;
- both Stage 5C bridge state fingerprint and Stage 5G post-runtime semantic
  fingerprint.

Stage 5G-f itself does not call raw broker callbacks as its lifecycle boundary.
It has one narrow call into the accepted Stage 5C bridge in
`stage5c_paper_host.rs`; raw `on_broker_order`, `on_broker_stop_order`, and
`on_broker_position` calls remain inside that Stage 5C settlement surface.

Awaiting/blocking transitions preserve the exact incoming authority when no
completion mutation is allowed.

Sibling cleanup is represented only by exact paper lifecycle escrow evidence or
an exact terminal sibling proof. A missing sibling proof is not treated as safe.
It is not broker dispatch and not native transport.

R5 closes the authenticated protective restart + cleanup-settlement projection
for the Stage 5G-f paper/mock boundary. It does not open Stage 5G-g/h
protective-order placement, Redis live consumption, FINAM transport or any
runtime-live execution.

## Current gate

Primary current-head gate:

```bash
bash scripts/stage5g_f_r6_gate.sh
```

Required current-head checks:

- Stage 5G-f checker;
- Stage 5G-f negative harness, floor `>=430`;
- Stage 5G-f preseal;
- focused GPRT debug and release tests;
- authenticated protective restart debug and release tests;
- full `strategy-runtime-core` lib test;
- doctests, fmt, clippy;
- debug/release GPRT artifact parity;
- detached submitted R5 `1f8d7f3` Stage 5G-f gate verification, which transitively covers
  the accepted R1/R2/R3/R4 lineage;
- detached bounded Stage 5G-e-d-c R3 predecessor verification;
- forbidden surfaces remain closed.

Only after independent Stage 5G-f acceptance may Stage 5G-g begin.
