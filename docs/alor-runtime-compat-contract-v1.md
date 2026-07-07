# ALOR runtime compatibility contract v1

Status: contract / implementation target.

Purpose: define the runtime-facing semantic contract that FINAM must satisfy
before it can replace the ALOR gateway for existing strategy-runtime systems.
This is not a FINAM live-order enablement document.

## Boundary

Allowed while implementing this contract:

- FINAM read-only and WebSocket market-data shadow;
- FINAM paper runtime;
- ALOR oracle reads for parity/seeding;
- paper/mock ACKs;
- local/CI fixtures and reports.

Still forbidden:

- continuous runtime-live;
- `command-consumer-to-real-FINAM`;
- strategy-driven real FINAM send;
- Stop/SLTP/bracket/replace/multi-leg;
- Runtime `LiveReady` for FINAM.

## Lifecycle order

FINAM runtime attachment must preserve the ALOR startup order:

1. `LoadBrokerTruthSnapshot`;
2. `LoadRuntimeState`;
3. `NotifyBootstrapSnapshot`;
4. `NotifyRuntimeStateRestored`;
5. `WarmupHistory`;
6. `RecoverPendingStreams`.

Acceptance:

- runtime state is never trusted before broker truth;
- warmup runs with live orders disabled;
- pending streams are recovered only after warmup;
- target-symbol active/unknown orders block live readiness unless explicitly
  adopted by strategy policy;
- account-wide rows are diagnostic only.

## Runtime-facing mappings

| Runtime-facing concept | Source of truth | FINAM/BrokerCore source | Required notes |
| --- | --- | --- | --- |
| Strategy symbol | Instrument registry | `InstrumentId` | Must include internal symbol, broker venue symbol, exchange, market. |
| Account/portfolio | Broker account map | `BrokerAccountId` | No implicit portfolio string assumptions. |
| Model bar | Market-data finality layer | canonical M10 `RuntimeBarInput` | Raw M1 must never become strategy model input. |
| Bar close time | Exchange/event time | `RuntimeBarInput.close_ts` | Must match ALOR closed-bar `close_time_utc` convention. |
| Position truth | Broker truth | target-symbol `BrokerPositionSnapshot` | Zero-qty rows are flat; account-wide row count is diagnostic. |
| Working order truth | Broker truth | active/unknown `BrokerOrderSnapshot` | Unknown or orphan orders block readiness. |
| Trade truth | Broker truth | `BrokerTradeSnapshot` | Must reconcile to request/client/broker order IDs. |
| Command request id | Runtime host | `StrategyRequestId` | Strategy pending clears only by exact request id. |
| Broker order id | Broker truth | `BrokerOrderId(String)` | String is source of truth; no lossy i64 unless durable surrogate policy is approved. |
| Client order id | FINAM order path | `ClientOrderId` | Must be durably mapped and collision-checked before send. |
| ACK | Runtime command lifecycle | `CommandAck` / broker-neutral ACK | Must preserve exact request-id parity. |
| Entry/exit class | Runtime host | `RuntimeIntentClass` | Entry gates must not silently block exit/cancel/repair. |
| Riskgate memory | Strategy/riskgate ledger | riskgate ledger/state | Must be real ledger integration or explicit oracle-seeded paper projection. |

## Market data contract

Strategy input is closed M10 bars.

Required:

- FINAM WS `BARS` M1 final bars are the online source;
- canonical M10 is built only from complete, final M1 buckets;
- stale backlog bars are not published as strategy live bars;
- reconnect/gap recovery must complete before accepting fresh live strategy
  bars;
- first fresh final live bar after restart is required before readiness can
  progress beyond market-data degraded states;
- weekend/non-tradable bars must not become strategy trading anchors.

Acceptance:

- FINAM canonical M10 and ALOR oracle M10 have matching close timestamps and
  OHLCV for the active session, or each divergence is classified;
- raw M1 is visible only for diagnostics/aggregation;
- gap after silence blocks entry but preserves exit/cancel/repair allowance.

## Broker-truth bootstrap contract

`BrokerTruthSnapshot` must be converted to `RuntimeHostBootstrapSnapshot` before
the strategy is trusted.

Required target-symbol rules:

- target non-zero qty is non-flat truth;
- target zero qty is flat even if a broker keeps a zero row;
- target active order is an adoption/manual-intervention candidate;
- target unknown status is a blocker;
- account-wide active orders are safety diagnostics, not target position truth.

Dirty-start policy:

- target flat + no active/unknown target orders: startup can continue;
- target non-flat + strategy can adopt: adopt with explicit state/audit;
- target non-flat + strategy cannot adopt: `manual_intervention_required`;
- orphan/unknown order/trade: readiness blocked until reconciled.

## Runtime state restore contract

Existing ALOR strategy state fields must keep their meaning:

- `active_cycle_id`;
- `next_cycle_seq`;
- `last_position_qty`;
- `current_owner`;
- `current_side`;
- pending entry/exit request ids;
- deferred entry/exit state;
- TP/SL ids when that future capability is enabled;
- day features;
- riskgate session/ledger summary.

Paper projection may be used only as a parity bridge. It must not be mistaken
for real strategy semantics until the real hybrid BO/MR orchestrator is attached.

## Request-id / client-order-id / broker-order-id chain

Before any strategy-driven real send:

```text
StrategyRequestId
  <-> ClientOrderId
  <-> BrokerOrderId(String)
```

must be durable and crash-safe.

Acceptance:

- duplicate `StrategyRequestId` does not create a second broker order;
- `ClientOrderId` collision blocks locally and emits manual-intervention
  diagnostics;
- ACK with mismatched request id never clears pending strategy state;
- broker order id string remains the authoritative broker identifier;
- restart restores the full mapping.

## Runtime command consumer contract

The next command-consumer stage must be paper/mock first.

Required flow:

```text
runtime command stream
  -> broker-neutral command
  -> live guard / instrument / account / risk preflight
  -> paper/mock ACK
  -> runtime-compatible ACK stream
```

Acceptance:

- runtime emits an intent;
- adapter receives it with exact request id;
- blocked entry rolls back or keeps state only by explicit strategy policy;
- exit/cancel/protective repair is classified separately from entry;
- XACK happens only after successful ACK/state publication or DLQ.

## Real FINAM command consumer gate

Not allowed until the paper/mock command consumer and broker-truth bootstrap are
accepted.

When eventually allowed, first scope is only:

- `MARKET`;
- `LIMIT`;
- `CANCEL`.

Still excluded from first runtime-live:

- stop;
- stop-limit;
- SLTP;
- bracket;
- replace;
- multi-leg.

## Observability contract

Every long-running FINAM runtime service must publish:

- health phase/reason;
- readiness phase/reason;
- market-data freshness;
- broker-truth freshness;
- order/trade stream freshness or polling SLA;
- token expiry/refresh diagnostics;
- rate-limit state;
- operator arm state;
- exact LiveReady blockers.

Daemon paths must not panic on recoverable failures. Internal failures must
degrade readiness, revoke operator live arm if applicable, and emit audit events.

## Acceptance for v1

This contract is accepted when:

- this field-level mapping is reviewed against ALOR runtime sources;
- at least one fixture maps ALOR runtime state into broker-core/paper runtime
  state without losing day/riskgate/cycle fields;
- FINAM paper state and ALOR runtime state can be compared field-by-field for
  IMOEXF hybrid;
- all divergences are classified as expected, implementation gap, or blocker.
