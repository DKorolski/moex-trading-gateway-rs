# Stage 8B paper-shadow VPS runbook

Status: deployment runbook for the isolated transport/runtime projection.

## Boundary

This stand contains two processes only:

1. read-only FINAM WebSocket M1 market data;
2. M1-to-final-M10 paper runtime shadow projection.

The WS loop validates the same access-token lease that it uses for each
subscription iteration and refuses it unless FINAM reports `readonly=true` and
a non-empty market-data permission set. The check intentionally runs in the WS
process: a separate `ExecStartPre` would create a second FINAM auth session
immediately before the BARS subscription and can leave that subscription
unconfirmed. The current developer full-trade token must not be installed.

Each connection generation has a 60-second subscription-confirmation bound.
If a desired subscription remains unconfirmed, the iteration closes as
`subscription_confirmation_timeout` and the loop reconnects after its bounded
delay. Once every desired subscription is confirmed, this timer is disarmed;
the normal long-lived connection duration remains unchanged. Before a
reconnect, the process discards only the cached access-token lease so the next
connection generation authenticates with a fresh JWT derived from the same
read-only secret. The first fresh final bar publishes the non-live transition
`Reconciliation / OperatorLiveArmMissing` immediately; it never publishes a
live-trading arm.

The runtime process has no FINAM client, order transport or broker dispatch
composition. Its `--strategy-invocation-shadow` output is a state projection,
not the deployable Stage 7B durable paper order/ACK lifecycle.

## Filesystem layout

```text
/opt/moex-finam-paper/bin/broker-cli
/opt/moex-finam-paper/source
/etc/moex-finam-paper/ws.env                 # root:root 0600, ignored/private
/etc/moex-finam-paper/ws.json
/etc/moex-finam-paper/runtime-unseeded.json
/var/lib/moex-finam-paper
```

Redis listens on loopback only. Production Generation-2 ceremony, manifests,
credentials and native-proof services are not used by this stand.

## First activation

The first transport smoke uses the unseeded runtime config and therefore may
prove transport, M10 construction and projection only. It must be labelled
`ReportUnseededBridge`; it is not an ALOR parity acceptance run.

Before a claimed parity session, publish a fresh ALOR runtime-state seed into
the isolated oracle stream and switch to the accepted seeded config with
`seed_required=true`.

## Verification

```bash
systemctl status --no-pager moex-finam-paper-ws.service
systemctl status --no-pager moex-finam-paper-runtime.service
redis-cli XLEN finam_imoexf_paper:ws:market_data
redis-cli XREVRANGE finam_imoexf_paper:ws:health + - COUNT 1
redis-cli XREVRANGE finam_imoexf_paper:ws:readiness + - COUNT 1
redis-cli XREVRANGE finam_imoexf_paper:runtime:state:hybrid_intraday:imoexf + - COUNT 1
redis-cli XPENDING finam_imoexf_paper:ws:market_data finam-imoexf-paper-runtime-m1
redis-cli XLEN finam_imoexf_paper:runtime:dlq
```

The current P0 consumer constructs complete M10 inputs in-process and persists
their runtime-state batches; it does not publish a separate M10 Redis stream.

Safety assertions remain:

```text
FINAM token readonly                  true
command consumer to real FINAM       false
order placement                      false
cancel                               false
runtime live                         false
Generation 2 active                  false
R2B authorization                    NOT_ISSUED
```
