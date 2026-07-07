# M4-3v — broker-neutral runtime host contract

Status: implementation slice / paper-shadow foundation.

This step adds a broker-neutral runtime host contract to `broker-core`. It is
based on the ALOR runtime lifecycle and service invariants, but it does not add
FINAM live order routing, command-consumer-to-real-FINAM, stop/SLTP/bracket, or
continuous runtime-live.

## Added module

```text
crates/broker-core/src/runtime_host.rs
```

Exported from `broker-core`:

- `RuntimeHostLifecycleStep`;
- `RuntimeHostLifecyclePlan`;
- `RuntimeHostLifecycleIssue`;
- `validate_runtime_lifecycle_sequence`;
- `RuntimeIntentClass`;
- `RuntimeHostBlockedIntentDisposition`;
- `RuntimeIntentBlockEvent`;
- `RuntimeCommandPrepared`;
- `RuntimeEventClock`;
- `RuntimeHostBootstrapSnapshot`;
- `RuntimeStrategyContext`;
- `RuntimeHostContract`;
- `RuntimeHostLiveGuardInput`;
- `RuntimeHostLiveGuardDecision`;
- `evaluate_runtime_live_guard`.

## ALOR-compatible lifecycle order

The canonical lifecycle sequence is now explicitly represented:

1. `LoadBrokerTruthSnapshot`;
2. `LoadRuntimeState`;
3. `NotifyBootstrapSnapshot`;
4. `NotifyRuntimeStateRestored`;
5. `WarmupHistory`;
6. `RecoverPendingStreams`.

The validator rejects:

- missing lifecycle steps;
- duplicate lifecycle steps;
- out-of-order steps;
- warmup with live orders enabled;
- strategy-state trust before broker truth;
- pending recovery before warmup.

## Strategy intent contract

The runtime host contract now has broker-neutral intent classes:

```text
Entry
Exit
CancelCleanup
ProtectiveRepair
```

This preserves the ALOR invariant: entry gates can block entries, but must not
silently drop exit/cancel/protective repair paths once there is open risk.

## Command-prepared seam

`RuntimeCommandPrepared` provides the place where a host-built
`StrategyRequestId` is handed back to strategy code before state persistence.

This protects the request-id parity invariant:

- strategy emits intent;
- host prepares the exact command/request id;
- strategy persists that exact pending id;
- ack clears only a matching pending id.

## Broker-truth bootstrap snapshot

`RuntimeHostBootstrapSnapshot::from_broker_truth()` converts canonical
`BrokerTruthSnapshot` into target-symbol scoped runtime bootstrap truth:

- target position quantity;
- target open positions;
- target active orders;
- account-wide active order count as diagnostic;
- target flat/non-flat flag.

This directly addresses the ALOR lesson: account-wide rows are diagnostic, while
target-symbol non-zero quantity is position truth.

## Event-time clock

`RuntimeEventClock` keeps monotonic strategy time even if an older event arrives
after a newer event. This is the FINAM-side foundation for the ALOR event-time
semantics from the hybrid Stage-2 freeze.

## Live guard shape

`evaluate_runtime_live_guard()` currently models the host-level gate:

- `allow_live_orders`;
- broker-neutral readiness phase/reasons;
- readiness staleness;
- first final strategy bar after restart;
- operator live arm;
- close-only passthrough when blocked but target position is open.

This is still only a contract/helper. It does not enable live orders.

## Verified

```text
cargo test -p broker-core runtime_host -- --nocapture
cargo check -p broker-cli
```

Result:

```text
runtime_host tests: 5 passed
broker-cli check: ok
```

## Next

Use this contract as the host seam for the next implementation slice:

1. wire FINAM paper runtime bootstrap to canonical broker truth;
2. add intent-class/block-disposition to the paper runtime adapter path;
3. attach/port the real ALOR hybrid orchestrator behind the paper-only boundary;
4. project true cycle/owner/side/pending state from strategy outputs.
