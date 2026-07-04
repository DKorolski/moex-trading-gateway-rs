# M3i-4a ACK correlation and idempotency hardening

M3i-4a hardens paper strategy ACK application before M3i stage closure.

The key invariant:

```text
An ACK must not mutate paper strategy state unless it correlates to a currently
pending request_id, or to an explicitly known terminal request_id replay.
```

## Policy

- ACK for a pending request may update state according to the M3i-4 ACK matrix.
- Replaying an ACK for an already terminal request is an idempotent no-op and is
  reported as `AlreadyResolved`.
- ACK for a completely unknown request is ignored without mutating
  acknowledged/dropped/duplicate/manual lists and is reported as
  `UnknownAckIgnored`.
- DuplicateCommand ACK for a non-pending request does not create false duplicate
  accounting.
- Missing/stale ACK keeps pending visible only when the request is actually
  pending.

## Reporting

The M3i-4 paper/shadow report now includes:

- `already_resolved_ack_count`;
- `ack_for_unknown_count`;
- existing pending/dropped/duplicate/manual counters.

All request identities remain redacted hashes.

## Boundary

Still forbidden:

- `LiveReady`;
- runtime live attachment;
- external FINAM POST/DELETE;
- non-loopback order endpoint;
- direct strategy publish to Redis/M3e;
- command-consumer-to-real-FINAM transport;
- Stop/SLTP/bracket/replace/multi-leg.

