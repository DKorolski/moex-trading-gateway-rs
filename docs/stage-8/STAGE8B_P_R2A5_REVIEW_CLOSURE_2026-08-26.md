# Stage 8B-P R2A5 source-truth closure

R2A5 is a narrow corrective candidate for the three findings in the R2A4
review. It does not authorize R2B or any real FINAM request.

## Corrections

1. Every authority carries immutable `source_observed_at_utc` separately from
   diagnostic `produced_at_utc`. Freshness and cross-source skew use only the
   source timestamp; producer invocation cannot make stale truth fresh.
2. `Stage8a1OperationalAuthorityIssuer` maps opaque, validated Stage 7B, Stage
   6 and Stage 8A authorities to ten closed operational records. The eleventh
   source is the producer's kernel boot-clock observation. Production has no
   manually authored R2A intermediate store.
3. The source adapter validates current durable authority, command identity,
   accepted config/policy/control and broker current sources before atomic
   no-follow publication to the fixed operational-authority root.
4. The controlled rehearsal starts from owner/API-tagged adapter fixtures and
   verifies the same fixed source filenames and closed schemas before producer
   and issuer custody. Direct final-intermediate seeding is absent.
5. An independent Ed25519 helper-acceptance key signs the exact helper/effect
   pair. Package issuer, helper-before-credentials and fd-bound launcher all
   require the accepted helper SHA.
6. Production authority remains `NOT_ISSUED`. Typed operator-decision semantics
   are explicitly carried forward to R2B, before any real credential or GET.

## Deliberately closed

- real credentials and real FINAM network;
- AuthService POST and broker GET;
- order POST/DELETE, arm, dispatch and effect transport;
- Redis execution, runtime-live and strategy orders.
