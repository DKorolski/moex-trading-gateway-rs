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

The 6,203-line oracle is split into five exact, hashed regions: imports,
configuration/state types, inherent implementation, source tests and callback
implementation. Every region has an allowed migration class and planned target
role.

Allowed changes remain limited to broker-neutral type migration and host
boundary extraction. Formula rewrites, parameter changes and behavioral
simplification are forbidden.

## Callback policy

Fourteen callbacks are implemented by the exact Hybrid oracle and must remain
source-equivalent. Seven additional generic-host seams are not overridden by
the source Hybrid. They are explicitly marked `Stage5CExplicitPolicy`; their
generic defaults cannot be silently presented as migrated Hybrid behavior.

Stage 5B-2a therefore prevents two common parity errors:

- omitting a source callback because its state effect is indirect;
- treating a host default as if it were an accepted Hybrid override.

## State and identity policy

The boundary map requires all eight state groups, including the transient
bracket terminal-reconciliation marker. It freezes these identity migrations:

```text
Uuid pending request id       -> StrategyRequestId
i64 order id                  -> BrokerOrderId(String)
i64 stop exchange order id    -> BrokerOrderId(String)
String stop order id          -> BrokerOrderId(String)
```

Numeric ids are imported as decimal strings. Surrogate or lossy mappings remain
forbidden.

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
