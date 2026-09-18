# Stage 8B-P1-e I1 committed Cancel/Day-expiry restart recovery

Status: source review candidate after independently accepted Day-expiry correction
`a66793885e425465e5c4f49426748333b1cc448b`.

## Scope

This slice resumes already committed signed-schedule V4 transitions. It does
not read the signed schedule again and does not broaden process startup or
operational deployment authority.

For committed Cancel, restart reconstructs and cross-validates the exact
strategy request, canonical command hash, target broker order, source M10,
publication seal and first retained successor. It then re-enters the accepted
P1-d3 target-first state machine without republishing the command. In the
target-first case the original source remains in the PEL after
`S_cancel_recovered`, remains pending after durable `S_truth`, and is XACKed
only by the final typed source acknowledgement. This is the required
truth-before-XACK order.

For committed Day-expiry, restart validates the exact active order, Working
book transition, predecessor, evaluated last-eligible M10 and recovered signed
schedule high-water. The continuation is source-free: it performs no M10
claim, schedule reread, command publication or source acknowledgement and
returns the terminal Ready owner after the inherited expiry transition.

## Restart evidence

The focused real-Redis acceptance tests prove:

- Cancel command stream length stays unchanged across restart;
- target-first Cancel retains one exact PEL entry through recovered Cancel and
  durable truth, then reaches zero only at XACK-last;
- a second restart continues from the durable pending state without repeating
  provider, callback or dispatch effects;
- the exact `(seq_ack, seq_truth)` progression and recovered high-water survive
  restart;
- Day-expiry begins and ends with an empty M10 PEL;
- Day-expiry restart performs no additional signed-schedule read and preserves
  its authenticated high-water.

The inherited signed Cancel fixtures were aligned to the production ordering:
the published Cancel is bound to the active Working M10 and its V4 candidate is
the exact first successor. Runtime validation remains strict; no compatibility
exception was added.

## Deliberately closed

- production owner-loop wiring remains closed and is the next separate source
  slice;
- the process signal/panic/SIGKILL/restart matrix follows that wiring slice;
- operational Redis DB0/DB15 and VPS activation remain closed;
- FINAM POST/DELETE/send, broker dispatch, runtime-live and real orders remain
  closed.

No closure or activation decision is implied by this source candidate.
