# M3j-0 ALOR oracle pre-live readiness comparison

M3j-0 starts the first-live-micro preparation stage after M3i closure.

This is a pre-live evidence gate only. It does not enable:

- `LiveReady`;
- runtime live attachment;
- external FINAM POST/DELETE;
- non-loopback order endpoint;
- command-consumer-to-real-FINAM transport;
- Stop/SLTP/bracket/replace/multi-leg.

## Purpose

M3j-0 compares the FINAM gateway state against the legacy ALOR operational
oracle and the R1 anti-regression checklist. It records which parity items are
already satisfied by M3d-M3i and which items must remain pending for later M3j
steps.

## Satisfied parity inputs

M3j-0 expects these dependencies to be closed before the comparison can be
considered internally consistent:

- M3i paper/shadow strategy stage closed;
- broker-truth reconciliation closed;
- first-live-bar gate closed;
- runtime shadow stage closed;
- command ACK lifecycle closed;
- request id exists before strategy pending mutation;
- dropped/non-emitted intent rolls back strategy state;
- duplicate request id cannot create a duplicate order;
- unknown order blocks readiness;
- active-order startup policy exists;
- schedule and instrument guards block trading.

## Pending pre-live inputs

These are intentionally pending after M3j-0:

- fresh real FINAM read-only evidence;
- no unknown active orders evidence;
- flat or explicitly expected position evidence;
- operator arm design;
- kill switch design;
- max orders / max quantity / max loss limits;
- one-account / one-symbol first-live-micro scope;
- end-of-day reconciliation plan.

Because these remain pending, `live_micro_go` is always `false` in M3j-0.

## Next steps

M3j-1: live gate design: operator arm, kill switch, max orders, max qty, max loss.

M3j-2: fresh read-only FINAM evidence package.

M3j-3: one-symbol dry shadow session report with clean reconciliation.

M3j-4: explicit pre-live NO-GO/GO decision package.

