# M3j-3 one-symbol dry shadow session

M3j-3 closes the one-symbol dry shadow session slot.

This stage proves the broker-neutral dry path:

```text
LiveFinal bar -> paper signal -> dry command -> M3e dry ACK
```

It does not enable:

- `LiveReady`;
- runtime live attachment;
- external FINAM POST/DELETE;
- command-consumer-to-real-FINAM transport;
- non-loopback order endpoint;
- Stop/SLTP/bracket/replace/multi-leg;
- first live micro.

## Required session scope

The dry shadow session must stay scoped to:

- one account;
- one symbol;
- one timeframe;
- one strategy.

## Required session evidence

The session report must include:

- live-final bar count;
- paper signal count;
- dry command count;
- M3e dry ACK count;
- suppression count;
- duplicate request count;
- pending count;
- dropped count;
- broker-truth clean before the session;
- broker-truth clean after the session;
- reconciliation status.

## M3j-2 typed optional failures

M3j-2 accepted the read-only evidence slot, but the typed fixture had optional
failures for:

- `account_trades_typed`;
- `account_transactions_typed`;
- `bars_typed`.

For M3j-3 these are acknowledged as first-live-micro waivers:

- own-trades broker truth is covered by the M3j-2 runtime `TradesSnapshot`;
- account transactions are not required for the one-symbol dry shadow session;
- bars typed backfill is not required for the live-final dry path.

The waivers are not a permanent resolution. M3j-4 must carry them forward in the
explicit GO/NO-GO decision package.

## Boundary interpretation

`one_symbol_dry_shadow_session_ok = true` means the dry session slot is closed.

It still does not mean live readiness. The M3j-3 report emits:

- `live_micro_go = false`;
- `live_ready_allowed = false`;
- `runtime_live_attachment_allowed = false`;
- `external_finam_post_delete_allowed = false`;
- `command_consumer_to_real_finam_transport_allowed = false`;
- `non_loopback_order_endpoint_allowed = false`.

## Next step

M3j-4: explicit pre-live NO-GO/GO decision package.
