# Stage 5G-c R2-a — authority recovery

R2-a is a governance-only successor of rejected commit
`16591e819c571aa2ccb8e4b0d087d28c84090415`.

It restores the frozen Stage 5F semantic route byte-for-byte and retains only a
read-only Stage 5C source-intent projection in four explicitly marked,
digest-pinned regions. Removing those regions reproduces the accepted Stage
5G-a Stage 5C authority exactly. The normalized public API is unchanged.

The Stage 5G test fixture no longer depends on a helper added to frozen Stage
5F. Rejected R1 production-witness semantics are deliberately not used as an
acceptance gate in R2-a; they remain ignored until the separately authorized
R2-b semantic hardening.

The R2-a gate checks:

- exact accepted Stage 5G-a inventory;
- exact frozen Stage 5F source;
- exact stripped Stage 5C baseline and four region digests;
- one production read-only accessor and zero new callback callsites;
- unchanged Stage 5C public API;
- 11 fail-closed adversarial authority mutations.

No Stage 5G order/position semantics, Stage 5C callbacks, Stage 5D source,
Broker Core source, Redis, FINAM transport, dispatch, runtime-live or real-order
surface is changed or opened by R2-a.

Independent acceptance is required before R2-b starts.
