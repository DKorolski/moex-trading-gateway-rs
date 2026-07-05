# M4-3c0 broker-neutral observability contract

Status: source-only / no-live / no order endpoints.

M4-3c0 closes the semantic gap found while comparing the mature ALOR contour
with the new FINAM shadow contour. The goal is not to make both gateways write
the same raw Redis stream names. The goal is to make both contours map into the
same broker-neutral observability contract before the IMOEXF hybrid runtime is
attached to FINAM.

```text
ALOR raw gateway/runtime streams ──┐
                                  ├─ canonical observability contract
FINAM shadow REST/WS streams ──────┘
```

This remains a parity and migration layer. It does not authorize live runtime,
order commands, Stop/SLTP/bracket, or FINAM cutover.

## Boundary

M4-3c0 must not:

- send FINAM `POST /orders`;
- send FINAM `DELETE /orders/{id}`;
- enable command-consumer-to-real-FINAM;
- enable continuous runtime live;
- open or close a position;
- enable Stop/SLTP/bracket/replace/multi-leg;
- make cutover automatic.

The source contract reports `live_order_authorized = false`,
`command_consumer_to_real_broker_enabled = false`, and
`continuous_runtime_live_enabled = false` during this phase.

## Canonical channel set

Both contours must provide the same canonical channel kinds:

| Channel kind | ALOR raw source example | FINAM raw source example | Owner |
| --- | --- | --- | --- |
| `GatewayHealth` | `events.health` | `finam_shadow:health`, `finam_ws_shadow:health` | Gateway |
| `GatewayReadiness` | `events.health`, `/readiness` | `finam_shadow:readiness`, `finam_ws_shadow:readiness` | Gateway |
| `BrokerTruth` | `broker.orders.*`, `broker.trades.*`, `broker.positions.*`, `broker.snapshots.*` | `finam_shadow:portfolio:snapshot`, `finam_shadow:orders:snapshot` | Gateway |
| `MarketData` | `md.bars.<portfolio>.10m` | `finam_ws_shadow:market_data` plus derived `M1_TO_10M` | Gateway |
| `CommandAckLifecycle` | `cmd.acks.*` | disabled sentinel until real command consumer is approved | Gateway |
| `RuntimeState` | `runtime.state.hybrid_intraday.*` | future FINAM shadow runtime state stream | Runtime |
| `OpsConsumerGroups` | `XINFO GROUPS` on ALOR runtime/command streams | `XINFO STREAM`/future `XINFO GROUPS` on FINAM shadow streams | OpsCollector |

The separation is intentional:

- gateway state means broker connectivity, auth, stream freshness, degraded
  flags, and control-path readiness;
- broker truth means positions, orders, trades, cash, margin, and instrument
  identity;
- runtime state means strategy memory, dedupe, lifecycle phase, and pending
  strategy-owned references;
- consumer-group state means operational lag/pending ownership, not trading
  truth.

## Why runtime state is not gateway state

ALOR `runtime.state.*` entries are strategy-specific snapshots. For example,
the IMOEXF hybrid runtime stores the hybrid strategy state, risk-gate ledger,
pending IDs, and dedupe memory. FINAM must not fake this inside the gateway.

The canonical contract therefore requires `RuntimeState` to be owned by
`Runtime`, not `Gateway`. A gateway-owned runtime state is a blocker.

## Consumer groups as first-class ops evidence

ALOR already uses consumer groups for strategy runtime and gateway command
consumption. These are operationally important because lag or pending ownership
can explain apparent strategy drift even when broker truth is clean.

M4-3c0 models consumer-group evidence as `BrokerConsumerGroupSnapshot`:

```text
stream
group
pending
lag
entries_read
```

A group is clean only when `pending = 0` and `lag <= operator threshold`.

## IMOEXF hybrid runtime implication

After M4-3c0, the next allowed step is to prepare the IMOEXF hybrid runtime for
shadow/dry attachment to canonical streams. That is still not live trading.

The minimum next gates before an IMOEXF hybrid runtime attachment are:

1. ALOR raw streams and FINAM shadow streams map to the same canonical channel
   set.
2. FINAM M1 WebSocket market data is aggregated into canonical final 10-minute
   bars.
3. Derived FINAM 10-minute bars match ALOR `md.bars.*.10m` over reviewed
   parity windows.
4. Broker truth is instrument-scoped and clean for the target instrument.
5. Consumer-group / stream lag evidence is clean.
6. The runtime is connected only in shadow/dry mode with command emission
   disabled or routed to a dry command path.

Continuous runtime live remains blocked until 10-minute bar parity, broker
truth parity, runtime decision parity, order ACK lifecycle, kill-switch policy,
and operator arm are separately reviewed.

## Source artifacts

M4-3c0 adds broker-core source-level observability contracts:

- `BrokerObservabilityChannelKind`;
- `BrokerObservabilityOwner`;
- `BrokerObservabilityContract`;
- `BrokerObservabilityReadinessReport`;
- `BrokerObservabilityBlocker`;
- `BrokerConsumerGroupSnapshot`.

The source tests demonstrate:

- ALOR raw outputs and FINAM shadow outputs can map to the same canonical
  channel kinds;
- runtime state is strategy/runtime-owned, not gateway-owned;
- continuous live runtime remains blocked until parity evidence exists;
- early enabled live/order surfaces are rejected by the parity contract;
- consumer-group lag/pending snapshots classify clean vs unsafe state.

## Acceptance

M4-3c0 is ready for review when:

- broker-core observability tests pass;
- the evidence script reports all source markers present;
- forbidden-surface scanners remain green;
- no broker API, Redis, WebSocket, or live order calls are performed;
- handoff archive excludes `.env`, `.git`, `target`, `tmp`, `reports`, logs,
  and local deployment overrides.
