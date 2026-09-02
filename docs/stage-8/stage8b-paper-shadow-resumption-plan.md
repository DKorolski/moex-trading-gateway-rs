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

P0 deployment and an isolated Redis DB 15 synthetic M1-to-M10-to-runtime-state
smoke are complete. The smoke is reproducible with
`scripts/stage8b-paper-shadow-db15-smoke.sh`; it does not contact FINAM and
cleans DB 15 on exit.

Live read-only activation is also complete. A separate token whose FINAM token
details report `readonly=true` now drives final M1 bars into the isolated
namespace. Two complete M10 buckets produced committed paper-only runtime-state
batches with consumer `pending=0` and `lag=0`. This proves the live transport
and projection path only: the runtime is unseeded, so ALOR state parity and the
durable paper order/ACK lifecycle are not claimed. See
`stage8b-paper-shadow-readonly-live-activation-evidence.json`.

## Slice P1: deployable durable paper service

After independent acceptance of
`stage8b-p1-durable-paper-lifecycle-composition-design.md`, add a dedicated
single-owner composition around the accepted Stage 7B service with:

- one fixed paper namespace and consumer group;
- file-backed Stage 6/7 recovery ownership;
- an explicit paper-only outcome provider;
- ACK/DLQ/XACK settlement;
- health/readiness publication;
- restart and PEL recovery;
- no FINAM client or transport dependency;
- no broker dispatch capability.

The composition must also source a real Stage 5G clean-restart package and
commitment key and define one owner for semantic continuation and paper
order/trade/position outcomes. A thin CLI around test fixtures or P0 projection
JSON is explicitly insufficient.

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
