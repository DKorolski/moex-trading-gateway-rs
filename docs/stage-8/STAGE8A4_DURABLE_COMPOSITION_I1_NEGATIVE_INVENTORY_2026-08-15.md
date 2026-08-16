# Stage 8A-4 I1 negative inventory

The I1 negative harness mutates the exact candidate contract and requires every
mutation to fail the semantic checker. Covered classes include predecessor and
review provenance drift, scope/status drift, opening any deferred surface,
changing supported versions or golden counts, removing V2 DTO/envelope fields,
adding a V2 writer/append token, adding a `finam-gateway` dependency, removing
canonical dispatch or full-record suffix checks, weakening stable-key conflict
handling, reducing the acceptance matrix, and changing a canonical golden.
I1 R2 adds direct mutations of the executable exact-state cross-binding branch:
Filled+Zero, Rejected+non-zero, Cancelled+Full and Working+Partial must all be
rejected. A separate governance mutation proves the leading current-status
authority cannot regress to Specification R2 pending or close I1 R2.

Runtime tests independently cover malformed/unknown/duplicate schema,
non-canonical V2, optional-ID retention, exact lookup binding mismatch,
empty/single/partial/complete suffixes, full-record source/causal drift,
duplicate idempotency, stable-key conflict and post-finalization V2.
The canonical decoder additionally rejects ten forbidden lifecycle/status/fill
cross-products while retaining all nine accepted exact combinations.
