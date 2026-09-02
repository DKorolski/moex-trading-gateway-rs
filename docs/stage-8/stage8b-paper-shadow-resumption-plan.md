# Stage 8B paper-shadow resumption plan

Status: active operational plan, 2026-09-02.

## Outcome

Resume useful ALOR-to-FINAM parity work without treating the deferred native
installation proof as a paper prerequisite.

## Slice P0: deploy the existing projection

On the isolated VPS deploy:

```text
FINAM WS final M1
  -> isolated Redis finam_imoexf_paper:*
  -> canonical complete M10
  -> paper-only hybrid runtime projection
  -> health/readiness/runtime-state evidence
```

The executable surfaces are:

- `broker-cli finam-ws-shadow-loop`;
- `broker-cli finam-paper-runtime-consume`.

The runtime command must use `--strategy-invocation-shadow` but remains unable
to emit broker commands. ALOR oracle seeding is optional for transport smoke
and required for a claimed state-parity session.

## Slice P1: deployable durable paper service

Add a dedicated binary around the accepted Stage 7B service with:

- one fixed paper namespace and consumer group;
- file-backed Stage 6/7 recovery ownership;
- an explicit paper-only outcome provider;
- ACK/DLQ/XACK settlement;
- health/readiness publication;
- restart and PEL recovery;
- no FINAM client or transport dependency;
- no broker dispatch capability.

This slice requires independent review before VPS activation because it opens
a persistent Redis consumer, even though it remains paper-only.

## Slice P2: multi-session parity

For several sessions compare:

- FINAM-derived and ALOR-native final M10 bars;
- strategy-facing timestamps and session high/low/close;
- owner/cycle/side/pending state;
- paper intents and ALOR live command shape;
- paper ACK/order/trade/position lifecycle;
- restart and gap recovery;
- riskgate ledger/state.

## Pre-live-micro return gate

Before any strategy-driven live micro:

1. add reviewed systemd failed-unit diagnostics;
2. complete the deferred two-run native proof;
3. close full-session parity findings;
4. explicitly issue a separate live-micro authorization.

No paper result implicitly activates Generation 2 or authorizes FINAM order
effects.
