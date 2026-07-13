# Stage 5C — acceptance and API-freeze report

Status: revised review candidate after closure HOLD. Date: 2026-07-13.

This package closes the functional Stage 5C implementation and freezes the
paper/mock host API candidate. It does not add a new runtime facade and does
not open any execution surface.

Accepted implementation baseline:

- commit: `69cc73b7f33d8cb418c784ac993856d8a487693d`;
- handoff archive: `moex-trading-project-69cc73b.zip`;
- archive SHA256:
  `0b614ebe83b0a8af85cde0ca7a1ae481457813edad72626cd4bb5972c9c83f91`.

## 1. Stage 5C slice status

| Slice | Status | Accepted boundary |
| --- | --- | --- |
| 5C-a | accepted | Stage 4-bound paper host admission. |
| 5C-b | accepted | One-shot bootstrap notification facade. |
| 5C-c | accepted | Runtime-state restore facade. |
| 5C-d | accepted | Canonical history warmup facade. |
| 5C-e | accepted | Pending-stream recovery facade. |
| 5C-f | accepted | First semantic-bar facade. |
| 5C-g | accepted | Paper intent settlement / escrow. |
| 5C-h | accepted | Controlled next-bar loop. |
| 5C-i | accepted | Paper ACK lifecycle / escrow resolution. |
| 5C-j | accepted | Paper broker lifecycle facade. |
| 5C-k | accepted | Controlled paper timer facade. |
| 5C-l | accepted | Timer-result settlement. |
| 5C-m | accepted | Timer/bar continuation arbitration. |
| 5C-n | accepted | Bounded deterministic paper-loop coordinator. |

Functional Stage 5C implementation is complete. Stage 5D work should not start
until this acceptance/API-freeze package is reviewed and accepted.

## 2. Frozen source hashes

| Path | SHA256 |
| --- | --- |
| `source-oracles/alor-stage5/hybrid_intraday_runtime.rs` | `6e15ab1b7212c56d3ecd8397b2d8991c1feccbde8eaa5e3d0051aec82a55f0aa` |
| `crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs` | `767815903b8bc07ee48ac96a9d4dac74553b6c32ae63326e141743b12d98b65c` |
| `crates/strategy-runtime-core/src/hybrid_intraday/mod.rs` | `c70e3847f1a99e00c5d078d19b7b5f103d9b4d26853886b0b47d4805818ac84c` |
| `crates/strategy-runtime-core/src/hybrid_intraday/intraday_breakout.rs` | `a3b125f282f201b66dfa8d2685f22aa94048856a5145d537b76dc8934a5f9ae5` |
| `crates/strategy-runtime-core/src/hybrid_intraday/mean_reversion.rs` | `4aecdeeb0bee8bcbae10cd2596c13d4450885b4ad7a8899346b14d743d4039ab` |
| `crates/strategy-runtime-core/src/hybrid_intraday/high180.rs` | `e1f39a3afdf9745682682da0083f97ac0fa5361f979525d5ea383d6a6aa64456` |
| `crates/strategy-runtime-core/src/hybrid_intraday/orchestrator.rs` | `1e784411d348fcf090887f7f50062b0cbd34494912288100c1ca1d851d8d5bd9` |
| `crates/strategy-runtime-core/src/hybrid_intraday/risk_gate.rs` | `c85779ec5023e602cb6088e116fb58ed0bc80c31828499a0bd4557e2034dee34` |
| `crates/strategy-runtime-core/src/stage5c_paper_host.rs` | `c846d0b22cec5fe4482b080e705e066709c9b35df9d611a3ad6afdbc96f0f857` |
| `crates/strategy-runtime-core/src/lib.rs` | `dc6625f571f07954c85e397e0e9835ed64cc73b843c9ad3f6b89565d10295e25` |

## 3. Public API freeze candidate

The Stage 5C public API is the re-exported paper-host surface in
`crates/strategy-runtime-core/src/lib.rs`. The freeze candidate includes:

- admission/bootstrap: `admit_stage5c_paper_host`,
  `notify_stage5c_bootstrap`, `prepare_stage5c_without_runtime_state`;
- restore/warmup/recovery: `restore_stage5c_runtime_state`,
  `notify_stage5c_runtime_state_restored`, `accept_stage5c_history_batch`,
  `warmup_stage5c_history`, `accept_stage5c_pending_recovery_evidence`,
  `prove_stage5c_pending_recovery_claim`, `recover_stage5c_pending_streams`;
