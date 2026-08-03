# Stage 5G-e-b R2 — historical exact-replay metadata rebind

Base commit: `1621307a6012fa1f9dcbc89a59651c801f6cc26f`.

## Purpose

R2 preserves the R1 owning proof and hard checkpoint validation while making
exact replay a metadata-only operation independent of package age and current
request-slot membership.

Both public raw evidence and Stage 5G-e owned canonical evidence enter the same
canonical application core. That core now performs:

1. account and monotonic local-sequence checks;
2. identity/fingerprint replay classification;
3. metadata-only exact replay, or NewPackage-only admission and broker-state
   application.

The exact branch may update only `last_total_sequence` and
`duplicate_evidence_count`. Continuation chronology, current-slot lookup,
account-wide broker guards, ACK/order/trade/position application and Stage 5C
callback are NewPackage-only operations.

## Executable witnesses

- `A → B → exact A → C` succeeds with A older than the B continuation
  watermark;
- raw historical A uses the same metadata authority;
- two historical exact A redeliveries precede C;
- an inherited R1 identity synchronizes while current slots belong to R2, then
  a current R2 NewPackage applies;
- a genuinely new identity before continuation remains blocked;
- a known historical identity with a conflicting fingerprint remains blocked.

All R1 and seven original Stage 5G-e-b tests remain green in debug and release.
The R2 negative matrix has 13 fail-closed mutations.

## Closed boundaries

R2 does not open canonical Stage 5D clean-process restart, GRST01–GRST12,
Stage 5G-f, Redis live consumers, FINAM/HTTP transport, broker execution,
runtime-live, real orders, Stage 6, main merge or deployment.
