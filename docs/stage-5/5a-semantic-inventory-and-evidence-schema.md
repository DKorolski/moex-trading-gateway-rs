# Stage 5A — semantic inventory and evidence schema

Status: review candidate / source-bound planning foundation.

Date: 2026-07-11.

## 1. Purpose

This document freezes the source, callback, state, configuration, fixture and
evidence inventory for IMOEXF `HybridIntradayRuntime` migration. It prevents a
partial or independently rewritten strategy from being presented as ALOR
semantic parity.

Stage 5A changes documentation only. It does not import strategy source, invoke
the real strategy, consume runtime commands or call FINAM order endpoints.

## 2. Source baseline and provenance

The source oracle is the sanitized ALOR repository at commit:

```text
43242c89944d335d9cb0729b38bdd7d658378d5e
```

Normative source files:

| Relative source path | SHA256 | Role |
| --- | --- | --- |
| `strategy-runtime/src/strategies/hybrid_intraday_runtime.rs` | `6e15ab1b7212c56d3ecd8397b2d8991c1feccbde8eaa5e3d0051aec82a55f0aa` | Integrated runtime wrapper/state machine. |
| `strategy-runtime/src/strategies/hybrid_intraday/intraday_breakout.rs` | `a3b125f282f201b66dfa8d2685f22aa94048856a5145d537b76dc8934a5f9ae5` | BO model semantics. |
| `strategy-runtime/src/strategies/hybrid_intraday/mean_reversion.rs` | `4aecdeeb0bee8bcbae10cd2596c13d4450885b4ad7a8899346b14d743d4039ab` | Classic MR model support used by the orchestrator family. |
| `strategy-runtime/src/strategies/hybrid_intraday/high180.rs` | `e1f39a3afdf9745682682da0083f97ac0fa5361f979525d5ea383d6a6aa64456` | Target high180 MR semantics. |
| `strategy-runtime/src/strategies/hybrid_intraday/orchestrator.rs` | `db4dfdb014592d99567db9239c84b02c7f61b7eb768ee97a9203bead1c8ed1c0` | BO/MR arbitration and ownership. |
| `strategy-runtime/src/strategies/hybrid_intraday/risk_gate.rs` | `c85779ec5023e602cb6088e116fb58ed0bc80c31828499a0bd4557e2034dee34` | Riskgate model helpers and tests. |
| `strategy-runtime/src/strategies/hybrid_intraday/types.rs` | `8b515e252bc493890483793887248a6a12bedcf072ab87c574d4d3efd3b7eedc` | Shared semantic types/actions. |
| `strategy-runtime/src/strategies/hybrid_intraday/mod.rs` | `c70e3847f1a99e00c5d078d19b7b5f103d9b4d26853886b0b47d4805818ac84c` | Module boundary. |
| `strategy-runtime/src/strategy_host.rs` | `191ac773cd24cd3f6598815e576639e7c1e7cfdcf793d5ce725c2714982148de` | Strategy callback/event contract. |
| `strategy-runtime/src/state.rs` | `c90f631264a0062462275127bf900a94a1fd5e6a24255d31fa676bbd412d469f` | Persisted runtime state contract. |
| `strategy-runtime/src/risk_gate_store.rs` | `bea24a3d8dbb32124f19d4b11bc3cdab01a2e0a3bb3b4189212a84c1c0f74179` | Riskgate persistence/session finalization. |

The scoped source is approximately 9,326 lines, including a 6,203-line
integrated runtime wrapper. This is evidence that the migration is not only a
copy of the small BO/MR formula modules.

The exact integrated wrapper is included in this repository at
`source-oracles/alor-stage5/hybrid_intraday_runtime.rs`. The previously reviewed
6,113-line file with SHA256 `9704181e...` is the direct parent commit
`f7525987...`. The selected `43242c8...` oracle adds the later MR bracket
terminal-reconciliation hardening. The source mismatch is therefore resolved
in favor of the newer sanitized commit, not treated as an unexplained hash
change.

The target live-profile semantics were characterized from the sanitized
account-aliased config role:

```text
configs/runtime.hybrid.live.<ACCOUNT_ALIAS>.riskgate-shadow.toml
sha256=b3c3a7b940a3c082a9925faa3dc3a6bb01ca988d4fc7a478bb733daf35bceeef
```

A reproducible synthetic/redacted semantic projection is included at
`config/imoexf-hybrid-high180-profile.redacted.toml` with SHA256
`15e31d7a285f1c8c80e9168a9098e37e56bbd60ab3ab3264592d23605708dfe4`.

