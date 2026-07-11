# Stage 5B-1 — pure Hybrid semantic-kernel import

Status: implementation review candidate.

Date: 2026-07-11.

## Purpose

Stage 5B-1 imports the pure ALOR Hybrid model/orchestrator/riskgate modules into
a broker-neutral workspace crate without importing the integrated runtime
wrapper or opening any execution surface.

This is the first implementation slice after Stage 5A. It is not full
`HybridIntradayRuntime` attachment and cannot claim Stage 5 semantic parity.

## Added crate

```text
crates/strategy-runtime-core
```

The crate has only these external dependencies:

- `chrono`;
- `csv`;
- `serde`.

It has no dependency on:

- `broker-finam` or `finam-gateway`;
- FINAM DTOs;
- `reqwest`;
- `tokio`;
- a Redis client;
- HTTP/WebSocket transports;
- command consumers;
- real order endpoints.

The copied `risk_gate` module retains its pure Redis field/key encoding helpers
for source parity, but it does not import or call a Redis client.

## Imported source

Imported modules:

- `types`;
- `intraday_breakout`;
- `mean_reversion`;
- `high180`;
- `orchestrator`;
- `risk_gate`;
- module exports.

Source baseline:

```text
ALOR sanitized commit 43242c89944d335d9cb0729b38bdd7d658378d5e
```

Machine-readable path/hash/change mapping:

```text
crates/strategy-runtime-core/source-correspondence.toml
```

Six of the seven imported files, including the module export file, are
byte-identical to the frozen ALOR source. `orchestrator.rs` differs only in one
`#[cfg(test)]` namespace import and its rustfmt layout. No production formula,
constant, branch, state transition or test expectation was changed.

## Preserved semantic coverage

The imported source tests cover:

- BO warmup/wait-hours and EOD exit;
- classic MR long signal, active window and tick rounding;
- high180 long/short midpoint conditions and max-hold exit;
- MR-first arbitration and overnight BO exit arming;
- riskgate seed parsing and identity validation;
- normal-append startup decisions;
- monotonic/weekday ledger validation;
- current/next-session gate calculations;
- materialized-state reconstruction and field round trips.

Current imported test result:

```text
27 passed; 0 failed
```

## Boundary scanner

The global forbidden-surface scanner now checks the new semantic-kernel crate
for transport/runtime dependencies and endpoint tokens. It rejects additions
of FINAM, reqwest, Tokio, Redis-client, network/process, HTTP method or order
endpoint surfaces to this crate.

Stage 5B-1a additionally locks the accepted source manifest in scanner code,
requires source-equal hashes for `CopiedUnchanged`, validates the exact
`NamespaceOnly` production region, and rejects untracked source-file additions.
Updating the adjacent ledger can no longer authorize a formula change.

## Explicitly not imported

Stage 5B-1 does not import:

- `hybrid_intraday_runtime.rs` integrated wrapper;
- ALOR `Strategy` trait implementation;
- runtime state codec;
- strategy host and callbacks;
- Redis transport or consumer groups;
- broker ACK/order/position adapters;
- real stop/bracket implementation;
- any real FINAM send path.

## Safety boundary

Stage 5B-1 keeps:

```text
paper_boundary = true
runtime_live_ready_enabled = false
command_consumer_to_real_finam_enabled = false
strategy_driven_real_order_enabled = false
external_order_endpoint_enabled = false
real_post_delete_added = false
stop_sltp_bracket_execution_enabled = false
```

## Acceptance

Stage 5B-1 can be accepted when review confirms:

- source commit and hashes match Stage 5A;
- correspondence ledger matches target files;
- only the documented test namespace differs;
- all 27 imported tests pass;
- workspace tests/clippy and safety scanners pass;
- no integrated runtime or execution surface was imported;
- Stage 5 remains incomplete until wrapper, host, state, lifecycle and
  differential parity gates are accepted.

## Next

The next reviewable slice is Stage 5B-2: inventory and broker-neutral extraction
boundary for the integrated `HybridIntradayRuntimeStrategy` wrapper. Before the
wrapper can invoke the kernel, Stage 5C must provide callback-complete host
types and Stage 5D must provide field-complete state mapping.

The exact wrapper oracle and active high180 binding are frozen by
[`5b-1a-correspondence-oracle-profile-hardening.md`](5b-1a-correspondence-oracle-profile-hardening.md).
