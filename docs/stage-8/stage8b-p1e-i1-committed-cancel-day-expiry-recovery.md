# Stage 8B-P1-e I1 committed Cancel/Day-expiry restart recovery

Status: SOURCE ACCEPTED at `efe56a9af6272f13b2d87f4ad14a2709c605cd42`
after independently accepted Day-expiry correction
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

The focused real-Redis source tests directly prove:

- Cancel command stream length stays unchanged across restart;
- target-first Cancel retains one exact PEL entry through recovered Cancel and
  durable truth, then reaches zero only at XACK-last;
- a second restart preserves the authenticated seal generation, journal frame
  count and callback count; durable dispatch/callback totals do not increase
  during recovered continuation;
- Day-expiry begins and ends with an empty M10 PEL;
- Day-expiry restart performs no additional signed-schedule read and preserves
  its authenticated high-water.

In this accepted source slice, absence of Day-expiry XACK is established by
the source-free call graph plus the unchanged empty PEL; the test did not yet
contain a direct XACK invocation counter. Exact request-scoped sequence
allocations and direct provider/callback/publication/claim/XACK counters are
part of the subsequent owner-loop wiring evidence, not evidence retroactively
attributed to `efe56a9`.

The inherited signed Cancel fixtures were aligned to the production ordering:
the published Cancel is bound to the active Working M10 and its V4 candidate is
the exact first successor. Runtime validation remains strict; no compatibility
exception was added.

## Deliberately closed

- production owner-loop wiring was closed at this immutable source boundary;
  it is opened only by the following separately reviewed source slice;
- the process signal/panic/SIGKILL/restart matrix follows that wiring slice;
- operational Redis DB0/DB15 and VPS activation remain closed;
- FINAM POST/DELETE/send, broker dispatch, runtime-live and real orders remain
  closed.

No closure or activation decision is implied by this source candidate.