No live account id, token, Redis payload or absolute local source path may be
copied into Stage 5 evidence or source handoffs.

## 3. Source migration classification

| Source area | Stage 5 decision | Required boundary |
| --- | --- | --- |
| Hybrid pure model modules | Reuse/migrate with behavior-preserving edits only. | No FINAM/Redis/HTTP dependency. |
| `HybridIntradayRuntimeStrategy` wrapper | Migrate to broker-neutral source contracts. | Preserve callback/state semantics; replace broker ids with typed string ids. |
| ALOR `Strategy` trait | Use as semantic oracle; map to a broker-neutral Stage 5 host trait. | Callback complete before strategy invocation. |
| ALOR protocol DTOs | Do not import as strategy truth. | Map to accepted `broker-core` DTOs/enums. |
| Runtime state codec | Migrate meaning and backward-compatible decode. | No lossy id conversion. |
| Riskgate store | Extract broker-neutral ledger/state behavior. | Redis implementation stays outside semantic kernel. |
| Runtime Redis transport | Do not import in Stage 5 semantic kernel. | Stage 7 owns command-consumer transport. |
| ALOR gateway/control WebSocket | Not part of Stage 5. | FINAM real transport remains closed. |
| Stop/bracket broker execution | Preserve semantic state/intents only. | Real capability remains Stage 13. |

Every Stage 5B source file must have a correspondence record with source path,
source hash, target path, change class and reviewer-visible semantic impact.

Allowed change classes:

- `CopiedUnchanged`;
- `NamespaceOnly`;
- `BrokerNeutralTypeMigration`;
- `HostBoundaryExtraction`;
- `PersistenceBoundaryExtraction`;
- `CompatibilityFixSeparatelyApproved`.

`FormulaRewrite`, `ParameterChange` and `BehavioralSimplification` are not
allowed under Stage 5B without a separate reviewed decision.

## 4. Callback inventory

The broker-neutral host must represent the complete source callback surface.

| ALOR callback/contract | Semantic role | Stage 5 disposition |
| --- | --- | --- |
| `on_bar` | Warmed final-bar decision and day/session progression. | Required; canonical final M10 only. |
| `on_ack` | Pending/deferred/reject/accept lifecycle. | Required before full strategy invocation. |
| `on_order` | Fill/terminal/working-order lifecycle and ownership. | Required with `BrokerOrderId(String)`. |
| `on_stop_order` | Protective semantic lifecycle and escalation state. | Required as paper semantic event; no FINAM stop endpoint. |
| `on_position` | Broker position truth, partial fills, flat/open transitions. | Required and instrument scoped. |
| `on_timer` | Timeouts, deferred retries, repair/escalation. | Required with monotonic event time. |
| `on_bootstrap_snapshot` | Position/order adoption and safe-mode decisions. | Required after accepted Stage 4 broker truth. |
| `on_runtime_state_restored` | Pending/known-id restore and stale-tail policy. | Required after bootstrap notification. |
| `warmup_from_history` | Day features and signal-engine warmup. | Required before eligible live/paper callback. |
| `tracked_order_ids` | Runtime-owned order attribution. | Required with typed broker ids. |
| `intent_comment_tag` | Strategy/cycle/owner/role attribution. | Preserve semantic tag shape; redact evidence. |
| `on_command_prepared` | Exact host-built request-id handoff. | Required broker-neutral seam before Stage 5G. |
| `on_intent_blocked` | Explicit rollback/keep-state behavior. | Required broker-neutral seam before Stage 5G. |
| `pending_request_ids` | Pending recovery and exact ACK identity. | Required before deterministic restart tests. |
| `exit_risk_status` | Open-risk/repair/manual intervention state. | Required for close-only paper policy. |
| `risk_gate_session_finalizations` | Pending ledger appends. | Required. |
| `acknowledge_risk_gate_session_finalizations` | Idempotent finalization acknowledgement. | Required. |
| `on_risk_gate_state` | Restored ledger-derived gate state. | Required before MR parity. |
| `drain_observation_journal_records` | Redacted strategy observations. | Preserve through a broker-neutral optional seam. |
| `state` / `set_state` | Persistent semantic snapshot/restore. | Required, field complete and backward compatible. |

### Identified host-contract gap

