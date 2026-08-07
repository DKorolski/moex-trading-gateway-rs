# Stage 5G-f — paper/mock MR protective completion

Stage 5G-f closes the protective target/stop completion seam for Mean
Reversion positions in paper/mock mode only.

It does not place FINAM native stops, SLTP, brackets, real orders, Redis
consumer work, broker dispatch, runtime-live, Stage 5G-g/h or Stage 6.

## R1 base and accepted predecessor

R1 is one direct successor to the submitted Stage 5G-f implementation:

`63e7f220f108ec539b61e73147938d461969daa8`

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

## Opaque path

Production code admits `Stage5gProtectiveCompletionAuthority` only through
`prepare_stage5g_protective_completion(Stage5gCleanRestartedCapability)`.
The authority is source-owned by the authenticated clean-restart package; raw
caller fields are not exported as a production API.

`Stage5gProtectiveCompletionAuthority` is then consumed by
`apply_stage5g_protective_completion`.

Standalone raw JSON restart of a protective transition is intentionally absent.
Restart continuity must use the accepted Stage 5D/Stage 5G authenticated package
boundary; this slice does not introduce a second durable store.

Completed transition consumes the authority and emits a completed evidence
summary derived after the canonical source runtime callback bridge runs the
accepted `on_broker_order`/`on_broker_stop_order` plus flat-position callback
path. Awaiting/blocking transitions preserve the exact incoming authority when
no completion mutation is allowed.

Sibling cleanup is represented only by exact paper lifecycle escrow evidence or
an exact terminal sibling proof. A missing sibling proof is not treated as safe.
It is not broker dispatch and not native transport.

## Current gate

Primary current-head gate:

```bash
bash scripts/stage5g_f_r1_gate.sh
```

Required current-head checks:

- Stage 5G-f checker;
- Stage 5G-f negative harness, floor `>=140`;
- Stage 5G-f preseal;
- focused GPRT debug and release tests;
- full `strategy-runtime-core` lib test;
- doctests, fmt, clippy;
- detached submitted `63e7f22` Stage 5G-f verification;
- detached bounded Stage 5G-e-d-c R3 predecessor verification;
- forbidden surfaces remain closed.

Only after independent Stage 5G-f acceptance may Stage 5G-g begin.
