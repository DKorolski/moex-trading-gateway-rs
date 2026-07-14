# Stage 5D-a2 — controlled additive freeze-extension design

Status: extension principle accepted, pending Stage 5D-a3 enforcement and
exact type-state bridge. Scope: design-only, no production source changes.

Stage 5D-a accepted the persistence ownership inventory, but final acceptance
is blocked until the implementable code/API seam is chosen. This slice defines
that seam.

## 1. Decision

Stage 5D will use a controlled additive freeze extension.

This does not reopen Stage 5C trading semantics and does not change the Stage
5C public type-state API. It does, however, formally updates the source-hash
baseline for a narrow set of files because the persistence bridge is impossible
to implement with the previous exact source hashes unchanged.

## 2. Files allowed to change in the extension

Only these files may change for the Stage 5D bridge:

| File | Allowed additive change |
| --- | --- |
| `crates/strategy-runtime-core/src/lib.rs` | Register a separate `stage5d_persistence` module and export only `Stage5d*` public DTO/API symbols. |
| `crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs` | Add crate-private runtime-private snapshot export/apply operations for Stage 5D. |
| `crates/strategy-runtime-core/src/stage5c_paper_host.rs` | Add crate-private Stage 5D transition/mapping helpers that preserve the linear Stage 5C type-state chain. |
| `crates/strategy-runtime-core/src/stage5d_persistence.rs` | New module containing the Stage 5D persistence envelope, runtime-private snapshot DTO, validation gates, and bridge orchestration. |

Any other production source change is out of scope unless a later review opens
it explicitly.

## 3. Public API policy

The Stage 5C public API remains frozen by shape:

- all accepted 95 Stage 5C public symbols remain present;
- Stage 5C symbol names, signatures, fields and type-state kinds do not change;
- no public `into_parts`, raw strategy extractor, public state mutator, or
  public Stage 5C capability constructor is added;
- no public method exposes runtime-private fields;
- public additions must be separate `Stage5d*` symbols.

The previous Stage 5C source hashes remain archived as the Stage 5C closure
baseline. The extension creates a new versioned source baseline:

```text
Stage 5C closure baseline: accepted frozen public API and source hashes.
Stage 5D additive baseline: same public Stage 5C API, plus reviewed private
bridge and Stage5d* exports.
```

The Stage 5C API checker must continue to validate the 95 Stage 5C public
symbols. A Stage 5D checker/manifest must separately pin:

- the Stage 5D public `Stage5d*` symbols;
- the additive source hashes for the four files in section 2;
- an evidence statement that Stage 5C public API shape is unchanged.

## 4. Runtime bridge

`hybrid_intraday_runtime.rs` needs crate-private operations equivalent to:

```text
export_stage5d_runtime_private_snapshot(...)
apply_stage5d_runtime_private_snapshot(...)
```

These operations return/apply a versioned DTO owned by Stage 5D, not by Stage
5C. The DTO must cover restart-sensitive private fields that are not fully
represented by `StrategyState`, including:

- pending-entry semantic payload: reason, entry style, stop price, take price,
  target quantity;
- partial-entry timeout clock;
- pending-exit owner and reason;
- bracket terminal reconciliation timer;
- cleanup stop retry count;
- broker working order and stop-order sets;
- last processed semantic/event watermarks;
- pending riskgate finalization vector.

The bridge must be crate-private. External crates may receive only validated
Stage 5D envelopes or redacted diagnostics, never raw strategy internals.

## 5. Type-state bridge

`stage5c_paper_host.rs` needs crate-private Stage 5D transition helpers. The
implementation can choose either specialized mapping functions or one explicit
Stage 5D transition per lifecycle point, but it must preserve the Stage 5C
linear capability discipline.

The required capability shape is:

```text
accepted Stage 5C capability
→ Stage 5D validates/applies private extension or riskgate state
→ next accepted Stage 5C capability
```

The helper must not:

- expose raw strategy parts publicly;
- let external crates construct Stage 5C capabilities;
- skip existing Stage 5C admission/bootstrap/restore/warmup/recovery gates;
- reorder accepted Stage 5C callbacks.

Minimum bridge points:

| Stage 5C lifecycle point | Stage 5D purpose |
| --- | --- |
| Loaded/restored state before broker-truth bootstrap | Apply validated semantic state and runtime-private extension when safe. |
| After broker-truth bootstrap | Verify broker-owned order/position truth before runtime re-entry. |
| Before/around riskgate callback | Inject authoritative materialized `RiskGateRuntimeState` without trusting cache fields. |
| Before bounded paper loop admission | Confirm warmup/history/event watermarks and extension integrity. |