At the frozen source commit, the target Hybrid implementation overrides the
main bar/broker/bootstrap/riskgate/state callbacks, but does not override
`tracked_order_ids`, `on_command_prepared`, `on_intent_blocked`,
`pending_request_ids`, `exit_risk_status`, or
`drain_observation_journal_records`. The generic host has these seams, and
other hardened strategies use several of them.

Stage 5 must not silently assume the target hybrid already has exact prepared-id
and blocked-intent behavior. Stage 5C must choose and test an explicit hybrid
policy while preserving existing observable behavior. Any semantic change
requires a compatibility fixture and review classification.

## 5. Runtime state field ledger

All fields below are semantically required unless a later review accepts an
explicit Stage 5 waiver.

### Cycle, ownership and position

- `active_cycle_id`;
- `next_cycle_seq`;
- `last_position_qty`;
- `current_owner`;
- `current_side`.

### Pending entry and deferred entry

- owner, side, cycle id and exact request id;
- created/deferred event timestamps;
- entry style and reason;
- stop/take semantic prices;
- original deferred request identity.

### Pending exit and deferred exit

- exact request id and created timestamp;
- owner, reason and cycle id;
- deferred timestamp and original request identity.

### Protective and repair state

- pending TP/SL request identities and timestamps;
- TP broker-order id;
- stop-order id and exchange broker-order id;
- SL triggered timestamp;
- MR take/stop prices;
- repair deadline/next retry/backoff/attempts.

These fields are required for state parity even though real protective order
execution remains disabled.

### Safety and readiness state

- `safe_mode_close_only` and reason;
- `entry_ready`;
- dirty-start/adoption/manual-intervention state represented by the accepted
  Stage 4 wrapper.

### Day/session feature state

- last processed/final bar and close;
- previous-day close/range/return;
- last/current local day;
- current-day high/low/close;
- day-before close and today-start marker;
- long/short-today flags;
- overnight-exit armed date;
- startup replay boundary/suppression state when externally represented.

### Riskgate state

- shadow session date/PnL/trade count;
- shadow entry timestamp/price/side/target/stop;
- pending session finalization date/PnL/trade count;
- MR enabled current/next session;
- rolling LB120 sum;
- last finalized session date;
- ledger row count and profile identity.

### State acceptance rule

JSON compatibility alone is insufficient. For every fixture:

```text
old state -> migrated state -> restored runtime -> emitted snapshot
```

must preserve the same owner/cycle/pending/deferred/safety/day/riskgate meaning.

## 6. Target configuration ledger

The Stage 5 target profile is frozen as:

| Group | Required values/policy |
| --- | --- |
| Runtime | IMOEXF, M10, paper/no-send, broker truth before state, no live authorization. |
| Profile | `imoexf_primary_riskgate_high180_lb120`. |
| MR | `high180`; current long/short range, entry, take and stop coefficients preserved from the source profile. |
| MR timing | Model session and MR cutoff/exit-offset behavior preserved. |
| BO | Existing `k`, stop ranges, big-move threshold, minimum-range mode/value and wait-hours preserved. |
| Arbitration | Existing MR-first/BO eligibility, one-owner and no-overlap semantics preserved. |
| Riskgate | `shadow_pnl_lb120_positive`, `normal_append`; not a new live authorization gate. |
| Session | Moscow timezone, accepted trading-period/clearing exclusions, weekends off. |
| Lifecycle | Pending, partial-fill, repair, escalation and backoff timeouts preserved semantically. |
| Order style | Paper intent shape may model the configured market style; no real send. |

### Active target high180 semantics

For `profile=imoexf_primary_riskgate_high180_lb120` and
`mr_variant=high180`, the integrated wrapper constructs
`High180MrEngine::new(High180MrConfig::default())` and feeds its candidate into
`on_bar_with_mr_override`. These are the active MR entry/exit parameters:

| Parameter/rule | Frozen active value |
| --- | --- |
| `min_rel_range` / `max_rel_range` | `0.005` / `0.050` |
| `k_long` / `k_short` | `0.085` / `0.090` |
| `stop_loss_mult` | `7.0` |
| `max_hold` | `180 minutes` |
| `entry_end_time` | `11:59:59` |
| Take target | Current-day high/low midpoint. |
| Stop distance | Seven times the entry-to-midpoint distance. |
| Exit rule | Midpoint take, stop, or max-hold timeout. |

### Classic MR source-compatible configuration

The following `MeanReversionConfig` values remain present in the source config
and runtime object, but they are not the active entry formula when
`mr_variant=high180`:

