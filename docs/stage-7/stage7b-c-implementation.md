# Stage 7B-c — recovery seal and restart ownership

Status: implementation candidate; independent acceptance pending.

Accepted predecessor: `ff3fa2e8908440863b40b838991d4716b33caad4`
(Stage 7B-b-R2 / Stage 7B-b CLOSED).

## Scope

This slice implements acceptance rows B-027 through B-042 without Redis,
FINAM, broker dispatch or runtime-live:

- first durable boot requires and validates a source-produced authenticated
  Stage 5G clean-restart seed before journal creation;
- the initial Stage 6 checkpoint, authenticated restart package and complete
  operational identity are bound into a canonical versioned recovery seal;
- the seal carries SHA-256 bindings and a separate domain-separated HMAC made
  by the opaque lifecycle commitment key; the key is never serialized;
- authoritative seal updates use a same-directory exclusive temp file, file
  sync, root-FD-relative atomic rename, parent-directory sync and committed
  byte reread; orphan temp files are never authority;
- restart distinguishes seal-without-journal, journal-without-seal, corrupt
  seal, checkpoint mismatch and authenticated Stage 5G/Stage 6 rejection;
- `Stage7bRecoveryReadyOwner` retains the recovered Stage 6 runtime and the
  writer/root lease for one lifetime. It is linear and non-serializable and
  exposes no mutable runtime or raw lease extractor;
- readiness and every exposed authority-sensitive read revalidate the live
  filesystem namespace. Future writable boundaries must do the same;
- `RecoveryBlocked` has no provider invocation, Redis settlement or XACK
  capability and reports readiness false.

Rows B-039 through B-041 are composed through the exact accepted Stage 6
authenticated restart implementation. Its regression witnesses prove that
additional finalized history is retained, unbound non-final authority is
rejected, and matching active Stage 5/Stage 6 requests retain dispatch-safety
classification. Stage 7B-c invokes that implementation with the owned file
journal rather than reimplementing its policy.

## Intentionally closed

- Redis consumer attach, ACK/DLQ settlement and XACK;
- FINAM POST/DELETE, broker transport and runtime-live;
- real strategy orders and Stop/SLTP/bracket;
- Stage 7B-d and the aggregate X01-X20 crash matrix.
