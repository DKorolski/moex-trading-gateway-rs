# Stage 5 — real strategy semantics attachment plan

Status: Stage 5A review candidate / planning and inventory foundation.

Date: 2026-07-11.

## 1. Purpose

Stage 5 attaches the real IMOEXF `HybridIntradayRuntime` semantics to the
accepted broker-neutral Stage 2–4 foundations. The target is deterministic
paper/shadow execution of the existing ALOR strategy semantics, not a new
FINAM-specific implementation of the strategy.

Stage 5 starts from these accepted inputs:

- Stage 2B broker-neutral runtime identity/state contract;
- Stage 3 final-M1 to canonical-final-M10 strategy-input contract;
- Stage 4 validated broker-truth bootstrap and lifecycle chain;
- the accepted source-migration ADR: `BrokerOrderId(String)` is authoritative;
- the Stage 1B IMOEXF compatibility freeze.

Stage 5 ends with the real hybrid strategy producing deterministic paper-only
semantic intents and state transitions. It does not connect those intents to a
real FINAM command consumer or real order endpoint.

## 2. Scope

In scope:

- instrument: `IMOEXF`;
- runtime: `HybridIntradayRuntime`;
- profile: `imoexf_primary_riskgate_high180_lb120`;
- canonical final M10 input only;
- Intraday Breakout (BO) semantics;
- high180 Mean Reversion (MR) semantics;
- BO/MR arbitration, ownership, no-overlap, cycle lifecycle;
- complete runtime callback and state semantics;
- event-time, warmup, session/day and weekend behavior;
- riskgate state/ledger semantics in `normal_append` paper mode;
- in-process deterministic paper broker feedback;
- same-input ALOR-vs-migrated-runtime differential replay;
- controlled same-session shadow evidence after offline parity is accepted.

Out of scope:

- USDRUBF, RI/RTS and `SessionGapStandalone`;
- real FINAM command consumption;
- strategy-driven FINAM `POST`/`DELETE`;
- durable `ClientOrderId` to `BrokerOrderId` execution chain;
- FINAM runtime `LiveReady`;
- continuous runtime-live;
- real stop/SLTP/bracket/replace/multi-leg execution.

Protective intent/state semantics may be represented and exercised by the
in-process paper harness. That representation is not permission to open a real
stop/bracket endpoint.

## 3. Architecture decision

The Stage 5 implementation must migrate/reuse the accepted ALOR source. It must
not independently reimplement BO/MR formulas for FINAM.

Required dependency direction:

```text
broker-core contracts
        |
        v
broker-neutral strategy host seam
        |
        v
migrated HybridIntradayRuntime semantic kernel
        |
        v
in-process paper/no-send sink and deterministic feedback
```

Forbidden dependency direction:

```text
strategy semantic kernel -> FINAM DTO / HTTP / WebSocket / Redis / secret
strategy semantic kernel -> real order endpoint
```

An adapter is allowed only as a typed broker-neutral host boundary. An `i64`
surrogate broker-order adapter or lossy broker-id conversion remains forbidden
without a new ADR.

## 4. Atomic hybrid acceptance rule

The ALOR runtime is one state machine. BO, MR, riskgate, owner/cycle state,
pending/deferred state and broker feedback are mutually dependent.

Implementation may use reviewable internal slices, but Stage 5 must not accept
any of these as strategy parity by itself:

- BO without MR arbitration;
- MR without the applicable riskgate state;
- BO/MR decisions without pending/ACK/order/position lifecycle;
- final state only, without event-by-event state transitions;
- oracle-seeded projection presented as real strategy execution.

BO/MR/riskgate semantics become accepted only after the complete orchestrator
passes the same-input differential harness.

## 5. Sub-stages

### Stage 5A — semantic inventory, source provenance and acceptance freeze

Deliverables:

- this macro plan;
- `stage-5/5a-semantic-inventory-and-evidence-schema.md`;
- source file hashes and source correspondence policy;
- callback, state, configuration and dependency ledgers;
- fixture and evidence matrices;
- explicit Stage 5/6/7/8/13 boundaries.

No strategy source is imported in Stage 5A.

### Stage 5B — controlled source import

Import the existing semantic source in two layers:

1. pure hybrid modules (`intraday_breakout`, `mean_reversion`, `high180`,
   `risk_gate`, `orchestrator`, `types`);
2. the runtime wrapper after replacing broker-coupled types with the accepted
   broker-neutral contracts.

Every changed source region must be classified in a source correspondence
ledger. Trading constants and formulas must remain unchanged unless a separate
reviewed compatibility fix explicitly authorizes a change.

Stage 5B is compile/unit-test only and cannot invoke real FINAM execution.

### Stage 5C — callback-complete broker-neutral host seam

Define the complete host surface before real strategy invocation:

- bootstrap snapshot and restored runtime state;
- history warmup and canonical final bar;
- ACK, order, stop-order semantic event, position and timer;
- riskgate state and session finalization acknowledgement;
- command prepared and intent blocked callbacks;
- pending request identity, exit-risk status and state snapshot/restore.

The stop-order callback may operate on paper semantic events. It must not gain a
real FINAM transport implementation in Stage 5.

### Stage 5D — full state and riskgate persistence

