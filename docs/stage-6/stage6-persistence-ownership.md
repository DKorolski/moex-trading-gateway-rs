# Stage 6 persistence ownership inventory

Stage 6 extends the accepted persistence model; it does not replace it.

| State or record | Accepted owner | Stage 6 policy | Future durable owner |
|---|---|---|---|
| Runtime snapshot and riskgate | Stage 5D persistence envelope | reuse unchanged | Stage 5D |
| Pending/deferred requests | Stage 5D runtime-private state | wrap with journal linkage | Stage 5D + journal reference |
| Restart authentication | Stage 5D and Stage 5G clean restart | reuse unchanged | Stage 5D/5G |
| Broker order/trade/position evidence | Stage 5G operational truth and reducer | reuse unchanged | Stage 5G |
| Protective cleanup ledger | Stage 5G-f protective completion | reuse unchanged | Stage 5G-f |
| Durable command attempts and outcomes | not yet represented | extend versioned | future Stage 6 journal |
| Redis stream delivery metadata | transport concern | do not reuse | Stage 7+ only |
| FINAM DTO or HTTP response | broker adapter concern | do not reuse | never core persistence |

The future journal owns durable command chronology and causal correlation, not
strategy state, broker truth, riskgate state or protective completion. Runtime
snapshots contain a validated journal frontier/reference rather than a copied
journal. Journal replay cannot mutate Stage 5D state directly; it produces a
typed recovery candidate consumed through accepted Stage 5G boundaries.
No second restart authority or duplicate runtime persistence model is allowed.
