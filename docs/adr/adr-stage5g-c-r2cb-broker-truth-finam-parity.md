# ADR: Stage 5G-c R2-c-b BrokerTruth / FINAM snapshot parity

Status: review candidate

## Decision

Stage 5G-c consumes each FINAM full snapshot as one canonical broker-truth package. The package receipt timestamp, at millisecond precision, is the continuation clock. Component source timestamps describe economic identity and chronology; they never replace the package receipt clock.

The accepted R3 terminal MARKET authority remains immutable. R2-c-b calls only:

- `validate_stage5c_market_terminal_outcome_r3`;
- `settle_stage5c_validated_market_terminal_outcome_r3`.

## Canonical semantics

- A repeated `BrokerTradeId` with the same immutable economic payload is idempotent even when a later full snapshot refreshes `received_ts`; the latest observation receipt is retained.
- Conflicting payloads for one trade ID fail closed, including duplicates inside one snapshot.
- Target positions are the aggregate of all canonically matching rows. No matching rows means canonical flat; account-wide row counts remain diagnostic only.
- A present target MARKET order is classified before position semantics. `filled_qty` and signed position delta must agree exactly.
- Collections are sorted before durable evidence hashing. Receipt milliseconds are bound into evidence identity, lifecycle state and fingerprints.
- Terminal MARKET evidence is retained only long enough to pass through the accepted opaque R3 settlement capability.

## Scope

This is paper-only reconciliation hardening. It adds no Redis consumer, FINAM transport, HTTP mutation, broker dispatch, runtime-live path, real order, Stage 5G-d capability, merge to `main`, or deployment.

## Evidence

The synthetic FINAM fixture models partial fill followed by a filled full snapshot. The second snapshot repeats the first trade with unchanged economic fields and a refreshed package receipt, then adds the final trade. Focused tests also cover aggregate/absent-flat position semantics, duplicate conflicts, canonical instrument identity, exact order/position coherence, collection permutation, and millisecond receipt separation.
