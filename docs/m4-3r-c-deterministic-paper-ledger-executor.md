# M4-3r-c deterministic paper ledger executor

Status: source-only implementation / no-send / no live orders.

M4-3r-c adds the first deterministic paper ledger executor to
`broker-core::paper`. This is not a FINAM runtime adapter and not a Redis
publisher. It is the pure broker-neutral accounting engine that future runtime
code will call after a strategy emits a paper intent.

## Boundary

Still disabled:

- FINAM `POST /orders`;
- FINAM `DELETE /orders/{id}`;
- command-consumer-to-real-FINAM;
- runtime `LiveReady`;
- continuous runtime-live attachment;
- Stop/SLTP/bracket/replace/multi-leg;
- Redis paper runtime adapter;
- live broker acknowledgements.

The executor only mutates an in-memory `PaperLedgerSnapshot`.

## Added executor contracts

M4-3r-c adds:

```text
PaperLedgerExecutorConfig
PaperLedgerExecutionOutcome
PaperLedgerExecutorError
PaperLedgerSnapshot::empty(...)
PaperLedgerSnapshot::apply_next_bar_open_market_intent(...)
PaperLedgerSnapshot::to_runtime_state(...)
```

The new executor path is intentionally narrow:

```text
PaperIntent
  + RuntimeBarInput(next final 10m bar)
  -> PaperOrder(status=Filled)
  -> PaperTrade(price=next_bar.open)
  -> PaperPosition(source=paper_synthetic_position_feedback)
  -> PaperAck(kind=Filled)
  -> updated PaperLedgerSnapshot
```

## Implemented fill policy

Implemented now:

```text
market paper entry/exit = next final bar open proxy
```

The fill price is:

```text
fill_price = fill_bar.open
```

The fill timestamp is:

```text
fill_ts = fill_bar.open_ts
```

## Execution gates

The executor rejects:

- open paper safety boundary;
- strategy id mismatch;
- instrument mismatch;
- execution mode mismatch;
- zero expected timeframe;
- non-positive quantity;
- missing side;
- missing order type;
- non-market order type for this path;
- fill policy other than `NextFinalBarOpen`;
- unsupported intent kind;
- non-final fill bar;
- wrong timeframe fill bar;
- non-live origin in `LiveOnly`;
- fill bar that precedes the intent timestamp.

`HistorySim` may accept historical/replay bars; `LiveOnly` may not.

## Idempotency

`RuntimeDecisionId` is the idempotency key. If the same intent id is seen again,
the executor returns:

```text
PaperLedgerExecutionOutcome::DuplicateIgnored
PaperAckKind::DuplicateIgnored
```

and does not append a second intent, order, trade, position, or normal ack.

## Position accounting

The executor maintains target-instrument paper position:

```text
buy  while flat/long  -> increase long quantity and weighted avg price
sell while flat/short -> increase short quantity and weighted avg price
sell against long     -> reduce/flatten/flip long
buy  against short    -> reduce/flatten/flip short
flat                  -> avg_price = 0
flip                  -> remaining side avg_price = fill_price
```

Closed-trade PnL reporting is not implemented in M4-3r-c. The current scope is
paper order/trade/position/ack state needed by runtime parity.

## Runtime state projection

`PaperLedgerSnapshot::to_runtime_state(...)` projects the ledger into
`PaperRuntimeState`:

- last bar key;
- last decision id;
- latest paper position;
- active order count;
- suppression count;
- closed safety boundary.

This is still in-memory only. Redis publication belongs to a later runtime
adapter step.

## What is intentionally not implemented

M4-3r-c does not yet implement:

- limit bar-touch fill;
- cancel-if-working transition;
- paper order working lifecycle across bars;
- closed-trade PnL records;
- strategy `on_position` callback invocation;
- Redis paper streams;
- restart restore/idempotency from durable storage;
- FINAM gateway runtime adapter;
- ALOR-vs-FINAM paper comparison.

## Test coverage

The `broker-core::paper` tests now cover:

- market intent fills at next final 10m bar open;
- entry + exit round-trip returns target flat;
- duplicate decision id is idempotent and does not append records;
- `LiveOnly` rejects history/gap-origin fill bars;
- `HistorySim` accepts history-origin fill bars;
- wrong timeframe is rejected;
- unsupported order type is rejected;
- existing M4-3r-b domain/invariant tests remain green.

## Acceptance for M4-3r-c

M4-3r-c is accepted when:

- deterministic market/next-final-bar-open executor exists;
- executor is broker-neutral and source-only;
- paper safety boundary remains closed;
- idempotency is enforced by `RuntimeDecisionId`;
- target instrument position is updated deterministically;
- tests pass;
- forbidden order/live surface scanners remain green.

Next stage: M4-3r-d FINAM M1-to-M10 paper stream publisher / runtime adapter prewire.
