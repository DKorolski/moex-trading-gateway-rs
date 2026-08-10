# Stage 6B — isolated durable journal backend

Accepted Stage 6A authority:
`c399e2bc2c7e62cc2116a6eac970058bb47c4a49`.

Stage 6B adds one append-only local filesystem backend and an in-memory parity
backend. Both use the same binary framing and scanner. Persisted records are
generated only with `Stage6JournalRecordV1::encode_canonical()` and admitted
only with `Stage6JournalRecordV1::decode_canonical()`.

## Physical v1 format

All integers use big-endian byte order.

Journal header:

```text
8 bytes  magic = "S6JNLV1\0"
2 bytes  storage schema version = 1
```

Each frame:

```text
4 bytes  magic = "S6F1"
2 bytes  frame version = 1
4 bytes  canonical record length (u32)
32 bytes previous frame SHA-256
N bytes  canonical Stage 6A record
32 bytes frame SHA-256
```

`frame_sha256 = SHA256("stage6-journal-frame-v1" || version || length ||
previous_sha256 || record_bytes)`.

The first previous digest is
`SHA256("stage6-journal-frame-genesis-v1")`. Record length is strictly in
`1..=1 MiB` and is checked before allocation.

Filesystem open first attempts a read/write open without create or truncate.
Every pre-existing path, including an existing zero-length file, is treated as
an existing journal and receives a complete streaming scan before any mutation
is allowed. Invalid existing bytes fail closed and remain byte-identical.

Only `NotFound` permits exclusive `create_new(true)` initialization. The exact
v1 header is written and synced before the newly-created file is scanned. An
`AlreadyExists` creation race retries the existing-file validation path and
never overwrites or initializes the competing file.

Every append verifies the writer frontier, writes one complete frame and calls
`sync_data()` before returning a receipt. Sync failure returns
`DurabilityUncertain`; the backend does not truncate, rewrite or compensate.
Reopen scanning decides whether a fully written frame exists.

`Stage6JournalFrontierV1` and its self-digested canonical checkpoint are
storage evidence only. No checkpoint sidecar is persisted in this slice; the
journal remains the sole source of truth. A valid stale checkpoint may be
validated against a longer scanned journal, while ahead/mismatched checkpoints
fail closed.

Stage 6B does not interpret lifecycle chronology, replay conflicts, duplicate
requests, causal ancestry or finalized state. Those remain Stage 6C.

Still closed: runtime attachment, Redis, FINAM, HTTP POST/DELETE, broker
dispatch, workers/schedulers, runtime-live, real orders and native protective
order schemas.

Stage 6B-R1 does not add parent-directory fsync or an external durable suffix
rollback anchor; those remain explicit later durability decisions.

Acceptance command: `bash scripts/stage6b_r1_gate.sh`.
