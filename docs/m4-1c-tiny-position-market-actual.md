# M4-1c tiny position lifecycle actual pre-authorization / market one-shot

Date: 2026-07-04

## Goal

M4-1c introduces a dedicated, operator-triggered tiny position lifecycle harness:

```text
market entry qty=1 -> broker position snapshot -> market exit qty=1 -> final flat reconciliation
```

This stage is not continuous runtime-live and does not enable command-consumer-to-real-FINAM.

## Approved scope for this operator run

The operator explicitly approved actual entry/exit and market order type.

Default scope:

- Symbol: `IMOEXF@RTSX`
- Entry: buy market
- Exit: sell market
- Quantity: `1`
- Max orders: `2`
- Max position lifetime: bounded by position observation and immediate exit
- No strategy runtime
- No command-consumer-to-real-FINAM
- No Stop/SLTP/bracket/replace/multi-leg

## Preflight gates

Actual entry is allowed only if:

- full trade token is present;
- token is bound to account;
- symbol exactly matches expected symbol;
- quantity is exactly `1`;
- active/unknown/orphan orders = `0`;
- positions = `0`;
- fresh quote is available for market notional guard;
- actual risk flag is present;
- binary is compiled with actual one-shot feature.

## Execution policy

- Send entry only after all preflight gates pass.
- Poll broker account until a position snapshot is observed.
- Send exit only after a broker position snapshot is observed.
- If position is not observed after entry acceptance, do not blindly send the exit order.
- Always perform final broker-truth refresh.

## Boundaries that remain disabled

- continuous runtime-live;
- command-consumer-to-real-FINAM;
- strategy runtime live attachment;
- Stop/SLTP/bracket/replace/multi-leg;
- portfolio-level strategy execution.
