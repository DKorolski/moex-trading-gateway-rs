# Stage 8B paper-shadow VPS runbook

Status: deployment runbook for the isolated transport/runtime projection.

## Boundary

This stand contains two processes only:

1. read-only FINAM WebSocket M1 market data;
2. M1-to-final-M10 paper runtime shadow projection.

The WS service runs `finam-paper-auth-preflight` before every start and refuses
tokens unless FINAM reports `readonly=true` and a non-empty market-data
permission set. The current developer full-trade token must not be installed.

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
redis-cli XREVRANGE finam_imoexf_paper:md:bars:10m + - COUNT 1
redis-cli XREVRANGE finam_imoexf_paper:runtime:state:hybrid_intraday:imoexf + - COUNT 1
redis-cli XPENDING finam_imoexf_paper:ws:market_data finam-imoexf-paper-runtime-m1
redis-cli XLEN finam_imoexf_paper:runtime:dlq
```

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