Implement semantic round-trip for the complete hybrid runtime state and the
riskgate ledger/state. Legacy numeric ALOR order ids must import as broker-order
strings; new state must use broker-neutral typed ids.

Acceptance includes clean flat, adopted non-flat, pending/deferred,
safe-mode/manual-intervention, protective-state and riskgate restart fixtures.

Current controlled sequence within Stage 5D is:

```text
5D-b2b-c1-r8 signed-zero/current-shadow/multi-frontier closure
 -> accepted review hardening
5D-b2b-d1-r3 controlled runtime-state-restored return closure
 -> canonical envelope export, round-trip and restart/crash fixture matrix
```

c1-r8/d1-r3 are no-I/O and accept only exact durable outbox-explained
projection lag. They prove signed-zero rejection, source-valid current-shadow
tuples, stepwise multi-row recovery-frontier executability with replay checks,
source-produced restored transitions, retained blocked-capability fingerprint
stability and type-state compile-fail boundaries. They do not pre-authorize
Redis, FINAM, transport or runtime-live.

### Stage 5E — Stage 4 lifecycle and event-time attachment

Attach the strategy only behind the accepted Stage 4 chain:

```text
validated broker truth
 -> runtime state restore
 -> bootstrap notification
 -> restored-state notification
 -> history warmup
 -> pending stream recovery
 -> first eligible strategy callback
```

Canonical final M10, monotonic event time, warmup completeness, session/day
rollover, weekend policy, reconnect/gap proof and first-fresh-bar rules are
mandatory gates. A blocked Stage 4 report must produce zero strategy callbacks.

### Stage 5F — atomic Hybrid orchestrator semantics attachment

Attach BO, high180 MR, riskgate and BO/MR arbitration as one semantic runtime.
Internal reviews may split pure BO, pure MR/riskgate and orchestrator wiring,
but no partial slice may claim hybrid semantic parity.

The output is a paper semantic intent/state transition only.

### Stage 5G — deterministic in-process paper lifecycle harness

Exercise the complete state machine without Redis command consumption and
without broker sends:

- entry -> accepted -> fill -> position;
- exit -> accepted -> fill -> flat;
- local block and explicit rollback/keep-state policy;
- reject, duplicate, mismatched request id and timeout/ambiguous outcomes;
- partial entry behavior;
- deferred entry/exit;
- restart while pending or open;
- semantic protective intent/cleanup/repair behavior.

This harness is semantic feedback inside Stage 5. The Redis paper/mock command
consumer remains Stage 7.

### Stage 5H — same-input offline differential replay

Run the ALOR oracle and migrated runtime with the same:

- canonical final M10 event sequence;
- initial broker truth;
- initial runtime state;
- riskgate ledger/state;
- deterministic clock and request-id seed.

Compare decisions and state after every event. Same-input replay is the
normative strategy parity gate because it isolates semantic differences from
broker market-data differences.

### Stage 5I — controlled same-session shadow evidence

After Stage 5H is accepted, compare ALOR and FINAM paper contours during the
same session. Differences must be classified separately as:

- `MarketDataDivergence`;
- `BootstrapTruthDivergence`;
- `StrategySemanticDivergence`;
- `ExpectedDivergence`;
- `WaivedDivergence`;
- `EvidenceIncomplete`.

Same-session evidence is operational evidence, not a replacement for the
same-input deterministic gate.

### Stage 5J — Stage 5 acceptance and closure

Stage 5 can close only when:

- the real migrated `HybridIntradayRuntime` is invoked;
- ALOR oracle data is not used as the migrated runtime's decision source;
- callback, state, riskgate and paper lifecycle matrices are complete;
- same-input replay has no blocking semantic divergence;
- restart/replay is deterministic;
- Stage 4 blockers suppress strategy invocation;
- paper semantic trace is deterministic and redacted;
- all real execution surfaces remain closed.

## 6. Identity and comparison policy

The following identities remain distinct:

- `StrategyRequestId`: strategy pending/ACK identity;
- `ClientOrderId`: future broker correlation identity (Stage 6);
- `BrokerOrderId(String)`: authoritative broker-order identity;
- `BrokerTradeId(String)`: authoritative broker-trade identity.

Within one deterministic contour, exact `StrategyRequestId` matching is
required. Cross-contour comparison may compare an exact request id only when
both contours use the same accepted deterministic seed/namespace. Otherwise it
must compare request role, cycle, intent class and internal exact-match
behavior, not unrelated broker/account-derived literals.

## 7. Safety boundary

All Stage 5 evidence and implementation must keep:

```text
paper_boundary = true
runtime_live_ready_enabled = false
command_consumer_to_real_finam_enabled = false
strategy_driven_real_order_enabled = false
external_order_endpoint_enabled = false
real_post_delete_added = false
stop_sltp_bracket_execution_enabled = false
raw_payload_exported = false
```

Any contradictory flag makes the Stage 5 result `SafetyBoundaryOpen` and
blocks acceptance.

## 8. Later-stage boundary

- Stage 6: durable request/client/broker identity chain;
- Stage 7: Redis runtime command consumer in paper/mock ACK mode;
- Stage 8: real FINAM execution under the accepted command consumer;
- Stage 9+: reconciliation, live readiness, dual-broker shadow and live micro;
- Stage 13: real stop/SLTP/bracket execution capability.

Stage 5 must not pull these capabilities forward.
