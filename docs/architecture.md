# Architecture

The new project is a broker-neutral MOEX gateway, not a Finam-only port of the legacy Alor gateway.

## Shape

```text
strategy-runtime / operator tools
          |
          v
broker-runtime-bridge       broker-reconciliation
          |                          ^
          v                          |
broker-core contracts  <-------------+
          ^
          |
finam-gateway shadow/read-only publisher
          ^
          |
broker-finam adapter
broker-tbank adapter later
broker-alor-legacy read-only/reference later
```

## Rules

1. Strategy code must never depend on broker-native payloads.
2. Adapters own broker quirks: symbols, timestamps, order ids, stream replay, partial fills, and rate limits.
3. `broker-core` owns normalized contracts only.
4. Read-only and reconciliation come before order-emitting functionality.
5. Every order-emitting mode must be gated by explicit readiness and operator configuration.
6. M2a gateway publication is shadow/read-only: Redis health/readiness and broker-truth snapshots are allowed, while command consumers and live order endpoints remain disabled.
7. M3 order submission must persist id mapping before network calls and must not
   blindly retry after ambiguous place-order timeout.
8. M3 dry order-path work may build durable/local contracts and synthetic ACKs,
   but FINAM order endpoints remain blocked until the live-order gate is
   explicitly opened.

## Why not port the Alor gateway directly

The Alor implementation contains valuable operational lessons, but also broker-specific CWS/action-scoped details. The new gateway should carry forward:

- readiness wait semantics;
- freeze-intent timing discipline;
- broker-truth reconciliation;
- normalized order/trade/position concepts;
- explicit orphan/unmatched classification.

It should not carry forward:

- Alor CWS session behavior;
- Alor-specific order replay assumptions;
- Alor-specific stop-order cleanup semantics;
- legacy portfolio/config hacks.