- semantic bar and intent settlement: `accept_stage5c_semantic_bar`,
  `apply_stage5c_semantic_bar`, `settle_stage5c_semantic_result`;
- lifecycle: `advance_stage5c_controlled_next_bar`,
  `resolve_stage5c_paper_intent_lifecycle`,
  `resolve_stage5c_paper_broker_lifecycle`,
  `settle_stage5c_broker_lifecycle_result`;
- timer and continuation: `resolve_stage5c_paper_timer`,
  `settle_stage5c_timer_result`,
  `advance_stage5c_timer_settlement_next_bar`,
  `advance_stage5c_timer_settlement_timer`;
- bounded coordinator: `advance_stage5c_paper_loop_once`.

The machine-readable companion manifest is:
`docs/stage-5/stage-5c-api-freeze-manifest.json`.

Manifest schema version 2 freezes the full Stage 5C public type-state surface,
not just the free functions:

- 95 public re-exported Stage 5C symbols;
- 22 public free functions with normalized signatures;
- 2 public constants;
- 71 public Stage 5C structs/enums, including public enum variants and public
  struct fields;
- 153 public methods with normalized signatures;
- 28 opaque public capability structs;
- 27 externally constructible public enums;
- normalized public-surface signature hash:
  `026cd7236db27936876c111352bd86c3a69b0b71faf3170897ecc785be175ae4`.

The manifest is enforced by `scripts/stage5c_api_freeze_check.py`, and
`scripts/forbidden_surface_scan.sh` invokes that checker as part of the
mandatory scanner path. Therefore future drift in source hashes, re-exports,
function signatures, public types, public fields, public variants, public
methods, opaque capability classification or executable evidence mapping is a
scanner failure.

## 4. Coordinator transition matrix

| State | Event | Result |
| --- | --- | --- |
| `PendingRecovered` | `FinalM10Bar` | `SemanticResult` via Stage 5C-f. |
| `SemanticResult` | `SettleSemanticResult` | `Settled` via Stage 5C-g. |
| `Settled` | `FinalM10Bar` | Controlled next-bar settlement via Stage 5C-h. |
| `Settled` | `Ack` | `IntentLifecycleResolved` via Stage 5C-i. |
| `IntentLifecycleResolved` | `BrokerLifecycleBatch` | `BrokerLifecycleResolved` only for terminal-complete Stage 5C-j batch. |
| `BrokerLifecycleResolved` | `SettleBrokerLifecycleResult` | `BrokerLifecycleSettlement`. |
| `BrokerLifecycleResolved` | `Timer` | Supported shortcut to `TimerResolved`; Stage 5C-k still checks unresolved lifecycle expectations, generated intent batches, expired broker truth and non-monotonic timer. |
| `BrokerLifecycleSettlement::GeneratedIntentBatch` | `Ack` | Re-enters Stage 5C-i. |
| `BrokerLifecycleSettlement::ReadyForTimer` | `Timer` | `TimerResolved` via Stage 5C-k. |
| `TimerResolved` | `SettleTimerResult` | `TimerSettlement` via Stage 5C-l. |
| `TimerSettlement::GeneratedIntentBatch` | `Ack` | Re-enters Stage 5C-i. |
| `TimerSettlement::ReadyForContinuation` | `FinalM10Bar` | Controlled next-bar settlement via Stage 5C-m. |
| `TimerSettlement::ReadyForContinuation` | `Timer` | Timer continuation via Stage 5C-m. |

Invalid state/event pairs fail closed and preserve the input state when a
recoverable state is available.

Explicitly blocked examples include:

- `GeneratedIntentBatch + Timer` before the generated ACK lifecycle resolves;
- `ReadyForTimer + Ack`;
- `IntentLifecycleResolved + Ack` without a terminal-complete broker batch;
- `UnresolvedBrokerLifecycle + Ack`;
- `UnresolvedBrokerLifecycle + Timer`;
- `PendingRecovered + Ack/Timer`;
- `SemanticResult + Ack/Timer`.

## 5. Callback coverage matrix