## 6. Riskgate injection rule

Riskgate authority order remains:

```text
durable ledger
→ materialized projection
→ StrategyState cache fields
→ diagnostics
```

Stage 5D-a2 chooses deterministic contradiction handling:

| Contradiction | Policy |
| --- | --- |
| StrategyState cache differs from reproducible materialized state | Overwrite cache from materialized state and emit restore evidence warning. |
| Ledger identity/profile/model/session-policy mismatch | Block restore. |
| Ledger generation/tail hash mismatch | Block restore. |
| Last finalized session reversal | Block restore. |
| Ledger row count mismatch but ledger is reproducible and tail hash matches | Rebuild materialized state and emit evidence warning. |
| Ledger row count mismatch and tail hash is not reproducible | Block restore. |
| Pending finalization outbox ahead of ledger with matching identity | Retry idempotent append. |
| Pending finalization outbox ahead of ledger with stale identity | Block restore. |

## 7. Additional runtime-private field classification

These fields are not blockers for paper-only Stage 5D, but the bridge must
classify them before DTO freeze:

| Runtime-private field/family | Policy |
| --- | --- |
| `last_warmup_log` | Diagnostic only; safe reset. |
| `startup_live_replay_boundary_ts_utc` | Live-only; must remain unset while runtime-live is closed. |
| `startup_replay_suppressed_bars` | Live-only diagnostic; must remain unset while runtime-live is closed. |
| `high180_mr` day aggregates | Derived exclusively from canonical warmup/history. |
| `risk_gate_shadow_mr` day aggregates | Derived from canonical warmup plus current shadow session. |
| Orchestrator engine internals | Rebuilt from semantic state plus canonical warmup; any contradiction blocks. |

## 8. Broker-object recovery indexes

`known_order_ids` remains a broker-order recovery index. Stage 5D-a3 chooses
the concrete design before DTO/API freeze:

```text
known_order_ids: Vec<BrokerOrderId>
known_stop_order_ids: Vec<BrokerStopOrderId>
known_trade_ids: Vec<BrokerTradeId>
known_client_order_ids: Vec<ClientOrderId>
pending_requests: Vec<StrategyRequestId>
```

A plain untyped string list is not acceptable for mixed broker objects.

## 9. Split-brain protection

`write_generation` is necessary but not sufficient. Stage 5D persistence
requires backend atomicity:

```text
expected_previous_revision
+ single_writer_generation
+ atomic compare-and-swap / lease
```

A writer must not persist a new envelope unless the backend confirms the
expected previous revision still matches. Failed compare-and-swap blocks or
retries through a reviewed recovery path; it must not silently overwrite.

## 10. Required gates for implementing the extension

Stage 5D-b may start only after a reviewed Stage 5D-a3 package proves:

1. the 95 Stage 5C public symbols/signatures are unchanged;
2. no new public extractor exposes opaque Stage 5C state;
3. runtime-private snapshot DTO is reachable only through the Stage 5D bridge;
4. export/restore round-trip covers partial entry, pending exit, bracket timer,
   cleanup retries, and riskgate finalization vector;
5. invalid/corrupt extension blocks before callback application;
6. riskgate injection preserves the linear Stage 5C capability chain;
7. Stage 5C callback order is unchanged;
8. BO/MR/high180/orchestrator/riskgate formulas are unchanged;
9. dual-baseline enforcement pins Stage 5D `Stage5d*` API, additive source
   hashes, approved bridge regions, and the historical Stage 5C baseline
   reference;
10. Redis, FINAM, transport, dispatch, runtime-live and broker execution remain
    closed.

## 11. Resulting roadmap

Stage 5D-a2 chooses the controlled additive-extension principle. Stage 5D-a3
chooses the enforcement migration and exact type-state seam. If Stage 5D-a3 is
accepted:

1. Stage 5D-b — Stage 5D manifest/checker plus versioned envelope DTO/API.
2. Stage 5D-c — runtime-private snapshot DTO fixtures and corruption gates.
3. Stage 5D-d — riskgate ledger/materialized-state round-trip fixtures.
4. Stage 5D-e — restore invariant matrix for flat/pending/open/safe-mode cases.
5. Stage 5D-f — durable external broker-event accumulation design for Stage
   5C-n terminal-complete batches.

Stage 5D remains no-send and paper-only until a later separately reviewed gate
opens additional surfaces.
