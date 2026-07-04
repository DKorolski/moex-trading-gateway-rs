# M3j-20 working LimitCancel micro

Date: 2026-07-04

## Goal

M3j-20 is a second controlled live micro after the frozen M3j closure and the
M3j-19 failure matrix. It verifies that a below-market limit order can be
observed in broker read-only state as active/working before cancellation.

## Approved scope

- Symbol: `IMOEXF@RTSX`
- Side: `buy`
- Order type: `limit`
- Quantity: `1`
- Limit price: `2210`
- Maximum orders: `1`
- Mode: `place -> observe working/active -> cancel`
- No intended position entry
- Stop/SLTP/bracket/replace/multi-leg: blocked

## Required gates

Before the actual boundary call:

- explicit operator live approval;
- fresh quote with limit price below reference;
- active/unknown/orphan orders = `0`;
- positions = `0`;
- full trade token for the approved account;
- command-consumer-to-real-FINAM disabled;
- continuous runtime-live disabled.

During the run:

- place exactly one limit buy;
- poll read-only orders for the placed broker/client order;
- record redacted active/working observation;
- cancel by broker order id;
- never retry blindly after ambiguous place/cancel.

After the run:

- active/unknown/orphan orders = `0`;
- positions = `0`;
- broker truth clean;
- raw broker bodies stay local-only;
- review evidence contains redacted shapes and hashes only.

## Boundary

M3j-20 does not enable continuous trading. It remains an operator-triggered
one-shot package. Runtime-live attachment, command-consumer-to-real-FINAM,
Stop/SLTP/bracket/replace/multi-leg and portfolio-level live execution remain
blocked after this stage.
