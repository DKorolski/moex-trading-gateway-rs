# Stage 8A-4 I1 negative inventory

The I1 negative harness mutates the exact candidate contract and requires every
mutation to fail the semantic checker. Covered classes include predecessor and
review provenance drift, scope/status drift, opening any deferred surface,
changing supported versions or golden counts, removing V2 DTO/envelope fields,
adding a V2 writer/append token, adding a `finam-gateway` dependency, removing
canonical dispatch or full-record suffix checks, weakening stable-key conflict
handling, reducing the acceptance matrix, and changing a canonical golden.

Runtime tests independently cover malformed/unknown/duplicate schema,
non-canonical V2, optional-ID retention, exact lookup binding mismatch,
empty/single/partial/complete suffixes, full-record source/causal drift,
duplicate idempotency, stable-key conflict and post-finalization V2.