| Parameter | Frozen source value |
| --- | --- |
| Classic MR long `min_range` / `max_range` / `k` / `take_k` / `stop_k` | `0.013` / `0.035` / `0.032` / `0.11` / `0.44` |
| Classic MR short `min_range` / `max_range` / `k` / `take_k` / `stop_k` | `0.010` / `0.045` / `0.055` / `0.16` / `0.43` |
| Classic `session_end_time` / configured `exit_offset` | `11:59:00` / `10 minutes` |

Classic timing does not replace the active high180 max-hold exit. It remains
source-compatible fallback/configuration for the classic variant.

### Shared profile, BO and lifecycle values

| Parameter | Frozen source value |
| --- | --- |
| `model_session_start_time` / `model_session_end_time` | `09:00:00` / `23:49:59` MSK |
| BO `k` / `stop1_range` / `stop2_range` | `0.53` / `0.51` / `0.35` |
| BO `big_move_threshold` | `0.025` |
| BO `min_range` / mode | `1.01` / `absolute` |
| BO `exclude_weekends` / `wait_hours` | `true` / `3.0` |
| Breakout EOD mode / overnight exit | `same_day` / `09:30:00` |
| Timezone / weekends | `UTC+3` / off |
| Trading breaks | `14:00:00–14:04:59`, `18:50:00–19:04:59` MSK |
| Tick size | `0.5` |
| Repair deadline / SL escalation | `180s` / `30s` |
| Repair retries / base / max backoff | `3` / `5s` / `60s` |
| Pending timeout / partial MR entry timeout | `60s` / `3000ms` |
| Stop end buffer / max silence | `60s` / `1200s` |

The source config quantity and account/stream names are deployment values, not
signal-formula constants. Paper fixtures must declare a synthetic quantity
explicitly and test quantity-dependent partial-fill behavior. A later operator
deployment must separately bind the approved quantity; Stage 5 does not change
live sizing.

`normal_append` records riskgate model memory but does not enforce the MR gate
in the frozen source. Stage 5 must preserve that distinction: ledger/state
parity is required, while enabling `RiskGateMode::Enforced` would be a separate
behavioral change.

Stage 5B must record this exact redacted parameter set in tests or synthetic
fixtures. Parameter changes are strategy changes and cannot be hidden as type
migration.

## 7. Event and identity mapping

| Source concept | Target contract | Rule |
| --- | --- | --- |
| `BarEvent` | canonical runtime M10 input | Final, complete, gap-proven, session-eligible only. |
| `CommandAck` | broker-neutral runtime ACK | Exact `StrategyRequestId` matching. |
| numeric `OrderEvent.order_id` | `BrokerOrderId(String)` | Legacy numeric decode to decimal string; no surrogate. |
| `TradeEvent.trade_id` | `BrokerTradeId(String)` | Preserve broker-native identity. |
| position event | canonical target-instrument position | Non-zero target qty is open truth; zero is flat. |
| stop-order event | paper protective semantic event | No real FINAM stop endpoint in Stage 5. |
| wall/event timestamps | `RuntimeEventClock` plus event timestamp | Monotonic strategy time; no wall-clock decision drift. |
| strategy request id | `StrategyRequestId` | Pending clear only on exact match. |

## 8. Fixture matrix

### Bootstrap and restore

- flat clean startup;
- explicit adopted non-flat position;
- non-flat position without owner -> safe mode/manual intervention;
- target active order owned/adopted/unowned;
- unknown/orphan target order or trade;
- pending entry and pending exit restart;
- deferred entry and deferred exit restart;
- protective ids preserved;
- riskgate pending finalization restart.

### Warmup/event time/session

- insufficient warmup produces zero entries;
- full previous/current-day feature warmup;
- previous-day range and return parity;
- weekend bars do not become Monday anchor;
- session and clearing exclusions;
- older event after newer event;
- restart replay suppression;
- reconnect/gap first-bar entry block;
- duplicate final bar suppression.

### Atomic Hybrid decisions

- no signal;
- BO long/short entry and exit;
- MR long/short entry and time exit;
- simultaneous BO/MR candidates;
- BO-owned position suppresses MR;
- MR-owned position suppresses BO;
- one owner/one cycle invariant;
- flat owner reset;
- no implicit owner change after restart.

### Paper lifecycle feedback

