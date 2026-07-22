# ADR: Stage 5D exact numeric persistence policy

Status: accepted for Stage 5E/6 entry criteria.

Stage 5D aggregate closure r2 does not migrate persisted numeric fields. It
freezes the current no-I/O source-oracle semantics and records the policy for
future production persistence.

## Decision

Before production durable persistence or live execution depends on Stage 5D
snapshots, broker-facing numeric values must have an exact canonical
representation.

Allowed future directions:

- fixed-point integer fields such as `price_ticks`, `qty_lots` and scaled PnL;
- canonical decimal-string fields validated by a broker-neutral decimal codec;
- versioned schema migration that keeps legacy `f64` source-oracle fields as
  compatibility inputs while adding exact broker-neutral fields.

Not allowed without a new ADR:

- treating approximate `f64` values as the long-lived broker-neutral source of
  truth for production reconciliation;
- fingerprinting production broker truth from non-canonical floats;
- silently accepting NaN, infinity, negative zero or non-canonical decimal
  aliases.

## Consequence

Stage 5D may close as a no-I/O restore-semantics foundation. Stage 5E/6 must not
claim production persistence readiness until exact numeric representation and
migration rules are implemented or explicitly waived by a new ADR.