| Source callback | Stage 5C facade | Boundary rule |
| --- | --- | --- |
| Runtime state load | 5C-c or clean-start 5C-c bypass | Persisted runtime state is loaded and validated before bootstrap notification; clean start uses `prepare_stage5c_without_runtime_state`. |
| Bootstrap notification | 5C-b | One-shot, admission-bound broker-truth bootstrap after runtime-state load/preparation; no later lifecycle step. |
| Runtime-state-restored callback | 5C-c | Emitted after bootstrap notification; stale broker IDs are removed before restore notification completes. |
| History warmup | 5C-d | Canonical history only; provenance/freshness checked. |
| Pending stream recovery | 5C-e | Pending streams claimed before semantic bars. |
| Final semantic bar | 5C-f/5C-g/5C-h | Closed final M10 bar only; paper intent batch is escrowed. |
| Broker ACK | 5C-i | Exact request ID coverage; ACK cannot replace `StrategyRequestId`. |
| Broker order | 5C-j/5C-n | Terminal-complete batch only in coordinator; no partial callback mutation. |
| Broker stop order | 5C-j/5C-n | Terminal-complete batch only; stop/exchange IDs stay separate. |
| Broker position | 5C-j/5C-n | Position confirmation is required for execution lifecycles. |
| Timer | 5C-k/5C-l/5C-m | Controlled paper timer only; generated intents re-enter ACK lifecycle. |

## 6. End-to-end deterministic scenario

The accepted Stage 5C path is:

```text
admission
→ load persisted runtime state, or prepare clean runtime state
→ bootstrap notification with broker truth
→ runtime-state-restored callback
→ canonical history warmup
→ pending recovery
→ accepted final M10 bar
→ semantic result
→ intent settlement
→ ACK lifecycle
→ terminal-complete broker batch
→ broker lifecycle settlement
→ generated broker intent ACK/lifecycle, when generated
→ timer
→ timer settlement
→ next final M10 bar or timer continuation
```

This path remains in-process, paper-only and deterministic. It has no Redis
consumer, no broker transport, no FINAM command consumer and no real order
endpoint.

The persisted-state startup path is:

```text
admit_stage5c_paper_host
→ restore_stage5c_runtime_state
→ notify_stage5c_bootstrap
→ notify_stage5c_runtime_state_restored
→ accept/warmup history
→ pending recovery
```

The clean-start startup path is:

```text
admit_stage5c_paper_host
→ prepare_stage5c_without_runtime_state
→ notify_stage5c_bootstrap
→ notify_stage5c_runtime_state_restored
→ accept/warmup history
→ pending recovery
```

Executable evidence is machine-readable in
`docs/stage-5/stage-5c-api-freeze-manifest.json` under
`executable_evidence_map`. The API-freeze checker verifies that every mapped
transition points to an existing Stage 5C regression test. The map covers
startup ordering, history warmup, pending recovery, semantic-bar settlement,
controlled next-bar progression, ACK lifecycle, terminal-complete broker
lifecycle, generated-intent ACK re-entry, timer settlement, timer continuation
and blocked transition preservation.

## 7. External buffering contract

Stage 5C-n intentionally accepts broker lifecycle only as a terminal-complete
batch. Therefore future stream bridges must:

- durably accumulate partial broker events outside Stage 5C;
- preserve global ordering and event identity;
- avoid treating a working-only event as a completed strategy lifecycle;
- pass exactly one terminal-complete canonical batch to Stage 5C-n;
- retry with the preserved `IntentLifecycleResolved` state when Stage 5C-n
  returns `BrokerLifecycleIncompleteBatch`.

This is a Stage 5D+/Redis concern, not a Stage 5C execution feature.

## 8. Still closed

Stage 5C acceptance does not authorize:

- autonomous paper loop;
- Redis stream bridge or consumer groups;
- intent sink;
- broker transport;
- FINAM command consumer;
- real POST/DELETE order endpoints;
- runtime-live;
- broker-side Stop/SLTP/bracket execution.

## 9. Nonblocking backlog

- Refactor Stage 5C-j and Stage 5C-n to share one crate-private broker
  canonical preflight helper.
- Define explicit aggregated error precedence for multi-problem broker batches.
- Add post-terminal broker-status regression hardening, such as
  filled/position followed by late working.
- Add an explicit no-callback fingerprint regression for incomplete broker
  batch rejection.
- Design the durable external broker-event accumulation policy in Stage 5D.

## 10. Closure decision requested

Requested reviewer decision:

```text
Stage 5C acceptance/API-freeze package: accepted.
Stage 5C formally closed.
Stage 5D may start with design/persistence work only.
```

If accepted, the next macro-substage is Stage 5D: state/riskgate persistence
and durable stream/buffering design. It must still keep FINAM execution,
runtime-live and broker-side protective order execution closed until separately
reviewed.
