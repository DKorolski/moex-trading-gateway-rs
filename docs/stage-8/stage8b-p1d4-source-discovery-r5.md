# Stage 8B-P1-d4 source discovery R5

Status: design evidence only; source implementation remains paused.

The R4 review correctly rejected the claim that exact publication creates an
independent command M10. Inspection of the accepted composition shows one
retained delivery end to end:

Exact publication does not create a second M10 source.

- `Stage8bP1RedisPrepublicationPending::publish_exact_command` moves the same
  `pending_m10` into `Stage8bP1RedisCommandPublished`;
- publication requires the source to remain in PEL;
- accepted P1-d2 pre-ACK and ACK recovery reclaim that same source;
- accepted P1-d2 permits XACK only after reread `S_truth`.

The implementation failure `Durable(Runtime(RestartRuntimeRequired))` has a
second, narrower cause: a P1-d3 replacement package is restored as
`OrderPositionAwaitingCommitted`, while the ordinary P1-d2 entry bridge
requires `P1SemanticPrepublication` plus Stage5C generated-intent settlement
authority. Dropping the P1-d3 book to manufacture the latter is invalid, and
embedding a raw Stage5C settlement in the P1-d3 replacement is also invalid.

The minimal source-compatible correction is therefore a peer composition
projection in the outer Stage5G package. It binds the existing P1-d3
replacement, one-intent semantic commit and later-bar source, while phase-
specific private bridges reuse the accepted ACK/truth reductions without
requiring a reconstructed Stage5C recovery receipt. The runtime serialized in
the replacement package is already the post-bar-callback runtime; the
composition records and authenticates that fingerprint.

This keeps the source pending and retains both state machines:

```text
P1-d3 working book: unchanged across generated Market feedback
P1-d2 Market feedback: Prepublication -> AckCommitted -> TruthCommitted
source M10: pending -------------------------------> XACK last
```

No new Stage6 journal record is required. Existing Market outcome V3 and
`RequestFinalized` remain authoritative. One exact additive package
projection plus typed recovery owners is required; pretending that an
existing P1-d2 or P1-d3 owner possesses both authorities is forbidden.

The saved uncommitted source work remains excluded from R5. In particular,
the exploratory S09 cancel-continuation union and early S05 XACK/completion
path are not accepted implementation evidence.
