# Stage 8B-P1-d4 source discovery R7

Status: committed-source evidence for a design-only correction.

Inspection is pinned to R6 commit
`cb6e6ddf863f314cc96b5f8ac0a75809e8c6824a`; the saved eight-file Rust WIP is
excluded.

## Publication gap

The accepted Lua command publication uses `XADD *` and writes the generated ID
into an atomic Redis marker. Its process-local receipt contains that ID, but
the prepublication package does not. Therefore a self-consistent marker/entry
replacement cannot be detected before `S_ack` from R6 durable state alone.

R7 resolves the information gap before the effect: read exact command-stream
`last-generated-id`, calculate its canonical immediate successor, persist that
reservation in the HMAC-covered combined Prepublication package, and use the
reserved explicit ID in Lua. The marker becomes evidence matching local
authority rather than the sole authority for its own ID.

## Classifier overlap

Committed restart routing currently evaluates:

```text
P1-d3 V3 classifier
P1-d2 complete V1 classifier
generic P1 classifier
```

The ordinary P1-d2 classifier accepts suffix lengths 3 and 4. Those are also
generated-Market partial frontiers, so suffix bytes alone cannot select an
owner. R7 uses authenticated package tri-state routing. `PresentValid` runs the
P1-d4 classifier first and wraps complete P1-d2 structural facts;
`PresentInvalid` blocks without fallback; `Absent` retains existing standalone
P1-d2 behavior byte-for-byte.

## Retained accepted facts

The R6 Stage6 V1 chain and sequence timing remain correct. No journal V3 or new
journal schema is needed. Dispatch-only and order-only remain separate durable
frontiers; `RequestFinalized` still precedes the sole sequence allocator.

The exact committed-source hashes and existing routing/publication tokens are
recorded in `stage8b-p1d4-source-shape-r7.json`. They are checked independently
of design artifact hashes so the negative harness cannot hide a source-shape
mutation.
