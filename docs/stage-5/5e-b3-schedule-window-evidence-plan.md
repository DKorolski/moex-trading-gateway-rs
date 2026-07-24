# Stage 5E-b3a-r1 — sealed ScheduleWindowEvidence

Baseline: `04431096e269daaf9715e253b2354b1ac8fcc3e8`.

This slice creates only the broker-neutral, no-I/O contract for a sealed,
instrument-scoped schedule window. The sole construction chain is:

```text
normalized schedule snapshot
→ validated opaque snapshot
→ exact accepted instrument-registry identity
+ accepted Stage 4 schedule evidence
+ lifecycle clock
→ selected TradableOpen schedule window evidence
```

No independently constructed schedule definition may reach the mapper. Both
Stage 4 and normalized-snapshot expiry are revalidated at the mapping boundary.
Production mapping remains a later separately reviewed broker adapter slice.

The window policy is inclusive: `open_from <= bar_close <= open_until`; mapper
validation requires `open_from < open_until`. The output is non-copyable,
private, carries lifecycle observation/expiry and a deterministic SHA-256
fingerprint over full instrument identity, broker symbol, MIC, board,
registry/source versions, raw and normalized payload hashes, canonical sessions,
selected window and Stage 4 identity. Its encoding is tagged and length-prefixed.

Callback, strategy mutation, intents, Redis, FINAM I/O, transport, dispatch,
runtime-live, autonomous loops and execution remain closed. Binding to an
observed bar is b3b, not this slice.
