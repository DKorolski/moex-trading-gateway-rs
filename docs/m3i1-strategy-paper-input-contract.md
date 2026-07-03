# M3i-1 strategy paper input contract

M3i-1 starts paper/shadow strategy migration after M3h closure. It adds only a
strategy-facing input adapter contract. It does not attach a live runtime, does
not emit live orders, and does not call FINAM.

Allowed input path:

```text
M3h broker-neutral shadow input
+ M3h StrategyDecisionTick
-> M3iStrategyPaperInput
```

Contract requirements:

- strategy receives broker-neutral runtime inputs only;
- FINAM DTOs are not visible to the strategy layer;
- strategy input requires an M3h `StrategyDecisionTick`;
- input `entry_id` and `bar_key` must match the M3h decision tick;
- only `LiveFinal` bars are accepted;
- `LiveUpdating`, historical/read-only, recovery, unknown, and non-bar inputs
  cannot reach the strategy as decision inputs.

Safety boundary:

- no `LiveReady`;
- no runtime live attachment;
- no external FINAM `POST / DELETE`;
- no direct/non-loopback endpoint;
- no command-consumer-to-real-FINAM transport;
- no stop/SLTP/bracket/replace/multi-leg.

Evidence:

```bash
python3 scripts/m3i1_strategy_paper_input_contract_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
