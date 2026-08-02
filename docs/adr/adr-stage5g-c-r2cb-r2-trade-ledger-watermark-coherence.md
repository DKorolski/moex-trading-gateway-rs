# ADR: Stage 5G-c R2-c-b R2 trade ledger and watermark coherence

Status: review candidate

## Context

R1 accepted repeated full-snapshot history (`A -> A+B -> A+B+C`) by classifying a known `BrokerTradeId` before the global trade watermark. Two continuations were still unsafe:

- a known-only subset could replace the global maximum with an older incoming maximum;
- a Market position-only snapshot could advance trade chronology although its trades were neither authenticated against an order nor retained in the committed ledger.

Both paths weakened the fail-closed unseen-late-trade contract.

## Decision

Trade watermarks are a projection of committed trade history, not of the current snapshot vector.

After a candidate has passed exact order/trade/position validation and its trade ledger transition has committed, source and receipt watermarks are refreshed as:

```text
max(previous watermark, maximum value in committed slot.trades)
```

Every accepted transition is additionally checked for monotonic order, trade, and position component watermarks.

For a Market slot, target-correlated trades without a target-correlated order row produce the typed retryable block `TargetTradeWithoutOrder` before chronology or candidate mutation. The original capability, fingerprint, ledger, position, sequence and callback count remain unchanged. Position-only progress remains permitted only when no target-correlated trade is present.

A terminal slot rejects any later target-correlated trade as `BrokerEvidenceAfterTerminalAck`.

## Evidence

- The accepted R1 three-poll full-snapshot and public runtime witnesses remain green.
- A public subset witness retains committed `B` after a known-only `A` refresh and rejects unseen late `C` between their source timestamps.
- A known `A` receipt between its previous receipt and the global `B` receipt preserves the global receipt maximum.
- A public Stage 5F intent and Stage 5G-b ACK reject position-only `A+B` without mutation, then converge once the coherent target order and `A+B+C` arrive.
- Malformed target-correlated position-only trades and post-terminal target trades cannot be ignored.

## Deferred replay identity gate

The existing request/account/receipt-millisecond replay identity remains unchanged. Stage 5G-d and stream reuse stay closed until a separate reviewed identity gate binds a source-owned package sequence or collision-safe canonical discriminator.

## Closed surfaces

This is deterministic paper reconciliation only. Stage 5G-d, Redis live consumers/groups, FINAM transport, HTTP POST/DELETE, broker dispatch/execution, runtime-live, real orders, Stage 6, `main` merge and deployment remain closed.