- command prepared with exact request id;
- blocked entry rollback;
- exit/cancel/repair not silently dropped with open risk;
- accepted/rejected/duplicate/mismatched ACK;
- working/partial/filled/terminal order events;
- partial MR entry and partial BO entry have distinct behavior;
- fill -> position -> protective semantic intents;
- protective terminal/cleanup/repair and residual flatten semantics;
- deferred entry/exit reissue;
- pending timeout and timer progression;
- deterministic restart at every lifecycle boundary.

### Riskgate

- identical shadow trades produce identical ledger rows;
- session finalization is idempotent;
- restart does not duplicate a row;
- rolling LB120 parity;
- current/next-session MR flags parity;
- missing/inconsistent ledger blocks semantic acceptance.

## 9. Differential comparison policy

The normative Stage 5H comparison uses identical inputs. It compares after
every event:

- callback accepted/rejected reason;
- BO and MR candidate decisions;
- selected action/intent class;
- owner, side, cycle and sequence;
- pending/deferred and safe-mode state;
- day/session features;
- riskgate state/finalizations;
- paper feedback outcome;
- serialized semantic snapshot.

Same-session Stage 5I comparison is secondary. A bar or broker-truth difference
must not be mislabeled as a strategy formula difference.

Allowed divergence classes:

- `ExpectedDivergence`;
- `WaivedDivergence`;
- `MarketDataDivergence`;
- `BootstrapTruthDivergence`;
- `StrategySemanticDivergence`;
- `ImplementationGap`;
- `EvidenceIncomplete`;
- `SafetyBoundaryOpen`.

`StrategySemanticDivergence`, `ImplementationGap`, `EvidenceIncomplete` and
`SafetyBoundaryOpen` block Stage 5 closure unless a specific review accepts a
documented waiver. Safety boundary violations cannot be waived inside Stage 5.

## 10. Evidence schema

Generated evidence belongs outside clean source archives, for example:

```text
reports/stage-5/<session-or-fixture-id>/semantic-parity.json
```

Top-level redacted shape:

```json
{
  "schema_version": 1,
  "stage": "Stage5RealStrategySemantics",
  "substage": "Stage5A",
  "generated_at": "2026-07-11T00:00:00Z",
  "source_commit": "short-or-full-sha",
  "source_archive_name": "moex-trading-project-<sha>.zip",
  "source_archive_sha256": "sha256",
  "alor_source_commit": "43242c89944d335d9cb0729b38bdd7d658378d5e",
  "source_correspondence": {},
  "scope": {
    "instrument": "IMOEXF",
    "runtime_kind": "HybridIntradayRuntime",
    "profile": "imoexf_primary_riskgate_high180_lb120",
    "timeframe_sec": 600,
    "paper_boundary": true
  },
  "callback_coverage": {},
  "state_coverage": {},
  "configuration_coverage": {},
  "fixture_coverage": {},
  "same_input_comparison": {},
  "same_session_comparison": {},
  "safety_boundary": {},
  "raw_payload_exported": false,
  "status": "InventoryComplete"
}
```

Allowed Stage 5 top-level statuses:

- `InventoryComplete`;
- `SourceImportComplete`;
- `HostContractComplete`;
- `StatePersistenceComplete`;
- `LifecycleAttached`;
- `SemanticParityComplete`;
- `BlockedDivergence`;
- `EvidenceIncomplete`;
- `SafetyBoundaryOpen`.

No raw market-data bars, broker rows, Redis payloads, comments containing live
account ids, tokens, secrets or absolute local paths may be exported.

## 11. Stage 5A acceptance checklist

Stage 5A can be accepted when review confirms:

- source commit and normative file hashes are fixed;
- the exact integrated wrapper oracle is included and source-lineage mismatch
  with its direct parent is resolved;
- callback surface is complete;
- state field groups are complete;
- target configuration role is frozen and redacted;
- active high180 parameters are separated from inactive classic MR config;
- the host-contract gap is explicit;
- source correspondence policy forbids silent formula rewrites;
- BO/MR/riskgate acceptance is atomic;
- in-process paper feedback is distinguished from Stage 7 Redis consumer;
- same-input replay is normative and same-session shadow is secondary;
- Stage 5/6/7/8/13 boundaries are explicit;
- all real execution flags remain false.

## 12. Next implementation gate

After Stage 5A review acceptance, Stage 5B may begin with the pure semantic
module import and source correspondence ledger. The integrated runtime wrapper
must not be attached to the Stage 4 lifecycle until Stage 5C host contracts and
Stage 5D state mappings required by that wrapper are reviewable.
