# Stage 5B-2a — wrapper correspondence and boundary map

Status: implementation-foundation review candidate.

Date: 2026-07-11.

## Outcome

Stage 5B-2a creates the separate correspondence manifest and callback/state
mapping required before importing the integrated wrapper. It intentionally does
not copy or compile the wrapper.

Normative inputs:

- wrapper oracle: `source-oracles/alor-stage5/hybrid_intraday_runtime.rs`;
- oracle SHA256: `6e15ab1b7212c56d3ecd8397b2d8991c1feccbde8eaa5e3d0051aec82a55f0aa`;
- correspondence manifest:
  `crates/strategy-runtime-core/stage5b2-source-correspondence.toml`;
- boundary fixture:
  `tests/fixtures/stage5/stage5b2_callback_state_mapping.json`.

The accepted Stage 5B-1 correspondence manifest remains unchanged.

## Explicit future Cargo target

The only planned wrapper target is:

```text
crate:  strategy-runtime-core
kind:   library module
module: hybrid_intraday_runtime
path:   crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs
export: pub mod hybrid_intraday_runtime;
gate:   Stage5B2bSeparateReview
```

This declaration is not activation. The current exact Rust/Cargo target set
continues to reject that file and export until the Stage 5B-2b patch updates the
manifest, scanner locks and tests together.

## Source-region policy

The 6,203-line oracle is split into five contiguous exact, hashed regions: imports,
configuration/state types, inherent implementation, source tests and callback
implementation. Every region has an allowed migration class and planned target
role.

Allowed changes remain limited to broker-neutral type migration and host
boundary extraction. Formula rewrites, parameter changes and behavioral
simplification are forbidden.

## Callback policy

Fifteen callbacks are implemented by the exact Hybrid oracle and must remain
source-equivalent. Six additional generic-host seams are not overridden by
the source Hybrid. They are explicitly marked `Stage5CExplicitPolicy`; their
generic defaults cannot be silently presented as migrated Hybrid behavior.

`acknowledge_risk_gate_session_finalizations` is one of the fifteen source
overrides. It removes only acknowledged pending finalizations and synchronizes
state; treating it as a generic host default would risk duplicate ledger rows.

The manifest test extracts direct methods from
`impl Strategy for HybridIntradayRuntimeStrategy` and requires exact equality
with all fixture records marked `source_override=true`. It separately requires
exactly 21 callback records, so duplicate names cannot hide an omission.

Stage 5B-2a therefore prevents two common parity errors:

- omitting a source callback because its state effect is indirect;
- treating a host default as if it were an accepted Hybrid override.

## State and identity policy

The boundary map requires all eight state groups, including the transient
bracket terminal-reconciliation marker. It freezes these identity migrations:

```text
all strategy request UUIDs    -> StrategyRequestId
i64 order/exchange-order ids  -> BrokerOrderId(String)
String stop-order ids         -> BrokerStopOrderId(String)
```

Numeric ids are imported as decimal strings. Surrogate or lossy mappings remain
forbidden. Stop-order ids and exchange order ids are separate correlation
namespaces and cannot be interchanged.

## Lossless callback contracts

Stage 5B-2a adds broker-neutral, wrapper-specific contracts in
`broker-core::hybrid_strategy_boundary` for the source-critical surfaces:

- exact ACK status including `Confirmed`, `TradingWindowClosed`, error message
  and processed timestamp;
- order events with internal sid/cycle/owner/role attribution;
- stop-order events with separate `BrokerStopOrderId` and exchange
  `BrokerOrderId` namespaces;
- position events with the source-significant `existing` flag;
- canonical bar origin (`History`, `HistoryGap`, `Live`, `Replay`) and complete
  strategy context including tick size, trade/paper modes and gateway phase;
- composite bootstrap carrying strategy-owned orders, protective orders,
  attribution and broker truth;
- restored-state and riskgate records.

Internal attribution and compatibility error detail reach the strategy
callback unchanged. Redaction occurs only when operator evidence is rendered.
The boundary fixture records required source fields, target fields, status,
timestamp, identity, origin and redaction mapping for all 21 seams. State
snapshot/restore and the six generic seams remain explicitly gated for Stage
5D and Stage 5C rather than being falsely marked lossless.

### Context-complete callback inputs

Every source callback which reads `StrategyCtx` receives a typed
`HybridRuntimeCallbackInput<T>` containing both the payload and full
`HybridRuntimeStrategyContext`. The fixture freezes the exact dependency set.
In particular:

- order, stop-order and bootstrap ownership checks require `strategy_id`;
- position and timer emergency/protective paths require strategy/account
  request namespace, instrument, tick size, trade mode and event time;
- timer additionally requires current context position quantity;
- restored-state replay protection requires trade mode, live-order permission
  and strategy clock.

Regression tests prove that changing account namespace changes deterministic
request identity, changing tick size changes protective stop-limit price, and
timer/restore inputs retain trade mode, position and strategy clock.

### Attribution source of truth

`HybridRuntimeAttribution::parse_source_comment` implements the source tag
parser and keeps raw internal comment as the source of truth. Structured
sid/cycle/owner/role fields are private and deserialization validates them
against the comment. Mismatches are rejected before the strategy callback.
Source behavior is preserved for `REPAIR`: the generic tag builder may emit it,
but the exact source parser leaves it without a recognized `TagRole`.

### Bootstrap consistency

`HybridRuntimeBootstrapSnapshot::validate` rejects duplicate target positions,
duplicate order/stop IDs, instrument mismatches and contradiction between
strategy position rows and canonical broker truth. The vector transport shape
therefore cannot silently weaken the source map uniqueness contract.

### ACK adapter boundary

`HybridRuntimeCommandAck` can retain source-compatible raw error code/message
and processed timestamp. The current generic `broker_core::CommandAck` remains
a normalized lifecycle DTO; mapping helpers preserve the accepted status
matrix but do not claim to reconstruct raw broker detail. Before Stage 5C, the
paper/host adapter must identify the safe raw error-code/message source and
construct `HybridRuntimeCommandAck` before redaction. `Timeout` and
`UnknownPending` remain blocked from the strategy callback until reconciliation.

## Workspace-wide target lock

While `currently_allowed_in_rust_target_set=false`, the scanner rejects across
the exact parsed workspace member set:

- any Rust file named `hybrid_intraday_runtime.rs`;
- any definition of `HybridIntradayRuntimeStrategy`;
- any `impl Strategy for HybridIntradayRuntimeStrategy`.
- any occurrence of the wrapper identifier, including macro-generated or
  comment-separated definitions;
- `include!` or `#[path]` activation of the source oracle.

Only three hash-locked inventory tests may read the oracle through
`include_str!`. The workspace member set is frozen, so a new member outside
`crates/` cannot bypass the gate. Stage 5B-2b must open exactly the declared
path while all alternate crate/path targets remain forbidden.

## Accepted review backlog

The executable Stage 5B-2b matrix must prove:

- each repeated qualifying TP/SL execution event restarts the grace timestamp;
- timeout quantity is `ctx.position_qty.unwrap_or(last_position_qty)`.

## Safety boundary

Wrapper copied/compiled, runtime-host attachment, runtime-live, real FINAM
consumer, strategy-driven orders, POST/DELETE and real Stop/SLTP/bracket remain
false. This slice contains no FINAM DTO, HTTP, Redis, network or process
dependency.

## Next gate

After Stage 5B-2a acceptance, Stage 5B-2b may mechanically import and adapt the
wrapper into the one declared library module. The import must update scanner
locks and correspondence records explicitly; it may not weaken the Stage 5B-1
freeze or attach the wrapper to a runtime host.
